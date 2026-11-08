//! Bounded song-level parallelism shared by desktop and headless preparation.
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub fn cpu_count() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}
/// Zero selects an automatic limit, reserving CPU capacity for playback and
/// limiting simultaneous NAS reads/decoders. Explicit requests may use all cores.
pub fn workers(requested: usize, songs: usize) -> usize {
    worker_limit(requested, songs, cpu_count())
}
fn worker_limit(requested: usize, songs: usize, cores: usize) -> usize {
    let cores = cores.max(1);
    let desired = if requested == 0 {
        cores.saturating_sub(2).clamp(1, 8)
    } else {
        requested
    };
    desired.clamp(1, cores.min(32)).min(songs)
}
/// Each item runs once, with at most `count` active callbacks. Returning false
/// stops new work (already-running callbacks finish). No per-song thread spawning.
pub fn for_each<T: Sync>(items: &[T], count: usize, work: impl Fn(&T) -> bool + Sync) {
    let next = AtomicUsize::new(0);
    let stopped = AtomicBool::new(false);
    std::thread::scope(|scope| {
        for _ in 0..count.min(items.len()) {
            scope.spawn(|| {
                while !stopped.load(Ordering::Relaxed) {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(index) else {
                        break;
                    };
                    if !work(item) {
                        stopped.store(true, Ordering::Relaxed);
                    }
                }
            });
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_and_explicit_limits_are_bounded() {
        assert_eq!(worker_limit(0, 100, 16), 8);
        assert_eq!(worker_limit(0, 100, 4), 2);
        assert_eq!(worker_limit(0, 100, 1), 1);
        assert_eq!(worker_limit(16, 100, 16), 16);
        assert_eq!(worker_limit(50, 3, 16), 3);
        assert_eq!(worker_limit(0, 0, 16), 0);
    }
    #[test]
    fn workers_overlap_and_process_each_song_once() {
        let barrier = std::sync::Barrier::new(3);
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let hits: Vec<_> = (0..12).map(|_| AtomicUsize::new(0)).collect();
        for_each(&(0..12).collect::<Vec<_>>(), 3, |&i| {
            hits[i].fetch_add(1, Ordering::SeqCst);
            let n = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(n, Ordering::SeqCst);
            if i < 3 {
                barrier.wait();
            }
            active.fetch_sub(1, Ordering::SeqCst);
            true
        });
        assert_eq!(peak.load(Ordering::SeqCst), 3);
        assert!(hits.iter().all(|n| n.load(Ordering::SeqCst) == 1));
    }
    #[test]
    fn receiver_shutdown_stops_new_jobs() {
        let hits = AtomicUsize::new(0);
        for_each(&[0; 30], 1, |_| {
            hits.fetch_add(1, Ordering::SeqCst);
            false
        });
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }
}
