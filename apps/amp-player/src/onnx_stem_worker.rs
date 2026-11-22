//! One supervised, persistent ONNX process. PCM stays in bounded pipes/RAM, never audio files.
//! Caller owns the source generation: cancel its token on seek/load/Stop and reject old generations.
use serde_json::{Value, json};
use std::{
    io::{BufReader, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

pub(crate) const MAX_FRAMES: usize = 485_100;
const HEADER_LIMIT: usize = 1024;
const QUEUE_CAPACITY: usize = 2;
const TICK: Duration = Duration::from_millis(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const WARM_TIMEOUT: Duration = Duration::from_secs(8);
const REAP_TIMEOUT: Duration = Duration::from_secs(2);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const PENDING: u8 = 0;
const RUNNING: u8 = 1;
const DONE: u8 = 2;
const CANCELLED: u8 = 3;
type Parts = [Vec<f32>; 3];
type Reply = Result<Parts, String>;
static BROKER: OnceLock<Result<Broker, String>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, PartialEq, Eq)]
struct Config {
    python: PathBuf,
    native: bool,
    memory_mib: u32,
    model: PathBuf,
    provider: String,
    cuda_dll_dir: Option<PathBuf>,
    script: &'static str,
}
impl Config {
    fn environment() -> Result<Self, String> {
        let module = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("addons").join("stems-live")));
        let bundled_worker = module
            .as_ref()
            .map(|p| {
                p.join(if cfg!(windows) {
                    "amp-stem-worker.exe"
                } else {
                    "amp-stem-worker"
                })
            })
            .filter(|p| p.is_file());
        let explicit_python = std::env::var_os("AMP_ONNX_PYTHON");
        let native_path = std::env::var_os("AMP_ONNX_WORKER")
            .map(PathBuf::from)
            .or_else(|| {
                if explicit_python.is_none() {
                    bundled_worker
                } else {
                    None
                }
            });
        let native = native_path.is_some();
        let python = native_path
            .or_else(|| explicit_python.map(PathBuf::from))
            .filter(|p| p.is_absolute() && p.is_file())
            .ok_or("The live stem engine is not installed")?;
        let model = std::env::var_os("AMP_ONNX_MODEL")
            .map(PathBuf::from)
            .or_else(|| module.map(|p| p.join("scnet.int8.onnx")))
            .filter(|p| p.is_absolute() && p.is_file())
            .ok_or("The live stem model is not installed")?;
        let provider = std::env::var("AMP_ONNX_PROVIDER")
            .unwrap_or_else(|_| if native { "cpu".into() } else { "cuda".into() });
        let memory_mib = if native { 3072 } else { 6144 };
        if native && provider != "cpu" {
            return Err("The compact native stem engine currently uses CPU processing".into());
        }
        if !matches!(provider.as_str(), "cuda" | "cpu" | "dml") {
            return Err("AMP_ONNX_PROVIDER must be cuda, cpu or dml".into());
        }
        let cuda_dll_dir = std::env::var_os("AMP_ONNX_CUDA_DLL_DIR").map(PathBuf::from);
        if cuda_dll_dir
            .as_ref()
            .is_some_and(|p| !p.is_absolute() || !p.is_dir())
        {
            return Err("AMP_ONNX_CUDA_DLL_DIR must be an installed local directory".into());
        }
        Ok(Self {
            python,
            native,
            memory_mib,
            model,
            provider,
            cuda_dll_dir,
            script: include_str!("../../../tools/stem_onnx_window.py"),
        })
    }
    fn expected_provider(&self) -> &str {
        match self.provider.as_str() {
            "cpu" => "CPUExecutionProvider",
            "dml" => "DmlExecutionProvider",
            _ => "CUDAExecutionProvider",
        }
    }
}

struct Request {
    pcm: Arc<Vec<f32>>,
    cancel: Arc<AtomicBool>,
    state: Arc<AtomicU8>,
    config: Config,
    deadline: Instant,
    reply: mpsc::SyncSender<Reply>,
}
struct Broker {
    tx: mpsc::SyncSender<Request>,
    stopping: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
}
impl Broker {
    fn start() -> Result<Self, String> {
        let (tx, rx) = mpsc::sync_channel::<Request>(QUEUE_CAPACITY);
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let finished = Arc::new(AtomicBool::new(false));
        let acknowledged = finished.clone();
        thread::Builder::new()
            .name("amp-onnx-broker".into())
            .spawn(move || {
                if broker_loop(rx, stop) {
                    acknowledged.store(true, Ordering::Release);
                }
            })
            .map_err(|e| format!("Cannot start stem supervisor: {e}"))?;
        Ok(Self {
            tx,
            stopping,
            finished,
        })
    }
    fn shutdown(&self) -> bool {
        self.stopping.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(3);
        while !self.finished.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::sleep(TICK);
        }
        self.finished.load(Ordering::Acquire)
    }
    fn submit(
        &self,
        pcm: Vec<f32>,
        cancel: Arc<AtomicBool>,
        config: Config,
        budget: Duration,
    ) -> Reply {
        validate_input(&pcm)?;
        if cancel.load(Ordering::Acquire) {
            return Err("Stem window cancelled".into());
        }
        if self.stopping.load(Ordering::Acquire) {
            return Err("Stem supervisor is shutting down".into());
        }
        let (reply, rx) = mpsc::sync_channel(1);
        let state = Arc::new(AtomicU8::new(PENDING));
        let deadline = Instant::now() + budget.min(REQUEST_TIMEOUT);
        let request = Request {
            pcm: Arc::new(pcm),
            cancel: cancel.clone(),
            state: state.clone(),
            config,
            deadline,
            reply,
        };
        self.tx.try_send(request).map_err(|e| match e {
            mpsc::TrySendError::Full(_) => {
                "Stem request queue is full; retry when a deck window completes".to_owned()
            }
            mpsc::TrySendError::Disconnected(_) => "Stem supervisor stopped".to_owned(),
        })?;
        loop {
            if Instant::now() >= deadline
                && state
                    .compare_exchange(PENDING, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                return Err(
                    "Stem window timed out in the queue; original audio remains available".into(),
                );
            }
            if (cancel.load(Ordering::Acquire) || self.stopping.load(Ordering::Acquire))
                && state
                    .compare_exchange(PENDING, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                return Err("Stem window cancelled before inference".into());
            }
            match rx.recv_timeout(TICK) {
                Ok(result) => {
                    // Even a completed IPC result cannot publish after the deck token was revoked.
                    return if cancel.load(Ordering::Acquire)
                        || self.stopping.load(Ordering::Acquire)
                    {
                        Err("Stem window cancelled".into())
                    } else if Instant::now() >= deadline {
                        Err("Stem window timed out; original audio remains available".into())
                    } else {
                        result
                    };
                }
                Err(mpsc::RecvTimeoutError::Timeout) => (),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Stem supervisor disconnected".into());
                }
            }
        }
    }
}

/// Configuration check only: no Python launch, model probing or ordinary-playback warm-up.
pub(crate) fn configured() -> bool {
    Config::environment().is_ok()
}

/// Blocking BACKGROUND API. Never call from the render/audio/UI thread.
/// Interleaved stereo44.1k f32; vocals, instruments=(bass+other), drums. No disk cache.
/// At most one inference + two queued input windows; overload rejects immediately.
/// Budget includes queue time; expiry never cancels the caller's deck/source token.
pub(crate) fn separate(pcm: Vec<f32>, cancel: Arc<AtomicBool>, budget: Duration) -> Reply {
    validate_input(&pcm)?;
    let config = Config::environment()?;
    BROKER
        .get_or_init(Broker::start)
        .as_ref()
        .map_err(Clone::clone)?
        .submit(pcm, cancel, config, budget)
}
/// App-exit only: bounded (three seconds) cancellation/reap acknowledgement.
/// Normal deck Stop must use its cancellation token, not block the UI here.
/// False means cleanup was not acknowledged; it never reports a quarantined process as stopped.
pub(crate) fn shutdown() -> bool {
    match BROKER.get() {
        Some(Ok(broker)) => broker.shutdown(),
        _ => true,
    }
}
fn validate_input(pcm: &[f32]) -> Result<(), String> {
    if pcm.is_empty() || !pcm.len().is_multiple_of(2) || pcm.len() > MAX_FRAMES * 2 {
        return Err("Stem input must be 1..485100 stereo44.1k frames".into());
    }
    if !pcm.iter().all(|v| v.is_finite()) {
        return Err("Stem input contains non-finite samples".into());
    }
    Ok(())
}

struct WireRequest {
    id: String,
    pcm: Arc<Vec<f32>>,
    reply: mpsc::SyncSender<Reply>,
}
struct Worker {
    child: Box<dyn process_wrap::std::ChildWrapper>,
    tx: mpsc::SyncSender<WireRequest>,
    config: Config,
    poisoned: bool,
    warmed: bool,
    stderr: Arc<Mutex<Vec<u8>>>,
}
fn wrapped(command: Command) -> process_wrap::std::CommandWrap {
    let mut command = process_wrap::std::CommandWrap::from(command);
    #[cfg(windows)]
    command
        .wrap(process_wrap::std::CreationFlags(
            windows::Win32::System::Threading::CREATE_NO_WINDOW
                | windows::Win32::System::Threading::BELOW_NORMAL_PRIORITY_CLASS,
        ))
        .wrap(process_wrap::std::JobObject);
    #[cfg(unix)]
    command.wrap(process_wrap::std::ProcessGroup::leader());
    command
}
impl Worker {
    fn spawn(config: Config) -> Result<Self, String> {
        let mut command = Command::new(&config.python);
        if !config.native {
            command.args(["-I", "-B", "-u", "-c", config.script]);
        }
        command
            .arg("--model")
            .arg(&config.model)
            .args([
                "--provider",
                &config.provider,
                "--device-id",
                "0",
                "--requests",
                "0",
                "--memory-mib",
                &config.memory_mib.to_string(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if config.native {
            let runtime = config.python.parent().map(|p| {
                p.join(if cfg!(windows) {
                    "onnxruntime.dll"
                } else {
                    "libonnxruntime.so"
                })
            });
            if let Some(runtime) = runtime.filter(|p| p.is_file()) {
                command.env("ORT_DYLIB_PATH", runtime);
            }
        }
        if let Some(path) = &config.cuda_dll_dir {
            command.arg("--cuda-dll-dir").arg(path);
        }
        let mut child = wrapped(command)
            .spawn()
            .map_err(|e| format!("Cannot start ONNX worker: {e}"))?;
        let pipes = (
            child.stdin().take(),
            child.stdout().take(),
            child.stderr().take(),
        );
        let (Some(mut input), Some(output), Some(mut errors)) = pipes else {
            let _ = child.start_kill();
            return Err("ONNX worker pipes unavailable".into());
        };
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let tail = stderr.clone();
        if let Err(error) = thread::Builder::new()
            .name("amp-onnx-errors".into())
            .spawn(move || {
                let mut block = [0u8; 1024];
                while let Ok(n) = errors.read(&mut block) {
                    if n == 0 {
                        break;
                    }
                    if let Ok(mut bytes) = tail.lock() {
                        bytes.extend_from_slice(&block[..n]);
                        let excess = bytes.len().saturating_sub(4096);
                        if excess > 0 {
                            bytes.drain(..excess);
                        }
                    }
                }
            })
        {
            let _ = child.start_kill();
            return Err(format!("Cannot supervise ONNX diagnostics: {error}"));
        }
        let (tx, rx) = mpsc::sync_channel::<WireRequest>(1);
        let provider = config.expected_provider().to_owned();
        if let Err(error) = thread::Builder::new()
            .name("amp-onnx-pipes".into())
            .spawn(move || {
                let mut output = BufReader::new(output);
                while let Ok(work) = rx.recv() {
                    let result = exchange(&mut input, &mut output, &work.id, &work.pcm, &provider);
                    let failed = result.is_err();
                    let _ = work.reply.send(result);
                    if failed {
                        break;
                    }
                }
            })
        {
            let _ = child.start_kill();
            return Err(format!("Cannot supervise ONNX pipes: {error}"));
        }
        Ok(Self {
            child,
            tx,
            config,
            poisoned: false,
            warmed: false,
            stderr,
        })
    }
    /// Bounded cancellation acknowledgement; retain/quarantine an un-reaped process.
    fn terminate(&mut self) -> bool {
        self.poisoned = true;
        let _ = self.child.start_kill();
        let deadline = Instant::now() + REAP_TIMEOUT;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return true,
                _ if Instant::now() < deadline => thread::sleep(TICK),
                _ => return false,
            }
        }
    }
    fn diagnostic(&self) -> String {
        self.stderr
            .lock()
            .map(|v| {
                String::from_utf8_lossy(&v)
                    .chars()
                    .filter(|c| *c != '\0')
                    .take(1024)
                    .collect()
            })
            .unwrap_or_default()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}
fn stop_worker(worker: &mut Option<Worker>) -> bool {
    if worker.as_mut().is_none_or(Worker::terminate) {
        *worker = None;
        true
    } else {
        false
    }
}
fn broker_loop(rx: mpsc::Receiver<Request>, stopping: Arc<AtomicBool>) -> bool {
    let mut worker: Option<Worker> = None;
    let mut last_use = Instant::now();
    loop {
        if stopping.load(Ordering::Acquire) {
            let reaped = stop_worker(&mut worker);
            while let Ok(work) = rx.try_recv() {
                let _ = work.reply.send(Err("Stem supervisor shutting down".into()));
            }
            return reaped;
        }
        let work = match rx.recv_timeout(TICK) {
            Ok(work) => work,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if last_use.elapsed() >= IDLE_TIMEOUT {
                    let _ = stop_worker(&mut worker);
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return stop_worker(&mut worker);
            }
        };
        if work
            .state
            .compare_exchange(PENDING, RUNNING, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
            || work.cancel.load(Ordering::Acquire)
            || Instant::now() >= work.deadline
        {
            let _ = work
                .reply
                .send(Err("Stem window cancelled before inference".into()));
            continue;
        }
        let result = process_window(&mut worker, &work, &stopping);
        work.state.store(DONE, Ordering::Release);
        let _ = work.reply.send(result);
        last_use = Instant::now();
    }
}
fn process_window(worker: &mut Option<Worker>, work: &Request, stopping: &AtomicBool) -> Reply {
    if work.cancel.load(Ordering::Acquire) || stopping.load(Ordering::Acquire) {
        return Err("Stem window cancelled before inference".into());
    }
    if Instant::now() >= work.deadline {
        return Err(
            "Stem window timed out before inference; original audio remains available".into(),
        );
    }
    if worker
        .as_ref()
        .is_some_and(|w| w.poisoned || w.config != work.config)
        && !stop_worker(worker)
    {
        return Err("Previous ONNX worker has not stopped; retry later".into());
    }
    if worker.is_none() {
        *worker = Some(Worker::spawn(work.config.clone())?);
    }
    let active = worker.as_mut().ok_or("ONNX worker unavailable")?;
    let deadline = work.deadline.min(
        Instant::now()
            + if active.warmed {
                WARM_TIMEOUT
            } else {
                REQUEST_TIMEOUT
            },
    );
    let id = format!("window-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let (reply, rx) = mpsc::sync_channel(1);
    // The broker has one active exchange; this channel cannot accumulate windows.
    if active
        .tx
        .try_send(WireRequest {
            id,
            pcm: work.pcm.clone(),
            reply,
        })
        .is_err()
    {
        let _ = stop_worker(worker);
        return Err("ONNX I/O worker disconnected".into());
    }
    loop {
        if work.cancel.load(Ordering::Acquire)
            || stopping.load(Ordering::Acquire)
            || Instant::now() >= deadline
        {
            let reaped = stop_worker(worker);
            return Err(if reaped {
                "Stem window cancelled or timed out; ONNX process stopped"
            } else {
                "ONNX cancellation not acknowledged; worker quarantined"
            }
            .into());
        }
        match rx.recv_timeout(TICK) {
            Ok(Ok(parts)) => {
                if let Some(worker) = worker {
                    worker.warmed = true;
                }
                return Ok(parts);
            }
            Ok(Err(error)) => {
                let detail = worker.as_ref().map(Worker::diagnostic).unwrap_or_default();
                let _ = stop_worker(worker);
                return Err(format!("ONNX response rejected: {error}. {detail}"));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let detail = worker.as_ref().map(Worker::diagnostic).unwrap_or_default();
                let _ = stop_worker(worker);
                return Err(format!(
                    "ONNX worker exited before completing PCM. {detail}"
                ));
            }
        }
    }
}
fn exchange(
    input: &mut impl Write,
    output: &mut impl Read,
    id: &str,
    pcm: &[f32],
    provider: &str,
) -> Reply {
    let frames = pcm.len() / 2;
    let mut header = serde_json::to_vec(
        &json!({"version":1,"request_id":id,"frames":frames,"sample_rate":44100,"channels":2}),
    )
    .map_err(|e| e.to_string())?;
    header.push(b'\n');
    if header.len() > HEADER_LIMIT {
        return Err("Request header too large".into());
    }
    input
        .write_all(&header)
        .map_err(|e| format!("Request header: {e}"))?;
    let mut bytes = [0u8; 8192];
    for chunk in pcm.chunks(2048) {
        for (sample, out) in chunk.iter().zip(bytes.as_chunks_mut::<4>().0.iter_mut()) {
            out.copy_from_slice(&sample.to_le_bytes());
        }
        input
            .write_all(&bytes[..chunk.len() * 4])
            .map_err(|e| format!("Request PCM: {e}"))?;
    }
    input.flush().map_err(|e| e.to_string())?;
    let mut line = Vec::with_capacity(HEADER_LIMIT);
    loop {
        if line.len() == HEADER_LIMIT {
            return Err("Response header exceeds 1024 bytes".into());
        }
        let mut byte = [0];
        output
            .read_exact(&mut byte)
            .map_err(|e| format!("Response header: {e}"))?;
        line.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    let h: Value = serde_json::from_slice(&line).map_err(|_| "Response is not JSON")?;
    if h["version"] != 1
        || h["request_id"] != id
        || h["frames"].as_u64() != Some(frames as u64)
        || h["sample_rate"] != 44100
        || h["channels"] != 2
        || h["parts"] != json!(["vocals", "drums", "bass", "other"])
        || h["provider"] != provider
        || h["hard_model_memory_limit_enforced"] != true
        || h["memory_limit_mib"]
            .as_u64()
            .is_none_or(|v| v == 0 || v > 6144)
        || h["persistent_audio_bytes"] != 0
    {
        return Err("Response identity, shape, provider or memory contract mismatch".into());
    }
    let mut read_part = || -> Result<Vec<f32>, String> {
        let mut raw = vec![0u8; pcm.len() * 4];
        output
            .read_exact(&mut raw)
            .map_err(|e| format!("Truncated stem PCM: {e}"))?;
        let samples: Vec<f32> = raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| f32::from_le_bytes([v[0], v[1], v[2], v[3]]))
            .collect();
        if !samples.iter().all(|v| v.is_finite()) {
            return Err("Non-finite stem PCM".into());
        }
        Ok(samples)
    };
    let vocals = read_part()?;
    let drums = read_part()?;
    let mut instruments = read_part()?;
    let other = read_part()?;
    for (bass, other) in instruments.iter_mut().zip(other) {
        *bass += other;
        if !bass.is_finite() {
            return Err("Instrument sum overflow".into());
        }
    }
    Ok([vocals, instruments, drums])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // Protocol/supervision FAKE ONLY. Never imports ONNX, loads weights or measures GPU quality.
    const FAKE: &str = r#"
import json,struct,sys,time,subprocess,os
mode=sys.argv[sys.argv.index('--model')+1]
count=0
while True:
 line=sys.stdin.buffer.readline()
 if not line: break
 h=json.loads(line); n=h['frames']*2; raw=sys.stdin.buffer.read(n*4)
 if len(raw)!=n*4: raise RuntimeError('fake truncated input')
 count+=1
 if mode.endswith('hang'):
  child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)'])
  with open(mode,'w') as marker: marker.write(str(child.pid))
  time.sleep(60)
 if mode.endswith('stderr'):
  sys.stderr.buffer.write((b'x\x00'*10000)+b'\x00FAIL\x00');sys.stderr.buffer.flush()
  sys.exit(2)
 answer={'version':1,'request_id':h['request_id'],'frames':h['frames'],'sample_rate':44100,'channels':2,'parts':['vocals','drums','bass','other'],'provider':'CPUExecutionProvider','hard_model_memory_limit_enforced':True,'memory_limit_mib':6144,'persistent_audio_bytes':0}
 sys.stdout.buffer.write(json.dumps(answer).encode()+b'\n')
 for value in [count,2.,3.,4.]: sys.stdout.buffer.write(struct.pack('<f',value)*n)
 sys.stdout.buffer.flush()
"#;
    fn config(mode: PathBuf) -> Config {
        let python = std::env::var_os("AMP_ONNX_TEST_PYTHON").map(PathBuf::from)
            .or_else(|| ["python3", "python"].into_iter().find_map(|name| {
                Command::new(name).arg("--version").stdout(Stdio::null()).stderr(Stdio::null())
                    .status().ok().filter(|s| s.success()).map(|_| PathBuf::from(name))
            })).expect("Fake IPC tests require standard Python (AMP_ONNX_TEST_PYTHON may select it); not ONNX/GPU");
        Config {
            python,
            native: false,
            memory_mib: 6144,
            model: mode,
            provider: "cpu".into(),
            cuda_dll_dir: None,
            script: FAKE,
        }
    }
    fn token() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }
    fn header(frames: usize) -> Value {
        json!({"version":1,"request_id":"test","frames":frames,"sample_rate":44100,"channels":2,
            "parts":["vocals","drums","bass","other"],"provider":"CPUExecutionProvider",
            "hard_model_memory_limit_enforced":true,"memory_limit_mib":6144,"persistent_audio_bytes":0})
    }
    fn wire(h: &Value, parts: &[f32]) -> Cursor<Vec<u8>> {
        let mut out = serde_json::to_vec(h).unwrap();
        out.push(b'\n');
        for sample in parts {
            out.extend(sample.to_le_bytes());
        }
        Cursor::new(out)
    }
    #[test]
    fn fake_persistent_worker_reuses_model_slot_and_combines_exact_parts() {
        let broker = Broker::start().unwrap();
        let config = config("good".into());
        let first = broker
            .submit(
                vec![0.25; 4],
                token(),
                config.clone(),
                Duration::from_secs(1),
            )
            .unwrap();
        let second = broker
            .submit(vec![0.5; 4], token(), config, Duration::from_secs(2))
            .unwrap();
        assert_eq!(first, [vec![1.; 4], vec![7.; 4], vec![2.; 4]]);
        assert_eq!(
            second[0],
            vec![2.; 4],
            "same fake process must handle second window"
        );
        assert!(broker.shutdown());
    }
    #[test]
    fn invalid_or_oversized_input_is_rejected_before_ipc() {
        for pcm in [
            vec![],
            vec![0.; 3],
            vec![f32::NAN; 2],
            vec![f32::INFINITY; 2],
            vec![0.; MAX_FRAMES * 2 + 2],
        ] {
            assert!(validate_input(&pcm).is_err());
        }
        assert!(validate_input(&vec![0.; MAX_FRAMES * 2]).is_ok());
    }
    #[test]
    fn wire_rejects_wrong_identity_shape_provider_and_memory_claims() {
        for (key, value) in [
            ("request_id", json!("stale")),
            ("frames", json!(264601)),
            ("version", json!(2)),
            ("channels", json!(1)),
            ("sample_rate", json!(48000)),
            ("provider", json!("CUDAExecutionProvider")),
            ("parts", json!(["drums", "vocals", "bass", "other"])),
            ("hard_model_memory_limit_enforced", json!(false)),
            ("memory_limit_mib", json!(6145)),
            ("persistent_audio_bytes", json!(1)),
        ] {
            let mut h = header(1);
            h[key] = value;
            assert!(
                exchange(
                    &mut Vec::new(),
                    &mut wire(&h, &[0.; 8]),
                    "test",
                    &[0.; 2],
                    "CPUExecutionProvider"
                )
                .is_err(),
                "{key}"
            );
        }
    }
    #[test]
    fn wire_caps_header_and_rejects_truncation_nan_and_sum_overflow() {
        let h = header(1);
        for mut output in [
            Cursor::new(vec![b' '; 1025]),
            wire(&h, &[0.; 7]),
            wire(&h, &[f32::NAN; 8]),
            wire(
                &h,
                &[0., 0., 0., 0., f32::MAX, f32::MAX, f32::MAX, f32::MAX],
            ),
        ] {
            assert!(
                exchange(
                    &mut Vec::new(),
                    &mut output,
                    "test",
                    &[0.; 2],
                    "CPUExecutionProvider"
                )
                .is_err()
            );
        }
        let output = exchange(
            &mut Vec::new(),
            &mut wire(&h, &[1., 2., 3., 4., 5., 6., 7., 8.]),
            "test",
            &[0.; 2],
            "CPUExecutionProvider",
        )
        .unwrap();
        assert_eq!(output, [vec![1., 2.], vec![12., 14.], vec![3., 4.]]);
    }
    #[test]
    fn queue_is_bounded_and_cancelled_pending_inputs_never_start() {
        let (tx, rx) = mpsc::sync_channel(QUEUE_CAPACITY);
        let broker = Broker {
            tx,
            stopping: token(),
            finished: token(),
        };
        let cfg = config("good".into());
        for _ in 0..QUEUE_CAPACITY {
            assert!(broker.tx.try_send(request(cfg.clone(), token())).is_ok());
        }
        assert!(
            broker
                .submit(vec![0.; 2], token(), cfg.clone(), Duration::from_secs(5))
                .unwrap_err()
                .contains("queue is full")
        );
        drop(rx);
        let (tx, rx) = mpsc::sync_channel(1);
        let broker = Broker {
            tx,
            stopping: token(),
            finished: token(),
        };
        let cancel = token();
        let flag = cancel.clone();
        let handle =
            thread::spawn(move || broker.submit(vec![0.; 2], flag, cfg, Duration::from_secs(5)));
        let queued = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(queued.state.load(Ordering::Acquire), PENDING);
        cancel.store(true, Ordering::Release);
        assert!(handle.join().unwrap().is_err());
        assert_eq!(
            queued.state.load(Ordering::Acquire),
            CANCELLED,
            "queued cancellation must prevent a later inference"
        );
    }
    fn request(config: Config, cancel: Arc<AtomicBool>) -> Request {
        let (reply, _) = mpsc::sync_channel(1);
        let deadline = Instant::now() + Duration::from_secs(5);
        Request {
            pcm: Arc::new(vec![0.; 4]),
            cancel,
            state: Arc::new(AtomicU8::new(RUNNING)),
            config,
            deadline,
            reply,
        }
    }
    fn wait_for_child(marker: &std::path::Path) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(pid) = std::fs::read_to_string(marker)
                .ok()
                .and_then(|s| s.parse().ok())
            {
                return pid;
            }
            assert!(
                Instant::now() < deadline,
                "Fake child did not start before the test deadline"
            );
            thread::sleep(TICK);
        }
    }
    #[test]
    fn fake_active_cancel_kills_process_group_and_does_not_publish_stale_pcm() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("hang");
        let config = config(marker.clone());
        let cancel = token();
        let work = request(config, cancel.clone());
        let handle = thread::spawn(move || {
            let mut worker = None;
            let result = process_window(&mut worker, &work, &AtomicBool::new(false));
            (worker, result)
        });
        let child_pid = wait_for_child(&marker);
        cancel.store(true, Ordering::Release);
        let (worker, result) = handle.join().unwrap();
        assert!(worker.is_none(), "actual process exit must be acknowledged");
        assert!(result.is_err());
        wait_descendant_stopped(child_pid);
    }
    fn wait_descendant_stopped(pid: u32) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            #[cfg(target_os = "linux")]
            let alive = std::fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .is_some_and(|s| {
                    s.rsplit_once(')')
                        .is_some_and(|(_, tail)| !tail.trim_start().starts_with('Z'))
                });
            #[cfg(windows)]
            let alive = unsafe {
                use windows::Win32::{
                    Foundation::CloseHandle,
                    System::Threading::{
                        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
                    },
                };
                match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                    Ok(handle) => {
                        let mut code = 0;
                        let status = GetExitCodeProcess(handle, &mut code);
                        let _ = CloseHandle(handle);
                        status.expect("query fake descendant exit");
                        code == 259
                    }
                    Err(error) => {
                        assert_eq!(
                            (error.code().0 as u32) & 0xffff,
                            87,
                            "unexpected descendant-query failure"
                        );
                        false
                    }
                }
            };
            #[cfg(not(any(target_os = "linux", windows)))]
            let alive = {
                let _ = pid;
                false
            };
            if !alive {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "fake descendant survived tree cancellation"
            );
            thread::sleep(TICK);
        }
    }
    #[test]
    fn fake_shutdown_acknowledges_active_process_tree_exit() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("hang");
        let broker = Arc::new(Broker::start().unwrap());
        let background = broker.clone();
        let cfg = config(marker.clone());
        let handle = thread::spawn(move || {
            background.submit(vec![0.; 4], token(), cfg, Duration::from_secs(5))
        });
        let pid = wait_for_child(&marker);
        let begin = Instant::now();
        assert!(broker.shutdown());
        assert!(begin.elapsed() < Duration::from_secs(3));
        assert!(handle.join().unwrap().is_err());
        wait_descendant_stopped(pid);
    }
    #[test]
    fn fake_timeout_stops_worker_and_stderr_is_bounded_and_nul_free() {
        let directory = tempfile::tempdir().unwrap();
        let cfg = config(directory.path().join("hang"));
        let mut work = request(cfg, token());
        work.deadline = Instant::now() + Duration::from_millis(250);
        let mut worker = None;
        let begin = Instant::now();
        assert!(process_window(&mut worker, &work, &AtomicBool::new(false)).is_err());
        assert!(worker.is_none());
        assert!(begin.elapsed() < Duration::from_secs(4));
        let result = process_window(
            &mut worker,
            &request(config("stderr".into()), token()),
            &AtomicBool::new(false),
        );
        let error = result.unwrap_err();
        assert!(!error.contains('\0'));
        assert!(error.len() < 1400);
        assert!(worker.is_none());
    }
    #[test]
    fn queued_deadline_does_not_cancel_deck_or_allow_late_inference() {
        let (tx, rx) = mpsc::sync_channel(1);
        let broker = Broker {
            tx,
            stopping: token(),
            finished: token(),
        };
        let cancel = token();
        let flag = cancel.clone();
        let cfg = config("good".into());
        let handle =
            thread::spawn(move || broker.submit(vec![0.; 2], flag, cfg, Duration::from_millis(80)));
        let queued = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(handle.join().unwrap().unwrap_err().contains("timed out"));
        assert!(
            !cancel.load(Ordering::Acquire),
            "queue expiry must preserve original-audio fallback"
        );
        assert_eq!(queued.state.load(Ordering::Acquire), CANCELLED);
    }
    #[test]
    fn active_deadline_reaps_worker_without_cancelling_deck_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("hang");
        let broker = Arc::new(Broker::start().unwrap());
        let background = broker.clone();
        let cancel = token();
        let flag = cancel.clone();
        let cfg = config(marker.clone());
        let handle = thread::spawn(move || {
            background.submit(vec![0.; 4], flag, cfg, Duration::from_secs(1))
        });
        let pid = wait_for_child(&marker);
        assert!(handle.join().unwrap().unwrap_err().contains("timed out"));
        assert!(
            !cancel.load(Ordering::Acquire),
            "active expiry must preserve original-audio fallback"
        );
        assert!(broker.shutdown());
        wait_descendant_stopped(pid);
    }
}
