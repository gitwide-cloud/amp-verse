//! A bounded pool of supervised, persistent ONNX processes. PCM stays in bounded pipes/RAM, never audio files.
//! Caller owns the source generation: cancel its token on seek/load/Stop and reject old generations.
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    io::{BufReader, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

pub(crate) const MAX_FRAMES: usize = 485_100;
const HEADER_LIMIT: usize = 1024;
#[cfg(test)]
const QUEUE_CAPACITY: usize = 2;
const TICK: Duration = Duration::from_millis(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const REAP_TIMEOUT: Duration = Duration::from_secs(2);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const PENDING: u8 = 0;
const RUNNING: u8 = 1;
const DONE: u8 = 2;
const CANCELLED: u8 = 3;
type Parts = [Vec<f32>; 3];
type Reply = Result<Parts, String>;
/// Background-only, nonblocking snapshot of remaining playable buffer time.
/// Zero irrevocably expires this request; it never cancels the shared source.
pub(crate) type LiveBudget = Arc<dyn Fn() -> Duration + Send + Sync>;

fn live_budget_expired(budget: Option<&LiveBudget>, state: &AtomicU8) -> bool {
    if state.load(Ordering::Acquire) == CANCELLED {
        return true;
    }
    if budget.is_some_and(|remaining| remaining().is_zero()) {
        state.store(CANCELLED, Ordering::Release);
        true
    } else {
        false
    }
}
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
    runtime: Option<PathBuf>,
    script: &'static str,
}
pub(crate) const CUDA_VENDOR_DLLS: [&str; 12] = [
    "cublasLt64_12.dll",
    "cublas64_12.dll",
    "cufft64_11.dll",
    "cudart64_12.dll",
    "cudnn_engines_runtime_compiled64_9.dll",
    "cudnn_engines_precompiled64_9.dll",
    "cudnn_heuristic64_9.dll",
    "cudnn_ops64_9.dll",
    "cudnn_cnn64_9.dll",
    "cudnn_adv64_9.dll",
    "cudnn_graph64_9.dll",
    "cudnn64_9.dll",
];
fn verified_cuda_module(directory: &std::path::Path) -> bool {
    let manifest = directory.join("module.json");
    let Ok(metadata) = std::fs::symlink_metadata(&manifest) else {
        return false;
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 16384 {
        return false;
    }
    let Ok(bytes) = std::fs::read(&manifest) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return false;
    };
    value["id"] == "stems-cuda"
        && value["version"] == 1
        && value["provider"] == "cuda"
        && value["onnxruntime_version"] == "1.22.0"
        && value["qualified"] == true
        && value["model_sha256"]
            == "98228931494151762a1c4ab1ec7899a894b1f81fd4a509921a9d2f9bacc50845"
        && [
            "onnxruntime.dll",
            "onnxruntime_providers_shared.dll",
            "onnxruntime_providers_cuda.dll",
        ]
        .iter()
        .all(|name| directory.join(name).is_file())
        && CUDA_VENDOR_DLLS
            .iter()
            .all(|name| directory.join("vendor").join(name).is_file())
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
            .or_else(|| module.as_ref().map(|p| p.join("scnet.int8.onnx")))
            .filter(|p| p.is_absolute() && p.is_file())
            .ok_or("The live stem model is not installed")?;
        let gpu_module = module
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.join("stems-cuda"));
        let verified_gpu = gpu_module.as_ref().filter(|p| verified_cuda_module(p));
        let provider = std::env::var("AMP_ONNX_PROVIDER").unwrap_or_else(|_| {
            if native {
                if verified_gpu.is_some() {
                    "cuda".into()
                } else {
                    "cpu".into()
                }
            } else {
                "cuda".into()
            }
        });
        let memory_mib = if native && provider == "cpu" {
            3072
        } else {
            6144
        };
        if !matches!(provider.as_str(), "cuda" | "cpu" | "dml") || (native && provider == "dml") {
            return Err(
                "AMP_ONNX_PROVIDER must be cpu or cuda for native stems (legacy also supports dml)"
                    .into(),
            );
        }
        let cuda_dll_dir = std::env::var_os("AMP_ONNX_CUDA_DLL_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                if provider == "cuda" {
                    verified_gpu.map(|p| p.join("vendor"))
                } else {
                    None
                }
            });
        if cuda_dll_dir
            .as_ref()
            .is_some_and(|p| !p.is_absolute() || !p.is_dir())
        {
            return Err("AMP_ONNX_CUDA_DLL_DIR must be an installed local directory".into());
        }
        let runtime = if native && provider == "cuda" {
            let path = std::env::var_os("AMP_ONNX_RUNTIME")
                .map(PathBuf::from)
                .or_else(|| verified_gpu.map(|p| p.join("onnxruntime.dll")))
                .or_else(|| std::env::var_os("ORT_DYLIB_PATH").map(PathBuf::from))
                .filter(|p| p.is_absolute() && p.is_file());
            if path.is_none() || cuda_dll_dir.is_none() {
                return Err("CUDA stem add-on is incomplete; install its runtime/vendor libraries or explicitly select CPU processing".into());
            }
            path
        } else if native {
            python
                .parent()
                .map(|p| {
                    p.join(if cfg!(windows) {
                        "onnxruntime.dll"
                    } else {
                        "libonnxruntime.so"
                    })
                })
                .filter(|p| p.is_file())
        } else {
            None
        };
        Ok(Self {
            python,
            native,
            memory_mib,
            model,
            provider,
            cuda_dll_dir,
            runtime,
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

// Diagnostic only: bounded metadata, never PCM, paths, or worker stderr.
pub(crate) fn trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("AMP_STEM_TRACE").is_ok_and(|v| v == "1"))
}
pub(crate) fn trace(event: Value) {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    if trace_enabled() && COUNT.fetch_add(1, Ordering::Relaxed) < 4096 {
        // Diagnostics must not panic if stderr has been closed.
        let _ = writeln!(std::io::stderr().lock(), "[stem-trace] {event}");
    }
}
fn margin_ms(deadline: Instant, now: Instant) -> f64 {
    if deadline >= now {
        deadline.duration_since(now).as_secs_f64() * 1000.
    } else {
        -now.duration_since(deadline).as_secs_f64() * 1000.
    }
}

struct Request {
    id: String,
    submitted: Instant,
    pcm: Arc<Vec<f32>>,
    cancel: Arc<AtomicBool>,
    state: Arc<AtomicU8>,
    config: Config,
    deadline: Instant,
    live_budget: Option<LiveBudget>,
    reply: mpsc::SyncSender<Reply>,
}
#[derive(Default)]
struct PoolMetrics {
    live: AtomicUsize,
    peak: AtomicUsize,
}
#[derive(Default)]
struct Pending {
    requests: Vec<Request>,
    outstanding: HashSet<usize>,
}
struct RequestQueue {
    pending: Mutex<Pending>,
    ready: Condvar,
    capacity: usize,
}
impl RequestQueue {
    fn new(capacity: usize) -> Self {
        Self {
            pending: Mutex::new(Pending::default()),
            ready: Condvar::new(),
            capacity,
        }
    }
    fn push(&self, request: Request) -> Result<(), String> {
        let mut pending = self.pending.lock().map_err(|_| "Stem queue unavailable")?;
        Self::discard_stale(&mut pending);
        if pending.requests.len() >= self.capacity {
            return Err("Stem request queue is full; retry when a deck window completes".into());
        }
        let source = Arc::as_ptr(&request.cancel) as usize;
        if !pending.outstanding.insert(source) {
            return Err("A stem request for this source is already outstanding".into());
        }
        pending.requests.push(request);
        self.ready.notify_one();
        Ok(())
    }
    fn discard_stale(pending: &mut Pending) {
        let now = Instant::now();
        let mut index = 0;
        while index < pending.requests.len() {
            let work = &pending.requests[index];
            if work.state.load(Ordering::Acquire) == CANCELLED
                || work.cancel.load(Ordering::Acquire)
                || now >= work.deadline
                || live_budget_expired(work.live_budget.as_ref(), &work.state)
            {
                let work = pending.requests.remove(index);
                if trace_enabled() {
                    trace(json!({"event":"queue_discard", "id":work.id,
                        "queue_ms":now.duration_since(work.submitted).as_secs_f64()*1000.,
                        "request_margin_ms":margin_ms(work.deadline, now),
                        "cancelled":work.cancel.load(Ordering::Acquire)}));
                }
                work.state.store(CANCELLED, Ordering::Release);
                pending
                    .outstanding
                    .remove(&(Arc::as_ptr(&work.cancel) as usize));
                let _ = work.reply.try_send(Err(
                    "Stem window cancelled or expired before inference".into(),
                ));
            } else {
                index += 1;
            }
        }
    }
    fn finish(&self, work: &Request) {
        if let Ok(mut pending) = self.pending.lock() {
            pending
                .outstanding
                .remove(&(Arc::as_ptr(&work.cancel) as usize));
        }
    }
    fn take(&self, stopping: &AtomicBool) -> Option<Request> {
        let mut pending = self.pending.lock().ok()?;
        Self::discard_stale(&mut pending);
        while pending.requests.is_empty() && !stopping.load(Ordering::Acquire) {
            pending = self.ready.wait_timeout(pending, TICK).ok()?.0;
            // Return periodically so idle slots can reap their model process.
            if pending.requests.is_empty() {
                return None;
            }
        }
        if stopping.load(Ordering::Acquire) {
            return None;
        }
        // Paused submissions can become urgent after Play. Compare all live
        // snapshots against one clock reading without extending any deadline.
        let now = Instant::now();
        let (index, _, live_remaining) = pending
            .requests
            .iter()
            .enumerate()
            .map(|(index, request)| {
                let live = request.live_budget.as_ref().map(|remaining| remaining());
                if live.is_some_and(|remaining| remaining.is_zero()) {
                    request.state.store(CANCELLED, Ordering::Release);
                }
                let fixed = request.deadline.saturating_duration_since(now);
                let remaining = live.map_or(fixed, |remaining| fixed.min(remaining));
                let effective = now.checked_add(remaining).unwrap_or(request.deadline);
                (index, effective, live)
            })
            // min_by_key retains the first request for equal deadlines (FIFO).
            .min_by_key(|(_, effective, _)| *effective)?;
        let request = pending.requests.remove(index);
        if trace_enabled() {
            trace(json!({"event":"queue_select", "id":request.id,
                "request_margin_ms":margin_ms(request.deadline, now),
                "live_budget_ms":live_remaining.map(|remaining| remaining.as_secs_f64()*1000.)}));
        }
        Some(request)
    }
    fn drain(&self) {
        if let Ok(mut pending) = self.pending.lock() {
            for work in pending.requests.drain(..) {
                let _ = work.reply.send(Err("Stem supervisor shutting down".into()));
            }
            pending.outstanding.clear();
        }
    }
}
struct Broker {
    queue: Arc<RequestQueue>,
    stopping: Arc<AtomicBool>,
    finished: Arc<AtomicUsize>,
    width: usize,
    memory_mib: Option<u32>,
    metrics: Arc<PoolMetrics>,
    provider: Option<String>,
}
impl Broker {
    #[cfg(test)]
    fn start() -> Result<Self, String> {
        Self::start_with(1, QUEUE_CAPACITY, None)
    }
    fn start_pool(config: &Config) -> Result<Self, String> {
        let (width, memory) = pool_plan(config);
        let mut broker = Self::start_with(width, 6, Some(memory))?;
        broker.provider = Some(config.provider.clone());
        Ok(broker)
    }
    fn start_with(width: usize, capacity: usize, memory_mib: Option<u32>) -> Result<Self, String> {
        let queue = Arc::new(RequestQueue::new(capacity));
        let stopping = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicUsize::new(0));
        let metrics = Arc::new(PoolMetrics::default());
        for slot in 0..width {
            let q = queue.clone();
            let stop = stopping.clone();
            let done = finished.clone();
            let stats = metrics.clone();
            if let Err(error) = thread::Builder::new()
                .name(format!("amp-onnx-slot-{slot}"))
                .spawn(move || {
                    if broker_loop(q, stop, stats) {
                        done.fetch_add(1, Ordering::AcqRel);
                    }
                })
            {
                stopping.store(true, Ordering::Release);
                queue.ready.notify_all();
                return Err(format!("Cannot start stem supervisor: {error}"));
            }
        }
        Ok(Self {
            queue,
            stopping,
            finished,
            width,
            memory_mib,
            metrics,
            provider: None,
        })
    }
    fn shutdown(&self) -> bool {
        self.stopping.store(true, Ordering::Release);
        self.queue.ready.notify_all();
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.finished.load(Ordering::Acquire) < self.width && Instant::now() < deadline {
            thread::sleep(TICK);
        }
        self.finished.load(Ordering::Acquire) == self.width
    }
    #[cfg(test)]
    fn submit(
        &self,
        pcm: Vec<f32>,
        cancel: Arc<AtomicBool>,
        config: Config,
        budget: Duration,
    ) -> Reply {
        self.submit_traced(pcm, cancel, config, budget, None, None)
    }
    fn submit_traced(
        &self,
        pcm: Vec<f32>,
        cancel: Arc<AtomicBool>,
        mut config: Config,
        budget: Duration,
        source: Option<(usize, u64, u64)>,
        live_budget: Option<LiveBudget>,
    ) -> Reply {
        if let Some(memory) = self.memory_mib {
            if self.width > 1 && (!config.native || config.provider != "cpu") {
                return Err(
                    "Stem pool provider changed; restart AMP before changing providers".into(),
                );
            }
            config.memory_mib = memory;
        }
        validate_input(&pcm)?;
        if cancel.load(Ordering::Acquire) {
            return Err("Stem window cancelled".into());
        }
        if self.stopping.load(Ordering::Acquire) {
            return Err("Stem supervisor is shutting down".into());
        }
        let (reply, rx) = mpsc::sync_channel(1);
        let state = Arc::new(AtomicU8::new(PENDING));
        let submitted = Instant::now();
        let deadline = submitted + budget.min(REQUEST_TIMEOUT);
        let id = format!("window-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
        if trace_enabled() {
            trace(json!({"event":"submit", "id":id, "source":source,
                "budget_ms":budget.as_secs_f64()*1000., "pool_width":self.width}));
        }
        let request = Request {
            id,
            submitted,
            pcm: Arc::new(pcm),
            cancel: cancel.clone(),
            state: state.clone(),
            config,
            deadline,
            live_budget: live_budget.clone(),
            reply,
        };
        self.queue.push(request)?;
        loop {
            // Return retained original audio without waiting for process kill/reap.
            // The broker retains its slot and request ownership during cleanup.
            if live_budget_expired(live_budget.as_ref(), &state) {
                return Err(
                    "Stem window exceeded live playback budget; original audio remains available"
                        .into(),
                );
            }
            if Instant::now() >= deadline {
                state.store(CANCELLED, Ordering::Release);
                return Err("Stem window timed out; original audio remains available".into());
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
                    } else if live_budget_expired(live_budget.as_ref(), &state)
                        || Instant::now() >= deadline
                    {
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

impl Drop for Broker {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        self.queue.ready.notify_all();
    }
}
fn pool_plan(config: &Config) -> (usize, u32) {
    let width = if config.native && config.provider == "cpu" {
        (thread::available_parallelism().map_or(1, usize::from) / 2).clamp(1, 3)
    } else {
        1
    };
    (width, config.memory_mib.min(6144 / width as u32))
}
/// Logical slots, per-process hard cap, unreaped processes, peak unreaped processes.
/// A quarantined child remains counted until confirmed reaped; this is not RSS.
pub(crate) fn pool_status() -> Option<(usize, u32, usize, usize)> {
    let broker = BROKER.get()?.as_ref().ok()?;
    Some((
        broker.width,
        broker.memory_mib.unwrap_or(0),
        broker.metrics.live.load(Ordering::Acquire),
        broker.metrics.peak.load(Ordering::Acquire),
    ))
}
/// Cheap provider gate for the UI; runtime hashing stays in receipt background work.
pub(crate) fn selected_cuda() -> bool {
    if std::env::var("AMP_ONNX_PROVIDER").is_ok_and(|p| p != "cuda") {
        return false;
    }
    if let Some(Ok(broker)) = BROKER.get() {
        return broker.provider.as_deref() == Some("cuda");
    }
    true // Before pool launch, successful receipt verification supplies CUDA evidence.
}
pub(crate) fn qualification_files() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let config = Config::environment().ok()?;
    if !config.native || config.provider != "cuda" {
        return None;
    }
    let directory = config.runtime.as_ref()?.parent()?;
    if !verified_cuda_module(directory)
        || config.cuda_dll_dir.as_ref()?.canonicalize().ok()?
            != directory.join("vendor").canonicalize().ok()?
        || config.runtime.as_ref()?.canonicalize().ok()?
            != directory.join("onnxruntime.dll").canonicalize().ok()?
    {
        return None;
    }
    Some((config.python, config.model, directory.join("module.json")))
}
/// Configuration check only: no Python launch, model probing or ordinary-playback warm-up.
pub(crate) fn configured() -> bool {
    Config::environment().is_ok()
}

/// Blocking BACKGROUND API. Never call from the render/audio/UI thread.
/// Interleaved stereo44.1k f32; vocals, instruments=(bass+other), drums. No disk cache.
/// At most three native CPU inferences plus six queued windows; overload rejects immediately.
/// Budget includes queue time; expiry never cancels the caller's deck/source token.
pub(crate) fn separate(pcm: Vec<f32>, cancel: Arc<AtomicBool>, budget: Duration) -> Reply {
    separate_traced(pcm, cancel, budget, None, None)
}
pub(crate) fn separate_traced(
    pcm: Vec<f32>,
    cancel: Arc<AtomicBool>,
    budget: Duration,
    source: Option<(usize, u64, u64)>,
    live_budget: Option<LiveBudget>,
) -> Reply {
    validate_input(&pcm)?;
    let config = Config::environment()?;
    BROKER
        .get_or_init(|| Broker::start_pool(&config))
        .as_ref()
        .map_err(Clone::clone)?
        .submit_traced(pcm, cancel, config, budget, source, live_budget)
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
    metrics: Option<Arc<PoolMetrics>>,
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
        if let Some(runtime) = &config.runtime {
            command.env("ORT_DYLIB_PATH", runtime);
        }
        if let Some(path) = &config.cuda_dll_dir {
            command.arg("--cuda-dll-dir").arg(path);
            if config.provider == "cuda" {
                let mut paths = vec![path.clone()];
                if let Some(existing) = std::env::var_os("PATH") {
                    paths.extend(std::env::split_paths(&existing));
                }
                let path_env = std::env::join_paths(paths)
                    .map_err(|e| format!("Invalid CUDA library search path: {e}"))?;
                command.env("PATH", path_env);
            }
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
            return Self::startup_failure(
                child,
                config,
                Arc::new(Mutex::new(Vec::new())),
                "ONNX worker pipes unavailable".into(),
            );
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
            return Self::startup_failure(
                child,
                config,
                stderr,
                format!("Cannot supervise ONNX diagnostics: {error}"),
            );
        }
        let (tx, rx) = mpsc::sync_channel::<WireRequest>(1);
        let provider = config.expected_provider().to_owned();
        let memory_limit = config.memory_mib;
        if let Err(error) = thread::Builder::new()
            .name("amp-onnx-pipes".into())
            .spawn(move || {
                let mut output = BufReader::new(output);
                while let Ok(work) = rx.recv() {
                    let result = exchange_with_limit(
                        &mut input,
                        &mut output,
                        &work.id,
                        &work.pcm,
                        &provider,
                        memory_limit,
                    );
                    let failed = result.is_err();
                    let _ = work.reply.send(result);
                    if failed {
                        break;
                    }
                }
            })
        {
            return Self::startup_failure(
                child,
                config,
                stderr,
                format!("Cannot supervise ONNX pipes: {error}"),
            );
        }
        Ok(Self {
            child,
            tx,
            config,
            poisoned: false,
            warmed: false,
            stderr,
            metrics: None,
        })
    }
    /// A partially launched child still owns this slot until exit is acknowledged.
    fn startup_failure(
        child: Box<dyn process_wrap::std::ChildWrapper>,
        config: Config,
        stderr: Arc<Mutex<Vec<u8>>>,
        message: String,
    ) -> Result<Self, String> {
        let (tx, _rx) = mpsc::sync_channel(1);
        if let Ok(mut errors) = stderr.lock() {
            errors.extend_from_slice(message.as_bytes());
            let excess = errors.len().saturating_sub(4096);
            errors.drain(..excess);
        }
        let mut failed = Self {
            child,
            tx,
            config,
            poisoned: true,
            warmed: false,
            stderr,
            metrics: None,
        };
        if failed.terminate() {
            Err(message)
        } else {
            Ok(failed)
        }
    }
    /// Bounded cancellation acknowledgement; retain/quarantine an un-reaped process.
    fn terminate(&mut self) -> bool {
        self.poisoned = true;
        let _ = self.child.start_kill();
        let deadline = Instant::now() + REAP_TIMEOUT;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    if let Some(metrics) = self.metrics.take() {
                        metrics.live.fetch_sub(1, Ordering::AcqRel);
                    }
                    return true;
                }
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
fn broker_loop(
    queue: Arc<RequestQueue>,
    stopping: Arc<AtomicBool>,
    metrics: Arc<PoolMetrics>,
) -> bool {
    let mut worker: Option<Worker> = None;
    let mut last_use = Instant::now();
    loop {
        if stopping.load(Ordering::Acquire) {
            let reaped = stop_worker(&mut worker);
            queue.drain();
            return reaped;
        }
        let Some(work) = queue.take(&stopping) else {
            if last_use.elapsed() >= IDLE_TIMEOUT {
                let _ = stop_worker(&mut worker);
            }
            continue;
        };
        if work
            .state
            .compare_exchange(PENDING, RUNNING, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
            || work.cancel.load(Ordering::Acquire)
            || Instant::now() >= work.deadline
            || live_budget_expired(work.live_budget.as_ref(), &work.state)
        {
            let _ = work
                .reply
                .send(Err("Stem window cancelled before inference".into()));
            queue.finish(&work);
            continue;
        }
        let started = Instant::now();
        if trace_enabled() {
            trace(json!({"event":"dequeue", "id":work.id,
                "queue_ms":started.duration_since(work.submitted).as_secs_f64()*1000.,
                "request_margin_ms":margin_ms(work.deadline, started)}));
        }
        let result = process_window_tracked(&mut worker, &work, &stopping, Some(&metrics));
        if trace_enabled() {
            trace(
                json!({"event":"complete", "id":work.id, "ok":result.is_ok(),
                "active_ms":started.elapsed().as_secs_f64()*1000.,
                "request_margin_ms":margin_ms(work.deadline, Instant::now())}),
            );
        }
        work.state.store(DONE, Ordering::Release);
        queue.finish(&work);
        let _ = work.reply.send(result);
        last_use = Instant::now();
    }
}
#[cfg(test)]
fn process_window(worker: &mut Option<Worker>, work: &Request, stopping: &AtomicBool) -> Reply {
    process_window_tracked(worker, work, stopping, None)
}
fn process_window_tracked(
    worker: &mut Option<Worker>,
    work: &Request,
    stopping: &AtomicBool,
    metrics: Option<&Arc<PoolMetrics>>,
) -> Reply {
    if work.cancel.load(Ordering::Acquire) || stopping.load(Ordering::Acquire) {
        return Err("Stem window cancelled before inference".into());
    }
    if Instant::now() >= work.deadline
        || live_budget_expired(work.live_budget.as_ref(), &work.state)
    {
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
        let mut spawned = Worker::spawn(work.config.clone())?;
        if let Some(metrics) = metrics {
            let live = metrics.live.fetch_add(1, Ordering::AcqRel) + 1;
            metrics.peak.fetch_max(live, Ordering::AcqRel);
            spawned.metrics = Some(metrics.clone());
        }
        *worker = Some(spawned);
    }
    if worker.as_ref().is_some_and(|worker| worker.poisoned) {
        let detail = worker.as_ref().map(Worker::diagnostic).unwrap_or_default();
        let reaped = stop_worker(worker);
        return Err(format!(
            "ONNX startup failed; {}. {detail}",
            if reaped {
                "process stopped"
            } else {
                "slot quarantined until process exit"
            }
        ));
    }
    let active = worker.as_mut().ok_or("ONNX worker unavailable")?;
    let exchange_started = Instant::now();
    let exchange_deadline = exchange_started + REQUEST_TIMEOUT;
    let deadline = work.deadline.min(exchange_deadline);
    if trace_enabled() {
        trace(
            json!({"event":"exchange_start", "id":work.id, "warm":active.warmed,
            "request_margin_ms":margin_ms(work.deadline, exchange_started),
            "exchange_margin_ms":margin_ms(exchange_deadline, exchange_started)}),
        );
    }
    let id = work.id.clone();
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
            || live_budget_expired(work.live_budget.as_ref(), &work.state)
        {
            let observed = Instant::now();
            let cancelled = work.cancel.load(Ordering::Acquire);
            let shutdown = stopping.load(Ordering::Acquire);
            let reaped = stop_worker(worker);
            if trace_enabled() {
                trace(json!({"event":"exchange_stop", "id":work.id,
                    "cancelled":cancelled, "shutdown":shutdown, "reaped":reaped,
                    "request_cancelled_or_expired":work.state.load(Ordering::Acquire) == CANCELLED,
                    "live_budget_guard_present":work.live_budget.is_some(),
                    "exchange_ms":observed.duration_since(exchange_started).as_secs_f64()*1000.,
                    "request_margin_ms":margin_ms(work.deadline, observed),
                    "exchange_margin_ms":margin_ms(exchange_deadline, observed),
                    "binding_deadline":if work.deadline <= exchange_deadline {"request"} else {"exchange"}}));
            }
            return Err(if reaped {
                "Stem window cancelled or timed out; ONNX process stopped"
            } else {
                "ONNX cancellation not acknowledged; worker quarantined"
            }
            .into());
        }
        match rx.recv_timeout(TICK) {
            Ok(Ok(parts)) => {
                if work.cancel.load(Ordering::Acquire)
                    || stopping.load(Ordering::Acquire)
                    || Instant::now() >= deadline
                    || live_budget_expired(work.live_budget.as_ref(), &work.state)
                {
                    let _ = stop_worker(worker);
                    return Err("Stem result expired before publication".into());
                }
                if trace_enabled() {
                    let now = Instant::now();
                    trace(json!({"event":"exchange_complete", "id":work.id,
                        "exchange_ms":now.duration_since(exchange_started).as_secs_f64()*1000.,
                        "request_margin_ms":margin_ms(work.deadline, now),
                        "exchange_margin_ms":margin_ms(exchange_deadline, now)}));
                }
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
#[cfg(test)]
fn exchange(
    input: &mut impl Write,
    output: &mut impl Read,
    id: &str,
    pcm: &[f32],
    provider: &str,
) -> Reply {
    exchange_with_limit(input, output, id, pcm, provider, 6144)
}
fn exchange_with_limit(
    input: &mut impl Write,
    output: &mut impl Read,
    id: &str,
    pcm: &[f32],
    provider: &str,
    memory_limit: u32,
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
            .is_none_or(|v| v == 0 || v > u64::from(memory_limit))
        || h["persistent_audio_bytes"] != 0
    {
        return Err("Response identity, shape, provider or memory contract mismatch".into());
    }
    if trace_enabled() {
        trace(json!({"event":"worker_header", "id":id,
            "inference_seconds":h["inference_seconds"].as_f64(),
            "load_seconds":h["load_seconds"].as_f64(),
            "peak_worker_rss_bytes":h["peak_worker_rss_bytes"].as_u64()}));
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
 if mode.endswith('parallel') and count==1:
  os.makedirs(mode,exist_ok=True)
  with open(os.path.join(mode,str(os.getpid())),'w') as marker: marker.write('ready')
  deadline=time.monotonic()+10
  while len(os.listdir(mode))<3:
   if time.monotonic()>deadline: raise RuntimeError('parallel fake workers did not overlap')
   time.sleep(0.01)
 if mode.endswith('hang'):
  child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)'])
  with open(mode,'w') as marker: marker.write(str(child.pid))
  time.sleep(60)
 if mode.endswith('slowwarm') and count==2:
  time.sleep(9)
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
            runtime: None,
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
    fn three_slots_run_concurrently_and_one_cancel_does_not_kill_healthy_slots() {
        let broker = Arc::new(Broker::start_with(3, 6, None).unwrap());
        let barrier = Arc::new(std::sync::Barrier::new(4));
        // Hold each fake process at its first request until all three are alive.
        // Fast replies alone do not establish overlapping process occupancy.
        let parallel_directory = tempfile::tempdir().unwrap();
        let parallel = parallel_directory.path().join("parallel");
        let mut handles = Vec::new();
        for _ in 0..3 {
            let b = broker.clone();
            let gate = barrier.clone();
            let parallel = parallel.clone();
            handles.push(thread::spawn(move || {
                gate.wait();
                b.submit(
                    vec![0.; 4],
                    token(),
                    config(parallel),
                    Duration::from_secs(15),
                )
                .unwrap()
            }));
        }
        barrier.wait();
        for h in handles {
            assert_eq!(
                h.join().unwrap()[0],
                vec![1.; 4],
                "each concurrent request needs a distinct persistent process"
            );
        }
        assert_eq!(broker.metrics.peak.load(Ordering::Acquire), 3);
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("hang");
        let b = broker.clone();
        let cancel = token();
        let flag = cancel.clone();
        let hanging = thread::spawn(move || {
            b.submit(
                vec![0.; 4],
                flag,
                config(marker.clone()),
                Duration::from_secs(5),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        while !directory.path().join("hang").exists() {
            assert!(Instant::now() < deadline);
            thread::sleep(TICK);
        }
        assert!(
            broker
                .submit(
                    vec![0.; 4],
                    token(),
                    config("slow".into()),
                    Duration::from_secs(2)
                )
                .is_ok()
        );
        cancel.store(true, Ordering::Release);
        assert!(hanging.join().unwrap().is_err());
        assert!(
            broker
                .submit(
                    vec![0.; 4],
                    token(),
                    config("slow".into()),
                    Duration::from_secs(2)
                )
                .is_ok()
        );
        let begin = Instant::now();
        assert!(broker.shutdown());
        assert!(begin.elapsed() < Duration::from_secs(3));
        assert_eq!(broker.metrics.live.load(Ordering::Acquire), 0);
        assert!(broker.metrics.peak.load(Ordering::Acquire) <= 3);
    }
    #[derive(Debug)]
    struct QuarantinedChild {
        exited: Arc<AtomicBool>,
    }
    impl process_wrap::std::ChildWrapper for QuarantinedChild {
        fn inner(&self) -> &dyn process_wrap::std::ChildWrapper {
            self
        }
        fn inner_mut(&mut self) -> &mut dyn process_wrap::std::ChildWrapper {
            self
        }
        fn into_inner(self: Box<Self>) -> Box<dyn process_wrap::std::ChildWrapper> {
            self
        }
        fn start_kill(&mut self) -> std::io::Result<()> {
            Ok(())
        }
        fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
            if !self.exited.load(Ordering::Acquire) {
                return Ok(None);
            }
            #[cfg(unix)]
            use std::os::unix::process::ExitStatusExt;
            #[cfg(windows)]
            use std::os::windows::process::ExitStatusExt;
            Ok(Some(std::process::ExitStatus::from_raw(0)))
        }
    }
    #[test]
    fn quarantined_slot_keeps_its_reservation_and_cannot_respawn_before_reap() {
        let exited = token();
        let metrics = Arc::new(PoolMetrics::default());
        metrics.live.store(1, Ordering::Release);
        metrics.peak.store(1, Ordering::Release);
        let (tx, _rx) = mpsc::sync_channel(1);
        let mut worker = Some(Worker {
            child: Box::new(QuarantinedChild {
                exited: exited.clone(),
            }),
            tx,
            config: config("good".into()),
            poisoned: true,
            warmed: true,
            stderr: Arc::new(Mutex::new(Vec::new())),
            metrics: Some(metrics.clone()),
        });
        let error = process_window_tracked(
            &mut worker,
            &request(config("good".into()), token()),
            &AtomicBool::new(false),
            Some(&metrics),
        )
        .unwrap_err();
        assert!(error.contains("has not stopped"));
        assert!(worker.is_some());
        assert_eq!(metrics.live.load(Ordering::Acquire), 1);
        assert_eq!(metrics.peak.load(Ordering::Acquire), 1);
        exited.store(true, Ordering::Release);
        assert!(stop_worker(&mut worker));
        assert!(worker.is_none());
        assert_eq!(metrics.live.load(Ordering::Acquire), 0);
    }
    #[test]
    fn response_cannot_claim_a_memory_cap_larger_than_its_pool_allocation() {
        let header = header(1);
        let mut bytes = wire(&header, &[0.; 8]);
        let error = exchange_with_limit(
            &mut Vec::new(),
            &mut bytes,
            "test",
            &[0.; 2],
            "CPUExecutionProvider",
            2048,
        )
        .unwrap_err();
        assert!(error.contains("memory contract"));
    }
    #[test]
    fn stale_pending_windows_are_reclaimed_before_capacity_and_duplicate_checks() {
        let queue = RequestQueue::new(6);
        let cfg = config("good".into());
        let mut states = Vec::new();
        let mut retry_source = None;
        for _ in 0..6 {
            let work = request(cfg.clone(), token());
            states.push(work.state.clone());
            queue.push(work).unwrap();
        }
        {
            let mut pending = queue.pending.lock().unwrap();
            for (i, work) in pending.requests.iter_mut().enumerate() {
                if i < 3 {
                    work.cancel.store(true, Ordering::Release);
                } else {
                    work.deadline = Instant::now() - TICK;
                    retry_source = Some(work.cancel.clone());
                }
            }
        }
        queue
            .push(request(cfg.clone(), retry_source.clone().unwrap()))
            .unwrap();
        assert!(
            states
                .iter()
                .all(|state| state.load(Ordering::Acquire) == CANCELLED)
        );
        assert_eq!(queue.pending.lock().unwrap().requests.len(), 1);
        assert_eq!(queue.pending.lock().unwrap().outstanding.len(), 1);
        let active = queue.take(&AtomicBool::new(false)).unwrap();
        assert!(
            queue
                .push(request(cfg, retry_source.unwrap()))
                .unwrap_err()
                .contains("already outstanding"),
            "running source reservation must not be reclaimed"
        );
        queue.finish(&active);
    }
    #[test]
    fn partial_startup_failure_retains_unreaped_child_in_a_poisoned_slot() {
        let exited = token();
        let failed = Worker::startup_failure(
            Box::new(QuarantinedChild {
                exited: exited.clone(),
            }),
            config("good".into()),
            Arc::new(Mutex::new(Vec::new())),
            "forced pipe supervision failure".into(),
        )
        .unwrap();
        assert!(failed.poisoned);
        assert!(failed.diagnostic().contains("forced pipe"));
        let mut worker = Some(failed);
        let error = process_window(
            &mut worker,
            &request(config("good".into()), token()),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(error.contains("has not stopped"));
        assert!(
            worker.is_some(),
            "failed child must retain its slot rather than allow a replacement"
        );
        exited.store(true, Ordering::Release);
        assert!(stop_worker(&mut worker));
        assert!(worker.is_none());
        assert!(
            matches!(Worker::startup_failure(Box::new(QuarantinedChild{exited}),config("good".into()),Arc::new(Mutex::new(Vec::new())),"already stopped".into()),Err(error) if error.contains("already stopped"))
        );
    }
    #[test]
    fn pending_edf_is_stable_and_duplicate_sources_are_rejected() {
        let queue = RequestQueue::new(6);
        let cfg = config("good".into());
        let a = token();
        let mut later = request(cfg.clone(), a.clone());
        later.deadline = Instant::now() + Duration::from_secs(5);
        let mut earlier = request(cfg.clone(), token());
        earlier.deadline = Instant::now() + Duration::from_secs(1);
        let early = earlier.cancel.clone();
        queue.push(later).unwrap();
        queue.push(earlier).unwrap();
        assert!(
            queue
                .push(request(cfg, a))
                .unwrap_err()
                .contains("already outstanding")
        );
        let picked = queue.take(&AtomicBool::new(false)).unwrap();
        assert!(Arc::ptr_eq(&picked.cancel, &early));
        queue.finish(&picked);
    }
    #[test]
    fn queued_paused_request_becomes_urgent_when_playback_starts() {
        let queue = RequestQueue::new(6);
        let cfg = config("good".into());
        let remaining = Arc::new(AtomicU64::new(20));
        let read = remaining.clone();
        let mut urgent = request(cfg.clone(), token());
        urgent.deadline = Instant::now() + Duration::from_secs(20);
        urgent.live_budget = Some(Arc::new(move || {
            Duration::from_secs(read.load(Ordering::Acquire))
        }));
        let urgent_id = urgent.id.clone();
        let mut prefetch = request(cfg, token());
        prefetch.deadline = Instant::now() + Duration::from_secs(5);
        queue.push(urgent).unwrap();
        queue.push(prefetch).unwrap();
        // Both entered while paused. Play makes the first window urgent after enqueue.
        remaining.store(1, Ordering::Release);
        let picked = queue.take(&AtomicBool::new(false)).unwrap();
        assert_eq!(
            picked.id, urgent_id,
            "live one-second budget must outrank static five-second prefetch"
        );
        queue.finish(&picked);
    }
    #[test]
    fn effective_deadline_ties_keep_fifo_and_never_extend_fixed_deadline() {
        let queue = RequestQueue::new(6);
        let cfg = config("good".into());
        let mut first = request(cfg.clone(), token());
        let mut second = request(cfg.clone(), token());
        let deadline = Instant::now() + Duration::from_secs(3);
        first.deadline = deadline;
        second.deadline = deadline;
        first.live_budget = Some(Arc::new(|| Duration::from_secs(20)));
        second.live_budget = Some(Arc::new(|| Duration::from_secs(20)));
        let first_id = first.id.clone();
        queue.push(first).unwrap();
        queue.push(second).unwrap();
        let picked = queue.take(&AtomicBool::new(false)).unwrap();
        assert_eq!(picked.id, first_id);
        assert_eq!(picked.deadline, deadline);
        queue.finish(&picked);
        let next = queue.take(&AtomicBool::new(false)).unwrap();
        assert_eq!(next.deadline, deadline);
        queue.finish(&next);
    }
    #[test]
    fn cuda_autodiscovery_requires_qualified_pinned_fixed_layout() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path();
        std::fs::create_dir(dir.join("vendor")).unwrap();
        for file in CUDA_VENDOR_DLLS {
            std::fs::write(dir.join("vendor").join(file), []).unwrap();
        }
        for file in [
            "onnxruntime.dll",
            "onnxruntime_providers_shared.dll",
            "onnxruntime_providers_cuda.dll",
        ] {
            std::fs::write(dir.join(file), []).unwrap();
        }
        let mut manifest = json!({"id":"stems-cuda","version":1,"provider":"cuda","onnxruntime_version":"1.22.0","qualified":false,"model_sha256":"98228931494151762a1c4ab1ec7899a894b1f81fd4a509921a9d2f9bacc50845"});
        let save = |value: &Value| {
            std::fs::write(dir.join("module.json"), serde_json::to_vec(value).unwrap()).unwrap()
        };
        save(&manifest);
        assert!(!verified_cuda_module(dir));
        manifest["qualified"] = json!(true);
        save(&manifest);
        assert!(verified_cuda_module(dir));
        manifest["model_sha256"] = json!("unverified");
        save(&manifest);
        assert!(!verified_cuda_module(dir));
        manifest["model_sha256"] =
            json!("98228931494151762a1c4ab1ec7899a894b1f81fd4a509921a9d2f9bacc50845");
        save(&manifest);
        std::fs::remove_file(dir.join("onnxruntime_providers_cuda.dll")).unwrap();
        assert!(!verified_cuda_module(dir));
    }
    #[test]
    fn native_pool_reservations_never_exceed_six_gib_and_legacy_stays_single() {
        let mut cfg = config("good".into());
        assert_eq!(pool_plan(&cfg), (1, 6144));
        cfg.native = true;
        cfg.memory_mib = 3072;
        let (width, memory) = pool_plan(&cfg);
        assert!((1..=3).contains(&width));
        assert!(memory <= 3072);
        assert!(width as u32 * memory <= 6144);
        cfg.provider = "cuda".into();
        cfg.memory_mib = 6144;
        assert_eq!(pool_plan(&cfg), (1, 6144));
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
    fn warm_worker_finishes_within_remaining_audio_budget_instead_of_fixed_cutoff() {
        let broker = Broker::start().unwrap();
        let config = config("slowwarm".into());
        broker
            .submit(
                vec![0.25; 4],
                token(),
                config.clone(),
                Duration::from_secs(2),
            )
            .unwrap();
        let source_cancel = token();
        let result = broker.submit(
            vec![0.5; 4],
            source_cancel.clone(),
            config,
            Duration::from_secs(15),
        );
        assert!(
            result.is_ok(),
            "warm request still had six seconds of its caller budget: {result:?}"
        );
        assert!(!source_cancel.load(Ordering::Acquire));
        assert_eq!(result.unwrap()[0], vec![2.; 4]);
        assert!(broker.shutdown());
    }
    #[test]
    fn live_budget_expiry_is_latched_even_if_playback_pauses_again() {
        let remaining = Arc::new(AtomicU64::new(20));
        let read = remaining.clone();
        let guard: LiveBudget = Arc::new(move || Duration::from_secs(read.load(Ordering::Acquire)));
        let state = AtomicU8::new(RUNNING);
        assert!(!live_budget_expired(Some(&guard), &state));
        remaining.store(0, Ordering::Release);
        assert!(live_budget_expired(Some(&guard), &state));
        remaining.store(20, Ordering::Release);
        assert!(live_budget_expired(Some(&guard), &state));
    }
    #[test]
    fn running_live_expiry_returns_original_before_cleanup_releases_slot() {
        // A broker-owned RUNNING request with deliberately withheld cleanup/reply.
        // This makes delayed cleanup deterministic, without platform kill timing.
        let broker = Arc::new(unstarted(1));
        let background = broker.clone();
        let remaining = Arc::new(AtomicU64::new(20));
        let read = remaining.clone();
        let guard: LiveBudget = Arc::new(move || Duration::from_secs(read.load(Ordering::Acquire)));
        let cancel = token();
        let source_cancel = cancel.clone();
        let handle = thread::spawn(move || {
            background.submit_traced(
                vec![0.; 4],
                cancel,
                config("good".into()),
                Duration::from_secs(15),
                None,
                Some(guard),
            )
        });
        let state = wait_pending(&broker);
        let work = broker.queue.pending.lock().unwrap().requests.remove(0);
        state.store(RUNNING, Ordering::Release);
        let before = Instant::now();
        remaining.store(0, Ordering::Release);
        assert!(
            handle
                .join()
                .unwrap()
                .unwrap_err()
                .contains("live playback budget")
        );
        assert!(before.elapsed() < Duration::from_millis(250));
        assert!(!source_cancel.load(Ordering::Acquire));
        assert_eq!(state.load(Ordering::Acquire), CANCELLED);
        assert_eq!(
            broker.queue.pending.lock().unwrap().outstanding.len(),
            1,
            "caller fallback must not release the broker-owned cleanup slot"
        );
        broker.queue.finish(&work);
        assert!(broker.queue.pending.lock().unwrap().outstanding.is_empty());
    }
    #[test]
    fn live_expiry_rejects_already_arriving_successful_reply() {
        let broker = Arc::new(unstarted(1));
        let background = broker.clone();
        let remaining = Arc::new(AtomicU64::new(20));
        let read = remaining.clone();
        let guard: LiveBudget = Arc::new(move || Duration::from_secs(read.load(Ordering::Acquire)));
        let source_cancel = token();
        let cancel = source_cancel.clone();
        let handle = thread::spawn(move || {
            background.submit_traced(
                vec![0.; 4],
                cancel,
                config("good".into()),
                Duration::from_secs(15),
                None,
                Some(guard),
            )
        });
        wait_pending(&broker);
        let work = broker.queue.pending.lock().unwrap().requests.remove(0);
        work.state.store(RUNNING, Ordering::Release);
        remaining.store(0, Ordering::Release);
        let _ = work.reply.send(Ok([vec![1.; 4], vec![2.; 4], vec![3.; 4]]));
        assert!(
            handle.join().unwrap().is_err(),
            "expired PCM must not publish"
        );
        assert!(!source_cancel.load(Ordering::Acquire));
        broker.queue.finish(&work);
    }
    #[test]
    fn live_budget_expiry_stops_real_fake_process_without_cancelling_source() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("hang");
        let broker = Arc::new(Broker::start().unwrap());
        let background = broker.clone();
        let remaining = Arc::new(AtomicU64::new(20));
        let read = remaining.clone();
        let guard: LiveBudget = Arc::new(move || Duration::from_secs(read.load(Ordering::Acquire)));
        let cancel = token();
        let source_cancel = cancel.clone();
        let cfg = config(marker.clone());
        let handle = thread::spawn(move || {
            background.submit_traced(
                vec![0.; 4],
                cancel,
                cfg,
                Duration::from_secs(15),
                None,
                Some(guard),
            )
        });
        let pid = wait_for_child(&marker);
        remaining.store(0, Ordering::Release);
        assert!(handle.join().unwrap().is_err());
        assert!(!source_cancel.load(Ordering::Acquire));
        assert!(broker.shutdown());
        wait_descendant_stopped(pid);
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
    fn unstarted(capacity: usize) -> Broker {
        Broker {
            queue: Arc::new(RequestQueue::new(capacity)),
            stopping: token(),
            finished: Arc::new(AtomicUsize::new(0)),
            width: 1,
            memory_mib: None,
            metrics: Arc::new(PoolMetrics::default()),
            provider: None,
        }
    }
    fn wait_pending(broker: &Broker) -> Arc<AtomicU8> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(request) = broker.queue.pending.lock().unwrap().requests.first() {
                return request.state.clone();
            }
            assert!(Instant::now() < deadline);
            thread::sleep(TICK);
        }
    }
    #[test]
    fn queue_is_bounded_and_cancelled_pending_inputs_never_start() {
        let broker = unstarted(QUEUE_CAPACITY);
        let cfg = config("good".into());
        for _ in 0..QUEUE_CAPACITY {
            assert!(broker.queue.push(request(cfg.clone(), token())).is_ok());
        }
        assert!(
            broker
                .submit(vec![0.; 2], token(), cfg.clone(), Duration::from_secs(5))
                .unwrap_err()
                .contains("queue is full")
        );
        let broker = Arc::new(unstarted(1));
        let copy = broker.clone();
        let cancel = token();
        let flag = cancel.clone();
        let handle =
            thread::spawn(move || copy.submit(vec![0.; 2], flag, cfg, Duration::from_secs(5)));
        let state = wait_pending(&broker);
        assert_eq!(state.load(Ordering::Acquire), PENDING);
        cancel.store(true, Ordering::Release);
        assert!(handle.join().unwrap().is_err());
        assert_eq!(
            state.load(Ordering::Acquire),
            CANCELLED,
            "queued cancellation must prevent later inference"
        );
    }
    fn request(config: Config, cancel: Arc<AtomicBool>) -> Request {
        let (reply, _) = mpsc::sync_channel(1);
        let deadline = Instant::now() + Duration::from_secs(5);
        Request {
            id: format!("window-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)),
            submitted: Instant::now(),
            pcm: Arc::new(vec![0.; 4]),
            cancel,
            state: Arc::new(AtomicU8::new(RUNNING)),
            config,
            deadline,
            live_budget: None,
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
        let broker = Arc::new(unstarted(1));
        let copy = broker.clone();
        let cancel = token();
        let flag = cancel.clone();
        let cfg = config("good".into());
        let handle =
            thread::spawn(move || copy.submit(vec![0.; 2], flag, cfg, Duration::from_millis(80)));
        let state = wait_pending(&broker);
        assert!(handle.join().unwrap().unwrap_err().contains("timed out"));
        assert!(
            !cancel.load(Ordering::Acquire),
            "queue expiry must preserve original-audio fallback"
        );
        assert_eq!(state.load(Ordering::Acquire), CANCELLED);
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
