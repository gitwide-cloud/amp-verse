//! Per-Player output ownership. Offline/test mixers have independent gates.
use crate::mixer::Controls;
use std::{
    cell::RefCell,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
thread_local! {static FRAME_GATE:RefCell<Option<Arc<Gate>>>=const{RefCell::new(None)};}
#[derive(Default)]
pub(crate) struct Gate {
    leased: AtomicBool,
    epoch: AtomicU64,
    renders: AtomicUsize,
    controls: Mutex<Vec<Weak<Controls>>>,
}
pub(crate) struct Scope(Option<Arc<Gate>>);
impl Drop for Scope {
    fn drop(&mut self) {
        FRAME_GATE.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}
pub(crate) fn scope(gate: Arc<Gate>) -> Scope {
    Scope(FRAME_GATE.with(|slot| slot.replace(Some(gate))))
}
pub(crate) fn current() -> Arc<Gate> {
    FRAME_GATE
        .with(|slot| slot.borrow().clone())
        .unwrap_or_default()
}
pub(crate) struct Lease(Arc<Gate>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.pause_known();
        self.0.epoch.fetch_add(1, Ordering::SeqCst);
        self.0.leased.store(false, Ordering::SeqCst);
    }
}
pub(crate) struct Render<'a>(&'a Gate);
impl Drop for Render<'_> {
    fn drop(&mut self) {
        self.0.renders.fetch_sub(1, Ordering::SeqCst);
    }
}
impl Gate {
    pub fn leased(&self) -> bool {
        self.leased.load(Ordering::SeqCst)
    }
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }
    pub fn local_allowed(&self) -> Result<(), String> {
        if self.leased() {
            Err("Stop listening to the station before local playback.".into())
        } else {
            Ok(())
        }
    }
    pub fn register(&self, controls: &Arc<Controls>) {
        let mut all = self.controls.lock().unwrap();
        all.retain(|c| c.strong_count() > 0);
        all.push(Arc::downgrade(controls));
    }
    fn pause_known(&self) {
        for controls in self
            .controls
            .lock()
            .unwrap()
            .iter()
            .filter_map(Weak::upgrade)
        {
            controls.pause_for_receiver();
        }
    }
    pub fn acquire(self: &Arc<Self>, cancel: &AtomicBool) -> Result<Lease, String> {
        if self
            .leased
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("Another station receiver owns this output.".into());
        }
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.pause_known();
        let lease = Lease(self.clone());
        let start = Instant::now();
        while self.renders.load(Ordering::SeqCst) > 0 {
            if cancel.load(Ordering::Acquire) || start.elapsed() > Duration::from_secs(1) {
                return Err("Local audio output did not pause in time.".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(lease)
    }
    pub fn render(&self) -> Option<Render<'_>> {
        self.renders.fetch_add(1, Ordering::SeqCst);
        if self.leased() {
            self.renders.fetch_sub(1, Ordering::SeqCst);
            None
        } else {
            Some(Render(self))
        }
    }
}
