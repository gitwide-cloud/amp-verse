//! Device-local recovery. Never publishes shared metadata or starts audio.
use super::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
pub struct Deck {
    pub path: PathBuf,
    pub position: f64,
}
fn default_pair() -> [usize; 2] {
    [0, 1]
}
fn deck_slots<'de, D: serde::Deserializer<'de>>(d: D) -> Result<[Option<Deck>; 6], D::Error> {
    let slots = Vec::<Option<Deck>>::deserialize(d)?;
    if slots.len() != 2 && slots.len() != 6 {
        return Err(serde::de::Error::custom(
            "Expected two legacy or six deck slots",
        ));
    }
    let mut result = std::array::from_fn(|_| None);
    for (index, value) in slots.into_iter().enumerate() {
        result[index] = value;
    }
    Ok(result)
}
#[derive(Clone, Serialize, Deserialize)]
struct DeckSettings {
    gain: f32,
    pitch: f32,
    eq: [f32; 3],
    filter: f32,
    key_lock: bool,
    synced: bool,
    stems: [f32; 3],
}
impl DeckSettings {
    fn valid(&self) -> bool {
        self.gain.is_finite()
            && (0. ..=1.).contains(&self.gain)
            && self.pitch.is_finite()
            && (-50. ..=100.).contains(&self.pitch)
            && self
                .eq
                .iter()
                .all(|v| v.is_finite() && (-24. ..=12.).contains(v))
            && self.filter.is_finite()
            && (200. ..=22000.).contains(&self.filter)
            && self
                .stems
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v))
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    deck_settings: Option<[DeckSettings; 6]>,
    #[serde(default)]
    clock: Option<(f32, u32)>,
    #[serde(default)]
    rack: deck_rack::Rack,
    #[serde(default = "default_pair")]
    pair: [usize; 2],
    #[serde(default)]
    sizes: [Option<[f32; 2]>; 6],
    #[serde(default)]
    splits: [Option<f32>; 3],
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    eq_open: bool,
    #[serde(default)]
    fx_open: bool,
    #[serde(default)]
    automatic_restore: bool,
    version: u32,
    root: PathBuf,
    #[serde(deserialize_with = "deck_slots")]
    decks: [Option<Deck>; 6],
    queue: Vec<PathBuf>,
    playlist: Option<(PathBuf, Playlist)>,
    requests: Vec<String>,
    playlist_id: Option<String>,
    choice: String,
    style: amp_core::show::Style,
    target: Option<f64>,
    profile: Option<amp_core::show::SmartPlaylist>,
    transitions: transition_styles::Settings,
    crossfade: bool,
    was_auto: bool,
    panels: [bool; 6],
    parent: [Option<usize>; 6],
    mode: u8,
    volume: u8,
    fader: f32,
    wave_span: f64,
}
#[derive(Default)]
pub struct Recovery {
    file: Option<PathBuf>,
    key: String,
    // Stable viewport-builder inputs. Live checkpoint measurements must never
    // become resize commands on the following frame.
    initial_sizes: [Option<[f32; 2]>; 6],
    automatic_restore: bool,
    attempted: bool,
    startup_size: Option<egui::Vec2>,
    manually_resized: bool,
    sizes: [Option<[f32; 2]>; 6],
    splits: [Option<f32>; 3],
    pub pending: Option<Snapshot>,
    worker: Option<std::thread::JoinHandle<Result<(), String>>>,
    last: Option<std::time::Instant>,
    pub error: Option<String>,
}
impl Recovery {
    pub(super) fn enabled(&self) -> bool {
        self.file.is_some()
    }
    pub fn open(root: &Path) -> Self {
        use sha2::{Digest, Sha256};
        let mut state = Self {
            key: format!(
                "session/{:x}",
                Sha256::digest(root.to_string_lossy().as_bytes())
            ),
            ..Default::default()
        };
        state.file =
            theme::settings_path().and_then(|p| p.parent().map(|p| p.join("ampdj/session.ampdb")));
        if let Some(file) = &state.file {
            match amp_core::catalog_db::local_record(file, &state.key) {
                Ok(Some(value)) => match serde_json::from_value::<Snapshot>(value) {
                    Ok(snapshot) if snapshot.valid(root) => {
                        state.automatic_restore = snapshot.automatic_restore;
                        state.pending = Some(snapshot);
                    }
                    Ok(_) => state.error = Some(
                        "Saved session belongs to another library or is invalid; it was preserved."
                            .into(),
                    ),
                    Err(e) => state.error = Some(format!("Saved session could not be read: {e}")),
                },
                Ok(None) => {}
                Err(e) => state.error = Some(e),
            }
        }
        state
    }
    fn save(&mut self, snapshot: Snapshot, wait: bool) {
        if let Some(worker) = self.worker.take() {
            if wait || worker.is_finished() {
                match worker.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => self.error = Some(e),
                    Err(_) => self.error = Some("Session checkpoint failed".into()),
                }
            } else {
                self.worker = Some(worker);
                return;
            }
        }
        let Some(file) = self.file.clone() else {
            return;
        };
        let key = self.key.clone();
        let value = serde_json::to_value(snapshot).expect("Finite validated session values");
        self.worker = Some(std::thread::spawn(move || {
            amp_core::catalog_db::save_local_record(&file, &key, &value)
        }));
        self.last = Some(std::time::Instant::now());
        if wait && let Some(worker) = self.worker.take() {
            match worker.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => self.error = Some(e),
                Err(_) => self.error = Some("Session checkpoint failed".into()),
            }
        }
    }
}
impl Snapshot {
    fn valid(&self, root: &Path) -> bool {
        self.deck_settings
            .as_ref()
            .is_none_or(|decks| decks.iter().all(DeckSettings::valid))
            && self.clock.is_none_or(|(b, q)| {
                b.is_finite() && (40. ..=240.).contains(&b) && [0, 1, 4, 8, 16, 32].contains(&q)
            })
            && self.rack.live[0] < 6
            && self.rack.live[1] < 6
            && self.rack.live[0].is_multiple_of(2)
            && self.rack.live[1] % 2 == 1
            && self.pair[0] < 6
            && self.pair[1] < 6
            && self.pair[0].is_multiple_of(2)
            && self.pair[1] % 2 == 1
            && self.sizes.iter().flatten().all(|size| {
                size.iter()
                    .all(|n| n.is_finite() && *n >= 80. && *n <= 16384.)
            })
            && self
                .splits
                .iter()
                .flatten()
                .all(|n| n.is_finite() && *n >= 0. && *n <= 16384.)
            && self.version == 1
            && self.root == root
            && self.volume <= 100
            && self.mode <= 2
            && self.fader.is_finite()
            && (0. ..=1.).contains(&self.fader)
            && self.wave_span.is_finite()
            && (4. ..=60.).contains(&self.wave_span)
            && self
                .decks
                .iter()
                .flatten()
                .all(|d| d.path.is_absolute() && d.position.is_finite() && d.position >= 0.)
            && self.queue.len() <= 100_000
            && self.requests.len() <= 10_000
            && self
                .target
                .is_none_or(|v| v.is_finite() && (1. ..=10.).contains(&v))
            && (0..6).all(|start| {
                let mut seen = [false; 6];
                let mut at = Some(start);
                while let Some(i) = at {
                    if i >= 6 || seen[i] {
                        return false;
                    }
                    seen[i] = true;
                    at = self.parent[i];
                }
                true
            })
            && self.parent[0].is_none()
    }
}
impl Player {
    pub(super) fn try_automatic_recovery(&mut self, ctx: &egui::Context) {
        if self.recovery.pending.is_some() && !self.recovery.attempted {
            let (size, maximized) = ctx.input(|i| {
                (
                    i.viewport().inner_rect.map(|r| r.size()),
                    i.viewport().maximized == Some(true) || i.viewport().fullscreen == Some(true),
                )
            });
            if maximized {
                self.recovery.manually_resized = true;
            }
            if let Some(size) = size {
                if let Some(initial) = self.recovery.startup_size {
                    if (initial - size).length() > 2. {
                        self.recovery.manually_resized = true;
                    }
                } else {
                    self.recovery.startup_size = Some(size);
                }
            }
        }

        if self.library.is_some()
            && self.recovery.pending.is_some()
            && self.recovery.automatic_restore
            && !self.recovery.attempted
        {
            self.recovery.attempted = true;
            self.restore_session(ctx);
        }
    }
    pub(super) fn capture_session_layout(&mut self, ctx: &egui::Context) {
        if self.recovery.pending.is_some() {
            return;
        }
        for pane in 0..6 {
            if let Some(rect) = ctx.input(|i| {
                i.raw
                    .viewports
                    .get(&window_layout::viewport(pane))
                    .filter(|v| {
                        !v.maximized.unwrap_or(false)
                            && !v.fullscreen.unwrap_or(false)
                            && !v.minimized.unwrap_or(false)
                    })
                    .and_then(|v| v.inner_rect)
            }) {
                self.recovery.sizes[pane] = Some([rect.width(), rect.height()]);
            }
        }
        for (index, key) in ["dj-wave-height", "dj-library-height", "dj-source-width"]
            .into_iter()
            .enumerate()
        {
            self.recovery.splits[index] = ctx.data_mut(|d| d.get_persisted(egui::Id::unique(key)));
        }
    }
    pub(super) fn recovered_window_size(
        &self,
        pane: usize,
        monitor: Option<egui::Vec2>,
    ) -> [f32; 2] {
        let fallback = window_layout::initial_size(pane, monitor);
        let Some(saved) = self.recovery.initial_sizes[pane] else {
            return fallback;
        };
        let bound = monitor.unwrap_or(egui::vec2(3840., 2160.)) - egui::vec2(48., 80.);
        [
            saved[0].clamp(300., bound.x.max(300.)),
            saved[1].clamp(100., bound.y.max(100.)),
        ]
    }
    fn session_snapshot(&self) -> Snapshot {
        Snapshot {
            deck_settings: Some(std::array::from_fn(|d| DeckSettings {
                gain: self.channel_gain[d],
                pitch: self.pitch[d],
                eq: self.eq[d],
                filter: self.filter[d],
                key_lock: self.audio.as_ref().is_some_and(|a| {
                    a.controls.key_lock[d].load(std::sync::atomic::Ordering::Relaxed)
                }),
                synced: self.audio.as_ref().is_some_and(|a| {
                    a.controls.synced[d].load(std::sync::atomic::Ordering::Relaxed)
                }),
                stems: std::array::from_fn(|i| {
                    self.audio.as_ref().map_or(1., |a| {
                        f32::from_bits(
                            a.controls.stem_level[d][i].load(std::sync::atomic::Ordering::Relaxed),
                        )
                    })
                }),
            })),
            clock: self.audio.as_ref().map(|a| {
                (
                    f32::from_bits(
                        a.controls
                            .master_bpm
                            .load(std::sync::atomic::Ordering::Relaxed),
                    ),
                    a.controls
                        .quantization
                        .load(std::sync::atomic::Ordering::Relaxed),
                )
            }),
            rack: self.rack.clone(),
            pair: self.pair(),
            sizes: self.recovery.sizes,
            splits: self.recovery.splits,
            hidden: self.dj_library_hidden,
            eq_open: self.eq_open,
            fx_open: self.fx_open,
            automatic_restore: self.recovery.automatic_restore,
            version: 1,
            root: self.root.clone(),
            decks: std::array::from_fn(|d| {
                self.loaded_snapshot[d].as_ref().map(|t| Deck {
                    path: self.loaded_origin[d]
                        .as_deref()
                        .unwrap_or(&self.root)
                        .join(&t.path),
                    position: self
                        .audio
                        .as_ref()
                        .map_or(t.preparation.cue_in, |a| a.source_position(d))
                        .max(0.),
                })
            }),
            queue: self
                .queue
                .iter()
                .filter_map(|i| {
                    self.library
                        .as_ref()?
                        .tracks
                        .get(*i)
                        .map(|t| amp_core::music_sources::audio_path(&self.root, &t.path))
                })
                .collect(),
            playlist: self.playlist_snapshot.clone(),
            requests: self.run.requests.iter().cloned().collect(),
            playlist_id: self.run.playlist.clone(),
            choice: self.run.choice.clone(),
            style: self.run.style,
            target: self.run.target,
            profile: self.run.profile.clone(),
            transitions: self.run.transitions,
            crossfade: self.run.crossfade,
            was_auto: self.run.auto,
            panels: self.open_panes(),
            parent: self.layout.parent,
            mode: self.player_mode,
            volume: self.volume,
            fader: self.fader,
            wave_span: self.wave_span,
        }
    }
    pub(super) fn checkpoint(&mut self, wait: bool) {
        if self.library.is_none()
            || self.recovery.file.is_none()
            || self.recovery.pending.is_some()
            || self.recovery.error.is_some()
        {
            return;
        }
        if wait
            || self
                .recovery
                .last
                .is_none_or(|last| last.elapsed() > Duration::from_secs(5))
        {
            let snapshot = self.session_snapshot();
            if snapshot.valid(&self.root) {
                self.recovery.save(snapshot, wait);
            }
        }
    }
    pub(super) fn recovery_ui(&mut self, ui: &mut egui::Ui) {
        if let Some(error) = &self.recovery.error {
            ui.colored_label(ORANGE, error);
            if self.recovery.pending.is_none() && ui.button("Start a new checkpoint").clicked() {
                self.recovery.error = None;
            }
        }
        if self.recovery.pending.is_none() {
            return;
        }
        ui.checkbox(
            &mut self.recovery.automatic_restore,
            "Automatically restore paused next time",
        );
        let active = self
            .audio
            .as_ref()
            .is_some_and(|a| (0..6).any(|d| a.playing(d)));
        let mut restore = false;
        let mut discard = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Previous session available — restore paused decks and queue?");
            restore = ui
                .add_enabled(
                    self.library.is_some() && !active,
                    egui::Button::new("Restore session"),
                )
                .clicked();
            discard = ui.button("Start fresh").clicked();
        });
        if discard {
            self.recovery.pending = None;
            self.recovery.error = None;
            return;
        }
        if restore {
            self.restore_session(ui.ctx());
        }
    }
    pub(super) fn restore_session(&mut self, ctx: &egui::Context) {
        if self.library.is_none()
            || self
                .audio
                .as_ref()
                .is_some_and(|a| (0..6).any(|d| a.playing(d)) || a.fading())
        {
            return;
        }
        if self.recovery.pending.is_none() {
            return;
        }
        let snapshot = self.recovery.pending.clone().unwrap();
        if !snapshot.valid(&self.root) {
            self.recovery.error = Some(
                "This session belongs to a different library. Start fresh to continue.".into(),
            );
            return;
        }
        let mut errors = Vec::new();
        if snapshot.pair != self.pair()
            && let Err(e) = self
                .ensure_audio()
                .and_then(|_| self.audio.as_ref().unwrap().assign_pair(snapshot.pair))
        {
            self.recovery.error = Some(e);
            return;
        }
        self.rack = snapshot.rack.clone();
        for (deck, saved) in snapshot.decks.iter().enumerate() {
            if let Some(saved) = saved
                && let Err(e) = self.safe_load_path(deck, saved.path.clone(), Some(saved.position))
            {
                errors.push(e);
            }
        }
        if let Some(a) = &self.audio {
            if let Some((bpm, quantization)) = snapshot.clock {
                let _ = a.set_clock(bpm, quantization);
            }
            if let Some(settings) = &snapshot.deck_settings {
                for (d, saved) in settings.iter().enumerate() {
                    self.channel_gain[d] = saved.gain;
                    self.pitch[d] = saved.pitch;
                    self.eq[d] = saved.eq;
                    self.filter[d] = saved.filter;
                    a.set_gain(d, saved.gain);
                    a.set_rate(d, 1. + saved.pitch / 100.);
                    for (band, value) in saved.eq.into_iter().enumerate() {
                        a.set_eq(d, band, value);
                    }
                    a.set_filter(d, saved.filter);
                    use std::sync::atomic::Ordering::Relaxed;
                    a.controls.key_lock[d].store(saved.key_lock, Relaxed);
                    let _ = a.restore_stems(d, saved.stems);
                    // Sync is restored while paused; loading never starts playback.
                    let _ = a.restore_sync(d, saved.synced);
                }
            }
        }
        self.queue =
            snapshot
                .queue
                .iter()
                .filter_map(|path| {
                    self.library.as_ref()?.tracks.iter().position(|t| {
                        amp_core::music_sources::audio_path(&self.root, &t.path) == *path
                    })
                })
                .collect();
        let missing = snapshot.queue.len() - self.queue.len();
        self.playlist_snapshot = snapshot.playlist.clone();
        self.run.playlist = snapshot.playlist_id.clone();
        self.run.requests = snapshot.requests.clone().into();
        self.run.choice = snapshot.choice.clone();
        self.run.profile = snapshot.profile.clone();
        if let Some(profile) = &snapshot.profile {
            self.dj_browser.select_smart(&profile.id);
        }
        self.run.style = snapshot.style;
        self.run.target = snapshot.target;
        self.run.transitions = snapshot.transitions;
        self.run.crossfade = snapshot.crossfade;
        self.run.auto = false;
        self.auto = false;
        self.run.preloaded = None;
        self.library_open = snapshot.panels[1];
        self.dj_open = snapshot.panels[2];
        self.lab_open = snapshot.panels[3];
        self.cast_open = snapshot.panels[4];
        self.addons_open = snapshot.panels[5];
        self.layout.parent = snapshot.parent;
        self.dj_library_hidden = snapshot.hidden;
        self.eq_open = snapshot.eq_open;
        self.fx_open = snapshot.fx_open;
        self.recovery.sizes = snapshot.sizes;
        self.recovery.initial_sizes = snapshot.sizes;
        self.recovery.splits = snapshot.splits;
        for (key, value) in ["dj-wave-height", "dj-library-height", "dj-source-width"]
            .into_iter()
            .zip(snapshot.splits)
        {
            if let Some(value) = value {
                ctx.data_mut(|d| d.insert_persisted(egui::Id::unique(key), value));
            }
        }
        if snapshot.sizes[0].is_some() && !self.recovery.manually_resized {
            let size = self.recovered_window_size(0, ctx.input(|i| i.viewport().monitor_size));
            ctx.send_viewport_cmd_to(
                egui::ViewportId::ROOT,
                egui::ViewportCommand::InnerSize(egui::vec2(size[0], size[1])),
            );
        }
        self.player_mode = snapshot.mode;
        self.volume = snapshot.volume;
        self.fader = snapshot.fader;
        self.wave_span = snapshot.wave_span;
        if let Some(a) = &self.audio {
            a.set_fader(self.fader);
        }
        self.status = if snapshot.was_auto {
            "Session restored paused. Press Start Auto DJ to resume."
        } else {
            "Session restored paused."
        }
        .into();
        if errors.is_empty() && missing == 0 {
            self.recovery.error = None;
            self.recovery.pending = None;
        } else {
            self.recovery.error = Some(format!(
                "Recovery skipped {} missing queue entries and {} unavailable decks. Original checkpoint retained; reconnect storage and retry, or Start fresh.",
                missing,
                errors.len()
            ));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_recovery_does_not_reapply_window_size_after_manual_resize() {
        let dir = tempfile::tempdir().unwrap();
        let mut player = Player {
            root: dir.path().into(),
            ..Default::default()
        };
        let mut snapshot = player.session_snapshot();
        snapshot.sizes[0] = Some([1100., 900.]);
        player.recovery.pending = Some(snapshot);
        player.recovery.automatic_restore = true;
        let ctx = egui::Context::default();
        for size in [egui::vec2(1100., 900.), egui::vec2(850., 650.)] {
            let mut raw = egui::RawInput::default();
            raw.viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .inner_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size));
            let mut output = ctx.run_ui(raw, |ui| player.try_automatic_recovery(ui.ctx()));
            output.textures_delta.clear();
        }
        assert!(player.recovery.manually_resized);
        player.library = Some(Library {
            tracks: vec![],
            tree: Folder {
                name: "music".into(),
                folders: vec![],
                tracks: vec![],
            },
        });
        let mut output = ctx.run_ui(Default::default(), |ui| {
            player.try_automatic_recovery(ui.ctx())
        });
        assert!(player.recovery.attempted);
        assert!(
            !output
                .viewport_output
                .values()
                .flat_map(|v| &v.commands)
                .any(|c| matches!(c, egui::ViewportCommand::InnerSize(_)))
        );
        output.textures_delta.clear();
    }
    #[test]
    fn rejects_cycles_wrong_library_and_nonfinite_positions() {
        let player = Player {
            root: PathBuf::from("C:/music"),
            ..Default::default()
        };
        let mut s = player.session_snapshot();
        assert!(s.valid(&player.root));
        assert!(!s.valid(Path::new("C:/other")));
        s.parent[1] = Some(2);
        s.parent[2] = Some(1);
        assert!(!s.valid(&player.root));
        s.parent = [None; 6];
        s.decks[0] = Some(Deck {
            path: PathBuf::from("C:/music/song.mp3"),
            position: f64::NAN,
        });
        assert!(!s.valid(&player.root));
    }
    #[test]
    fn database_round_trip_preserves_paths_and_requests() {
        let dir = tempfile::tempdir().unwrap();
        let player = Player {
            root: dir.path().to_path_buf(),
            ..Default::default()
        };
        let mut s = player.session_snapshot();
        s.requests = vec!["requested-song".into()];
        s.queue = vec![dir.path().join("missing.mp3")];
        let file = dir.path().join("session.ampdb");
        let mut recovery = Recovery {
            file: Some(file.clone()),
            key: "session".into(),
            ..Default::default()
        };
        recovery.save(s.clone(), true);
        assert!(recovery.error.is_none());
        let value = amp_core::catalog_db::local_record(&file, "session")
            .unwrap()
            .unwrap();
        let loaded: Snapshot = serde_json::from_value(value).unwrap();
        assert_eq!(loaded.requests, s.requests);
        assert_eq!(loaded.queue, s.queue);
        assert!(loaded.valid(&player.root));
    }
    #[test]
    fn missing_tracks_remain_retryable_and_never_restart_automation() {
        let dir = tempfile::tempdir().unwrap();
        let mut player = Player {
            root: dir.path().to_path_buf(),
            library: Some(Library {
                tracks: vec![],
                tree: Folder {
                    name: String::new(),
                    folders: vec![],
                    tracks: vec![],
                },
            }),
            ..Default::default()
        };
        let mut saved = player.session_snapshot();
        saved.was_auto = true;
        saved.queue = vec![dir.path().join("missing.mp3")];
        saved.decks[0] = Some(Deck {
            path: dir.path().join("missing.mp3"),
            position: 12.,
        });
        saved.requests = vec!["keep-request".into()];
        player.recovery.pending = Some(saved);
        player.run.auto = true;
        player.auto = true;
        player.restore_session(&egui::Context::default());
        assert!(!player.run.auto && !player.auto);
        assert!(player.audio.is_none());
        assert_eq!(player.run.requests.front().unwrap(), "keep-request");
        assert!(player.recovery.pending.is_some());
        assert!(player.recovery.error.is_some());
        player.checkpoint(true);
        assert_eq!(player.recovery.pending.as_ref().unwrap().queue.len(), 1);
    }
    #[test]
    fn window_recovery_clamps_to_available_monitor() {
        let mut player = Player::default();
        player.recovery.initial_sizes[2] = Some([5000., 4000.]);
        assert_eq!(
            player.recovered_window_size(2, Some(egui::vec2(1280., 720.))),
            [1232., 640.]
        );
    }
    #[test]
    fn live_resize_does_not_change_viewport_builder_size() {
        let mut player = Player::default();
        player.recovery.initial_sizes[2] = Some([1100., 800.]);
        let monitor = Some(egui::vec2(2560., 1440.));
        let initial = player.recovered_window_size(2, monitor);
        let ctx = egui::Context::default();
        for (size, maximized, fullscreen) in [
            ([1300., 900.], false, false),
            ([2560., 1400.], true, false),
            ([2560., 1440.], false, true),
            ([1000., 700.], false, false),
        ] {
            let mut raw = egui::RawInput::default();
            raw.viewports.insert(
                window_layout::viewport(2),
                egui::ViewportInfo {
                    inner_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(size[0], size[1]),
                    )),
                    maximized: Some(maximized),
                    fullscreen: Some(fullscreen),
                    ..Default::default()
                },
            );
            let mut output = ctx.run_ui(raw, |ui| player.capture_session_layout(ui.ctx()));
            output.textures_delta.clear();
            assert_eq!(
                player.recovered_window_size(2, monitor),
                initial,
                "Live measurements must not emit another native resize"
            );
            if !maximized && !fullscreen {
                assert_eq!(player.recovery.sizes[2], Some(size));
            } else {
                assert_eq!(player.recovery.sizes[2], Some([1300., 900.]));
            }
        }
    }
}
