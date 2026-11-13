//! Headless background preparation; shares claims/analysis with AMP Library.
use serde::Serialize;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Default, Serialize)]
pub struct Progress {
    pub enabled: bool,
    pub scanning: bool,
    pub paused_for_radio: bool,
    pub total: usize,
    pub completed: usize,
    pub reused_or_claimed: usize,
    pub failed: usize,
    pub workers: usize,
    pub error: Option<String>,
}
pub struct Worker {
    pub enabled: AtomicBool,
    pub pause_for_radio: AtomicBool,
    started: AtomicBool,
    stopped: AtomicBool,
    progress: Mutex<Progress>,
}
impl Worker {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            enabled: AtomicBool::new(false),
            pause_for_radio: AtomicBool::new(true),
            started: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            progress: Mutex::new(Progress::default()),
        })
    }
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
    pub fn snapshot(&self) -> Progress {
        let mut p = self.progress.lock().unwrap().clone();
        p.enabled = self.enabled.load(Ordering::Relaxed);
        p
    }
    pub fn start(
        self: &Arc<Self>,
        cast: &Arc<crate::Cast>,
        workers: usize,
        interval: u64,
        pause_for_radio: bool,
    ) {
        if self.started.swap(true, Ordering::Relaxed) {
            return;
        }
        self.pause_for_radio
            .store(pause_for_radio, Ordering::Relaxed);
        let state = self.clone();
        let cast = Arc::downgrade(cast);
        std::thread::spawn(move || {
            while !state.enabled.load(Ordering::Relaxed) {
                if state.stopped.load(Ordering::Relaxed) || cast.strong_count() == 0 {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            let Some(host) = cast.upgrade() else { return };
            let queue = match amp_core::analysis_queue::Queue::for_library(&host.root, "cast") {
                Ok(q) => q,
                Err(e) => {
                    state.progress.lock().unwrap().error = Some(e);
                    return;
                }
            };
            if let Err(e) = queue.seed_known(&host.tracks) {
                state.progress.lock().unwrap().error = Some(e);
            }
            drop(host);
            let scanner_queue = queue.clone();
            let scanner_state = state.clone();
            let scanner_cast = cast.clone();
            let scanner = std::thread::spawn(move || {
                loop {
                    if scanner_state.stopped.load(Ordering::Relaxed) {
                        break;
                    }
                    let Some(host) = scanner_cast.upgrade() else {
                        break;
                    };
                    if scanner_state.enabled.load(Ordering::Relaxed) {
                        scanner_state.progress.lock().unwrap().scanning = true;
                        let outcome = amp_core::scan_job_inputs(&host.root)
                            .map_err(|e| e.to_string())
                            .and_then(|library| scanner_queue.enqueue_tracks(&library.tracks));
                        let mut p = scanner_state.progress.lock().unwrap();
                        p.scanning = false;
                        if let Err(e) = outcome {
                            p.error = Some(e);
                        }
                    }
                    drop(host);
                    for _ in 0..interval.clamp(10, 86400) {
                        if scanner_state.stopped.load(Ordering::Relaxed) {
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                }
            });
            loop {
                if state.stopped.load(Ordering::Relaxed) {
                    break;
                }
                let Some(host) = cast.upgrade() else { break };
                let busy = state.pause_for_radio.load(Ordering::Relaxed)
                    && host.station.blocking_lock().running;
                state.progress.lock().unwrap().paused_for_radio = busy;
                if state.enabled.load(Ordering::Relaxed) && !busy {
                    let count = amp_core::batch::workers(workers, 32).max(1);
                    let slots = vec![(); count];
                    amp_core::batch::for_each(&slots, count, |_| {
                        loop {
                            if state.stopped.load(Ordering::Relaxed)
                                || !state.enabled.load(Ordering::Relaxed)
                                || (state.pause_for_radio.load(Ordering::Relaxed)
                                    && host.station.blocking_lock().running)
                            {
                                break;
                            }
                            let job = match queue.next() {
                                Ok(Some(j)) => j,
                                Ok(None) => break,
                                Err(e) => {
                                    state.progress.lock().unwrap().error = Some(e);
                                    break;
                                }
                            };
                            let _heartbeat = queue.keep_alive(&job);
                            let outcome =
                                amp_core::library_engine::analyze_job(&host.root, &job.path, false);
                            let result = match &outcome {
                                Ok(amp_core::coordination::AnalysisResult::Skipped(
                                    amp_core::coordination::Skip::Busy,
                                )) => queue.defer(&job),
                                Ok(_) => queue.finish(&job, Ok(())),
                                Err(e) => queue.finish(&job, Err(e)),
                            };
                            let mut p = state.progress.lock().unwrap();
                            p.workers = count;
                            match outcome {
                                Ok(amp_core::coordination::AnalysisResult::Prepared(_)) => {
                                    p.completed += 1
                                }
                                Ok(_) => p.reused_or_claimed += 1,
                                Err(e) => {
                                    p.failed += 1;
                                    p.error = Some(e);
                                }
                            }
                            if let Err(e) = result {
                                p.error = Some(e);
                            }
                            if let Ok(stats) = queue.stats() {
                                p.total = stats.total;
                            }
                        }
                        true
                    });
                }
                drop(host);
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            state.stopped.store(true, Ordering::Relaxed);
            let _ = scanner.join();
        });
    }
}
