use amp_core::Playlist;
// Playlist feedback is saved before playback is changed. Audio files are never deleted.
impl Player {
    pub(super) fn can_dislike_deck(&self, deck: usize) -> bool {
        deck < 6
            && self.pair().contains(&deck)
            && self.loaded_track(deck).is_some()
            && self.loaded_origin[deck].as_deref() == Some(self.root.as_path())
            && (self.run.profile.is_some()
                || (self.run.has_regular_playlist() && self.playlist_snapshot.is_some()))
    }
    fn remove_deck_song_from_playlist(&mut self, deck: usize) -> Result<String, String> {
        if !self.can_dislike_deck(deck) {
            return Err(
                "Dislike needs a loaded deck and its playback playlist. Load a playlist first."
                    .into(),
            );
        }
        let track = self.loaded_track(deck).ok_or("No song loaded")?.clone();
        let id = show::song_id(&track);
        let playlist_name;
        if let Some(profile) = self.run.profile.as_ref() {
            let profile_id = profile.id.clone();
            let mut fresh = show::Settings::load(&self.root).map_err(|e| e.to_string())?;
            let saved = fresh
                .playlists
                .iter_mut()
                .find(|p| p.id == profile_id)
                .ok_or("The active Smart Playlist is no longer saved; nothing was changed")?;
            if !saved.excluded.contains(&id) {
                saved.excluded.push(id.clone());
            }
            playlist_name = saved.name.clone();
            fresh.save(&self.root).map_err(|e| e.to_string())?;
            // Keep performance overrides, only apply the explicit new exclusion to this run.
            if let Some(active) = &mut self.run.profile
                && !active.excluded.contains(&id)
            {
                active.excluded.push(id.clone());
            }
            self.run.settings = show::Settings::load(&self.root).map_err(|e| e.to_string())?;
            self.show_editor.refresh_saved(&self.run.settings);
            if let Some(active) = &mut self.player_sequence.smart
                && active.id == profile_id
                && !active.excluded.contains(&id)
            {
                active.excluded.push(id.clone());
            }
        } else {
            let (path, snapshot) = self
                .playlist_snapshot
                .as_ref()
                .ok_or("No saved playlist is loaded")?;
            if self.run.playlist.as_deref() != Some(snapshot.name.as_str()) {
                return Err("The playback queue no longer matches the loaded playlist; reload it before using Dislike".into());
            }
            let path = path.clone();
            let mut saved = Playlist::load(&path, &self.root).map_err(|e| e.to_string())?;
            if !saved.tracks.contains(&track.path) {
                return Err(
                    "This song is not in the active saved playlist; nothing was changed".into(),
                );
            }
            saved.tracks.retain(|p| p != &track.path);
            playlist_name = saved.name.clone();
            saved
                .save_in_library(&path, &self.root)
                .map_err(|e| e.to_string())?;
            let saved = Playlist::load(&path, &self.root).map_err(|e| e.to_string())?;
            self.playlist_snapshot = Some((path.clone(), saved.clone()));
            if let Some((_, list)) = self
                .dj_browser
                .playlists
                .iter_mut()
                .find(|(p, _)| p == &path)
            {
                *list = saved.clone();
            }
            self.apply_disliked_playlist(&path, &track.path, &saved);
        }
        if let Some(lib) = &self.library {
            self.queue
                .retain(|i| lib.tracks.get(*i).is_some_and(|t| t.path != track.path));
        }
        self.queue_selected = None;
        self.run.requests.retain(|request| request != &id);
        self.run.future_candidates.clear();
        Ok(format!("{} removed from {playlist_name}", track.title))
    }
    pub(super) fn dislike_deck(&mut self, deck: usize) {
        if self.can_dislike_deck(deck)
            && self.run.pending.is_some_and(|(incoming, started)| {
                incoming != deck && self.run.feedback_handoff == Some(started)
            })
        {
            self.status = "Dislike already saved · next song is being prepared".into();
            return;
        }
        let message = match self.remove_deck_song_from_playlist(deck) {
            Ok(message) => message,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        // Rebuild any reservation against the updated membership. An already-audible,
        // different incoming song can finish its existing handoff without restarting.
        let fading = self.audio.as_ref().is_some_and(|a| a.fading());
        let bad_incoming = self
            .run
            .preloaded
            .filter(|(_, i)| {
                self.library
                    .as_ref()
                    .and_then(|lib| lib.tracks.get(*i))
                    .is_some_and(|t| {
                        self.loaded_track(deck)
                            .is_some_and(|current| t.path == current.path)
                    })
            })
            .map(|(d, _)| d)
            .or(self.run.transition.filter(|incoming| *incoming == deck))
            .or_else(|| (!fading && deck != self.active_deck()).then_some(deck));
        let automatic = self.run.auto;
        if !fading || bad_incoming.is_some() {
            if let Some(incoming) = bad_incoming
                && let Some(audio) = &mut self.audio
            {
                let survivor = audio.controls.other(incoming);
                audio.set_fader((survivor % 2) as f32);
                audio.stop(incoming);
                self.playing[incoming] = false;
                self.run.active = survivor;
            }
            self.run.cancel_automation();
            self.run.auto = automatic;
            if let Some(regular) = &mut self.run.regular {
                regular.unstarted = false;
                regular.reservation = None;
            }
        } else if let Some(reservation) = self
            .run
            .regular
            .as_mut()
            .and_then(|r| r.reservation.as_mut())
            && let Some(slot) = self.queue.iter().position(|i| *i == reservation.index)
        {
            reservation.slot = slot;
            reservation.queue = self.queue.clone();
        }
        match self.execute_control(Action::MixNext { immediate: true }) {
            Ok(()) => {
                self.run.feedback_handoff = self.run.pending.map(|(_, started)| started);
                self.status = format!("{message} · mixing next now");
            }
            Err(error) => {
                self.stop(deck);
                self.status = format!("{message} · no next song could start: {error}");
            }
        }
    }
}
#[cfg(test)]
mod dislike_tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Player) {
        let dir = tempfile::tempdir().unwrap();
        for name in ["one.mp3", "two.mp3", "three.mp3"] {
            std::fs::write(dir.path().join(name), b"original audio fixture").unwrap();
        }
        let library = amp_core::scan(dir.path()).unwrap();
        let mut player = Player {
            root: dir.path().into(),
            library: Some(library),
            ..Default::default()
        };
        player.loaded[0] = Some(0);
        player.loaded_snapshot[0] = Some(player.library.as_ref().unwrap().tracks[0].clone());
        player.loaded_origin[0] = Some(dir.path().into());
        (dir, player)
    }
    #[test]
    fn dislike_smart_persists_only_an_exclusion_and_keeps_audio() {
        let (dir, mut player) = fixture();
        let initial = show::Settings::default();
        initial.save(dir.path()).unwrap();
        player.run.settings = show::Settings::load(dir.path()).unwrap();
        player.run.profile = Some(player.run.settings.playlists[0].clone());
        let id = show::song_id(player.loaded_track(0).unwrap());
        player.remove_deck_song_from_playlist(0).unwrap();
        let saved = show::Settings::load(dir.path()).unwrap();
        assert!(saved.playlists[0].excluded.contains(&id));
        assert!(player.run.profile.as_ref().unwrap().excluded.contains(&id));
        assert_eq!(
            std::fs::read(dir.path().join(&player.loaded_track(0).unwrap().path)).unwrap(),
            b"original audio fixture"
        );
        player.remove_deck_song_from_playlist(0).unwrap();
        assert_eq!(
            show::Settings::load(dir.path()).unwrap().playlists[0]
                .excluded
                .iter()
                .filter(|s| *s == &id)
                .count(),
            1
        );
    }
    #[test]
    fn dislike_regular_removes_all_occurrences_but_keeps_other_order_and_sources() {
        let (dir, mut player) = fixture();
        let paths: Vec<_> = player
            .library
            .as_ref()
            .unwrap()
            .tracks
            .iter()
            .map(|t| t.path.clone())
            .collect();
        let path = dir.path().join("normal.json");
        let list = Playlist {
            version: 1,
            name: "Normal".into(),
            tracks: vec![
                paths[0].clone(),
                paths[1].clone(),
                paths[0].clone(),
                paths[2].clone(),
            ],
            companion_available: false,
            sync: None,
        };
        list.save_in_library(&path, dir.path()).unwrap();
        player.playlist_snapshot = Some((path.clone(), list));
        player.run.load_regular_playlist("Normal".into());
        player.queue = vec![0, 1, 0, 2];
        player.remove_deck_song_from_playlist(0).unwrap();
        assert_eq!(
            Playlist::load(&path, dir.path()).unwrap().tracks,
            vec![paths[1].clone(), paths[2].clone()]
        );
        assert_eq!(player.queue, vec![1, 2]);
        assert!(paths.iter().all(|p| dir.path().join(p).is_file()));
    }
    #[test]
    fn dislike_can_reject_a_loaded_incoming_deck_without_rejecting_the_current_song() {
        let (dir, mut player) = fixture();
        let initial = show::Settings::default();
        initial.save(dir.path()).unwrap();
        player.run.settings = show::Settings::load(dir.path()).unwrap();
        player.run.profile = Some(player.run.settings.playlists[0].clone());
        player.loaded[1] = Some(1);
        player.loaded_snapshot[1] = Some(player.library.as_ref().unwrap().tracks[1].clone());
        player.loaded_origin[1] = Some(dir.path().into());
        let incoming = show::song_id(player.loaded_track(1).unwrap());
        let current = show::song_id(player.loaded_track(0).unwrap());
        player.remove_deck_song_from_playlist(1).unwrap();
        let saved = show::Settings::load(dir.path()).unwrap();
        assert!(saved.playlists[0].excluded.contains(&incoming));
        assert!(!saved.playlists[0].excluded.contains(&current));
    }
    #[test]
    fn repeated_dislike_does_not_restart_the_pending_handoff() {
        let (dir, mut player) = fixture();
        show::Settings::default().save(dir.path()).unwrap();
        player.run.settings = show::Settings::load(dir.path()).unwrap();
        player.run.profile = Some(player.run.settings.playlists[0].clone());
        player.remove_deck_song_from_playlist(0).unwrap();
        let before = serde_json::to_value(show::Settings::load(dir.path()).unwrap()).unwrap();
        let stamp = Instant::now();
        player.run.pending = Some((1, stamp));
        player.run.feedback_handoff = Some(stamp);
        player.dislike_deck(0);
        assert_eq!(player.run.pending, Some((1, stamp)));
        assert_eq!(
            serde_json::to_value(show::Settings::load(dir.path()).unwrap()).unwrap(),
            before
        );
    }
    #[test]
    fn dislike_without_a_playback_playlist_or_on_an_empty_deck_changes_nothing() {
        let (_dir, mut player) = fixture();
        player.queue = vec![0, 1];
        assert!(player.remove_deck_song_from_playlist(0).is_err());
        assert!(player.remove_deck_song_from_playlist(1).is_err());
        assert_eq!(player.queue, vec![0, 1]);
    }
}
