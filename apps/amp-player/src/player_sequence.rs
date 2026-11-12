//! Normal Player continuation: playlist order / Smart selection, with song-end fades.
use super::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Sequence {
    #[serde(default)]
    pub resume_auto: bool,
    pub smart: Option<amp_core::show::SmartPlaylist>,
    pub recent: Vec<String>,
    #[serde(skip)]
    pub explicit_regular: bool,
    #[serde(skip)]
    pub failed: Vec<String>,
    #[serde(skip)]
    pub advance: bool,
}
fn envelope(position: f64, remaining: Option<f32>, seconds: f32) -> f32 {
    let fade = seconds.max(0.05) as f64;
    let incoming = (position / fade).clamp(0., 1.) as f32;
    incoming.min(remaining.map_or(1., |left| (left / seconds.max(0.05)).clamp(0., 1.)))
}
impl Player {
    pub(super) fn resume_player_sequence(&mut self) {
        if self.playback_owner == playback_owner::Owner::Player && self.player_sequence.resume_auto
        {
            self.auto = true;
            self.player_sequence.resume_auto = false;
        }
    }
    pub(super) fn player_fade_controls(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.label("Song fade duration");
        let before = self.fade_seconds;
        ui.horizontal(|ui| {
            if ui.small_button("−").clicked() {
                self.fade_seconds -= 0.5;
            }
            ui.add(
                egui::DragValue::new(&mut self.fade_seconds)
                    .range(0.5..=30.)
                    .suffix(" s"),
            );
            if ui.small_button("+").clicked() {
                self.fade_seconds += 0.5;
            }
        });
        self.fade_seconds = self.fade_seconds.clamp(0.5, 30.);
        if self.fade_seconds != before
            && let Err(error) = crossfade_settings::save(self.fade_seconds)
        {
            self.status = error.to_string();
        }
        ui.weak("Fade out at song end, then fade the next song in. Shared with DJ mix duration.");
    }
    pub(super) fn inherit_player_sequence(&mut self) {
        if self.player_sequence.explicit_regular {
            self.player_sequence.smart = None;
            self.player_sequence.explicit_regular = false;
        } else {
            self.player_sequence.smart = self.run.profile.clone();
            if self.player_sequence.smart.is_none()
                && self.queue.first().copied() == self.loaded[0]
                && !self.queue.is_empty()
            {
                self.queue.remove(0);
            }
        }
        self.player_sequence.recent = self.run.recent_songs();
        self.player_sequence.failed.clear();
        self.player_sequence.resume_auto = false;
        self.auto = self.player_sequence.smart.is_some() || !self.queue.is_empty();
    }
    pub(super) fn player_song_started(&mut self) {
        if let Some(track) = self.loaded_track(0) {
            self.player_sequence
                .recent
                .push(amp_core::show::song_id(track));
            if self.player_sequence.recent.len() > 100 {
                self.player_sequence.recent.remove(0);
            }
        }
        if let Some(audio) = &self.audio {
            audio.set_player_envelope(0.);
        }
    }
    pub(super) fn player_sequence_tick(&mut self) {
        let Some(audio) = &self.audio else {
            return;
        };
        if self.playback_owner != playback_owner::Owner::Player {
            audio.set_player_envelope(1.);
            return;
        }
        if audio.load_current(0) && audio.playing(0) {
            audio.set_player_envelope(envelope(
                audio.position(0) / self.deck_rate(0),
                audio.remaining(0),
                self.fade_seconds,
            ));
        }
    }
    pub(super) fn next_player_smart(&mut self) -> Option<usize> {
        let mut profile = self.player_sequence.smart.clone()?;
        profile
            .excluded
            .extend(self.player_sequence.failed.iter().cloned());
        let library = self.library.as_ref()?;
        amp_core::show::choose(
            &profile,
            &library.tracks,
            self.loaded[0],
            &self.player_sequence.recent,
            profile.target_energy,
            0,
        )
    }
    pub(super) fn switch_dj_song_to_player(&mut self) -> Result<(), String> {
        let active = self.active_deck();
        let mut track = self
            .loaded_track(active)
            .cloned()
            .ok_or("Load a DJ song first")?;
        normal_track(&mut track);
        let root = self.loaded_origin[active]
            .clone()
            .unwrap_or_else(|| self.root.clone());
        let mut runtime = track.clone();
        if let Some(audio) = &self.audio {
            let end = runtime.preparation.cue_out.or(runtime
                .preparation
                .analysis
                .as_ref()
                .map(|a| a.duration));
            let at = audio.source_position(active);
            runtime.preparation.cue_in = end.map_or(at, |end| at.min((end - 0.02).max(0.)));
        }
        self.ensure_audio()?;
        self.audio
            .as_mut()
            .unwrap()
            .load(0, &root, &runtime, true)?;
        self.loaded[0] = self.loaded[active];
        self.loaded_snapshot[0] = Some(track);
        self.loaded_origin[0] = Some(root);
        self.claim_playback(playback_owner::Owner::Player, 0, false);
        self.player_song_started();
        Ok(())
    }
}
pub(super) fn normal_track(track: &mut amp_core::Track) {
    if !track.preparation.manual_override && track.preparation.manual_cues.is_empty() {
        track.preparation.cue_in = 0.;
        track.preparation.cue_out = None;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smart_handoff_retains_filters_and_regular_selection_replaces_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = amp_core::scan(dir.path()).unwrap();
        for (name, genre) in [("a.wav", "Rock"), ("b.wav", "Rock"), ("c.wav", "R&B")] {
            let mut prep = amp_core::Preparation::default();
            prep.extensions
                .insert("genre".into(), serde_json::json!(genre));
            lib.tracks.push(amp_core::Track {
                path: name.into(),
                title: name.into(),
                preparation: prep,
            });
        }
        let mut p = Player {
            library: Some(lib),
            loaded: [Some(0), None, None, None, None, None],
            playback_owner: playback_owner::Owner::Dj,
            ..Default::default()
        };
        let mut profile = p.run.settings.playlists[0].clone();
        profile.genres = vec!["Rock".into()];
        p.run.profile = Some(profile);
        p.claim_playback(playback_owner::Owner::Player, 0, false);
        assert!(p.auto && !p.run.auto);
        assert_eq!(p.next_player_smart(), Some(1));
        p.player_sequence.explicit_regular = true;
        p.queue = vec![2, 1];
        p.claim_playback(playback_owner::Owner::Player, 0, false);
        assert!(p.player_sequence.smart.is_none());
        assert_eq!(p.queue, vec![2, 1]);
    }
    #[test]
    fn player_ignores_automatic_entry_points_but_keeps_approved_cues() {
        let mut track = amp_core::Track {
            path: "a.wav".into(),
            title: "A".into(),
            preparation: Default::default(),
        };
        track.preparation.cue_in = 20.;
        track.preparation.cue_out = Some(40.);
        let original = track.preparation.clone();
        normal_track(&mut track);
        assert_eq!(track.preparation.cue_in, 0.);
        assert_eq!(track.preparation.cue_out, None);
        track.preparation = original;
        track.preparation.manual_override = true;
        normal_track(&mut track);
        assert_eq!(track.preparation.cue_in, 20.);
        assert_eq!(track.preparation.cue_out, Some(40.));
    }
    #[test]
    #[ignore = "requires native audio and AMP_WAVEFORM_FIXTURE; muted"]
    fn native_player_playlist_advances_and_stops_without_enabling_dj() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.wav", "b.wav"] {
            std::fs::copy(
                std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap(),
                dir.path().join(name),
            )
            .unwrap();
            let prep = amp_core::Preparation {
                manual_override: true,
                cue_out: Some(0.8),
                ..Default::default()
            };
            amp_core::preparation::save(dir.path(), Path::new(name), &prep).unwrap();
        }
        let lib = amp_core::scan(dir.path()).unwrap();
        let mut p = Player {
            root: dir.path().into(),
            library: Some(lib),
            volume: 0,
            channel_gain: [0.; 6],
            fade_seconds: 0.2,
            queue: vec![1],
            auto: true,
            ..Default::default()
        };
        p.play(0, 0);
        std::thread::sleep(Duration::from_millis(100));
        p.execute_control(control::Action::Pause {
            deck: control::Deck::Auto,
        })
        .unwrap();
        assert!(!p.auto);
        p.execute_control(control::Action::Play {
            deck: control::Deck::Auto,
        })
        .unwrap();
        assert!(p.auto);
        let end = std::time::Instant::now() + Duration::from_secs(8);
        let mut next_seen = false;
        while p.auto {
            p.poll();
            next_seen |= p.loaded[0] == Some(1);
            assert!(!p.run.auto);
            assert_eq!(p.playback_owner, playback_owner::Owner::Player);
            assert!(std::time::Instant::now() < end, "{}", p.status);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(next_seen);
        assert!(p.queue.is_empty());
        assert!(
            p.audio.as_ref().unwrap().controls.finished[0]
                .load(std::sync::atomic::Ordering::Relaxed)
        );
        let mut smart = p.run.settings.playlists[0].clone();
        smart.fallback = amp_core::show::Fallback::Stop;
        p.run.playlist = Some(smart.id.clone());
        p.run.profile = Some(smart);
        p.load_deck(1, 1, true);
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        while !p.audio.as_ref().unwrap().controls.ready[1]
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        p.toggle_player();
        assert_eq!(p.loaded[0], Some(1));
        assert!(p.auto && p.player_sequence.smart.is_some());
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let mut smart_next = false;
        while p.auto {
            p.poll();
            smart_next |= p.loaded[0] == Some(0);
            assert!(!p.run.auto);
            assert!(std::time::Instant::now() < deadline, "{}", p.status);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(smart_next);
    }
    #[test]
    fn normal_player_envelope_fades_each_end_without_touching_deck_gain() {
        assert_eq!(envelope(0., Some(60.), 3.), 0.);
        assert_eq!(envelope(1.5, Some(58.5), 3.), 0.5);
        assert_eq!(envelope(20., Some(40.), 3.), 1.);
        assert_eq!(envelope(58.5, Some(1.5), 3.), 0.5);
        assert_eq!(envelope(60., Some(0.), 3.), 0.);
    }
}
