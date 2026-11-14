//! Single authority for Player versus DJ playback. Views and remotes do not own audio.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Owner {
    #[default]
    Player,
    Dj,
}

impl Player {
    pub(super) fn claim_playback(&mut self, owner: Owner, starting: usize, transition: bool) {
        crate::preparation_speed::set_playing(true);
        if owner == Owner::Dj {
            self.auto = false;
            if let Some(audio) = &self.audio {
                audio.set_player_envelope(1.);
            }
        } else if let Some(audio) = &self.audio {
            audio.set_fader(0.);
            audio.scratch(0, false, 0.);
        }
        if self.playback_owner == owner {
            if owner == Owner::Player && self.player_sequence.explicit_regular {
                self.player_sequence.smart = None;
                self.player_sequence.explicit_regular = false;
            }
            return;
        }
        if let Some(audio) = &mut self.audio {
            if owner == Owner::Player {
                self.run.cancel_effects(audio);
                audio.cancel_section(0);
                for deck in 1..6 {
                    audio.pause(deck);
                }
                let _ = audio.sample_stop(None);
            } else if starting != 0 && !transition {
                audio.pause(0);
            }
            if owner == Owner::Dj
                && !transition
                && audio.controls.pair_indices().contains(&starting)
            {
                audio.set_fader((starting % 2) as f32);
            }
        }
        if owner == Owner::Player {
            self.inherit_player_sequence();
            self.jog_resume = [None; 6];
            self.jog_auto = [false; 6];
            self.run.cancel_automation();
            if let Some(audio) = &self.right_sample_audio {
                let _ = audio.sample_stop(None);
            }
            for side in &mut self.sides {
                side.stop();
            }
        } else {
            self.auto = false;
        }
        self.playback_owner = owner;
    }
    pub(super) fn remote_owner(&self, deck: control::Deck) -> Owner {
        if deck == control::Deck::Auto {
            self.playback_owner
        } else {
            Owner::Dj
        }
    }
    pub(super) fn enforce_playback_owner(&mut self) {
        if self.playback_owner == Owner::Player && self.run.auto {
            if let Some(audio) = &mut self.audio {
                self.run.cancel_effects(audio);
            }
            self.run.cancel_automation();
        }
    }
    pub(super) fn transport_play(&mut self, owner: Owner, deck: usize) -> Result<(), String> {
        let audio = self.audio.as_ref().ok_or("Load a track first")?;
        if deck >= 6 {
            return Err("Choose a valid deck".into());
        }
        if !audio.load_current(deck)
            || !audio.loaded(deck)
            || audio.controls.finished[deck].load(std::sync::atomic::Ordering::Relaxed)
        {
            if let Some(track) = self.loaded_snapshot[deck].clone() {
                let root = self.loaded_origin[deck]
                    .clone()
                    .unwrap_or_else(|| self.root.clone());
                self.audio
                    .as_mut()
                    .unwrap()
                    .load(deck, &root, &track, true)?;
                self.claim_playback(owner, deck, false);
                self.resume_player_sequence();
                return Ok(());
            }
            return Err("Deck is not ready".into());
        }
        self.claim_playback(owner, deck, false);
        self.audio.as_ref().unwrap().play(deck)?;
        self.resume_player_sequence();
        Ok(())
    }
    pub(super) fn flush_podcast_progress(&mut self) {
        if !self.podcasts.is_active() {
            return;
        }
        let path = self.loaded_track(0).and_then(|t| {
            self.loaded_origin[0]
                .as_ref()
                .map(|root| root.join(&t.path))
        });
        let position = self.audio.as_ref().map_or(0., |a| a.source_position(0));
        self.podcasts.flush_progress(path.as_deref(), position);
    }
    pub(super) fn pause_playback(&mut self) {
        self.flush_podcast_progress();
        if self.playback_owner == Owner::Dj {
            let was_auto = self.run.auto;
            if let Some(audio) = &mut self.audio {
                self.run.cancel_effects(audio);
                audio.set_fader(audio.fader());
                for deck in 0..6 {
                    audio.pause(deck);
                }
            }
            self.run.cancel_automation();
            self.run.paused = was_auto;
        } else if let Some(audio) = &mut self.audio {
            audio.pause(0);
            self.player_sequence.resume_auto = self.auto;
            self.auto = false;
        }
    }
    pub(super) fn toggle_player(&mut self) {
        self.flush_podcast_progress();
        if self.playback_owner == Owner::Dj {
            if let Err(error) = self.switch_dj_song_to_player() {
                self.status = error;
            }
            return;
        }
        let was_playing = self.audio.as_ref().is_some_and(|a| a.playing(0));
        if was_playing {
            self.player_sequence.resume_auto = self.auto;
            self.auto = false;
        }
        self.toggle_owned_deck(Owner::Player, 0);
        if !was_playing && self.audio.as_ref().is_some_and(|a| a.playing(0)) {
            self.resume_player_sequence();
        }
    }
    pub(super) fn stop_player(&mut self) {
        self.flush_podcast_progress();
        if self.playback_owner == Owner::Player {
            self.player_sequence.resume_auto = self.auto;
            self.stop(0);
        }
    }
    pub(super) fn stop_playback(&mut self) {
        if self.playback_owner == Owner::Player {
            self.stop_player();
        } else {
            let _ = self.execute_control(control::Action::StopAllDecks);
            let _ = self.execute_control(control::Action::StopSamplers { side: None });
            for side in &mut self.sides {
                side.stop();
            }
        }
    }
    pub(super) fn toggle_owned_deck(&mut self, owner: Owner, deck: usize) {
        let switched = self.playback_owner != owner;
        if self.audio.as_ref().is_some_and(|a| {
            a.loaded(deck) && !a.controls.finished[deck].load(std::sync::atomic::Ordering::Relaxed)
        }) {
            if switched {
                if let Err(error) = self.transport_play(owner, deck) {
                    self.status = error;
                }
            } else {
                self.cancel_deck_automation(deck);
                let audio = self.audio.as_mut().unwrap();
                if audio.playing(deck) {
                    audio.pause(deck);
                } else if audio.controls.synced[deck].load(std::sync::atomic::Ordering::Relaxed) {
                    if let Err(error) = audio.launch(deck) {
                        self.status = error;
                    }
                } else {
                    if let Err(error) = audio.play(deck) {
                        self.status = error;
                    }
                }
            }
        } else if let Some(track) = self.loaded_snapshot[deck].clone() {
            let root = self.loaded_origin[deck]
                .clone()
                .unwrap_or_else(|| self.root.clone());
            if let Some(audio) = &mut self.audio {
                match audio.load(deck, &root, &track, true) {
                    Ok(()) => {
                        self.claim_playback(owner, deck, false);
                        self.cancel_deck_automation(deck);
                    }
                    Err(error) => self.status = error,
                }
            }
        } else if let Some(index) = self.loaded[deck].or(self.selected) {
            self.load_deck_owned(deck, index, true, owner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_owner_routes_remote_auto_and_excludes_opposing_automation() {
        let mut p = Player::default();
        assert_eq!(p.remote_owner(control::Deck::Auto), Owner::Player);
        assert_eq!(p.requested_deck(control::Deck::Auto, false), 0);
        p.auto = true;
        p.claim_playback(Owner::Dj, 1, false);
        assert!(!p.auto);
        p.run.auto = true;
        assert_eq!(p.remote_owner(control::Deck::Auto), Owner::Dj);
        p.claim_playback(Owner::Player, 0, false);
        assert!(!p.run.auto);
        assert_eq!(p.control_snapshot()["amp"]["playback_owner"], "player");
        assert!(p.transport_play(Owner::Dj, 1).is_err());
        assert_eq!(p.playback_owner, Owner::Player);
    }
    #[test]
    #[ignore = "requires native audio and AMP_WAVEFORM_FIXTURE; muted"]
    fn native_player_dj_handoff_and_remote_commands_have_one_owner() {
        let path = PathBuf::from(std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap())
            .canonicalize()
            .unwrap();
        let mut p = Player {
            volume: 0,
            channel_gain: [0.; 6],
            ..Default::default()
        };
        for d in 0..3 {
            p.safe_load_path(d, path.clone(), None).unwrap();
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        while !(0..3).all(|d| {
            p.audio.as_ref().unwrap().controls.ready[d].load(std::sync::atomic::Ordering::Relaxed)
        }) {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        p.transport_play(Owner::Player, 0).unwrap();
        p.auto = true;
        p.execute_control(control::Action::Play {
            deck: control::Deck::B1,
        })
        .unwrap();
        assert_eq!(p.playback_owner, Owner::Dj);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(p.audio.as_ref().unwrap().fader(), 1.);
        assert!(!p.auto && !p.audio.as_ref().unwrap().playing(0));
        assert!(p.audio.as_ref().unwrap().playing(1));
        p.execute_control(control::Action::Play {
            deck: control::Deck::A2,
        })
        .unwrap();
        assert!(p.audio.as_ref().unwrap().playing(1) && p.audio.as_ref().unwrap().playing(2));
        p.stop_player();
        assert!(p.audio.as_ref().unwrap().playing(1));
        p.run.auto = true;
        p.toggle_player();
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(p.playback_owner, Owner::Player);
        assert!(!p.run.auto);
        assert_eq!(p.audio.as_ref().unwrap().fader(), 0.);
        assert!(p.audio.as_ref().unwrap().playing(0));
        assert!((1..6).all(|d| !p.audio.as_ref().unwrap().playing(d)));
        p.execute_control(control::Action::Pause {
            deck: control::Deck::Auto,
        })
        .unwrap();
        assert!(!p.audio.as_ref().unwrap().playing(0));
        p.execute_control(control::Action::Play {
            deck: control::Deck::Auto,
        })
        .unwrap();
        assert_eq!(p.playback_owner, Owner::Player);
        assert!(p.audio.as_ref().unwrap().playing(0));
        p.execute_control(control::Action::Stop {
            deck: control::Deck::Auto,
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(50));
        p.execute_control(control::Action::Play {
            deck: control::Deck::Auto,
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(p.playback_owner, Owner::Player);
        assert!(p.audio.as_ref().unwrap().playing(0));
        p.execute_control(control::Action::Play {
            deck: control::Deck::B1,
        })
        .unwrap();
        p.execute_control(control::Action::Stop {
            deck: control::Deck::Auto,
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert!((0..6).all(|d| !p.audio.as_ref().unwrap().playing(d)));
        assert_eq!(p.playback_owner, Owner::Dj);
        assert!(
            p.execute_control(control::Action::Play {
                deck: control::Deck::B3
            })
            .is_err()
        );
        assert_eq!(p.playback_owner, Owner::Dj);
        assert!((0..6).all(|d| !p.audio.as_ref().unwrap().playing(d)));
    }
}
