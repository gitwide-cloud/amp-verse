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
            loop {
                let Some(cast) = cast.upgrade() else {
                    break;
                };
                if state.stopped.load(Ordering::Relaxed) {
                    break;
                }
                let busy = pause_for_radio && cast.station.blocking_lock().running;
                state.progress.lock().unwrap().paused_for_radio = busy;
                if state.enabled.load(Ordering::Relaxed) && !busy {
                    let outcome = (|| {
                        let library = amp_core::scan(&cast.root).map_err(|e| e.to_string())?;
                        let count = amp_core::batch::workers(workers, library.tracks.len());
                        *state.progress.lock().unwrap() = Progress {
                            enabled: true,
                            scanning: true,
                            total: library.tracks.len(),
                            workers: count,
                            ..Default::default()
                        };
                        amp_core::batch::for_each(&library.tracks, count, |track| {
                            if state.stopped.load(Ordering::Relaxed)
                                || !state.enabled.load(Ordering::Relaxed)
                                || (pause_for_radio && cast.station.blocking_lock().running)
                            {
                                return false;
                            }
                            let outcome = amp_core::coordination::analyze_shared(
                                &cast.root,
                                &track.path,
                                false,
                            );
                            let mut p = state.progress.lock().unwrap();
                            match outcome {
                                Ok(amp_core::coordination::AnalysisResult::Prepared(_)) => {
                                    p.completed += 1
                                }
                                Ok(amp_core::coordination::AnalysisResult::Skipped(_)) => {
                                    p.reused_or_claimed += 1
                                }
                                Err(e) => {
                                    p.failed += 1;
                                    p.error = Some(e);
                                }
                            }
                            true
                        });
                        Ok::<_, String>(())
                    })();
                    let mut p = state.progress.lock().unwrap();
                    p.scanning = false;
                    if let Err(e) = outcome {
                        p.error = Some(e);
                    }
                }
                drop(cast);
                for _ in 0..interval.clamp(10, 86400) {
                    if state.stopped.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
        });
    }
}
