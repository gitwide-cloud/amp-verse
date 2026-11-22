use amp_stem_worker::{Result, Spectral, load_model, read_request, write_parts};
use ort::{
    execution_providers::CPUExecutionProvider,
    session::{Session, builder::GraphOptimizationLevel},
    tensor::TensorElementType,
    value::Tensor,
};
use serde_json::json;
use std::{ffi::OsString, path::PathBuf, time::Instant};

struct Config {
    model: PathBuf,
    requests: usize,
    memory_mib: usize,
}
impl Config {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let args: Vec<_> = args.into_iter().collect();
        if args.len() > 14 || !args.len().is_multiple_of(2) {
            return Err("Use --model path [--provider cpu] [--device-id0] [--requests0..256] [--memory-mib512..6144]".into());
        }
        let mut model = None;
        let mut requests = 0;
        let mut memory_mib = 3072;
        let mut seen = std::collections::HashSet::new();
        for pair in args.chunks_exact(2) {
            let flag = pair[0].to_str().ok_or("Invalid option")?;
            if !seen.insert(flag) {
                return Err("Duplicate option".into());
            }
            let integer = || {
                pair[1]
                    .to_str()
                    .and_then(|v| v.parse::<usize>().ok())
                    .ok_or("Invalid numeric option")
            };
            match flag {
                "--model" => model = Some(PathBuf::from(&pair[1])),
                "--provider" if pair[1] == "cpu" => (),
                "--provider" => return Err("Native worker currently supports CPU only".into()),
                "--device-id" if integer()? == 0 => (),
                "--device-id" => return Err("CPU device-id must be0".into()),
                "--requests" => {
                    requests = integer()?;
                    if requests > 256 {
                        return Err("requests must be0..256".into());
                    }
                }
                "--memory-mib" => {
                    memory_mib = integer()?;
                    if !(512..=6144).contains(&memory_mib) {
                        return Err("memory-mib must be512..6144".into());
                    }
                }
                _ => return Err(format!("Unknown option: {flag}")),
            }
        }
        Ok(Self {
            model: model.ok_or("--model is required")?,
            requests,
            memory_mib,
        })
    }
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn run(config: Config) -> Result<()> {
    let _limit = limits::install(config.memory_mib)?;
    let input = std::io::stdin();
    let mut input = input.lock();
    let output = std::io::stdout();
    let mut output = output.lock();
    let Some(mut request) = read_request(&mut input)? else {
        return Ok(());
    };
    let runtime = std::env::var_os("ORT_DYLIB_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe().ok()?.parent().map(|p| {
                p.join(if cfg!(windows) {
                    "onnxruntime.dll"
                } else {
                    "libonnxruntime.so"
                })
            })
        })
        .filter(|p| p.is_absolute() && p.is_file())
        .ok_or("Set ORT_DYLIB_PATH to the installed native ONNX Runtime library")?;
    let model = load_model(&config.model)?;
    let begin = Instant::now();
    ort::init_from(runtime.to_str().ok_or("Runtime path must be Unicode")?)
        .with_name("AMP native stems")
        .commit()
        .map_err(err)?;
    let mut session = Session::builder()
        .map_err(err)?
        .with_execution_providers([CPUExecutionProvider::default()
            .with_arena_allocator(false)
            .build()])
        .map_err(err)?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(err)?
        .with_intra_threads(2)
        .map_err(err)?
        .with_inter_threads(1)
        .map_err(err)?
        .with_parallel_execution(false)
        .map_err(err)?
        .with_memory_pattern(false)
        .map_err(err)?
        .with_intra_op_spinning(false)
        .map_err(err)?
        .with_inter_op_spinning(false)
        .map_err(err)?
        .commit_from_memory(&model)
        .map_err(err)?;
    drop(model);
    if session.inputs.len() != 1
        || session.inputs[0].name != "mix_spec"
        || session.inputs[0].input_type.tensor_type() != Some(TensorElementType::Float32)
        || !session.inputs[0]
            .input_type
            .tensor_shape()
            .is_some_and(|s| **s == [1, 4, 2049, 476])
        || session.outputs.len() != 1
        || session.outputs[0].name != "stems_spec"
        || session.outputs[0].output_type.tensor_type() != Some(TensorElementType::Float32)
    {
        return Err("Unexpected pinned SCNet input/output contract".into());
    }
    let load_seconds = begin.elapsed().as_secs_f64();
    let mut spectral = Spectral::new();
    let mut index = 0;
    loop {
        let started = Instant::now();
        let (spec, scale, _) = spectral.analyze(&request.pcm)?;
        let tensor =
            Tensor::from_array(([1usize, 4, 2049, 476], spec.into_boxed_slice())).map_err(err)?;
        let result = session.run(ort::inputs!["mix_spec"=>tensor]).map_err(err)?;
        let (shape, values) = result["stems_spec"]
            .try_extract_tensor::<f32>()
            .map_err(err)?;
        if **shape != [1, 16, 2049, 476] {
            return Err("Unexpected SCNet output dimensions".into());
        }
        let parts = spectral.separate_output(values, &request.pcm, scale)?;
        let seconds = started.elapsed().as_secs_f64();
        let header = json!({"version":1,"request_id":request.header.request_id,"frames":request.header.frames,
            "sample_rate":44100,"channels":2,"parts":["vocals","drums","bass","other"],
            "model_kind":"scnet","provider":"CPUExecutionProvider","configured_providers":["CPUExecutionProvider"],
            "peak_worker_rss_bytes":limits::peak_rss()?,"hard_model_memory_limit_enforced":true,
            "memory_limit_mib":config.memory_mib,"memory_limit_kind":limits::KIND,
            "load_seconds":if index==0{load_seconds}else{0.},"inference_seconds":seconds,
            "real_time_factor":seconds/(request.header.frames as f64/44100.),"persistent_audio_bytes":0});
        write_parts(&mut output, &header, &parts)?;
        // Release spectral outputs before reading another window; no retained audio cache.
        drop(result);
        drop(parts);
        index += 1;
        if config.requests != 0 && index >= config.requests {
            break;
        }
        match read_request(&mut input)? {
            Some(next) => request = next,
            None => break,
        }
    }
    Ok(())
}
fn main() {
    if std::env::args_os().nth(1).is_some_and(|v| v == "--help") {
        eprintln!(
            "amp-stem-worker --model PINNED_SCNET.onnx [--provider cpu] [--device-id0] [--requests0] [--memory-mib3072]\nNative ONNX Runtime via ORT_DYLIB_PATH; no downloads or audio files. Model/runtime notices: crates/amp-stem-worker/NOTICES.md"
        );
        return;
    }
    if let Err(error) = Config::parse(std::env::args_os().skip(1)).and_then(run) {
        eprintln!("AMP native stem worker: {error}");
        std::process::exit(1);
    }
}

#[cfg(target_os = "linux")]
mod limits {
    use super::*;
    pub const KIND: &str = "linux-address-space-bytes";
    pub struct Limit;
    pub fn install(mib: usize) -> Result<Limit> {
        let mut old = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if unsafe { libc::getrlimit(libc::RLIMIT_AS, &mut old) } != 0 {
            return Err("Cannot inspect worker memory limit".into());
        }
        let cap = (mib as u64 * 1024 * 1024).min(old.rlim_max);
        let value = libc::rlimit {
            rlim_cur: cap,
            rlim_max: old.rlim_max,
        };
        if unsafe { libc::setrlimit(libc::RLIMIT_AS, &value) } != 0 {
            return Err("Cannot enforce worker memory limit".into());
        }
        Ok(Limit)
    }
    pub fn peak_rss() -> Result<u64> {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
            return Err("Worker memory telemetry unavailable".into());
        }
        Ok(unsafe { usage.assume_init() }.ru_maxrss as u64 * 1024)
    }
}
#[cfg(windows)]
mod limits {
    use super::*;
    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_JOB_MEMORY,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
            Threading::GetCurrentProcess,
        },
    };
    pub const KIND: &str = "windows-job-commit-bytes";
    pub struct Limit(HANDLE);
    impl Drop for Limit {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
    pub fn install(mib: usize) -> Result<Limit> {
        let job = Limit(unsafe { CreateJobObjectW(None, None) }.map_err(err)?);
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_JOB_MEMORY;
        info.JobMemoryLimit = mib * 1024 * 1024;
        unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &info as *const _ as _,
                std::mem::size_of_val(&info) as u32,
            )
        }
        .map_err(err)?;
        unsafe { AssignProcessToJobObject(job.0, GetCurrentProcess()) }.map_err(err)?;
        Ok(job)
    }
    pub fn peak_rss() -> Result<u64> {
        let mut info = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut info,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            )
        }
        .map_err(err)?;
        Ok(info.PeakWorkingSetSize as u64)
    }
}
#[cfg(not(any(target_os = "linux", windows)))]
mod limits {
    use super::*;
    pub const KIND: &str = "unsupported";
    pub fn install(_: usize) -> Result<()> {
        Err("Worker OS memory limits are not implemented on this platform".into())
    }
    pub fn peak_rss() -> Result<u64> {
        Err("Worker memory telemetry unavailable".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Config> {
        Config::parse(args.iter().map(OsString::from))
    }
    #[test]
    fn cpu_only_cli_and_memory_bounds() {
        assert!(parse(&["--model", "a", "--provider", "cuda"]).is_err());
        assert!(parse(&["--model", "a", "--provider", "dml"]).is_err());
        assert!(parse(&["--model", "a", "--device-id", "1"]).is_err());
        assert!(parse(&["--model", "a", "--memory-mib", "511"]).is_err());
        assert!(parse(&["--model", "a", "--memory-mib", "6145"]).is_err());
        assert!(parse(&["--model", "a", "--requests", "257"]).is_err());
        assert!(parse(&["--model", "a", "--model", "b"]).is_err());
        let cfg = parse(&["--model", "a"]).unwrap();
        assert_eq!(cfg.requests, 0);
        assert_eq!(cfg.memory_mib, 3072);
    }
}
