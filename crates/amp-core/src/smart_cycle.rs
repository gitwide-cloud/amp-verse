//! Session-local Smart shuffle cycles, independent of bounded presentation history.
use crate::{
    Preparation, Track,
    show::{self, SmartPlaylist},
};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
#[derive(Clone, Default)]
pub struct SmartCycle {
    scope: String,
    played: HashSet<String>,
    failed: HashSet<String>,
    seed: u64,
}
impl SmartCycle {
    fn bind(&mut self, library: &str, playlist: &str) {
        let scope = format!("{library}\0{playlist}");
        if self.scope != scope {
            *self = Self {
                scope,
                seed: entropy(),
                ..Self::default()
            };
        }
    }
    pub fn played(&mut self, library: &str, playlist: &str, track: &Track) {
        self.bind(library, playlist);
        self.played.insert(show::song_id(track));
    }
    pub fn failed(&mut self, library: &str, playlist: &str, track: &Track) {
        self.bind(library, playlist);
        self.failed.insert(show::song_id(track));
    }
    pub fn ranked(
        &mut self,
        library: &str,
        p: &SmartPlaylist,
        tracks: &[Track],
        current: Option<usize>,
        target: Option<f64>,
        snapshot: Option<&Preparation>,
    ) -> Vec<usize> {
        self.bind(library, &p.id);
        let known: HashSet<_> = tracks.iter().map(show::song_id).collect();
        self.played.retain(|id| known.contains(id));
        self.failed.retain(|id| known.contains(id));
        let pool: HashSet<_> = tracks
            .iter()
            .filter(|t| show::eligible(p, t))
            .map(show::song_id)
            .filter(|id| !self.failed.contains(id))
            .collect();
        if pool.is_empty() {
            return vec![];
        }
        if pool.is_subset(&self.played) {
            if p.fallback == show::Fallback::Stop {
                return Vec::new();
            }
            self.played.clear();
            self.seed = self.seed.wrapping_add(0x9e3779b97f4a7c15);
        }
        let mut rules = p.clone();
        rules.recent_repeat = 0;
        rules.excluded.extend(self.failed.iter().cloned());
        let mut ranked = show::ranked_with_snapshot(&rules, tracks, current, &[], target, snapshot);
        ranked.retain(|i| !self.played.contains(&show::song_id(&tracks[*i])));
        if ranked.is_empty() && pool.len() == 1 {
            if let Some(index) = tracks.iter().position(|t| {
                pool.contains(&show::song_id(t)) && !self.played.contains(&show::song_id(t))
            }) {
                ranked.push(index);
            }
        }
        // Vary the best mix-compatible unplayed window, rather than always
        // choosing the same deterministic catalog-index tie winner.
        let window = ranked.len().min(8);
        ranked[..window].sort_by_cached_key(|i| {
            let mut hash = Sha256::new();
            hash.update(self.seed.to_le_bytes());
            hash.update(show::song_id(&tracks[*i]));
            let bytes = hash.finalize();
            u64::from_le_bytes(bytes[..8].try_into().unwrap())
        });
        ranked
    }
}
fn entropy() -> u64 {
    let hash = Sha256::digest(crate::coordination::token().as_bytes());
    u64::from_le_bytes(hash[..8].try_into().unwrap())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tracks(n: usize) -> Vec<Track> {
        (0..n)
            .map(|i| Track {
                path: format!("song-{i}.wav").into(),
                title: format!("Song {i}"),
                preparation: Default::default(),
            })
            .collect()
    }
    #[test]
    fn full_cycle_exceeds_recent_window_and_never_repeats_at_boundary() {
        let tracks = tracks(129);
        let mut p = show::Settings::default().playlists.remove(0);
        p.recent_repeat = 2;
        let mut cycle = SmartCycle::default();
        let mut current = None;
        for _ in 0..2 {
            let mut heard = HashSet::new();
            for _ in 0..tracks.len() {
                let next = cycle.ranked("library", &p, &tracks, current, None, None)[0];
                assert_ne!(Some(next), current);
                assert!(heard.insert(next));
                cycle.played("library", &p.id, &tracks[next]);
                current = Some(next);
            }
            assert_eq!(heard.len(), tracks.len());
        }
    }
    #[test]
    fn played_set_is_independent_of_thousand_item_history_cap() {
        let tracks = tracks(1210);
        let p = show::Settings::default().playlists.remove(0);
        let mut cycle = SmartCycle::default();
        for t in &tracks[..1209] {
            cycle.played("l", &p.id, t);
        }
        assert_eq!(
            cycle.ranked("l", &p, &tracks, Some(1208), None, None),
            vec![1209]
        );
    }
    #[test]
    fn previews_cancellation_failures_and_library_changes_do_not_fake_consumption() {
        let tracks = tracks(3);
        let p = show::Settings::default().playlists.remove(0);
        let mut cycle = SmartCycle::default();
        let first = cycle.ranked("one", &p, &tracks, None, None, None)[0];
        assert_eq!(
            cycle.ranked("one", &p, &tracks, None, None, None)[0],
            first,
            "a cancelled reservation is still eligible"
        );
        cycle.failed("one", &p.id, &tracks[first]);
        let next = cycle.ranked("one", &p, &tracks, None, None, None)[0];
        assert_ne!(next, first);
        cycle.played("one", &p.id, &tracks[next]);
        let last = cycle.ranked("one", &p, &tracks, Some(next), None, None)[0];
        assert_ne!(last, first);
        assert_ne!(last, next);
        cycle.played("one", &p.id, &tracks[last]);
        assert_eq!(
            cycle.ranked("one", &p, &tracks, Some(last), None, None)[0],
            next
        );
        assert_eq!(cycle.ranked("two", &p, &tracks, None, None, None).len(), 3);
    }
    #[test]
    fn stop_fallback_finishes_full_pool_without_resetting_or_repeating() {
        for count in [1, 2, 9] {
            let tracks = tracks(count);
            let mut p = show::Settings::default().playlists.remove(0);
            p.recent_repeat = 1;
            p.fallback = show::Fallback::Stop;
            let mut cycle = SmartCycle::default();
            let mut current = None;
            let mut heard = HashSet::new();
            for _ in 0..count {
                let next = cycle.ranked("l", &p, &tracks, current, None, None)[0];
                assert!(heard.insert(next));
                cycle.played("l", &p.id, &tracks[next]);
                current = Some(next);
            }
            assert!(
                cycle
                    .ranked("l", &p, &tracks, current, None, None)
                    .is_empty()
            );
            assert!(
                cycle
                    .ranked("l", &p, &tracks, current, None, None)
                    .is_empty(),
                "a repeated query must not restart a completed Stop cycle"
            );
            p.fallback = show::Fallback::RelaxRecent;
            let next = cycle.ranked("l", &p, &tracks, current, None, None)[0];
            if count > 1 {
                assert_ne!(Some(next), current);
            }
        }
    }
    #[test]
    fn all_failed_or_no_eligible_candidates_stop_without_relaxing_into_repeats() {
        let tracks = tracks(2);
        let mut p = show::Settings::default().playlists.remove(0);
        let mut cycle = SmartCycle::default();
        for track in &tracks {
            cycle.failed("l", &p.id, track);
        }
        assert!(
            cycle
                .ranked("l", &p, &tracks, Some(0), None, None)
                .is_empty()
        );
        p.excluded = tracks.iter().map(show::song_id).collect();
        assert!(
            cycle
                .ranked("other", &p, &tracks, Some(0), None, None)
                .is_empty()
        );
    }
    #[test]
    fn zero_one_two_and_new_eligible_members_are_sensible() {
        let p = show::Settings::default().playlists.remove(0);
        let mut cycle = SmartCycle::default();
        assert!(cycle.ranked("l", &p, &[], None, None, None).is_empty());
        let one = tracks(1);
        for _ in 0..3 {
            assert_eq!(cycle.ranked("l", &p, &one, Some(0), None, None), vec![0]);
            cycle.played("l", &p.id, &one[0]);
        }
        let two = tracks(2);
        assert_eq!(cycle.ranked("l", &p, &two, Some(0), None, None), vec![1]);
        cycle.played("l", &p.id, &two[1]);
        assert_eq!(cycle.ranked("l", &p, &two, Some(1), None, None), vec![0]);
    }
}
