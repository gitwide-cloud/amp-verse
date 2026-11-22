//! Experimental on-demand stem scheduling. No filesystem, network or model downloads.
//! Not wired into playback. Budget covers owned PCM, not model/driver workspace.
pub mod map;
use std::sync::atomic::{AtomicBool, Ordering};

pub const DECKS: usize = 6;
pub const DEFAULT_PCM_BUDGET: usize = 64 * 1024 * 1024;
pub const MAX_SECONDS: u64 = 11;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    InvalidWindow,
    InvalidDeck,
    InvalidAudio,
    Busy,
    BudgetExceeded,
    TokenExhausted,
    Worker(String),
}

#[derive(Debug, Clone, Copy)]
pub struct Window {
    pub start_frame: u64,
    pub frames: usize,
    pub sample_rate: u32,
}
impl Window {
    fn samples(self) -> Result<usize, Error> {
        if ![44100, 48000].contains(&self.sample_rate)
            || self.frames == 0
            || self.frames as u64 > u64::from(self.sample_rate) * MAX_SECONDS
            || self.start_frame.checked_add(self.frames as u64).is_none()
        {
            return Err(Error::InvalidWindow);
        }
        self.frames.checked_mul(2).ok_or(Error::InvalidWindow)
    }
    fn reserved(self) -> Result<usize, Error> {
        self.samples()?
            .checked_mul(5 * size_of::<f32>())
            .ok_or(Error::InvalidWindow)
    }
}
/// Four interleaved stereo parts: vocals, drums, bass, other. Never a canned effect.
#[derive(Debug)]
pub struct Block {
    pub parts: [Vec<f32>; 4],
}
impl Block {
    fn validate(&self, window: Window) -> Result<(), Error> {
        let samples = window.samples()?;
        if self
            .parts
            .iter()
            .any(|p| p.len() != samples || p.iter().any(|v| !v.is_finite()))
        {
            return Err(Error::InvalidAudio);
        }
        Ok(())
    }
    fn bytes(&self) -> usize {
        self.parts
            .iter()
            .map(|p| p.capacity() * size_of::<f32>())
            .sum()
    }
}
/// A real model adapter must honor this bounded-window contract and cancellation.
/// Model activation/VRAM limits require a separately supervised worker.
pub trait Separator {
    fn separate(
        &mut self,
        window: Window,
        input: &[f32],
        cancel: &AtomicBool,
    ) -> Result<Block, Error>;
}
#[derive(Debug)]
pub struct Ticket {
    id: u64,
    deck: usize,
    window: Window,
    cancel: std::sync::Arc<AtomicBool>,
}
impl Ticket {
    pub fn window(&self) -> Window {
        self.window
    }
    pub fn canceled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
    pub fn cancellation(&self) -> &AtomicBool {
        &self.cancel
    }
    pub fn validate_input(&self, input: &[f32]) -> Result<(), Error> {
        if input.len() != self.window.samples()? || input.iter().any(|v| !v.is_finite()) {
            return Err(Error::InvalidAudio);
        }
        Ok(())
    }
}
#[derive(Default)]
struct Slot {
    block: Option<(Window, Block)>,
}
struct Pending {
    id: u64,
    deck: usize,
    reserved: usize,
    cancel: std::sync::Arc<AtomicBool>,
}
/// One worker in flight. Cancel never frees its reservation before terminal ACK.
pub struct Session {
    slots: [Slot; DECKS],
    pending: Option<Pending>,
    budget: usize,
    next_id: u64,
}
impl Default for Session {
    fn default() -> Self {
        Self::new(DEFAULT_PCM_BUDGET)
    }
}
impl Session {
    pub fn new(budget: usize) -> Self {
        Self {
            slots: std::array::from_fn(|_| Slot::default()),
            pending: None,
            budget,
            next_id: 0,
        }
    }
    pub fn accounted_bytes(&self) -> usize {
        self.slots
            .iter()
            .filter_map(|s| s.block.as_ref())
            .map(|(_, b)| b.bytes())
            .sum::<usize>()
            + self.pending.as_ref().map_or(0, |p| p.reserved)
    }
    /// Reserve before decoding input or allocating inference output.
    pub fn begin(&mut self, deck: usize, window: Window) -> Result<Ticket, Error> {
        if deck >= DECKS {
            return Err(Error::InvalidDeck);
        }
        let reserved = window.reserved()?;
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        if self
            .accounted_bytes()
            .checked_add(reserved)
            .is_none_or(|n| n > self.budget)
        {
            return Err(Error::BudgetExceeded);
        }
        let id = self.next_id.checked_add(1).ok_or(Error::TokenExhausted)?;
        self.next_id = id;
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        self.pending = Some(Pending {
            id,
            deck,
            reserved,
            cancel: cancel.clone(),
        });
        Ok(Ticket {
            id,
            deck,
            window,
            cancel,
        })
    }
    pub fn unload(&mut self, deck: usize) -> Result<(), Error> {
        let slot = self.slots.get_mut(deck).ok_or(Error::InvalidDeck)?;
        slot.block = None;
        if let Some(p) = &self.pending
            && p.deck == deck
        {
            p.cancel.store(true, Ordering::Release);
        }
        Ok(())
    }
    /// Call only after the worker has released input/temporary tensors, even on error.
    /// Returns false for canceled/stale audio; never lets it reach the new deck.
    pub fn finish(&mut self, ticket: Ticket, output: Result<Block, Error>) -> Result<bool, Error> {
        if !self.pending.as_ref().is_some_and(|p| {
            p.id == ticket.id
                && p.deck == ticket.deck
                && std::sync::Arc::ptr_eq(&p.cancel, &ticket.cancel)
        }) {
            return Ok(false);
        }
        self.pending = None;
        if ticket.canceled() {
            return Ok(false);
        }
        let block = output?;
        block.validate(ticket.window)?;
        // Capacity, not just length, counts: reject oversized backend allocations.
        let old = self.slots[ticket.deck]
            .block
            .as_ref()
            .map_or(0, |(_, b)| b.bytes());
        if self
            .accounted_bytes()
            .saturating_sub(old)
            .checked_add(block.bytes())
            .is_none_or(|n| n > self.budget)
        {
            return Err(Error::BudgetExceeded);
        }
        self.slots[ticket.deck].block = Some((ticket.window, block));
        Ok(true)
    }
    pub fn block(&self, deck: usize, frame: u64) -> Option<&Block> {
        let (w, b) = self.slots.get(deck)?.block.as_ref()?;
        (frame >= w.start_frame && frame < w.start_frame + w.frames as u64).then_some(b)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn window() -> Window {
        Window {
            start_frame: 10,
            frames: 8,
            sample_rate: 44100,
        }
    }
    fn block() -> Block {
        Block {
            parts: std::array::from_fn(|_| vec![0.; 16]),
        }
    }
    #[test]
    fn rejects_unbounded_audio_before_worker_can_start() {
        let mut s = Session::default();
        assert!(matches!(
            s.begin(
                0,
                Window {
                    frames: usize::MAX,
                    ..window()
                }
            ),
            Err(Error::InvalidWindow)
        ));
        assert!(matches!(s.begin(6, window()), Err(Error::InvalidDeck)));
        assert_eq!(s.accounted_bytes(), 0);
    }
    #[test]
    fn cancellation_keeps_worker_and_budget_until_ack() {
        let mut s = Session::default();
        let t = s.begin(0, window()).unwrap();
        let bytes = s.accounted_bytes();
        s.unload(0).unwrap();
        assert!(t.canceled());
        assert_eq!(s.accounted_bytes(), bytes);
        assert!(matches!(s.begin(1, window()), Err(Error::Busy)));
        assert!(!s.finish(t, Ok(block())).unwrap());
        assert_eq!(s.accounted_bytes(), 0);
        assert!(s.block(0, 10).is_none());
        assert!(s.begin(1, window()).is_ok());
    }
    #[test]
    fn budget_refuses_more_work_and_releases_on_unload() {
        let mut s = Session::new(window().reserved().unwrap());
        let t = s.begin(0, window()).unwrap();
        assert!(s.finish(t, Ok(block())).unwrap());
        assert!(matches!(s.begin(1, window()), Err(Error::BudgetExceeded)));
        s.unload(0).unwrap();
        assert!(s.begin(1, window()).is_ok());
    }
    #[test]
    fn bad_samples_never_publish_and_errors_release_credit() {
        let mut s = Session::default();
        let t = s.begin(0, window()).unwrap();
        assert_eq!(t.validate_input(&[f32::NAN; 16]), Err(Error::InvalidAudio));
        let mut b = block();
        b.parts[2][0] = f32::INFINITY;
        assert_eq!(s.finish(t, Ok(b)), Err(Error::InvalidAudio));
        assert_eq!(s.accounted_bytes(), 0);
        let t = s.begin(0, window()).unwrap();
        assert!(s.finish(t, Err(Error::Worker("failed".into()))).is_err());
        assert_eq!(s.accounted_bytes(), 0);
    }
    #[test]
    fn block_ranges_and_seek_do_not_return_old_song() {
        let mut s = Session::default();
        let t = s.begin(0, window()).unwrap();
        s.finish(t, Ok(block())).unwrap();
        assert!(s.block(0, 9).is_none());
        assert!(s.block(0, 10).is_some());
        assert!(s.block(0, 17).is_some());
        assert!(s.block(0, 18).is_none());
        s.unload(0).unwrap();
        assert!(s.block(0, 10).is_none());
    }
    #[test]
    fn tickets_from_another_session_cannot_publish_audio() {
        let mut a = Session::default();
        let mut b = Session::default();
        let ta = a.begin(0, window()).unwrap();
        let tb = b.begin(0, window()).unwrap();
        assert!(!b.finish(ta, Ok(block())).unwrap());
        assert!(b.block(0, 10).is_none());
        assert!(matches!(b.begin(0, window()), Err(Error::Busy)));
        assert!(b.finish(tb, Ok(block())).unwrap());
    }
    #[test]
    fn backend_capacity_counts_even_when_length_is_small() {
        let mut s = Session::new(320);
        let t = s.begin(0, window()).unwrap();
        let mut b = block();
        b.parts[0].reserve(1024);
        assert_eq!(s.finish(t, Ok(b)), Err(Error::BudgetExceeded));
        assert_eq!(s.accounted_bytes(), 0);
        assert!(s.block(0, 10).is_none());
    }
    #[test]
    fn long_sessions_do_not_accumulate_song_audio() {
        let mut s = Session::default();
        for _ in 0..10000 {
            let t = s.begin(0, window()).unwrap();
            s.finish(t, Ok(block())).unwrap();
            assert_eq!(s.accounted_bytes(), 256);
            s.unload(0).unwrap();
        }
        assert_eq!(s.accounted_bytes(), 0);
    }
}
