//! UI modules are constructed only by an explicit feature action. Passive status
//! reads use `get`/`get_mut` and must never wake an unused module.
use std::{
    ops::{Deref, DerefMut},
    sync::OnceLock,
};

pub struct Module<T>(OnceLock<Box<T>>);
impl<T> Default for Module<T> {
    fn default() -> Self {
        Self(OnceLock::new())
    }
}
impl<T> Module<T> {
    pub fn get(&self) -> Option<&T> {
        self.0.get().map(Box::as_ref)
    }
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.0.get_mut().map(Box::as_mut)
    }
}
impl<T: Default> Deref for Module<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.0.get_or_init(|| Box::new(T::default()))
    }
}
impl<T: Default> DerefMut for Module<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.0.get_or_init(|| Box::new(T::default()));
        self.0.get_mut().unwrap()
    }
}

// Playback and recovery query these even when Podcasts has never been opened.
impl Module<crate::podcasts::Podcasts> {
    pub fn is_active(&self) -> bool {
        self.get().is_some_and(|p| p.is_active())
    }
    pub fn leave_playback(&mut self) {
        if let Some(p) = self.get_mut() {
            p.leave_playback();
        }
    }
    pub fn next_episode(&mut self) -> bool {
        self.get_mut().is_some_and(|p| p.next_episode())
    }
    pub fn previous_episode(&mut self) -> bool {
        self.get_mut().is_some_and(|p| p.previous_episode())
    }
    pub fn flush_progress(&mut self, path: Option<&std::path::Path>, seconds: f64) {
        if let Some(p) = self.get_mut() {
            p.flush_progress(path, seconds);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Feature {
        value: usize,
    }
    #[test]
    fn status_reads_do_not_construct_modules_and_switching_retains_work() {
        let mut module = Module::<Feature>::default();
        assert!(module.get().is_none());
        assert!(module.get_mut().is_none());
        module.value = 42;
        assert_eq!(module.get().unwrap().value, 42);
        assert_eq!(module.value, 42);
    }
}
