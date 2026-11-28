//! Device-local recovery. Never publishes shared metadata or starts audio.
use super::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
pub struct Deck {
    pub path: PathBuf,
    pub position: f64,
}
fn default_automatic_restore() -> bool {
    true
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
struct SideSnapshot {
    deck: Option<Deck>,
    config: side_deck::Config,
}
impl SideSnapshot {
    fn valid(&self) -> bool {
        self.config.volume.is_finite()
            && (0. ..=1.).contains(&self.config.volume)
            && self
                .deck
                .as_ref()
                .is_none_or(|d| d.path.is_absolute() && d.position.is_finite() && d.position >= 0.)
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    playback_owner: crate::playback_owner::Owner,
    #[serde(default)]
    player_sequence: crate::player_sequence::Sequence,
    #[serde(default)]
    side_decks: Option<[SideSnapshot; 2]>,
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
    #[serde(default = "default_automatic_restore")]
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
    source_probe: Option<mpsc::Receiver<Vec<String>>>,
    sources_checked: bool,
    probe_started: Option<std::time::Instant>,
    probe_wait_notice: bool,
    details: Vec<String>,
}
/// Check only saved deck inputs, never the queue or the complete music library.
/// Runs on a worker so a sleeping NAS cannot block window input.
fn probe_sources(root: &Path, paths: &[(String, PathBuf)]) -> Vec<String> {
    let mut errors = Vec::new();
    if let Ok(root) = root.canonicalize() {
        amp_core::storage::register(&root);
    }
    for (label, path) in paths {
        let result = (|| {
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                return Err("Saved input is not an audio file".into());
            }
            amp_core::preparation::load(path).map(|_| ())
        })();
        if let Err(error) = result {
            errors.push(format!("{label}: {error}"));
        }
    }
    errors
}
impl Snapshot {
    fn source_paths(&self) -> Vec<(String, PathBuf)> {
        let mut paths: Vec<_> = self
            .decks
            .iter()
            .enumerate()
            .filter_map(|(i, saved)| {
                saved
                    .as_ref()
                    .map(|deck| (format!("Deck {}", deck_rack::NAMES[i]), deck.path.clone()))
            })
            .collect();
        if let Some(sides) = &self.side_decks {
            for (i, side) in sides.iter().enumerate() {
                if let Some(deck) = &side.deck {
                    paths.push((format!("Cue deck {}", i + 1), deck.path.clone()));
                }
            }
        }
        paths
    }
}
impl Recovery {
    fn sources_ready(&mut self, ctx: &egui::Context) -> bool {
        if self.sources_checked {
            return true;
        }
        if let Some(rx) = &self.source_probe {
            match rx.try_recv() {
                Ok(details) => {
                    self.details.extend(details);
                    self.probe_started = None;
                    if self.probe_wait_notice {
                        self.error = None;
                        self.probe_wait_notice = false;
                    }
                    self.source_probe = None;
                    self.sources_checked = true;
                    return true;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    let elapsed = self.probe_started.map(|t| t.elapsed()).unwrap_or_default();
                    let deadline = Duration::from_secs(15);
                    if elapsed >= deadline {
                        if !self.probe_wait_notice {
                            self.error = Some("Saved music storage is taking longer to respond. Your checkpoint is kept; paused recovery will continue when it responds. You can start fresh instead.".into());
                            self.probe_wait_notice = true;
                        }
                    } else {
                        ctx.request_repaint_after(deadline - elapsed);
                    }
                    return false;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.source_probe = None;
                    self.attempted = true;
                    self.error = Some("Saved-track check was interrupted. Your checkpoint is kept; choose Restore session to retry.".into());
                    return false;
                }
            }
        }
        let Some(snapshot) = &self.pending else {
            return false;
        };
        let paths = snapshot.source_paths();
        if paths.is_empty() {
            self.sources_checked = true;
            return true;
        }
        let root = snapshot.root.clone();
        let (tx, rx) = mpsc::channel();
        self.source_probe = Some(rx);
        self.probe_started = Some(std::time::Instant::now());
        ctx.request_repaint_after(Duration::from_secs(15));
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(probe_sources(&root, &paths));
            ctx.request_repaint();
        });
        false
    }
    pub(super) fn needs_attention(&self) -> bool {
        self.error.is_some()
            || (self.pending.is_some() && (!self.automatic_restore || self.attempted))
    }

    pub(super) fn enabled(&self) -> bool {
        self.file.is_some()
    }
    pub fn open(root: &Path) -> Self {
        use sha2::{Digest, Sha256};
        let mut state = Self {
            automatic_restore: true,
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
        self.side_decks
            .as_ref()
            .is_none_or(|sides| sides.iter().all(SideSnapshot::valid))
            && self
                .deck_settings
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
        // Poll an existing check even if the user disables automatic recovery.
        // Keep a single worker: the OS may still be waiting for network I/O.
        if self.recovery.source_probe.is_some() {
            self.recovery.sources_ready(ctx);
        }
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
            && !self.run.auto
            && !self.auto
            && self
                .audio
                .as_ref()
                .is_none_or(|a| (0..6).all(|d| !a.playing(d)) && !a.fading())
            && self.recovery.sources_ready(ctx)
        {
            // Establish the output once before any deck loads. If this fails,
            // retain the actual device error instead of counting each deck as missing.
            let needs_audio = self
                .recovery
                .pending
                .as_ref()
                .is_some_and(|s| s.decks.iter().any(Option::is_some));
            if needs_audio && let Err(error) = self.ensure_audio() {
                self.recovery.details.push(format!("Audio output: {error}"));
                self.recovery.error = Some("The saved session is kept. Check your audio output, then choose Restore session.".into());
                self.recovery.attempted = true;
                return;
            }
            self.recovery.attempted = true;
            self.restore_session(ctx);
        }
    }
    pub(super) fn capture_session_layout(&mut self, ctx: &egui::Context) {
        if self.recovery.pending.is_some() {
            return;
        }
        for (pane, size) in self.recovery.sizes.iter_mut().enumerate() {
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
                *size = Some([rect.width(), rect.height()]);
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
            side_decks: Some(std::array::from_fn(|side| SideSnapshot {
                config: self.sides[side].config.clone(),
                deck: self.sides[side]
                    .track
                    .as_ref()
                    .and_then(|_| self.sides[side].config.path.as_ref())
                    .map(|path| Deck {
                        path: path.clone(),
                        position: self.sides[side].position().max(0.),
                    }),
            })),
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
            playback_owner: self.playback_owner,
            player_sequence: {
                let mut sequence = self.player_sequence.clone();
                if self.playback_owner == crate::playback_owner::Owner::Player {
                    sequence.resume_auto |= self.auto;
                }
                sequence
            },
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
            || self.sides.iter().any(side_deck::SideDeck::playing)
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
            if !self.recovery.details.is_empty() {
                ui.collapsing("Details", |ui| {
                    for detail in &self.recovery.details {
                        ui.weak(detail);
                    }
                });
            }
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
                    self.library.is_some() && !active && self.recovery.source_probe.is_none(),
                    egui::Button::new("Restore session"),
                )
                .clicked();
            discard = ui.button("Start fresh").clicked();
        });
        if discard {
            self.recovery.source_probe = None;
            self.recovery.details.clear();
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
                errors.push(format!("Deck {}: {e}", deck_rack::NAMES[deck]));
            }
        }
        if let Some(sides) = &snapshot.side_decks {
            for (side, saved) in sides.iter().enumerate() {
                self.sides[side].config = saved.config.clone();
                if let Some(deck) = &saved.deck {
                    // load and seek preserve a paused state; never resume cue audio on recovery.
                    if let Err(error) = self.sides[side]
                        .load(deck.path.clone())
                        .and_then(|_| self.sides[side].seek(deck.position))
                    {
                        errors.push(format!("Cue deck {}: {error}", side + 1));
                    }
                }
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
        self.playback_owner = snapshot.playback_owner;
        self.player_sequence = snapshot.player_sequence.clone();
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
            self.recovery.details.clear();
            self.recovery.source_probe = None;
            self.recovery.error = None;
            self.recovery.pending = None;
        } else {
            self.recovery.details.extend(errors.iter().cloned());
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
    fn recovery_prompt_waits_for_automatic_attempt_but_retains_real_errors() {
        let mut player = Player::default();
        player.recovery.pending = Some(player.session_snapshot());
        player.recovery.automatic_restore = true;
        assert!(!player.recovery.needs_attention());
        player.recovery.attempted = true;
        assert!(player.recovery.needs_attention());
        player.recovery.attempted = false;
        player.recovery.error = Some("Storage unavailable".into());
        assert!(player.recovery.needs_attention());
    }
    #[test]
    fn legacy_session_defaults_to_paused_recovery_and_keeps_explicit_opt_out() {
        let snapshot = Player::default().session_snapshot();
        let mut value = serde_json::to_value(snapshot).unwrap();
        value.as_object_mut().unwrap().remove("automatic_restore");
        assert!(
            serde_json::from_value::<Snapshot>(value.clone())
                .unwrap()
                .automatic_restore
        );
        value["automatic_restore"] = serde_json::json!(false);
        assert!(
            !serde_json::from_value::<Snapshot>(value)
                .unwrap()
                .automatic_restore
        );
    }

    #[test]
    fn cached_empty_recovery_does_not_wait_for_unrelated_full_refresh() {
        let dir = tempfile::tempdir().unwrap();
        let mut player = Player {
            root: dir.path().into(),
            library: Some(Library {
                tracks: vec![],
                tree: Folder {
                    name: "music".into(),
                    folders: vec![],
                    tracks: vec![],
                },
            }),
            ..Default::default()
        };
        player.recovery.pending = Some(player.session_snapshot());
        player.recovery.automatic_restore = true;
        let (_tx, rx) = std::sync::mpsc::channel();
        player.scan = Some(rx);
        player.try_automatic_recovery(&egui::Context::default());
        assert!(
            player.recovery.attempted,
            "a cached session must not wait for a full NAS refresh"
        );
        assert!(player.scan.is_some());
        assert!(player.recovery.pending.is_none());
        assert!(
            player.audio.is_none(),
            "an empty session must not open an audio output"
        );
    }

    #[test]
    fn active_automix_defers_delayed_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let mut player = Player {
            root: dir.path().into(),
            library: Some(Library {
                tracks: vec![],
                tree: Folder {
                    name: "music".into(),
                    folders: vec![],
                    tracks: vec![],
                },
            }),
            ..Default::default()
        };
        player.recovery.pending = Some(player.session_snapshot());
        player.recovery.automatic_restore = true;
        player.run.auto = true;
        player.try_automatic_recovery(&egui::Context::default());
        assert!(!player.recovery.attempted && player.recovery.pending.is_some());
        player.run.auto = false;
        player.auto = true;
        player.try_automatic_recovery(&egui::Context::default());
        assert!(!player.recovery.attempted && player.recovery.pending.is_some());
        assert!(!player.recovery.needs_attention());
    }
    #[test]
    fn recovery_probe_is_bounded_to_loaded_inputs_and_preserves_real_failures() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("track.mp3");
        std::fs::write(&audio, b"fixture, never decoded by the source check").unwrap();
        let corrupt = amp_core::sidecar(&audio);
        std::fs::write(&corrupt, b"not json").unwrap();
        let player = Player {
            root: dir.path().into(),
            ..Default::default()
        };
        let mut snapshot = player.session_snapshot();
        snapshot.decks[0] = Some(Deck {
            path: audio.clone(),
            position: 0.,
        });
        snapshot.decks[3] = Some(Deck {
            path: dir.path().join("missing.mp3"),
            position: 0.,
        });
        snapshot.queue = vec![dir.path().join("do-not-probe-queued-song.mp3"); 1000];
        let paths = snapshot.source_paths();
        assert_eq!(paths.len(), 2);
        let errors = probe_sources(dir.path(), &paths);
        assert_eq!(errors.len(), 2);
        assert!(errors[0].starts_with("Deck A1:"));
        assert!(errors[1].starts_with("Deck B2:"));
        assert_eq!(
            std::fs::read(&audio).unwrap(),
            b"fixture, never decoded by the source check"
        );
        assert_eq!(std::fs::read(&corrupt).unwrap(), b"not json");
    }
    #[test]
    fn recovery_probe_completion_is_independent_of_the_full_scan() {
        let (tx, rx) = mpsc::channel();
        let mut recovery = Recovery {
            source_probe: Some(rx),
            ..Default::default()
        };
        assert!(!recovery.sources_ready(&egui::Context::default()));
        tx.send(vec!["Deck A1: source is offline".into()]).unwrap();
        assert!(recovery.sources_ready(&egui::Context::default()));
        assert_eq!(recovery.details, ["Deck A1: source is offline"]);
        assert!(recovery.sources_checked);
        assert!(!recovery.attempted);
    }
    #[test]
    fn probe_reads_remain_valid_during_portable_catalog_writes() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("source.mp3");
        let original = b"source bytes stay unchanged";
        std::fs::write(&audio, original).unwrap();
        let prep = amp_core::Preparation::default();
        amp_core::preparation::save(dir.path(), Path::new("source.mp3"), &prep).unwrap();
        amp_core::catalog_db::ensure(dir.path()).unwrap();
        assert!(!amp_core::preparation::portable(&audio).is_file());
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let worker_barrier = barrier.clone();
        let root = dir.path().to_path_buf();
        let writer = std::thread::spawn(move || {
            worker_barrier.wait();
            for i in 0..32 {
                let path = root.join("source.mp3");
                let (mut prep, revision) =
                    amp_core::preparation::load_with_revision(&path).unwrap();
                prep.extensions
                    .insert("probe_test".into(), serde_json::json!(i));
                amp_core::preparation::save_if_unchanged(
                    &root,
                    Path::new("source.mp3"),
                    &prep,
                    revision.as_deref(),
                )
                .unwrap();
            }
        });
        barrier.wait();
        for _ in 0..64 {
            assert!(probe_sources(dir.path(), &[("Deck A1".into(), audio.clone())]).is_empty());
        }
        writer.join().unwrap();
        assert_eq!(
            amp_core::preparation::load(&audio).unwrap().extensions["probe_test"],
            31
        );
        assert_eq!(std::fs::read(audio).unwrap(), original);
    }
    #[test]
    fn slow_probe_reports_wait_without_starting_another_worker() {
        let (tx, rx) = mpsc::channel();
        let mut recovery = Recovery {
            source_probe: Some(rx),
            probe_started: Some(std::time::Instant::now() - Duration::from_secs(16)),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        assert!(!recovery.sources_ready(&ctx));
        assert!(recovery.probe_wait_notice && recovery.error.is_some());
        assert!(recovery.source_probe.is_some());
        assert!(!recovery.sources_ready(&ctx));
        tx.send(Vec::new()).unwrap();
        assert!(recovery.sources_ready(&ctx));
        assert!(recovery.source_probe.is_none());
        assert!(recovery.error.is_none());
        assert!(!recovery.probe_wait_notice);
    }
    #[test]
    fn disconnected_probe_keeps_the_checkpoint_retryable() {
        let (tx, rx) = mpsc::channel();
        drop(tx);
        let mut recovery = Recovery {
            source_probe: Some(rx),
            ..Default::default()
        };
        assert!(!recovery.sources_ready(&egui::Context::default()));
        assert!(recovery.attempted && recovery.error.is_some());
    }
    #[test]
    #[ignore = "requires local audio fixture and native output; recovery remains silent"]
    fn auxiliary_cue_positions_restore_paused_without_changing_preparation() {
        let path = PathBuf::from(std::env::var_os("AMP_WAVEFORM_FIXTURE").expect("fixture"))
            .canonicalize()
            .unwrap();
        let root = path.parent().unwrap().to_path_buf();
        let before = amp_core::preparation::load(&path).unwrap();
        let mut original = Player {
            root: root.clone(),
            library: Some(amp_core::scan(&root).unwrap()),
            ..Default::default()
        };
        original.sides[0].poll(0.);
        original.sides[0].load(path.clone()).unwrap();
        original.sides[0].seek(2.).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let snapshot = original.session_snapshot();
        let saved = snapshot.side_decks.as_ref().unwrap()[0]
            .deck
            .as_ref()
            .unwrap()
            .position;
        let mut restored = Player {
            root: root.clone(),
            library: Some(amp_core::scan(&root).unwrap()),
            ..Default::default()
        };
        restored.sides[0].poll(0.);
        restored.recovery.pending = Some(snapshot);
        restored.restore_session(&egui::Context::default());
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(
            restored.recovery.error.is_none(),
            "{:?}",
            restored.recovery.error
        );
        assert!(!restored.sides[0].playing());
        assert!((restored.sides[0].position() - saved).abs() < 0.05);
        assert!(!restored.run.auto);
        assert_eq!(amp_core::preparation::load(&path).unwrap(), before);
    }
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
        s.playback_owner = crate::playback_owner::Owner::Dj;
        s.side_decks.as_mut().unwrap()[1].deck = Some(Deck {
            path: dir.path().join("cue.wav"),
            position: 23.5,
        });
        s.side_decks.as_mut().unwrap()[1].config.volume = 0.42;
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
        assert_eq!(loaded.playback_owner, crate::playback_owner::Owner::Dj);
        assert_eq!(loaded.queue, s.queue);
        assert_eq!(
            loaded.side_decks.as_ref().unwrap()[1]
                .deck
                .as_ref()
                .unwrap()
                .position,
            23.5
        );
        assert_eq!(loaded.side_decks.as_ref().unwrap()[1].config.volume, 0.42);
        let mut invalid = loaded.clone();
        invalid.side_decks.as_mut().unwrap()[0].config.volume = f32::NAN;
        assert!(!invalid.valid(&player.root));
        let mut legacy = serde_json::to_value(&s).unwrap();
        legacy.as_object_mut().unwrap().remove("side_decks");
        assert!(
            serde_json::from_value::<Snapshot>(legacy)
                .unwrap()
                .valid(&player.root)
        );
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
