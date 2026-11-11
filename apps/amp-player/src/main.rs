#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use amp_core::{Folder, Library, Playlist};
use eframe::egui::{self, RichText};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};
mod addons;
mod cast;
mod control;
mod crossfade_settings;
mod dj_browser;
mod dj_mixer;
mod docking;
mod dragdrop;
mod engine;
mod engine_download;
mod google_cast;
mod hot_cues;
mod icons;
mod lab;
mod lab_editor;
mod library_cache;
mod live_mix;
mod mixer;
mod monitor;
mod organized_library;
mod performance;
mod preparer;
mod sample_editor;
mod sampler;
mod show_editor;
mod side_deck;
mod skin;
mod smart_actions;
mod smart_preview;
mod station_download;
mod stems;
mod theme;
mod transition_styles;
mod waveform;
mod window_layout;
use skin::{CYAN, GOLD, ORANGE};

fn player_on_top_path() -> Option<PathBuf> {
    theme::settings_path().map(|path| path.with_file_name("player-on-top.txt"))
}
fn load_player_on_top() -> bool {
    player_on_top_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .is_some_and(|value| value.trim() == "true")
}
fn save_player_on_top(enabled: bool) -> std::io::Result<()> {
    use std::io::Write;
    let path =
        player_on_top_path().ok_or_else(|| std::io::Error::other("Settings folder unavailable"))?;
    let directory = path
        .parent()
        .ok_or_else(|| std::io::Error::other("Settings folder unavailable"))?;
    std::fs::create_dir_all(directory)?;
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(if enabled { b"true" } else { b"false" })?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn library_preference() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".config")
        })
        .join("amp-verse/music-root.txt")
}
fn app_icon() -> std::sync::Arc<egui::IconData> {
    static ICON: std::sync::OnceLock<std::sync::Arc<egui::IconData>> = std::sync::OnceLock::new();
    ICON.get_or_init(|| {
        std::sync::Arc::new(
            eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/amp-verse-icon.png"))
                .expect("Bundled AMP Verse icon must be valid PNG"),
        )
    })
    .clone()
}

fn capture(ctx: &egui::Context, path: PathBuf) {
    use eframe::icon_data::IconDataExt;
    let context = ctx.clone();
    ctx.request_screenshot(move |image| {
        let icon = egui::IconData {
            width: image.size[0] as u32,
            height: image.size[1] as u32,
            rgba: image.pixels.iter().flat_map(|c| c.to_array()).collect(),
        };
        if let Ok(bytes) = icon.to_png_bytes() {
            let _ = std::fs::write(path, bytes);
        }
        context.send_viewport_cmd(egui::ViewportCommand::Close);
    });
}

struct Player {
    root: PathBuf,
    library: Option<Library>,
    scan: Option<mpsc::Receiver<Result<library_cache::Snapshot, String>>>,
    last_scan: std::time::Instant,
    selected: Option<usize>,
    folder: PathBuf,
    search: String,
    audio: Option<mixer::Engine>,
    sampler: sampler::Sampler,
    live_mix: Option<live_mix::Broadcast>,
    right_sampler: sampler::Sampler,
    right_sample_audio: Option<mixer::Engine>,
    sides: [side_deck::SideDeck; 2],
    pending_load: [Option<(PathBuf, Option<f64>)>; 2],
    loaded_origin: [Option<PathBuf>; 2],
    dj_browser: dj_browser::Browser,
    dj_tab: usize,
    dj_cues: [bool; 2],
    player_mode: u8,
    player_on_top: bool,
    run: performance::Run,
    show_editor: show_editor::Editor,
    smart_preview: smart_preview::Preview,
    control: Option<control::Control>,
    control_attempted: bool,
    control_restart: Option<std::time::Instant>,
    control_status: String,
    control_bind: String,
    control_password: String,
    catalog_updated: std::time::Instant,
    last_master: u8,
    waveforms: [waveform::Lane; 2],
    wave_span: f64,
    pitch: [f32; 2],
    channel_gain: [f32; 2],
    fader: f32,
    fade_seconds: f32,
    jog_resume: [Option<(bool, egui::ViewportId)>; 2],
    jog_angle: [f32; 2],
    mixer_panel: dj_mixer::Panel,
    stem_panel: stems::Panel,
    eq_open: bool,
    fx_open: bool,
    eq: [[f32; 3]; 2],
    filter: [f32; 2],
    loaded: [Option<usize>; 2],
    loaded_snapshot: [Option<amp_core::Track>; 2],
    playing: [bool; 2],
    volume: u8,
    new_playlist_name: Option<String>,
    playlist_snapshot: Option<(PathBuf, Playlist)>,
    queue: Vec<usize>,
    queue_selected: Option<usize>,
    auto: bool,
    shuffle: bool,
    repeat_one: bool,
    library_open: bool,
    library_page: usize,
    playback_history: Vec<(usize, String)>,
    layout: window_layout::Layout,
    preview_wave: waveform::Lane,
    dj_open: bool,
    lab_open: bool,
    lab: lab::Arrangement,
    preparer: preparer::Preparer,
    addons_open: bool,
    cast_open: bool,
    cast: cast::Dashboard,
    addons: addons::Manager,
    status: String,
    screenshot_frames: u32,
    dock_frames: u32,
    screenshot_view: String,
}
impl Default for Player {
    fn default() -> Self {
        Self {
            root: std::env::args_os()
                .nth(1)
                .map(PathBuf::from)
                .or_else(|| {
                    std::fs::read_to_string(library_preference())
                        .ok()
                        .map(|s| PathBuf::from(s.trim()))
                })
                .unwrap_or_default(),
            library: None,
            scan: None,
            last_scan: std::time::Instant::now(),
            selected: None,
            folder: PathBuf::new(),
            search: String::new(),
            audio: None,
            sampler: Default::default(),
            live_mix: None,
            right_sampler: sampler::Sampler::separate_bank("sampler-right.json"),
            right_sample_audio: None,
            sides: [side_deck::SideDeck::new(0), side_deck::SideDeck::new(1)],
            pending_load: Default::default(),
            loaded_origin: Default::default(),
            dj_browser: Default::default(),
            dj_tab: 0,
            dj_cues: [false; 2],
            player_mode: 0,
            player_on_top: load_player_on_top(),
            run: Default::default(),
            show_editor: Default::default(),
            smart_preview: Default::default(),
            control: None,
            control_attempted: false,
            control_restart: None,
            control_status: "Show control starting…".into(),
            control_bind: std::env::var("AMP_CONTROL_BIND")
                .unwrap_or_else(|_| "127.0.0.1:8790".into()),
            control_password: std::env::var("AMP_CONTROL_PASSWORD").unwrap_or_default(),
            catalog_updated: std::time::Instant::now() - std::time::Duration::from_secs(3),
            last_master: 75,
            waveforms: Default::default(),
            wave_span: 16.,
            pitch: [0.; 2],
            channel_gain: [1.; 2],
            fader: 0.,
            fade_seconds: crossfade_settings::load(),
            jog_resume: [None; 2],
            jog_angle: [0.; 2],
            mixer_panel: Default::default(),
            stem_panel: Default::default(),
            eq_open: false,
            fx_open: false,
            eq: [[0.; 3]; 2],
            filter: [22000.; 2],
            loaded: [None; 2],
            loaded_snapshot: [None, None],
            playing: [false; 2],
            volume: 75,
            new_playlist_name: None,
            playlist_snapshot: None,
            queue: vec![],
            queue_selected: None,
            auto: false,
            shuffle: false,
            repeat_one: false,
            layout: if std::env::var_os("AMP_DOCK_SMOKE").is_some() {
                window_layout::Layout { parent: [None; 6] }
            } else {
                Default::default()
            },
            preview_wave: Default::default(),
            library_page: 0,
            playback_history: Vec::new(),
            library_open: std::env::var_os("AMP_SCREENSHOT_VIEW").is_none()
                || std::env::var_os("AMP_DOCK_SMOKE").is_some()
                || std::env::var("AMP_SCREENSHOT_VIEW").is_ok_and(|s| s == "combined"),
            dj_open: false,
            lab_open: false,
            lab: lab::Arrangement::default(),
            preparer: preparer::Preparer::default(),
            addons_open: false,
            cast_open: false,
            cast: cast::Dashboard::default(),
            addons: addons::Manager::default(),
            status: "Choose your music folder in Library".into(),
            screenshot_frames: 0,
            dock_frames: 0,
            screenshot_view: std::env::var("AMP_SCREENSHOT_VIEW").unwrap_or_default(),
        }
    }
}
impl Player {
    fn header(&mut self, ui: &mut egui::Ui, title: &str, purple: bool) {
        let action = skin::heading(ui, title, purple);
        self.activate(ui, action);
    }
    fn activate(&mut self, ui: &mut egui::Ui, module: Option<skin::Module>) {
        let pane = match &module {
            Some(skin::Module::Library) => Some(1),
            Some(skin::Module::Dj) => Some(2),
            Some(skin::Module::LabSetup) => Some(3),
            Some(skin::Module::Cast) => Some(4),
            Some(skin::Module::Addons) => Some(5),
            _ => None,
        };
        if pane.is_some() && self.player_mode != 0 {
            self.set_player_mode(ui.ctx(), 0);
        }
        if let Some(pane) = pane {
            let mut host = pane;
            let mut hidden = false;
            while let Some(parent) = self.layout.parent[host] {
                host = parent;
                hidden |= !self.open_panes()[host];
            }
            if hidden {
                self.layout.parent[pane] = Some(0);
                self.resize_player(ui.ctx(), true);
            } else if host == 0 {
                self.resize_player(ui.ctx(), true);
            }
        }
        match module {
            Some(skin::Module::Verse) => ui
                .ctx()
                .send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus),
            Some(skin::Module::Library) => {
                self.library_open = true;
                if self.layout.parent[1] == Some(0) {
                    ui.ctx().send_viewport_cmd_to(
                        egui::ViewportId::ROOT,
                        egui::ViewportCommand::MinInnerSize(egui::vec2(740., 480.)),
                    );
                    ui.ctx().send_viewport_cmd_to(
                        egui::ViewportId::ROOT,
                        egui::ViewportCommand::InnerSize(egui::vec2(1100., 900.)),
                    );
                } else {
                    ui.ctx().send_viewport_cmd_to(
                        egui::ViewportId::from_hash_of("amp-library"),
                        egui::ViewportCommand::Focus,
                    );
                }
            }
            Some(skin::Module::Dj) => {
                self.dj_open = true;
                ui.ctx().send_viewport_cmd_to(
                    egui::ViewportId::from_hash_of("amp-dj"),
                    egui::ViewportCommand::Focus,
                );
            }
            Some(skin::Module::Cast) => {
                self.cast_open = true;
                ui.ctx().send_viewport_cmd_to(
                    egui::ViewportId::from_hash_of("amp-cast"),
                    egui::ViewportCommand::Focus,
                );
            }
            Some(skin::Module::Addons) => {
                self.addons_open = true;
                ui.ctx().send_viewport_cmd_to(
                    egui::ViewportId::from_hash_of("amp-addons"),
                    egui::ViewportCommand::Focus,
                );
            }
            Some(skin::Module::LabSetup) => {
                self.lab_open = true;
            }
            None => (),
        }
        if pane.is_some() && self.player_mode != 0 {
            self.set_player_mode(ui.ctx(), 0);
        }
        if let Some(pane) = pane {
            let mut host = pane;
            while let Some(parent) = self.layout.parent[host] {
                host = parent;
            }
            ui.ctx()
                .send_viewport_cmd_to(window_layout::viewport(host), egui::ViewportCommand::Focus);
        }
    }
    fn scan(&mut self) {
        if self.scan.is_some() {
            return;
        }
        if self.library.is_none()
            && let Some(snapshot) = library_cache::load(&self.root)
        {
            self.accept_scan(snapshot);
            self.status = "Saved library ready; checking for new tracks in background…".into();
        } else {
            self.status = if self.library.is_some() {
                "Library ready; checking for new tracks in background…"
            } else {
                "Scanning music folders…"
            }
            .into();
        }
        let root = self.root.clone();
        let (tx, rx) = mpsc::channel();
        self.scan = Some(rx);
        self.last_scan = std::time::Instant::now();
        std::thread::spawn(move || {
            let result = library_cache::scan(&root);
            if let Ok(snapshot) = &result {
                let _ = library_cache::save(&root, snapshot);
            }
            let _ = tx.send(result);
        });
    }
    fn accept_scan(&mut self, snapshot: library_cache::Snapshot) {
        self.accept_library_scan(snapshot.library);
        if let Some(settings) = snapshot.settings {
            self.run.settings = settings;
        }
        for (path, playlist) in snapshot.playlists {
            if let Some((_, current)) = self
                .dj_browser
                .playlists
                .iter_mut()
                .find(|(p, _)| *p == path)
            {
                *current = playlist;
            } else {
                self.dj_browser.playlists.push((path, playlist));
            }
        }
    }
    fn accept_library_scan(&mut self, lib: Library) {
        let lookup: std::collections::HashMap<_, _> = lib
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.path.clone(), i))
            .collect();
        let remap = |index: usize| {
            self.library
                .as_ref()
                .and_then(|old| old.tracks.get(index))
                .and_then(|t| lookup.get(&t.path))
                .copied()
        };
        let loaded = self.loaded.map(|i| i.and_then(remap));
        let selected = self.selected.and_then(remap);
        let queue = self.queue.iter().filter_map(|&i| remap(i)).collect();
        let queue_selected = self
            .queue_selected
            .filter(|&row| row < self.queue.len())
            .and_then(|row| {
                remap(self.queue[row]).map(|_| {
                    self.queue[..row]
                        .iter()
                        .filter(|&&i| remap(i).is_some())
                        .count()
                })
            });
        let preloaded = self
            .run
            .preloaded
            .and_then(|(d, i)| remap(i).map(|i| (d, i)));
        self.loaded = loaded;
        self.selected = selected;
        self.queue = queue;
        self.queue_selected = queue_selected;
        self.run.preloaded = preloaded;
        self.run.future_candidates = self
            .run
            .future_candidates
            .iter()
            .filter_map(|&index| remap(index))
            .collect();
        if self
            .run
            .pending
            .is_some_and(|(d, _)| self.loaded[d].is_none())
        {
            self.run.pending = None;
            self.run.pending_at = None;
        }
        self.status = format!(
            "{} tracks / library refreshed; playback retained",
            lib.tracks.len()
        );
        self.library = Some(lib);
    }
    fn auxiliary_playing(&self) -> bool {
        self.sides.iter().any(side_deck::SideDeck::playing)
            || self.right_sample_audio.as_ref().is_some_and(|audio| {
                audio
                    .controls
                    .sample_playing
                    .iter()
                    .any(|voice| voice.load(std::sync::atomic::Ordering::Relaxed))
            })
    }
    fn poll(&mut self) {
        self.preparer.guard_preview(
            !self.library_open
                || self.run.auto
                || self.run.locked
                || self.smart_preview.active()
                || self.auxiliary_playing()
                || self
                    .audio
                    .as_ref()
                    .is_some_and(|a| a.playing(0) || a.playing(1)),
        );
        if let Some(sample) = self.lab.take_sample() {
            let result = self.ensure_audio();
            if let Err(error) = result {
                self.status = error;
            }
            if sample.right {
                if let Err(error) = self.ensure_right_samples() {
                    self.status = error;
                }
                self.right_sampler.import_clip(sample);
            } else {
                self.sampler.import_clip(sample);
            }
        }

        if !self.root.as_os_str().is_empty()
            && self.scan.is_none()
            && self.last_scan.elapsed() >= Duration::from_secs(120)
        {
            self.scan();
        }
        if self.smart_preview.active()
            && (self.auxiliary_playing()
                || self.run.locked
                || self.run.auto
                || self.audio.as_ref().is_some_and(|audio| {
                    audio.playing(0)
                        || audio.playing(1)
                        || audio
                            .controls
                            .sample_playing
                            .iter()
                            .any(|voice| voice.load(std::sync::atomic::Ordering::Relaxed))
                }))
        {
            self.smart_preview.stop();
        }
        self.smart_preview.poll();
        for side in &mut self.sides {
            side.poll(self.volume as f32 / 100.);
        }
        if let Some(audio) = &mut self.right_sample_audio {
            audio.set_master(self.volume as f32 / 100.);
            self.right_sampler.poll(audio);
        }
        if let Some(engine) = &mut self.audio {
            self.sampler.poll(engine);
        }
        self.preparer.poll(&self.root, self.library.as_mut());
        if let Some(rx) = &self.scan {
            match rx.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(lib) => {
                            self.accept_scan(lib);
                            self.catalog_updated =
                                std::time::Instant::now() - std::time::Duration::from_secs(3);

                            if let Some(path) = std::env::var_os("AMP_SAMPLER_SMOKE") {
                                self.volume = 0;
                                if self.ensure_audio().is_ok() {
                                    self.sampler.started = true;
                                    self.sampler.request_load(0, PathBuf::from(path));
                                }
                            }

                            if std::env::var_os("AMP_WAVEFORM_SMOKE").is_some() {
                                self.selected = Some(0);
                                self.volume = 0;
                                self.load_deck(0, 0, false);
                                self.load_deck(1, 0, false);
                            }

                            if self.screenshot_view == "preparation"
                                && let Some(t) =
                                    self.library.as_ref().and_then(|l| l.tracks.first())
                            {
                                self.preparer.edit(&self.root, t);
                            }
                        }
                        Err(e) => self.status = e,
                    }
                    self.scan = None;
                    self.last_scan = std::time::Instant::now();
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.scan = None;
                    self.last_scan = std::time::Instant::now();
                    self.status = "Background scan interrupted; saved library retained".into();
                }
                Err(mpsc::TryRecvError::Empty) => (),
            }
        }
        if let Some(audio) = &self.audio {
            self.playing = [audio.playing(0), audio.playing(1)];
            self.fader = audio.fader();
            if self.volume != self.last_master {
                audio.set_master(self.volume as f32 / 100.);
            } else {
                self.volume = (audio.master_level() * 100.).round().clamp(0., 100.) as u8;
            }
            self.last_master = self.volume;
            self.channel_gain = [audio.channel_level(0), audio.channel_level(1)];
            if let Some(error) = audio.take_error() {
                self.run.cancel_automation();
                self.status = error;
            }
        } else {
            self.playing = [false; 2];
        }
        let decode_errors = self
            .audio
            .as_ref()
            .map(|audio| audio.take_decode_errors())
            .unwrap_or_default();
        for (deck, error) in decode_errors {
            self.recover_failed_song(deck, &error);
        }
        if self.auto
            && (self.loaded[0].is_none()
                || self.audio.as_ref().is_some_and(|a| {
                    a.controls.finished[0].load(std::sync::atomic::Ordering::Relaxed)
                }))
        {
            self.next();
        }
    }
    fn ensure_audio(&mut self) -> Result<(), String> {
        if self.audio.is_none() {
            let audio = mixer::Engine::new()?;
            audio.set_master(self.volume as f32 / 100.);
            for d in 0..2 {
                audio.set_gain(d, self.channel_gain[d]);
                audio.set_rate(d, 1. + self.pitch[d] / 100.);
                for b in 0..3 {
                    audio.set_eq(d, b, self.eq[d][b]);
                }
                audio.set_filter(d, self.filter[d]);
            }
            self.mixer_panel.apply(&audio);
            self.audio = Some(audio);
        }
        Ok(())
    }
    fn load_deck(&mut self, deck: usize, index: usize, play: bool) {
        self.run.cancel_automation();
        if deck == 0 && !play {
            self.auto = false;
        }
        if let Some(_track) = self
            .library
            .as_ref()
            .and_then(|l| l.tracks.get(index))
            .cloned()
        {
            let track = match self.fresh_track(index) {
                Ok(track) => track,
                Err(e) => {
                    self.status = e;
                    return;
                }
            };
            let result = self.ensure_audio().and_then(|_| {
                self.audio
                    .as_mut()
                    .unwrap()
                    .load(deck, &self.root, &track, play)
            });
            if result.is_ok() {
                self.loaded_origin[deck] = Some(self.root.clone());
            }
            match result {
                Ok(()) => {
                    if let Some(rate) = track
                        .preparation
                        .extensions
                        .get("playback_rate")
                        .and_then(|v| v.as_f64())
                    {
                        self.pitch[deck] = ((rate as f32 - 1.) * 100.).clamp(-16., 16.);
                        if let Some(a) = &self.audio {
                            a.set_rate(deck, 1. + self.pitch[deck] / 100.);
                        }
                    }
                    if !play {
                        self.run.preloaded = Some((deck, index));
                    }
                    if play {
                        self.playback_history.push((deck, track.title.clone()));
                        if self.playback_history.len() > 1000 {
                            self.playback_history.remove(0);
                        }
                    }
                    self.loaded_snapshot[deck] = Some(track.clone());
                    self.loaded_origin[deck] = Some(self.root.clone());
                    self.loaded[deck] = Some(index);
                    self.playing[deck] = play;
                    self.status = format!("Deck {} / {}", deck + 1, track.title);
                }
                Err(e) => self.status = e,
            }
        }
    }
    fn play(&mut self, deck: usize, index: usize) {
        self.load_deck(deck, index, true);
    }
    fn play_cue(&mut self, deck: usize, index: usize, seconds: f64) {
        self.run.cancel_automation();
        if let Some(_track) = self
            .library
            .as_ref()
            .and_then(|l| l.tracks.get(index))
            .cloned()
        {
            let mut track = match self.fresh_track(index) {
                Ok(track) => track,
                Err(e) => {
                    self.status = e;
                    return;
                }
            };
            track.preparation.cue_in = seconds;
            if track.preparation.cue_out.is_some_and(|out| out <= seconds) {
                return;
            }
            let result = self.ensure_audio().and_then(|_| {
                self.audio
                    .as_mut()
                    .unwrap()
                    .load(deck, &self.root, &track, true)
            });
            match result {
                Ok(()) => {
                    self.loaded_snapshot[deck] = Some(track.clone());
                    self.loaded_origin[deck] = Some(self.root.clone());
                    self.loaded[deck] = Some(index);
                    self.status = format!("Deck {} cue {:.2}s", deck + 1, seconds);
                }
                Err(e) => self.status = e,
            }
        }
    }
    fn stop(&mut self, deck: usize) {
        self.run.cancel_automation();
        if let Some(audio) = &mut self.audio {
            audio.stop(deck);
        }
        self.playing[deck] = false;
        if deck == 0 {
            self.auto = false;
        }
        self.status = format!("Deck {} stopped", deck + 1);
    }
    fn toggle_deck(&mut self, deck: usize) {
        self.run.cancel_automation();
        if let Some(audio) = &self.audio
            && audio.loaded(deck)
            && !audio.controls.finished[deck].load(std::sync::atomic::Ordering::Relaxed)
        {
            audio.toggle(deck);
        } else if let Some(track) = self.loaded_snapshot[deck].clone() {
            let root = self.loaded_origin[deck].as_deref().unwrap_or(&self.root);
            if let Some(audio) = &mut self.audio
                && let Err(error) = audio.load(deck, root, &track, true)
            {
                self.status = error;
            }
        } else if let Some(i) = self.loaded[deck].or(self.selected) {
            self.play(deck, i);
        }
    }
    fn sync_deck(&mut self, deck: usize) {
        let bpm = |d: usize| self.loaded_track(d).and_then(|t| t.preparation.bpm);
        if let (Some(own), Some(other)) = (bpm(deck), bpm(1 - deck)) {
            let target = other * (1. + self.pitch[1 - deck] as f64 / 100.);
            let requested = ((target / own - 1.) * 100.) as f32;
            self.pitch[deck] = requested.clamp(-16., 16.);
            if let Some(audio) = &self.audio {
                audio.set_rate(deck, 1. + self.pitch[deck] / 100.);
            }
            self.status = if requested.abs() > 16. {
                "Tempo request exceeds pitch range; limited to +/-16%."
            } else {
                "Tempo matched. Beat phase and key lock are not implemented."
            }
            .into();
        } else {
            self.status = "Prepare both tracks with BPM metadata before tempo matching.".into();
        }
    }
    fn auto_crossfade(&mut self) {
        if self.run.profile.is_some() {
            if let Err(error) = self.execute_control(control::Action::MixNext { immediate: true }) {
                self.status = error;
            }
            return;
        }
        if self.audio.as_ref().is_some_and(|a| a.fading()) {
            self.run.cancel_automation();
        }
        self.auto = false;
        if let Some(audio) = &self.audio {
            let cancel = audio.fading();
            match audio.crossfade(self.fade_seconds) {
                Ok(()) => {
                    self.status = if cancel {
                        "Auto crossfade canceled"
                    } else {
                        "Auto crossfade: incoming deck starts when buffered"
                    }
                    .into()
                }
                Err(e) => self.status = e,
            }
        } else {
            self.status = "Load both decks and start one before Auto Crossfade.".into();
        }
    }
    fn next(&mut self) {
        if self.repeat_one
            && let Some(i) = self.loaded[0]
        {
            self.play(0, i);
            return;
        }
        if !self.queue.is_empty() {
            let at = if self.shuffle {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() as usize
                    % self.queue.len()
            } else {
                0
            };
            let index = self.queue.remove(at);
            self.queue_selected = None;
            self.play(0, index);
        } else {
            self.auto = false;
            self.status = "Playlist finished".into();
        }
    }
    fn fresh_track(&self, index: usize) -> Result<amp_core::Track, String> {
        let mut track = self
            .library
            .as_ref()
            .and_then(|l| l.tracks.get(index))
            .cloned()
            .ok_or("Song unavailable")?;
        let audio = amp_core::resolve(&self.root, &track.path).map_err(|e| e.to_string())?;
        track.preparation = amp_core::preparation::load(&audio)?;
        if let Some(title) = track
            .preparation
            .extensions
            .get("title")
            .and_then(|v| v.as_str())
        {
            track.title = title.into();
        }
        Ok(track)
    }
    fn loaded_track(&self, deck: usize) -> Option<&amp_core::Track> {
        self.loaded_snapshot[deck]
            .as_ref()
            .or_else(|| self.loaded[deck].and_then(|i| self.library.as_ref()?.tracks.get(i)))
    }
    fn title(&self, deck: usize) -> &str {
        self.loaded_track(deck)
            .map(|t| t.title.as_str())
            .unwrap_or("NO TRACK LOADED")
    }
    fn choose_folder(&mut self) {
        if let Some(root) = rfd::FileDialog::new().pick_folder() {
            if let Some(audio) = &mut self.audio {
                audio.stop(0);
                audio.stop(1);
            }
            self.auto = false;
            self.loaded = [None; 2];
            self.loaded_snapshot = [None, None];
            self.waveforms = Default::default();
            self.preview_wave = Default::default();
            self.run.cancel_automation();
            self.playing = [false; 2];
            self.selected = None;
            self.queue.clear();
            self.queue_selected = None;
            self.library = None;
            self.show_editor.open = false;
            self.scan = None; // Discard any previous root's in-flight result.
            self.root = root;
            let pref = library_preference();
            if let Some(parent) = pref.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(pref, self.root.to_string_lossy().as_bytes());
            self.scan();
        }
    }
    fn create_playlist_at(&mut self, path: PathBuf, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Enter a playlist name".into());
        }
        if path.exists() {
            return Err("A playlist already exists there. Choose a different filename.".into());
        }
        let list = Playlist {
            sync: None,
            version: 1,
            name: name.into(),
            tracks: Vec::new(),
        };
        list.save_in_library(&path, &self.root)
            .map_err(|error| error.to_string())?;
        let saved = Playlist::load(&path, &self.root).map_err(|error| error.to_string())?;
        self.queue.clear();
        self.queue_selected = None;
        self.playlist_snapshot = Some((path.clone(), saved.clone()));
        self.dj_browser.playlists.push((path, saved));
        self.new_playlist_name = None;
        self.status = "Playlist created. Add songs from Library, then Save.".into();
        Ok(())
    }
    fn save_playlist(&mut self) {
        let path = self
            .playlist_snapshot
            .as_ref()
            .map(|(path, _)| path.clone())
            .or_else(|| {
                rfd::FileDialog::new()
                    .add_filter("AMP playlist", &["json"])
                    .set_file_name("Playlist.json")
                    .save_file()
            });
        let Some(path) = path else {
            return;
        };
        let tracks: Option<Vec<_>> = self
            .queue
            .iter()
            .map(|&index| {
                self.library
                    .as_ref()?
                    .tracks
                    .get(index)
                    .map(|track| track.path.clone())
            })
            .collect();
        let Some(tracks) = tracks else {
            self.status = "Wait for the library scan before saving these songs".into();
            return;
        };
        let list = Playlist {
            sync: self
                .playlist_snapshot
                .as_ref()
                .filter(|(p, _)| p == &path)
                .and_then(|(_, list)| list.sync.clone()),
            version: 1,
            name: self
                .playlist_snapshot
                .as_ref()
                .filter(|(p, _)| p == &path)
                .map(|(_, list)| list.name.clone())
                .unwrap_or_else(|| {
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                }),
            tracks,
        };
        match list
            .save_in_library(&path, &self.root)
            .and_then(|()| Playlist::load(&path, &self.root))
        {
            Ok(saved) => {
                self.playlist_snapshot = Some((path.clone(), saved.clone()));
                if let Some(entry) = self
                    .dj_browser
                    .playlists
                    .iter_mut()
                    .find(|(p, _)| p == &path)
                {
                    entry.1 = saved;
                } else {
                    self.dj_browser.playlists.push((path, saved));
                }
                self.status = "Playlist saved".into();
            }
            Err(error) => self.status = error.to_string(),
        }
    }
    fn load_playlist(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("AMP playlist", &["json"])
            .pick_file()
        {
            match Playlist::load(&path, &self.root) {
                Ok(list) => {
                    if let Some(lib) = &self.library {
                        let indices: Option<Vec<_>> = list
                            .tracks
                            .iter()
                            .map(|p| lib.tracks.iter().position(|t| &t.path == p))
                            .collect();
                        if let Some(indices) = indices {
                            self.queue = indices;
                            self.queue_selected = None;
                            self.status = "Playlist loaded".into();
                            self.playlist_snapshot = Some((path.clone(), list.clone()));
                        } else {
                            self.status = "Rescan: playlist includes an unindexed track".into();
                        }
                    }
                }
                Err(e) => self.status = e.to_string(),
            }
        }
    }
    fn folder_tree(ui: &mut egui::Ui, folder: &Folder, path: &Path, selected: &mut PathBuf) {
        let mut label = egui::text::LayoutJob::simple_singleline(
            folder.name.clone(),
            egui::TextStyle::Body.resolve(ui.style()),
            skin::accent(ui),
        );
        label.wrap.max_width = (ui.available_width() - 38.).max(12.);
        label.wrap.max_rows = 1;
        label.wrap.break_anywhere = true;
        let label = ui.fonts_mut(|fonts| fonts.layout_job(label));
        let response = egui::CollapsingHeader::new(label)
            .id_salt(path)
            .default_open(path.as_os_str().is_empty())
            .show(ui, |ui| {
                for child in &folder.folders {
                    Self::folder_tree(ui, child, &path.join(&child.name), selected);
                }
                if folder.folders.is_empty() {
                    ui.weak(format!("{} audio files", folder.tracks.len()));
                }
            });
        response.header_response.clone().on_hover_text(&folder.name);
        if response.header_response.clicked() {
            *selected = path.to_path_buf();
        }
    }
    fn open_panes(&self) -> [bool; 6] {
        [
            true,
            self.library_open,
            self.dj_open,
            self.lab_open,
            self.cast_open,
            self.addons_open,
        ]
    }
    fn pane_ui(&mut self, ui: &mut egui::Ui, pane: usize) {
        match pane {
            0 => self.player_ui(ui),
            1 => self.library_ui(ui),
            2 => self.dj_ui(ui),
            3 => {
                let action = self.lab.show(ui, &self.root, self.library.as_ref());
                self.activate(ui, action);
            }
            4 => self.cast_ui(ui),
            _ => self.addons_ui(ui),
        }
    }
    fn group_ui(&mut self, ui: &mut egui::Ui, host: usize) {
        if host == 0 && self.player_mode != 0 {
            self.small_player_ui(ui);
            window_layout::resize_edges(ui);
            return;
        }
        if self.layout.parent[host].is_none() {
            ui.painter().rect_filled(
                ui.max_rect(),
                0.,
                theme::current(ui.ctx()).palette().background,
            );
            // Fit the entire magnetic group, including its title bars, in this viewport.
            // A fixed scene transform also maps pointer input to the scaled controls.
            let open = self.open_panes();
            let panes: Vec<_> = (0..6)
                .filter(|&p| {
                    if p != host && !open[p] {
                        return false;
                    }
                    let mut at = Some(p);
                    while let Some(parent) = at {
                        if parent == host {
                            return true;
                        }
                        at = self.layout.parent[parent];
                    }
                    false
                })
                .collect();
            let expanded_dj = window_layout::dj_pads_below(
                ui.ctx().viewport_rect().size(),
                ui.input(|i| {
                    i.viewport().maximized == Some(true) || i.viewport().fullscreen == Some(true)
                }),
            );
            let wave_height = ui
                .ctx()
                .data_mut(|d| d.get_persisted::<f32>(egui::Id::unique("dj-wave-height")))
                .unwrap_or(72.)
                .clamp(56., 140.);
            let preferred = |p| {
                if p == 0 {
                    135.
                } else if p == 2 {
                    (if expanded_dj { 1100. } else { 920. }) + (wave_height - 72.).max(0.) * 2.
                } else {
                    680.
                }
            };
            let minimum_height: f32 = panes.iter().map(|&p| preferred(p) + 8.).sum();
            let available = ui.available_size().max(egui::vec2(1., 1.));
            let minimum_width = if panes.contains(&2) {
                900. + self
                    .sides
                    .iter()
                    .filter(|side| !side.config.collapsed)
                    .count() as f32
                    * 250.
            } else {
                900.
            };
            let scale = (available.x / minimum_width)
                .min(available.y / minimum_height)
                .min(1.);
            let logical = available / scale;
            for &pane in &panes {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::unique(("pane-fit-height", pane)),
                        (preferred(pane) + 8.) * logical.y / minimum_height - 8.,
                    )
                });
            }
            window_layout::fitted_content(ui, "magnetic-group", logical, scale, |ui| {
                self.group_contents(ui, host)
            });
            window_layout::resize_edges(ui);
        } else {
            self.group_contents(ui, host);
        }
    }
    fn group_contents(&mut self, ui: &mut egui::Ui, host: usize) {
        let ctx = ui.ctx().clone();
        let children: Vec<_> = (1..6)
            .filter(|&p| self.open_panes()[p] && self.layout.parent[p] == Some(host))
            .collect();
        let target = self.layout.target(&ctx, host, self.open_panes());
        let embedded = self.layout.parent[host].is_some();
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("pane-magnet"),
                skin::Magnet {
                    enabled: embedded || target.is_some(),
                    attached: embedded,
                    clicked: false,
                },
            )
        });
        window_layout::set_embedded(&ctx, embedded);
        ui.push_id(("pane", host), |ui| {
            let height = ctx
                .data(|d| d.get_temp::<f32>(egui::Id::unique(("pane-fit-height", host))))
                .unwrap_or(600.);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    // Bound each pane before rendering: an outer scroll area otherwise gives
                    // Library infinite remaining height and pushes attached children away.
                    ui.set_max_height(height);
                    self.pane_ui(ui, host);
                    ctx.data_mut(|d| {
                        d.insert_temp(egui::Id::unique(("pane-bounds", host)), ui.min_rect());
                        d.insert_temp(egui::Id::unique(("pane-layer", host)), ui.layer_id())
                    });
                },
            );
        });
        let clicked = ctx.data(|d| {
            d.get_temp::<skin::Magnet>(egui::Id::unique("pane-magnet"))
                .is_some_and(|m| m.clicked)
        });
        if clicked && host == 0 {
            if let Some(target) = target {
                self.layout.join(target, 0);
                self.resize_player(&ctx, true);
            }
        } else if clicked {
            if embedded {
                self.layout.parent[host] = None;
                if !self.layout.parent.contains(&Some(0)) {
                    self.resize_player(&ctx, false);
                }
            } else if let Some(target) = target {
                self.layout.join(host, target);
            }
        }
        window_layout::set_embedded(&ctx, false);
        for child in children {
            self.group_ui(ui, child);
        }
    }
    fn player_ui(&mut self, ui: &mut egui::Ui) {
        skin::panel(ui, true, |ui| {
            self.header(ui, "AMP VERSE", true);
            ui.horizontal(|ui| {
                for (caption, action) in [
                    ("|<", 0),
                    (if self.playing[0] { "PAUSE" } else { "PLAY" }, 1),
                    ("STOP", 2),
                    (">|", 3),
                ] {
                    let icon = match action {
                        0 => icons::Icon::Previous,
                        1 if self.playing[0] => icons::Icon::Pause,
                        1 => icons::Icon::Play,
                        2 => icons::Icon::Stop,
                        _ => icons::Icon::Next,
                    };
                    if icons::button(ui, icon, "", action == 1, egui::vec2(58., 48.))
                        .on_hover_text(caption)
                        .clicked()
                    {
                        match action {
                            0 => {
                                if let Some(i) = self.loaded[0] {
                                    self.play(0, i.saturating_sub(1));
                                }
                            }
                            1 => {
                                if let Some(i) =
                                    self.selected.filter(|i| Some(*i) != self.loaded[0])
                                {
                                    self.play(0, i);
                                } else {
                                    self.toggle_deck(0);
                                }
                                if let Some(a) = &self.audio {
                                    a.set_fader(0.);
                                }
                            }
                            2 => self.stop(0),
                            _ => self.next(),
                        }
                    }
                }
                skin::display(ui, |ui| {
                    let position = self.audio.as_ref().map_or(0., |a| a.source_position(0));
                    let duration = self
                        .loaded_track(0)
                        .and_then(|t| t.preparation.analysis.as_ref().map(|a| a.duration));
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "{:02}:{:02} / {}",
                                position as u64 / 60,
                                position as u64 % 60,
                                duration
                                    .map(|d| format!("{:02}:{:02}", d as u64 / 60, d as u64 % 60))
                                    .unwrap_or_else(|| "--:--".into())
                            ))
                            .monospace()
                            .size(23.)
                            .color(skin::accent(ui)),
                        );
                        ui.vertical(|ui| {
                            ui.set_max_width(235.);
                            ui.add(
                                egui::Label::new(RichText::new(self.title(0)).strong()).truncate(),
                            );
                            ui.weak(
                                self.loaded_track(0)
                                    .map(|t| {
                                        format!(
                                            "{} · {} BPM · {}",
                                            t.path
                                                .extension()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                                .to_uppercase(),
                                            t.preparation
                                                .bpm
                                                .map(|b| format!("{b:.1}"))
                                                .unwrap_or_else(|| "--".into()),
                                            t.preparation.harmonic_key.as_deref().unwrap_or("--")
                                        )
                                    })
                                    .unwrap_or_else(|| "Local / network library".into()),
                            );
                        });
                    });
                });
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(105., 42.), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2., egui::Color32::BLACK);
                if let Some(audio) = &self.audio {
                    for i in 0..48 {
                        let value = f32::from_bits(
                            audio.controls.scope[0][i * 2]
                                .load(std::sync::atomic::Ordering::Relaxed),
                        )
                        .clamp(0., 1.);
                        let x = rect.left() + i as f32 * rect.width() / 48.;
                        ui.painter().line_segment(
                            [
                                egui::pos2(x, rect.bottom() - 3.),
                                egui::pos2(x, rect.bottom() - 3. - value * 36.),
                            ],
                            egui::Stroke::new(1.5, GOLD),
                        );
                    }
                }
                response.on_hover_text("Live measured output amplitude history");
                ui.add(egui::Slider::new(&mut self.volume, 0..=100).text("Vol"));
                if ui.available_width() >= 90. {
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), 48.),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = 0.;
                            ui.spacing_mut().interact_size.y = 14.;
                            ui.spacing_mut().slider_width =
                                (ui.available_width() - 38.).clamp(45., 115.);
                            for (band, label) in ["Low", "Mid", "High"].iter().enumerate() {
                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.eq[0][band], -12. ..=12.)
                                            .text(*label)
                                            .show_value(false),
                                    )
                                    .on_hover_text("Player EQ · -12 to +12 dB")
                                    .changed()
                                    && let Some(audio) = &self.audio
                                {
                                    audio.set_eq(0, band, self.eq[0][band]);
                                }
                            }
                        },
                    );
                }
            });
            ui.horizontal(|ui| {
                self.player_view_menu(ui);
                if ui
                    .button(if self.library_open && (self.layout.parent[1] == Some(0)) {
                        "Collapse Library"
                    } else {
                        "Expand Library"
                    })
                    .clicked()
                {
                    if self.library_open && self.layout.parent[1] == Some(0) {
                        self.library_open = false;
                        self.resize_player(ui.ctx(), false);
                    } else {
                        self.library_open = true;
                        self.layout.parent[1] = Some(0);
                        self.resize_player(ui.ctx(), true);
                    }
                }
                if ui.checkbox(&mut self.auto, "Play queue").changed() && self.auto {
                    self.run.cancel_automation();
                }
                ui.toggle_value(&mut self.shuffle, "Shuffle");
                ui.toggle_value(&mut self.repeat_one, "Repeat one");
                if ui
                    .button("EQ")
                    .on_hover_text("Open expanded EQ in Library")
                    .clicked()
                {
                    self.library_page = 7;
                    self.library_open = true;
                    self.layout.parent[1] = Some(0);
                    self.resize_player(ui.ctx(), true);
                }

                ui.weak(&self.status);
            });
        });
    }
    fn set_player_mode(&mut self, ctx: &egui::Context, mode: u8) {
        self.player_mode = mode;
        if mode == 0 {
            ctx.send_viewport_cmd_to(
                egui::ViewportId::ROOT,
                egui::ViewportCommand::WindowLevel(if self.player_on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                }),
            );
            self.resize_player(ctx, self.library_open && self.layout.parent[1] == Some(0));
        } else {
            let size = if mode == 1 {
                egui::vec2(520., 210.)
            } else {
                egui::vec2(620., 100.)
            };
            ctx.send_viewport_cmd_to(
                egui::ViewportId::ROOT,
                egui::ViewportCommand::MinInnerSize(size * 0.7),
            );
            ctx.send_viewport_cmd_to(
                egui::ViewportId::ROOT,
                egui::ViewportCommand::InnerSize(size),
            );
            ctx.send_viewport_cmd_to(
                egui::ViewportId::ROOT,
                egui::ViewportCommand::WindowLevel(if self.player_on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                }),
            );
        }
    }
    fn player_view_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Player view", |ui| {
            let height = (ui.ctx().viewport_rect().height() - 65.).max(55.);
            egui::ScrollArea::vertical()
                .id_salt("player-view-menu-scroll")
                .max_height(height)
                .show(ui, |ui| {
                    for (mode, name) in [
                        (0, "Full player"),
                        (1, "Compact player"),
                        (2, "Taskbar mini player"),
                    ] {
                        if ui
                            .selectable_label(self.player_mode == mode, name)
                            .clicked()
                        {
                            self.set_player_mode(ui.ctx(), mode);
                            ui.close();
                        }
                    }
                    if ui
                        .checkbox(&mut self.player_on_top, "Always on top")
                        .changed()
                    {
                        if let Err(error) = save_player_on_top(self.player_on_top) {
                            self.status = format!("Always on top changed; could not save: {error}");
                        }
                        ui.ctx().send_viewport_cmd_to(
                            egui::ViewportId::ROOT,
                            egui::ViewportCommand::WindowLevel(if self.player_on_top {
                                egui::WindowLevel::AlwaysOnTop
                            } else {
                                egui::WindowLevel::Normal
                            }),
                        );
                    }
                });
        });
    }
    fn small_player_ui(&mut self, ui: &mut egui::Ui) {
        let mini = self.player_mode == 2;
        let design = if mini {
            egui::vec2(620., 100.)
        } else {
            egui::vec2(520., 210.)
        };
        let scale = (ui.available_width() / design.x)
            .min(ui.available_height() / design.y)
            .max(0.01);
        window_layout::fitted_content(ui, "small-player", design, scale, |ui| {
            skin::panel(ui, false, |ui| {
                ui.set_min_size(design - egui::vec2(20., 20.));
                ui.horizontal(|ui| {
                    let title = ui.add(
                        egui::Label::new(RichText::new("AMP VERSE").strong())
                            .sense(egui::Sense::click_and_drag()),
                    );
                    if title.drag_started() {
                        ui.ctx().send_viewport_cmd_to(
                            egui::ViewportId::ROOT,
                            egui::ViewportCommand::StartDrag,
                        );
                    }
                    self.player_view_menu(ui);
                    if ui
                        .button("Full")
                        .on_hover_text("Restore full player and attached panels")
                        .clicked()
                    {
                        self.set_player_mode(ui.ctx(), 0);
                    }
                    if ui
                        .button("_")
                        .on_hover_text("Minimize to taskbar")
                        .clicked()
                    {
                        ui.ctx().send_viewport_cmd_to(
                            egui::ViewportId::ROOT,
                            egui::ViewportCommand::Minimized(true),
                        );
                    }
                });
                if !mini {
                    ui.add_sized(
                        [480., 30.],
                        egui::Label::new(
                            RichText::new(self.title(0))
                                .strong()
                                .color(skin::signal(ui)),
                        )
                        .truncate(),
                    );
                    let position = self.audio.as_ref().map_or(0., |a| a.source_position(0));
                    ui.label(format!(
                        "{:02}:{:02}",
                        position as u64 / 60,
                        position as u64 % 60
                    ));
                }
                ui.horizontal(|ui| {
                    for (label, action) in [
                        ("|<", 0),
                        (if self.playing[0] { "Pause" } else { "Play" }, 1),
                        ("Stop", 2),
                        (">|", 3),
                    ] {
                        if ui.add_sized([46., 30.], egui::Button::new(label)).clicked() {
                            match action {
                                0 => {
                                    if let Some(i) = self.loaded[0] {
                                        self.play(0, i.saturating_sub(1));
                                    }
                                }
                                1 => {
                                    if self.loaded[0].is_none() {
                                        if let Some(i) =
                                            self.selected.or_else(|| self.queue.first().copied())
                                        {
                                            self.play(0, i);
                                        }
                                    } else {
                                        self.toggle_deck(0);
                                    }
                                    if let Some(a) = &self.audio {
                                        a.set_fader(0.);
                                    }
                                }
                                2 => self.stop(0),
                                _ => self.next(),
                            }
                        }
                    }
                    if mini {
                        ui.add_sized([170., 28.], egui::Label::new(self.title(0)).truncate());
                    }
                    ui.spacing_mut().slider_width = 90.;
                    ui.add(egui::Slider::new(&mut self.volume, 0..=100).show_value(false))
                        .on_hover_text("Master volume");
                });
                if !mini {
                    ui.horizontal(|ui| {
                        ui.toggle_value(&mut self.shuffle, "Shuffle");
                        ui.toggle_value(&mut self.repeat_one, "Repeat one");
                        if ui.button("EQ").clicked() {
                            self.eq_open = true;
                        }
                    });
                }
            });
        });
    }
    fn resize_player(&self, ctx: &egui::Context, expanded: bool) {
        ctx.send_viewport_cmd_to(
            egui::ViewportId::ROOT,
            egui::ViewportCommand::MinInnerSize(if expanded {
                egui::vec2(740., 480.)
            } else {
                egui::vec2(740., 160.)
            }),
        );
        ctx.send_viewport_cmd_to(
            egui::ViewportId::ROOT,
            egui::ViewportCommand::InnerSize(if expanded {
                egui::vec2(1100., 900.)
            } else {
                egui::vec2(1100., 170.)
            }),
        );
    }
    fn expand_smart_list(&mut self) {
        if self.queue.len() > 1000 {
            self.status = "Smart Lists support up to 1,000 songs at a time".into();
            return;
        }
        if let Some(lib) = &self.library {
            let songs: Vec<_> = lib.tracks.iter().map(|t| &t.preparation).collect();
            match amp_core::smart::curate(&songs, &self.queue, self.queue.len().saturating_add(20))
            {
                Ok(queue) => {
                    let added = queue.len().saturating_sub(self.queue.len());
                    self.queue = queue;
                    self.queue_selected = None;
                    self.status = format!(
                        "Smart List added {added} prepared neighbors within ±6% BPM and ±20% energy; review and Save the playlist"
                    );
                }
                Err(e) => self.status = e,
            }
        }
    }
    fn curate_playlist(&mut self) {
        if self.queue.len() > 1000 {
            self.status = "Smart Lists support up to 1,000 songs at a time".into();
            return;
        }
        if let Some(lib) = &self.library {
            let songs: Vec<_> = self
                .queue
                .iter()
                .map(|&i| &lib.tracks[i].preparation)
                .collect();
            let unknown = songs
                .iter()
                .filter(|p| {
                    p.bpm.is_none()
                        || p.harmonic_key.is_none()
                        || amp_core::smart::energy(p).is_none()
                })
                .count();
            let order = amp_core::smart::order(&songs);
            self.queue = order.iter().map(|&i| self.queue[i]).collect();
            self.queue_selected = None;
            self.status = format!(
                "Smart List ordered {} songs · first song anchors energy · {unknown} missing BPM/key/energy; review before mixing",
                self.queue.len()
            );
        }
    }
    fn start_control(&mut self, ctx: &egui::Context) {
        if self.control.take().is_some() {
            self.control_restart = Some(std::time::Instant::now() + Duration::from_millis(200));
            self.control_status = "Restarting show control; audio continues".into();
            return;
        }
        self.control_restart = None;
        self.control = None;
        match control::Control::start(
            ctx,
            &self.control_bind,
            if self.control_password.is_empty() {
                None
            } else {
                Some(self.control_password.clone())
            },
        ) {
            Ok(c) => {
                self.control_status = format!("Companion HTTP / WebSocket · {}", c.address);
                c.catalog(self.control_catalog());
                self.control = Some(c);
            }
            Err(e) => self.control_status = format!("Show control unavailable: {e}"),
        }
    }
    fn poll_control(&mut self) {
        let mut commands = Vec::new();
        if let Some(c) = &self.control {
            for _ in 0..8 {
                match c.commands.try_recv() {
                    Ok(command) => commands.push(command),
                    Err(_) => break,
                }
            }
        }
        for command in commands {
            let result = if std::time::Instant::now() > command.deadline {
                Err("Command expired before execution".into())
            } else {
                self.execute_control(command.request.command)
            };
            let reply = control::Reply {
                id: command.request.id,
                ok: result.is_ok(),
                status: if result.is_ok() {
                    "accepted"
                } else {
                    "rejected"
                }
                .into(),
                error: result.err(),
            };
            let _ = command.reply.send(reply);
        }
    }
    fn edit_smart_in_library(&mut self, ui: &mut egui::Ui, id: &str) {
        match amp_core::show::Settings::load(&self.root) {
            Ok(settings) if settings.playlists.iter().any(|p| p.id == id) => {
                self.show_editor.open_playlist(id, &self.root, &settings);
                self.activate(ui, Some(skin::Module::Library));
            }
            Ok(_) => {
                self.status =
                    "That Smart Playlist no longer exists. Reload saved rules in Library.".into()
            }
            Err(error) => self.status = format!("Could not open Smart Playlist: {error}"),
        }
    }
    fn performance_controls(&mut self, ui: &mut egui::Ui) {
        let original_style = ui.style().clone();
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(16.));
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(16.));
        ui.spacing_mut().button_padding = egui::vec2(12., 10.);

        let mut selected = self.run.choice.clone();
        let mut context_edit = None;
        ui.horizontal_wrapped(|ui| {
            let selector = egui::ComboBox::from_id_salt("performance-playlist")
                .width(180.)
                .truncate()
                .selected_text(
                    self.run
                        .settings
                        .playlists
                        .iter()
                        .find(|p| p.id == selected)
                        .map_or("Select Smart Playlist", |p| p.name.as_str()),
                )
                .show_ui(ui, |ui| {
                    for p in &self.run.settings.playlists {
                        let row = ui.selectable_value(&mut selected, p.id.clone(), &p.name);
                        if let Some(action) = show_editor::playlist_menu(&row) {
                            context_edit = Some((p.id.clone(), action));
                        }
                    }
                });
            if !selected.is_empty()
                && let Some(action) = show_editor::playlist_menu(&selector.response)
            {
                context_edit = Some((selected.clone(), action));
            }
            if ui.button("Load").clicked()
                && let Err(e) = self.execute_control(control::Action::LoadSmartPlaylist {
                    playlist: selected.clone(),
                    execution: control::Execution::LoadOnly,
                    style: None,
                    energy: None,
                })
            {
                self.status = e;
            }
            if ui
                .add(
                    egui::Button::selectable(
                        self.run.auto,
                        if self.run.auto {
                            "Stop Auto DJ"
                        } else {
                            "Start Auto DJ"
                        },
                    )
                    .frame(true),
                )
                .clicked()
            {
                let result = if self.run.auto {
                    self.execute_control(control::Action::AutoDj { enabled: false })
                } else {
                    self.execute_control(control::Action::LoadSmartPlaylist {
                        playlist: selected.clone(),
                        execution: control::Execution::LoadOnly,
                        style: None,
                        energy: None,
                    })
                    .and_then(|()| self.execute_control(control::Action::AutoDj { enabled: true }))
                };
                if let Err(e) = result {
                    self.status = e;
                }
            }
            if ui.button("Mix Next").clicked()
                && let Err(e) = self.execute_control(control::Action::MixNext { immediate: true })
            {
                self.status = e;
            }
            skin::switch(ui, &mut self.run.crossfade, "Auto Crossfade");
            if ui
                .checkbox(&mut self.run.locked, "Manual DJ Lock")
                .changed()
                && self.run.locked
            {
                self.run.cancel_automation();
            }
            if self.run.locked {
                ui.weak("Unlock Manual DJ Lock to enable Auto Mix");
            }
            let mut style = self.run.style;
            egui::ComboBox::from_id_salt("dj-mixing-style")
                .width(180.)
                .selected_text(format!("{:?}", style))
                .show_ui(ui, |ui| {
                    for value in [
                        amp_core::show::Style::Hype,
                        amp_core::show::Style::Steady,
                        amp_core::show::Style::Chill,
                    ] {
                        ui.selectable_value(&mut style, value, format!("{:?}", value));
                    }
                });
            if style != self.run.style
                && let Err(error) = self.execute_control(control::Action::SetStyle { style })
            {
                self.status = error;
            }
        });
        ui.set_style(original_style);
        if self.run.choice != selected {
            self.dj_browser.select_smart(&selected);
        }
        self.run.choice = selected;
        if let Some((id, action)) = context_edit {
            self.smart_playlist_action(ui, &id, action);
        }
    }
    fn sampler_ui(&mut self, ui: &mut egui::Ui) {
        if !self.sampler.started {
            self.sampler.started = true;
            if self.sampler.has_saved() {
                match self.ensure_audio() {
                    Ok(()) => self.sampler.restore(),
                    Err(e) => self.sampler.status = e,
                }
            }
        }
        let actions = self.sampler.show(ui, self.audio.as_ref());
        for action in actions {
            if let Err(e) = self.ensure_audio() {
                self.sampler.status = e;
                continue;
            }
            let engine = self.audio.as_mut().unwrap();
            let result = match action {
                sampler::Action::Load(pad) => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter(
                            "Sound effects",
                            &["wav", "mp3", "flac", "ogg", "opus", "m4a", "aac"],
                        )
                        .pick_file()
                    {
                        self.sampler.request_load(pad, path);
                    }
                    Ok(())
                }
                sampler::Action::Trigger(pad) => self.sampler.trigger(engine, pad),
                sampler::Action::Stop(pad) => engine.sample_stop(Some(pad)),
                sampler::Action::Clear(pad) => self.sampler.clear(pad, engine),
                sampler::Action::StopAll => engine.sample_stop(None),
            };
            if let Err(e) = result {
                self.sampler.status = e;
            }
        }
    }
    fn dj_ui(&mut self, ui: &mut egui::Ui) {
        self.release_jog_wheels(ui.ctx());
        if !ui.ctx().egui_wants_keyboard_input() && ui.input(|input| input.focused) {
            for event in ui.input(|input| input.events.clone()) {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } = event
                    && !modifiers.ctrl
                    && !modifiers.alt
                    && !modifiers.command
                    && let Some(pad) = sampler::shortcut(key)
                {
                    self.side_sample_action(
                        usize::from(modifiers.shift),
                        sampler::Action::Trigger(pad),
                    );
                }
            }
        }
        skin::panel(ui, true, |ui| {
            ui.set_min_width(ui.available_width());
            self.header(ui, "AMP DJ", true);
            self.performance_controls(ui);
            ui.add_space(12.);
            let wave_id = egui::Id::unique("dj-wave-height");
            let mut wave_height = ui
                .ctx()
                .data_mut(|d| d.get_persisted::<f32>(wave_id))
                .unwrap_or(72.)
                .clamp(56., 140.);
            for d in 0..2 {
                let track = self.loaded_track(d).cloned();
                let position = self.audio.as_ref().map_or(0., |a| a.source_position(d));
                self.waveforms[d].height = Some(wave_height);
                self.waveforms[d].show(
                    ui,
                    self.loaded_origin[d].as_deref().unwrap_or(&self.root),
                    track.as_ref(),
                    position,
                    self.wave_span,
                    d,
                );
            }
            window_layout::splitter(
                ui,
                "dj-wave-divider",
                &mut wave_height,
                56. ..=140.,
                false,
                0.5,
                10.,
            );
            ui.ctx()
                .data_mut(|d| d.insert_persisted(wave_id, wave_height));
            egui::CollapsingHeader::new("Transition options")
                .id_salt("dj-advanced")
                .show(ui, |ui| {
                    let previous=self.run.transitions;
                    ui.toggle_value(&mut self.run.transitions.enabled,"Auto DJ transition effects");
                    egui::ComboBox::from_id_salt("auto-dj-transition-style").selected_text(self.run.transitions.style.label()).show_ui(ui,|ui|{
                        for style in [transition_styles::Style::Variety,transition_styles::Style::Fade,transition_styles::Style::Scratch,transition_styles::Style::Stutter,transition_styles::Style::Brake,transition_styles::Style::Filter,transition_styles::Style::BassSwap,transition_styles::Style::Beatmatch]{ui.selectable_value(&mut self.run.transitions.style,style,style.label());}
                    });
                    ui.horizontal_wrapped(|ui|{ui.label("Use an effect every");ui.add(egui::DragValue::new(&mut self.run.transitions.every).range(2..=12));ui.label("mixes");});
                    ui.horizontal_wrapped(|ui| {for (i, name) in ["Scratch", "Stutter", "Tape brake", "Filter sweep", "Bass swap", "Beatmatch"].iter().enumerate() { ui.toggle_value(&mut self.run.transitions.techniques[i], *name); }});
                    ui.weak("Variety rotates enabled techniques occasionally. Mix time stays unchanged. Beatmatch adjusts tempo within +/-16%; beat phase stays manual.");
                    self.run.scratch_assist=self.run.transitions.enabled;
                    if previous!=self.run.transitions && let Err(error)=transition_styles::save(self.run.transitions){self.status=error;}

                    ui.add(
                        egui::Slider::new(&mut self.wave_span, 4. ..=60.).text("Waveform seconds"),
                    );
                });
            ui.add_space(8.);
            let pads_below = window_layout::dj_pads_below(
                ui.ctx().viewport_rect().size(),
                ui.input(|i| {
                    i.viewport().maximized == Some(true) || i.viewport().fullscreen == Some(true)
                }),
            );
            let bank_height = if pads_below { 230. } else { 0. };
            let library_id = egui::Id::unique("dj-library-height");
            let library_max = (ui.available_height() - 70. - 300.).max(180.);
            let mut browser_height = ui
                .ctx()
                .data_mut(|d| d.get_persisted::<f32>(library_id))
                .unwrap_or((ui.available_height() * 0.32).max(250.))
                .clamp(180., library_max);
            let height = (ui.available_height() - browser_height - 70.).max(300.);
            let side_widths = std::array::from_fn::<_, 2, _>(|side| {
                if pads_below {
                    0.
                } else if self.sides[side].config.collapsed {
                    30.
                } else {
                    230.
                }
            });
            let deck_width =
                ((ui.available_width() - 298. - side_widths.iter().sum::<f32>() - 36.) / 2.)
                    .max(250.);
            egui::ScrollArea::vertical()
                .id_salt("dj-resizable-mixer")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .max_height(height)
                .min_scrolled_height(height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        if !pads_below {
                            self.side_panel(ui, 0, side_widths[0], height, false);
                        }
                        ui.vertical(|ui| {
                            self.deck_ui(ui, 0, deck_width, (height - bank_height).max(230.));
                            if pads_below {
                                self.side_panel(ui, 0, deck_width, bank_height, true);
                            }
                        });
                        ui.allocate_ui_with_layout(
                            egui::vec2(280., height),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                let scale = (height / 520.).min(1.);
                                let logical = egui::vec2(280. / scale, height / scale);
                                window_layout::fitted_content(
                                    ui,
                                    "dj-center-mixer",
                                    logical,
                                    scale,
                                    |ui| {
                                        skin::panel(ui, true, |ui| {
                                            ui.set_width(logical.x - 18.);
                                            ui.set_min_height(logical.y - 18.);
                                            ui.spacing_mut().interact_size.y =
                                                ((logical.y - 220.) / 14.).clamp(24., 44.);
                                            egui::ScrollArea::vertical()
                                                .id_salt("dj-channel-controls")
                                                .max_height((logical.y - 480.).max(100.))
                                                .auto_shrink([false, true])
                                                .show(ui, |ui| self.inline_mixer(ui, logical.y));
                                            let fading =
                                                self.audio.as_ref().is_some_and(|a| a.fading());
                                            let fade = ui.add_sized(
                                                [240., 36.],
                                                egui::Button::new(
                                                    RichText::new(
                                                        if fading && self.run.profile.is_some() {
                                                            "Mixing…"
                                                        } else if fading {
                                                            "Cancel mix"
                                                        } else {
                                                            "Auto crossfade"
                                                        },
                                                    )
                                                    .size(16.)
                                                    .color(GOLD),
                                                )
                                                .fill(skin::accent(ui).gamma_multiply(0.16))
                                                .stroke(egui::Stroke::new(1., skin::accent(ui)))
                                                .wrap(),
                                            );
                                            ui.ctx().data_mut(|d| {
                                                d.insert_temp(
                                                    egui::Id::unique("dj-auto-fade-bounds"),
                                                    fade.rect,
                                                );
                                                d.insert_temp(
                                                    egui::Id::unique("dj-auto-fade-bounds")
                                                        .with("screen"),
                                                    (fade.rect, ui.layer_id()),
                                                )
                                            });
                                            if fade.clicked() {
                                                self.auto = false;
                                                self.auto_crossfade();
                                            }
                                            ui.horizontal(|ui| {
                                                ui.label("A");
                                                ui.spacing_mut().slider_width = 88.;
                                                if ui
                                                    .add(
                                                        egui::Slider::new(
                                                            &mut self.fader,
                                                            0. ..=1.,
                                                        )
                                                        .show_value(false),
                                                    )
                                                    .changed()
                                                    && let Some(a) = &self.audio
                                                {
                                                    self.run.cancel_automation();
                                                    a.set_fader(self.fader);
                                                }
                                                ui.label("B");
                                            });
                                            let previous = self.fade_seconds;
                                            ui.horizontal(|ui| {
                                                ui.label("Mix time");
                                                if ui
                                                    .button("−")
                                                    .on_hover_text("Shorten by 0.5 seconds")
                                                    .clicked()
                                                {
                                                    self.fade_seconds -= 0.5;
                                                }
                                                ui.add(
                                                    egui::DragValue::new(&mut self.fade_seconds)
                                                        .range(0.5..=30.)
                                                        .speed(0.1)
                                                        .fixed_decimals(1)
                                                        .suffix(" s"),
                                                );
                                                if ui
                                                    .button("+")
                                                    .on_hover_text("Lengthen by 0.5 seconds")
                                                    .clicked()
                                                {
                                                    self.fade_seconds += 0.5;
                                                }
                                            });
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.fade_seconds,
                                                    0.5..=30.,
                                                )
                                                .show_value(false),
                                            );
                                            self.fade_seconds = self.fade_seconds.clamp(0.5, 30.);
                                            if previous != self.fade_seconds
                                                && let Err(error) =
                                                    crossfade_settings::save(self.fade_seconds)
                                            {
                                                self.status =
                                                    format!("Could not save mix time: {error}");
                                            }
                                            ui.separator();
                                            ui.columns(2, |columns| {
                                                for (deck, column) in columns.iter_mut().enumerate()
                                                {
                                                    let stem_path =
                                                        self.loaded_track(deck).map(|track| {
                                                            self.loaded_origin[deck]
                                                                .as_deref()
                                                                .unwrap_or(&self.root)
                                                                .join(&track.path)
                                                        });
                                                    if self.stem_panel.show(
                                                        column,
                                                        deck,
                                                        stem_path.as_deref(),
                                                        self.audio.as_ref(),
                                                        !self.run.auto && !self.playing[deck],
                                                    ) && let Some(path) = stem_path
                                                        && let Err(error) =
                                                            self.safe_load_path(deck, path, None)
                                                    {
                                                        self.status = error;
                                                    }
                                                }
                                            });
                                        });
                                    },
                                );
                            },
                        );
                        ui.vertical(|ui| {
                            self.deck_ui(ui, 1, deck_width, (height - bank_height).max(230.));
                            if pads_below {
                                self.side_panel(ui, 1, deck_width, bank_height, true);
                            }
                        });
                        if !pads_below {
                            self.side_panel(ui, 1, side_widths[1], height, false);
                        }
                    });
                });
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.volume, 0..=100).text("Master"))
                    .on_hover_text(
                        self.audio
                            .as_ref()
                            .map(|a| a.device.as_str())
                            .unwrap_or("Audio output"),
                    );
                ui.add(egui::Label::new(&self.status).truncate());
            });
            window_layout::splitter(
                ui,
                "dj-library-divider",
                &mut browser_height,
                180. ..=library_max,
                false,
                -1.,
                22.,
            );
            ui.ctx()
                .data_mut(|d| d.insert_persisted(library_id, browser_height));
            // Decks stop growing once their controls reach a useful size. Give all
            // remaining space to the song list rather than leaving an unused strip.
            let remaining_height = (ui.available_height() - 12.).max(160.);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), remaining_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_min_height(remaining_height);
                    self.dj_library_ui(ui);
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            egui::Id::unique("dj-browser-screen").with("screen"),
                            (ui.min_rect(), ui.layer_id()),
                        )
                    });
                },
            );
        });
    }
    fn add_dropped_to_queue(&mut self, path: PathBuf) {
        let result = (|| -> Result<usize, String> {
            let absolute = path.canonicalize().map_err(|error| error.to_string())?;
            let base = self
                .root
                .canonicalize()
                .map_err(|error| error.to_string())?;
            let relative = absolute
                .strip_prefix(base)
                .map_err(|_| "Add this folder to the indexed Library before queueing its songs")?;
            self.library
                .as_ref()
                .and_then(|library| {
                    library
                        .tracks
                        .iter()
                        .position(|track| track.path == relative)
                })
                .ok_or("Rescan Library to index this song before queueing it".into())
        })();
        match result {
            Ok(index) => {
                if self.run.auto {
                    let eligible = self
                        .run
                        .profile
                        .as_ref()
                        .zip(self.library.as_ref())
                        .is_some_and(|(profile, library)| {
                            amp_core::show::eligible(profile, &library.tracks[index])
                        });
                    if !eligible {
                        self.status =
                            "That song is outside the active Auto Mix rules; it was not queued"
                                .into();
                        return;
                    }
                    self.run.future_candidates.push_back(index);
                }
                self.queue.push(index);
                self.status = "Song added to the queue; playback unchanged".into();
            }
            Err(error) => self.status = error,
        }
    }
    fn cue_command(&mut self, side: usize, operation: side_deck::Operation) -> Result<(), String> {
        if side > 1 {
            return Err("Cue side must be 0 (left) or 1 (right)".into());
        }
        match operation {
            side_deck::Operation::Load { song } => {
                let index = self.find_song(&song)?;
                let track = self
                    .library
                    .as_ref()
                    .and_then(|library| library.tracks.get(index))
                    .ok_or("Track unavailable")?;
                self.sides[side].load(self.root.join(&track.path))?;
            }
            side_deck::Operation::Play => self.sides[side].play()?,
            side_deck::Operation::Pause => self.sides[side].pause(),
            side_deck::Operation::Stop => self.sides[side].stop(),
            side_deck::Operation::Seek { seconds } => self.sides[side].seek(seconds)?,
            side_deck::Operation::Volume { value } => self.sides[side].gain(value)?,
            side_deck::Operation::HotCue { number } => {
                let cue = self.sides[side]
                    .track
                    .as_ref()
                    .and_then(|track| {
                        u16::try_from(number)
                            .ok()
                            .and_then(|number| hot_cues::slot(&track.preparation, number))
                    })
                    .filter(|_| number > 0)
                    .ok_or("Cue number not found")?
                    .seconds;
                self.sides[side].seek(cue)?;
            }
            side_deck::Operation::Send { deck } => {
                let target = match deck {
                    control::Deck::A => 0,
                    control::Deck::B => 1,
                    control::Deck::Auto => {
                        if !self.playing[0] {
                            0
                        } else {
                            1
                        }
                    }
                };
                let path = self.sides[side]
                    .config
                    .path
                    .clone()
                    .ok_or("Load a cue track first")?;
                self.safe_load_path(target, path, Some(self.sides[side].position()))?;
            }
        }
        Ok(())
    }
    fn safe_load_path(
        &mut self,
        deck: usize,
        path: PathBuf,
        cue: Option<f64>,
    ) -> Result<(), String> {
        if deck > 1 {
            return Err("Deck must be A or B".into());
        }
        if self
            .audio
            .as_ref()
            .is_some_and(|audio| audio.playing(deck) || audio.fading())
            || (self.run.auto
                && self
                    .run
                    .preloaded
                    .is_some_and(|(reserved, _)| reserved == deck))
        {
            self.pending_load[deck] = Some((path, cue));
            self.status = format!(
                "Deck {} replacement staged. Stop it, then Apply staged track.",
                if deck == 0 { "A" } else { "B" }
            );
            return Ok(());
        }
        let path = path.canonicalize().map_err(|error| error.to_string())?;
        let root = path
            .parent()
            .ok_or("Audio file folder missing")?
            .to_path_buf();
        let mut preparation = amp_core::preparation::load(&path)?;
        if let Some(cue) = cue {
            let end = preparation.cue_out.or(preparation
                .analysis
                .as_ref()
                .map(|analysis| analysis.duration));
            preparation.cue_in = end.map_or(cue.max(0.), |end| cue.clamp(0., (end - 0.02).max(0.)));
        }
        let track = amp_core::Track {
            path: path.file_name().ok_or("Audio filename missing")?.into(),
            title: preparation
                .extensions
                .get("title")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                }),
            preparation,
        };
        self.ensure_audio()?;
        self.audio
            .as_mut()
            .unwrap()
            .load(deck, &root, &track, false)?;
        self.run.cancel_automation();
        self.auto = false;
        self.loaded[deck] = path.strip_prefix(&self.root).ok().and_then(|relative| {
            self.library
                .as_ref()?
                .tracks
                .iter()
                .position(|track| track.path == relative)
        });
        self.loaded_snapshot[deck] = Some(track);
        self.loaded_origin[deck] = Some(root);
        self.pending_load[deck] = None;
        self.status = format!(
            "Deck {} prepared; playback unchanged",
            if deck == 0 { "A" } else { "B" }
        );
        Ok(())
    }
    fn recue_deck(&mut self, deck: usize) {
        if let Some(track) = self.loaded_track(deck).cloned()
            && let Some(audio) = &mut self.audio
        {
            let root = self.loaded_origin[deck].as_deref().unwrap_or(&self.root);
            if let Err(error) = audio.load(deck, root, &track, false) {
                self.status = error;
            }
            self.run.cancel_automation();
            self.auto = false;
        }
    }
    fn side_panel(&mut self, ui: &mut egui::Ui, side: usize, width: f32, height: f32, below: bool) {
        let accent = if side == 0 { ORANGE } else { CYAN };
        let panel = ui.push_id(("side-tools", side), |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(width, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    skin::panel(ui, false, |ui| {
                        ui.set_width((width - 16.).max(14.));
                        ui.set_min_height((height - 16.).max(50.));
                        self.sides[side].header(ui, accent);
                        if self.sides[side].config.collapsed {
                            ui.add(
                                egui::Label::new(if self.sides[side].config.cue {
                                    "C"
                                } else {
                                    "S"
                                })
                                .truncate(),
                            );
                            return;
                        }
                        egui::ScrollArea::vertical()
                            .id_salt("side-controls")
                            .max_height((height - 80.).max(150.))
                            .show(ui, |ui| {
                                if self.sides[side].config.cue {
                                    if let Some((path, cue, deck)) =
                                        self.sides[side].show_cue(ui, accent)
                                        && let Err(error) =
                                            self.safe_load_path(deck, path, Some(cue))
                                    {
                                        self.status = error;
                                    }
                                } else {
                                    if side == 0 {
                                        if !self.sampler.started {
                                            self.sampler.started = true;
                                            if self.sampler.has_saved()
                                                && self.ensure_audio().is_ok()
                                            {
                                                self.sampler.restore();
                                            }
                                        }
                                        let actions = self.sampler.compact(
                                            ui,
                                            self.audio.as_ref(),
                                            accent,
                                            if below { 9 } else { 3 },
                                        );
                                        if (!actions.is_empty() || self.sampler.needs_audio())
                                            && let Err(error) = self.ensure_audio()
                                        {
                                            self.sampler.status = error;
                                            return;
                                        }
                                        for action in actions {
                                            self.side_sample_action(0, action);
                                        }
                                    } else {
                                        if !self.right_sampler.started {
                                            self.right_sampler.started = true;
                                            if self.right_sampler.has_saved()
                                                && self.ensure_right_samples().is_ok()
                                            {
                                                self.right_sampler.restore();
                                            }
                                        }
                                        let actions = self.right_sampler.compact(
                                            ui,
                                            self.right_sample_audio.as_ref(),
                                            accent,
                                            if below { 9 } else { 3 },
                                        );
                                        if (!actions.is_empty() || self.right_sampler.needs_audio())
                                            && let Err(error) = self.ensure_right_samples()
                                        {
                                            self.right_sampler.status = error;
                                            return;
                                        }
                                        for action in actions {
                                            self.side_sample_action(1, action);
                                        }
                                    }
                                }
                            });
                    });
                },
            );
        });
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::unique(("dj-side", side)).with("screen"),
                (panel.response.rect, ui.layer_id()),
            )
        });
    }
    fn ensure_right_samples(&mut self) -> Result<(), String> {
        if self.right_sample_audio.is_none() {
            let audio = mixer::Engine::new()?;
            audio.set_master(self.volume as f32 / 100.);
            self.right_sample_audio = Some(audio);
        }
        Ok(())
    }
    fn side_sample_action(&mut self, side: usize, action: sampler::Action) {
        let (sampler, audio) = if side == 0 {
            (&mut self.sampler, self.audio.as_mut())
        } else {
            (&mut self.right_sampler, self.right_sample_audio.as_mut())
        };
        let Some(audio) = audio else {
            return;
        };
        let result = match action {
            sampler::Action::Load(pad) => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "Audio",
                        &["wav", "mp3", "flac", "ogg", "m4a", "opus", "aac"],
                    )
                    .pick_file()
                {
                    sampler.request_load(pad, path);
                }
                Ok(())
            }
            sampler::Action::Trigger(pad) => sampler.trigger(audio, pad),
            sampler::Action::Stop(pad) => audio.sample_stop(Some(pad)),
            sampler::Action::Clear(pad) => sampler.clear(pad, audio),
            sampler::Action::StopAll => audio.sample_stop(None),
        };
        if let Err(error) = result {
            sampler.status = error;
        }
    }
    fn deck_ui(&mut self, ui: &mut egui::Ui, deck: usize, width: f32, height: f32) {
        let accent = if deck == 0 { ORANGE } else { CYAN };
        let platter_side = (height - 274.).clamp(64., 260.);
        let area=ui.allocate_ui_with_layout(egui::vec2(width,0.),egui::Layout::top_down(egui::Align::Center),|ui|{
            skin::panel(ui,true,|ui|{ui.set_width(width-18.);
                ui.horizontal(|ui|{skin::label(ui,if deck==0{"DECK A"}else{"DECK B"});let position=self.audio.as_ref().map_or(0.,|a|a.position(deck));ui.label(RichText::new(format!("{:02}:{:02}",position as u64/60,position as u64%60)).monospace().color(skin::signal(ui)));});
                ui.separator();
                ui.add(egui::Label::new(RichText::new(self.title(deck)).strong().size(15.).color(skin::foreground(ui))).truncate());
                let track=self.loaded_track(deck);
                ui.label(track.map(|t|format!("{} BPM   {}",t.preparation.bpm.map(|b|format!("{:.1}",b*(1.+self.pitch[deck]as f64/100.))).unwrap_or_else(||"--".into()),t.preparation.harmonic_key.as_deref().unwrap_or("--"))).unwrap_or_else(||"Load a track from the library".into()));
                ui.separator();
                ui.add_space(4.);
                if self.dj_cues[deck] {
                    egui::ScrollArea::vertical().id_salt(("deck-cues", deck)).max_height(platter_side).show(ui, |ui| self.main_hot_cues(ui, deck, accent));
                } else {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label("PITCH");
                            ui.spacing_mut().slider_width=platter_side;
                            if ui.add(egui::Slider::new(&mut self.pitch[deck], -16. ..=16.).vertical().show_value(false)).changed() && let Some(a)=&self.audio { a.set_rate(deck,1.+self.pitch[deck]/100.); }
                            ui.label(format!("{:+.1}%",self.pitch[deck]));
                        });
                        let phase=self.audio.as_ref().map_or(0.,|audio|audio.source_position(deck)) as f32;
                        let wheel=skin::platter(ui,platter_side.min((width-85.).max(80.)),accent,self.playing[deck] || self.jog_resume[deck].is_some(),phase).on_hover_text("Drag around the wheel to scratch forward/backward. Scroll to seek; Shift = 5 seconds.");
                        wheel.context_menu(|ui| {
                            if ui.button("Seek back 1 second").clicked() { self.seek_deck_relative(deck, -1.); ui.close(); }
                            if ui.button("Seek forward 1 second").clicked() { self.seek_deck_relative(deck, 1.); ui.close(); }
                            if ui.button("Reset pitch").clicked() { self.pitch[deck] = 0.; if let Some(a)=&self.audio { a.set_rate(deck, 1.); } ui.close(); }
                        });
                        self.jog_wheel(ui, deck, &wheel);
                    });
                }
                let transport_width=(ui.available_width()-ui.spacing().item_spacing.x*3.)/4.;
                let transport=ui.horizontal(|ui|{
                    if skin::deck_button(ui,"CUE",accent,transport_width,false).on_hover_text("Return to the prepared cue IN and pause").clicked(){self.recue_deck(deck);}
                    if skin::deck_button(ui,if self.playing[deck]{"PAUSE"}else{"PLAY"},accent,transport_width,true).clicked(){if deck==0{self.auto=false;}self.toggle_deck(deck);}
                    if skin::deck_button(ui,"SYNC",accent,transport_width,false).on_hover_text("Match prepared BPM to the other deck. Changes pitch with speed; beat-phase sync/key lock are not available.").clicked(){self.sync_deck(deck);}
                    if skin::deck_button(ui,"STOP",accent,transport_width,false).clicked(){self.stop(deck);}
                });
                ui.ctx().data_mut(|d|{d.insert_temp(egui::Id::unique(("dj-transport",deck)),transport.response.rect);d.insert_temp(egui::Id::unique(("dj-transport",deck)).with("screen"),(transport.response.rect,ui.layer_id()));});
                ui.horizontal(|ui| {
                    let button_width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.;
                    let looping=self.audio.as_ref().is_some_and(|a|a.controls.looping[deck].load(std::sync::atomic::Ordering::Relaxed));
                    if ui.add_sized([button_width, 36.], egui::Button::new(if looping { "Loop off" } else { "Loop" })).clicked() {
                        let seconds=self.loaded_track(deck).and_then(|t|t.preparation.bpm).map_or(2.,|bpm|4.*60./(bpm*(1.+self.pitch[deck]as f64/100.)));
                        if let Some(a)=&self.audio && let Err(e)=a.loop_toggle(deck,seconds as f32){self.status=e;}
                    }
                    ui.add_sized([button_width, 36.], egui::Button::selectable(self.dj_cues[deck], "Hot Cues").frame(true)).clicked().then(|| self.dj_cues[deck] = !self.dj_cues[deck]);
                });

            });
        });
        let response = ui
            .interact(
                area.response.rect,
                ui.scope_id().with(("deck-drop", deck)),
                egui::Sense::hover(),
            )
            .on_hover_text(
                "Drop to preload. Playing decks stage the replacement without interrupting audio.",
            );
        if let Some(path) = dragdrop::target(&response, accent)
            && let Err(error) = self.safe_load_path(deck, path, None)
        {
            self.status = error;
        }
        if self.pending_load[deck].is_some()
            && ui
                .add_enabled(!self.playing[deck], egui::Button::new("Apply staged track"))
                .clicked()
            && let Some((path, cue)) = self.pending_load[deck].take()
            && let Err(error) = self.safe_load_path(deck, path, cue)
        {
            self.status = error;
        }
    }
    fn main_hot_cues(&mut self, ui: &mut egui::Ui, deck: usize, accent: egui::Color32) {
        let prep = self.loaded_track(deck).map(|t| t.preparation.clone());
        if let Some(action) = hot_cues::pads(ui, prep.as_ref(), accent) {
            let position = self.audio.as_ref().map_or(0., |a| a.source_position(deck));
            match action {
                hot_cues::Action::Jump(seconds) => {
                    self.seek_deck_relative(deck, seconds - position)
                }
                _ => {
                    if let Some(track) = self.loaded_track(deck).cloned() {
                        let path = self.loaded_origin[deck]
                            .as_deref()
                            .unwrap_or(&self.root)
                            .join(&track.path);
                        match hot_cues::edit(
                            self.loaded_origin[deck].as_deref().unwrap_or(&self.root),
                            &path,
                            &track.preparation,
                            action,
                            position,
                        ) {
                            Ok(saved) => {
                                let mut snapshot = track;
                                hot_cues::update_snapshot(&mut snapshot.preparation, saved);
                                self.loaded_snapshot[deck] = Some(snapshot);
                            }
                            Err(error) => self.status = error,
                        }
                    }
                }
            }
        }
    }
    fn seek_deck_relative(&mut self, deck: usize, delta: f64) {
        let Some(mut track) = self.loaded_track(deck).cloned() else {
            self.status = "Load a track before seeking".into();
            return;
        };
        let Some(audio) = &self.audio else {
            return;
        };
        let was_playing = audio.playing(deck);
        let end = track.preparation.cue_out.or(track
            .preparation
            .analysis
            .as_ref()
            .map(|a| a.duration));
        let position = (audio.source_position(deck) + delta).max(0.);
        track.preparation.cue_in = end.map_or(position, |end| position.min((end - 0.02).max(0.)));
        self.run.cancel_automation();
        self.auto = false;
        if let Some(audio) = &mut self.audio
            && let Err(error) = audio.load(
                deck,
                self.loaded_origin[deck].as_deref().unwrap_or(&self.root),
                &track,
                was_playing,
            )
        {
            self.status = error;
        }
    }
    fn jog_wheel(&mut self, ui: &mut egui::Ui, deck: usize, wheel: &egui::Response) {
        let Some(position) = wheel.interact_pointer_pos() else {
            if wheel.hovered() {
                let scroll: f32 = ui.input(|i| {
                    i.raw
                        .events
                        .iter()
                        .filter_map(|event| {
                            if let egui::Event::MouseWheel { delta, .. } = event {
                                Some(delta.y)
                            } else {
                                None
                            }
                        })
                        .sum()
                });
                if scroll != 0. {
                    self.seek_deck_relative(
                        deck,
                        scroll.signum() as f64
                            * if ui.input(|i| i.modifiers.shift) {
                                5.
                            } else {
                                1.
                            },
                    );
                }
            }
            return;
        };
        let relative = position - wheel.rect.center();
        let angle = relative.y.atan2(relative.x);
        if wheel.is_pointer_button_down_on()
            && ui.input(|i| i.pointer.primary_down())
            && self.jog_resume[deck].is_none()
            && let Some(audio) = &self.audio
        {
            if !audio.loaded(deck) {
                return;
            }
            self.jog_resume[deck] = Some((audio.playing(deck), ui.ctx().viewport_id()));
            self.run.cancel_effects(audio);
            self.jog_angle[deck] = angle;
            self.run.cancel_automation();
            self.auto = false;
            if audio.controls.looping[deck].load(std::sync::atomic::Ordering::Relaxed) {
                let _ = audio.loop_toggle(deck, 2.);
            }
            audio.set_fader(audio.fader());
            audio.scratch(deck, true, 0.);
            audio.controls.play[deck].store(true, std::sync::atomic::Ordering::Relaxed);
            ui.ctx().request_repaint_after(Duration::from_millis(8));
            return;
        }
        if wheel.is_pointer_button_down_on() && self.jog_resume[deck].is_some() {
            let delta = (angle - self.jog_angle[deck] + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            self.jog_angle[deck] = angle;
            let dt = ui.input(|i| i.stable_dt).max(0.001);
            let rate = if relative.length() < wheel.rect.width() * 0.2 {
                // Near the spindle angle is unstable. A straight finger movement
                // still moves the record, instead of jumping through half a turn.
                let motion = ui.input(|i| i.pointer.delta());
                (motion.x - motion.y) / wheel.rect.width().max(1.) / dt
            } else {
                delta / std::f32::consts::TAU * 1.8 / dt
            };
            if let Some(audio) = &self.audio {
                audio.scratch(deck, true, rate);
            }
            ui.ctx().request_repaint_after(Duration::from_millis(8));
        }
    }
    fn release_jog_wheels(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.pointer.primary_down() && i.viewport().focused.unwrap_or(true)) {
            return;
        }
        for deck in 0..2 {
            if self.jog_resume[deck].is_some_and(|(_, viewport)| viewport != ctx.viewport_id())
                && self.dj_open
            {
                continue;
            }
            if let Some((resume, _)) = self.jog_resume[deck].take()
                && let Some(audio) = &self.audio
            {
                audio.scratch(deck, false, 0.);
                audio.controls.play[deck].store(resume, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
    fn library_equalizer(&mut self, ui: &mut egui::Ui) {
        ui.heading("Equalizer");
        ui.weak("Player / Deck A and DJ Deck B · live three-band EQ");
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("library-eq")
            .show(ui, |ui| {
                ui.columns(2, |columns| {
                    for (deck, column) in columns.iter_mut().enumerate() {
                        skin::panel(column, false, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.strong(if deck == 0 {
                                "Player / Deck A"
                            } else {
                                "Deck B"
                            });
                            ui.add_space(12.);
                            ui.horizontal(|ui| {
                                for (band, label) in ["Low", "Mid", "High"].iter().enumerate() {
                                    ui.push_id((deck, band), |ui| {
                                        ui.vertical(|ui| {
                                            ui.label(*label);
                                            ui.spacing_mut().slider_width = 150.;
                                            if ui
                                                .add(
                                                    egui::Slider::new(
                                                        &mut self.eq[deck][band],
                                                        -12. ..=12.,
                                                    )
                                                    .vertical()
                                                    .suffix(" dB"),
                                                )
                                                .changed()
                                                && let Some(audio) = &self.audio
                                            {
                                                audio.set_eq(deck, band, self.eq[deck][band]);
                                            }
                                        });
                                    });
                                }
                            });
                            ui.add_space(12.);
                            if ui.button("Reset to flat").clicked() {
                                self.eq[deck] = [0.; 3];
                                if let Some(audio) = &self.audio {
                                    for band in 0..3 {
                                        audio.set_eq(deck, band, 0.);
                                    }
                                }
                            }
                        });
                    }
                });
            });
    }
    fn dj_effects(&mut self, ctx: &egui::Context) {
        if self.eq_open {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("amp-equalizer"),
                egui::ViewportBuilder::default()
                    .with_title("AMP Verse · Equalizer")
                    .with_icon(app_icon())
                    .with_inner_size([420., 410.])
                    .with_min_inner_size([300., 240.])
                    .with_resizable(true)
                    .with_clamp_size_to_monitor_size(true)
                    .with_window_level(if self.player_on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    }),
                |ui, _| {
                    if ui.input(|i| i.viewport().close_requested()) {
                        self.eq_open = false;
                    }
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.heading("Equalizer");
                        ui.weak("Deck A is used for player playback.");
                        ui.spacing_mut().slider_width =
                            (ui.available_width() - 130.).clamp(80., 240.);
                        for d in 0..2 {
                            ui.separator();
                            ui.strong(if d == 0 { "Player / Deck A" } else { "Deck B" });
                            for (b, name) in ["Low", "Mid", "High"].iter().enumerate() {
                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.eq[d][b], -12. ..=12.)
                                            .text(*name)
                                            .suffix(" dB"),
                                    )
                                    .changed()
                                    && let Some(a) = &self.audio
                                {
                                    a.set_eq(d, b, self.eq[d][b]);
                                }
                            }
                            if ui.button("Reset to flat").clicked() {
                                self.eq[d] = [0.; 3];
                                if let Some(a) = &self.audio {
                                    for b in 0..3 {
                                        a.set_eq(d, b, 0.);
                                    }
                                }
                            }
                        }
                    });
                },
            );
        }
        if self.fx_open {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("amp-filter-effects"),
                egui::ViewportBuilder::default()
                    .with_title("AMP Verse · Filter FX")
                    .with_icon(app_icon())
                    .with_inner_size([420., 250.])
                    .with_min_inner_size([300., 180.])
                    .with_resizable(true)
                    .with_clamp_size_to_monitor_size(true)
                    .with_window_level(if self.player_on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    }),
                |ui, _| {
                    if ui.input(|i| i.viewport().close_requested()) {
                        self.fx_open = false;
                    }
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.heading("Filter FX");
                        ui.spacing_mut().slider_width =
                            (ui.available_width() - 155.).clamp(80., 220.);
                        for d in 0..2 {
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.filter[d], 200. ..=22000.)
                                        .logarithmic(true)
                                        .text(if d == 0 { "A low-pass" } else { "B low-pass" })
                                        .suffix(" Hz"),
                                )
                                .changed()
                                && let Some(a) = &self.audio
                            {
                                a.set_filter(d, self.filter[d]);
                            }
                            if ui.button(format!("Bypass {}", d + 1)).clicked() {
                                self.filter[d] = 22000.;
                                if let Some(a) = &self.audio {
                                    a.set_filter(d, 22000.);
                                }
                            }
                        }
                        ui.weak("Channel low-pass filter.");
                    });
                },
            );
        }
    }
    fn cast_ui(&mut self, ui: &mut egui::Ui) {
        self.cast.live_active = self.live_mix.as_ref().is_some_and(|b| b.running());
        if let Some(broadcast) = &self.live_mix {
            self.cast.live_status = broadcast.status.lock().unwrap().clone();
        }
        skin::panel(ui, true, |ui| {
            ui.set_min_size(ui.available_size());
            self.header(ui, "AMP Cast", true);
            egui::ScrollArea::vertical()
                .id_salt("cast-content")
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    self.cast.show(
                        ui,
                        &self.root,
                        self.library.is_some() && self.scan.is_none(),
                    );
                });
        });
        if let Some(start) = self.cast.live_action.take() {
            if start {
                let result = (|| {
                    self.ensure_audio()?;
                    self.ensure_right_samples()?;
                    let (address, password) = self.cast.endpoint();
                    live_mix::Broadcast::start(
                        self.audio.as_ref().unwrap(),
                        self.right_sample_audio.as_ref().unwrap(),
                        address,
                        password,
                    )
                })();
                match result {
                    Ok(broadcast) => self.live_mix = Some(broadcast),
                    Err(error) => self.cast.live_status = error,
                }
            } else {
                self.live_mix = None;
                if let Some(audio) = &self.audio {
                    audio.stop_broadcast_tap();
                }
                if let Some(audio) = &self.right_sample_audio {
                    audio.stop_broadcast_tap();
                }
                self.cast.live_status = "Live DJ broadcast stopped".into();
            }
        }
    }
    fn addons_ui(&mut self, ui: &mut egui::Ui) {
        skin::panel(ui, false, |ui| {
            ui.set_min_size(ui.available_size());
            self.header(ui, "Plugin Manager", false);
            let action = self.addons.show(ui);
            self.activate(ui, action);
        });
    }
}
impl eframe::App for Player {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        if !self.control_attempted && self.screenshot_view.is_empty() {
            self.control_attempted = true;
            if std::env::var_os("AMP_CONTROL_DISABLE").is_none() {
                self.start_control(ui.ctx());
            }
        }
        if self
            .control_restart
            .is_some_and(|at| std::time::Instant::now() >= at)
        {
            self.start_control(ui.ctx());
        }
        self.poll();
        self.release_jog_wheels(ui.ctx());
        self.poll_control();
        self.performance_tick();
        if let Some(c) = &self.control {
            c.publish(self.control_snapshot());
            if self.catalog_updated.elapsed() > Duration::from_secs(2) {
                c.catalog(self.control_catalog());
                self.catalog_updated = std::time::Instant::now();
            }
        }
        if let Some((path, seconds)) = self.preparer.preview.take() {
            if self.run.locked
                || self.run.auto
                || self.audio.as_ref().is_some_and(|a| {
                    a.playing(0)
                        || a.playing(1)
                        || a.controls
                            .sample_playing
                            .iter()
                            .any(|p| p.load(std::sync::atomic::Ordering::Relaxed))
                })
            {
                self.status =
                    "Stop deck playback before auditioning a Library recommendation".into();
            } else if let Some(i) = self
                .library
                .as_ref()
                .and_then(|l| l.tracks.iter().position(|t| t.path == path))
            {
                self.auto = false;
                self.run.cancel_automation();
                self.play_cue(0, i, seconds);
            }
        }
        if !self.lab_open {
            self.lab.stop_preview();
        }
        self.cast.poll();
        self.dj_effects(ui.ctx());
        let ctx = ui.ctx().clone();
        let mut active = vec![egui::ViewportId::ROOT];
        for pane in 1..6 {
            if self.open_panes()[pane] && self.layout.parent[pane].is_none() {
                active.push(window_layout::viewport(pane));
            }
        }
        docking::retain(&ctx, &active);
        match self.screenshot_view.as_str() {
            "library" | "preparation" => self.library_ui(ui),
            "playlists" => {
                self.library_page = 2;
                self.library_ui(ui);
            }
            "analysis" => {
                self.library_page = 6;
                self.library_ui(ui);
            }
            "devices" => {
                self.library_page = 3;
                self.library_ui(ui);
            }
            "rules" => {
                self.show_editor.open = true;
                self.library_page = 1;
                self.library_ui(ui);
            }
            "dj" => self.dj_ui(ui),
            "dj-window" => self.group_ui(ui, 2),
            "compact" => {
                self.player_mode = 1;
                self.group_ui(ui, 0);
            }
            "mini" => {
                self.player_mode = 2;
                self.group_ui(ui, 0);
            }
            "dj-library" => self.dj_library_ui(ui),
            "sampler" => self.sampler_ui(ui),
            "addons" => self.addons_ui(ui),
            "cast" => self.cast_ui(ui),
            "lab" => {
                let action = self.lab.show(ui, &self.root, self.library.as_ref());
                self.activate(ui, action);
            }
            _ => self.group_ui(ui, 0),
        }
        let open = self.open_panes();
        for (pane, is_open) in open.iter().enumerate().skip(1) {
            if !is_open || self.layout.parent[pane].is_some() {
                continue;
            }
            let size = window_layout::initial_size(pane, ctx.input(|i| i.viewport().monitor_size));
            ctx.show_viewport_immediate(
                window_layout::viewport(pane),
                egui::ViewportBuilder::default()
                    .with_title(window_layout::NAMES[pane])
                    .with_decorations(false)
                    .with_icon(app_icon())
                    .with_inner_size(size)
                    .with_min_inner_size([size[0].min(740.), size[1].min(480.)])
                    .with_resizable(true)
                    .with_clamp_size_to_monitor_size(true),
                |ui, _| {
                    if ui.ctx().input(|i| i.viewport().close_requested()) {
                        match pane {
                            1 => self.library_open = false,
                            2 => self.dj_open = false,
                            3 => self.lab_open = false,
                            4 => self.cast_open = false,
                            _ => self.addons_open = false,
                        }
                    }
                    self.group_ui(ui, pane);
                },
            );
        }
        if let Some(path) = std::env::var_os("AMP_DOCK_SMOKE") {
            self.dock_frames += 1;
            docking::smoke(&ctx, self.dock_frames, &PathBuf::from(path));
        }
        if let Some(path) = std::env::var_os("AMP_SCREENSHOT")
            && self.scan.is_none()
        {
            self.screenshot_frames += 1;
            if self.screenshot_frames == 20 {
                capture(&ctx, path.into());
            }
        }
        ctx.request_repaint_after(Duration::from_millis(if self.playing.iter().any(|p| *p) {
            33
        } else {
            120
        }));
    }
}
fn main() -> eframe::Result {
    let mut player = Player::default();
    if !player.root.as_os_str().is_empty() {
        player.scan();
    }
    let size = match player.screenshot_view.as_str() {
        "combined" => [1100., 900.],
        "library" | "playlists" | "analysis" | "devices" => [1280., 860.],
        "preparation" | "rules" => [1200., 830.],
        "dj" => [1000., 920.],
        "dj-window" => [1440., 1000.],
        "compact" => [520., 210.],
        "mini" => [620., 100.],
        "dj-library" => [1100., 430.],
        "sampler" => [800., 850.],
        "lab" => [1100., 760.],
        "addons" => [1120., 760.],
        "cast" => [1120., 800.],
        "player" => [1100., 170.],
        _ => [1280., 900.],
    };
    eframe::run_native(
        "AMP VERSE",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_decorations(false)
                .with_window_level(if player.player_on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                })
                .with_icon(app_icon())
                .with_inner_size(size)
                .with_min_inner_size([size[0].min(740.), size[1].min(160.)])
                .with_resizable(true)
                .with_clamp_size_to_monitor_size(true),
            ..Default::default()
        },
        Box::new(|cc| {
            skin::configure(&cc.egui_ctx);
            Ok(Box::new(player))
        }),
    )
}

#[cfg(test)]
mod window_tests {
    use super::*;
    #[test]
    fn appearance_choices_are_visible_and_clickable_inside_module_menu() {
        for selected in theme::Theme::ALL {
            let ctx = egui::Context::default();
            let mut player = Player::default();
            let mut draw = |position: egui::Pos2, pressed: Option<bool>| {
                let mut raw = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100., 900.),
                    )),
                    ..Default::default()
                };
                raw.events.push(egui::Event::PointerMoved(position));
                if let Some(pressed) = pressed {
                    raw.events.push(egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    });
                }
                let mut output = ctx.run_ui(raw, |ui| player.group_ui(ui, 0));
                output.textures_delta.clear();
            };
            for press in [None, Some(true), Some(false), None] {
                draw(egui::pos2(100., 25.), press);
            }
            let (rect, clip, layer) = ctx
                .data(|d| {
                    d.get_temp::<(egui::Rect, egui::Rect, egui::LayerId)>(egui::Id::unique((
                        "theme-choice",
                        selected.id(),
                    )))
                })
                .expect("Theme must be directly visible");
            let target =
                ctx.layer_transform_to_global(layer).unwrap_or_default() * rect.intersect(clip);
            assert!(target.height() > 10.);
            for press in [None, Some(true), Some(false), None] {
                draw(target.center(), press);
            }
            assert_eq!(theme::current(&ctx), selected);
        }
    }
    #[test]
    fn short_player_module_menu_is_visible_and_clickable() {
        let ctx = egui::Context::default();
        let mut player = Player {
            library_open: false,
            dj_open: false,
            ..Default::default()
        };
        let mut draw = |position: egui::Pos2, pressed: Option<bool>| {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100., 170.),
                )),
                ..Default::default()
            };
            raw.events.push(egui::Event::PointerMoved(position));
            if let Some(pressed) = pressed {
                raw.events.push(egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                });
            }
            let mut output = ctx.run_ui(raw, |ui| player.group_ui(ui, 0));
            output.textures_delta.clear();
        };
        for press in [None, Some(true), Some(false), None] {
            draw(egui::pos2(100., 25.), press);
        }
        let (rect, clip, layer) = ctx
            .data(|data| {
                data.get_temp::<(egui::Rect, egui::Rect, egui::LayerId)>(egui::Id::unique((
                    "module-menu-item",
                    "AMP DJ",
                )))
            })
            .expect("Module menu must open");
        let transform = ctx.layer_transform_to_global(layer).unwrap_or_default();
        let target = transform * rect.intersect(clip);
        assert!(
            target.height() > 5. && target.top() >= 0. && target.bottom() <= 170.,
            "Menu item must remain visible in the short player: {target:?}"
        );
        for press in [None, Some(true), Some(false)] {
            draw(target.center(), press);
        }
        assert!(
            player.dj_open,
            "Clicking the visible menu item must open DJ"
        );
    }

    fn frame(ctx: &egui::Context, player: &mut Player, pressed: Option<bool>) {
        frame_neighbor(ctx, player, pressed, 0);
    }
    fn frame_neighbor(
        ctx: &egui::Context,
        player: &mut Player,
        pressed: Option<bool>,
        neighbor: usize,
    ) {
        let mut raw = egui::RawInput::default();
        let info = raw.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
        info.inner_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1100., 800.),
        ));
        info.outer_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1100., 170.),
        ));
        raw.viewports.insert(
            window_layout::viewport(1),
            egui::ViewportInfo {
                inner_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0., 170.),
                    egui::vec2(1100., 800.),
                )),
                outer_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0., 170.),
                    egui::vec2(1100., 800.),
                )),
                ..Default::default()
            },
        );
        if neighbor == 2 {
            raw.viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .outer_rect = Some(egui::Rect::from_min_size(
                egui::pos2(5000., 0.),
                egui::vec2(1100., 170.),
            ));
            raw.viewports.insert(
                window_layout::viewport(2),
                egui::ViewportInfo {
                    outer_rect: Some(egui::Rect::from_min_size(
                        egui::pos2(0., 970.),
                        egui::vec2(1100., 800.),
                    )),
                    ..Default::default()
                },
            );
        }
        let pos = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("magnet-button-rect")))
            .map_or(egui::pos2(978., 29.), |r| r.center());
        raw.events.push(egui::Event::PointerMoved(pos));
        if let Some(pressed) = pressed {
            raw.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        }
        let mut output = ctx.run_ui(raw, |ui| player.group_ui(ui, 1));
        output.textures_delta.clear();
    }
    #[test]
    fn library_refresh_remaps_indices_and_keeps_session_snapshots() {
        let track = |name: &str, bpm| amp_core::Track {
            path: name.into(),
            title: name.into(),
            preparation: amp_core::Preparation {
                bpm: Some(bpm),
                ..Default::default()
            },
        };
        let tree = || Folder {
            name: "Music".into(),
            folders: vec![],
            tracks: vec![],
        };
        let old = Library {
            tracks: vec![
                track("a.wav", 120.),
                track("b.wav", 120.),
                track("c.wav", 120.),
            ],
            tree: tree(),
        };
        let mut player = Player {
            loaded: [Some(1), None],
            loaded_snapshot: [Some(old.tracks[1].clone()), None],
            selected: Some(0),
            queue: vec![2, 1, 1, 0],
            queue_selected: Some(2),
            playing: [true, false],
            auto: true,
            library: Some(old),
            ..Default::default()
        };
        player.run.auto = true;
        player.run.preloaded = Some((1, 2));
        player.accept_library_scan(Library {
            tracks: vec![
                track("c.wav", 120.),
                track("a.wav", 120.),
                track("b.wav", 126.),
                track("new.wav", 124.),
            ],
            tree: tree(),
        });
        assert_eq!(player.loaded, [Some(2), None]);
        assert_eq!(player.queue, vec![0, 2, 2, 1]);
        assert_eq!(player.queue_selected, Some(2));
        assert_eq!(player.selected, Some(1));
        assert_eq!(player.run.preloaded, Some((1, 0)));
        assert!(player.playing[0] && player.auto && player.run.auto);
        assert_eq!(player.loaded_track(0).unwrap().preparation.bpm, Some(120.));
        assert_eq!(
            player.library.as_ref().unwrap().tracks[2].preparation.bpm,
            Some(126.)
        );
    }
    #[test]
    fn small_player_modes_preserve_session_and_magnetic_layout() {
        let ctx = egui::Context::default();
        let mut player = Player {
            loaded: [Some(4), Some(6)],
            playing: [true, false],
            queue: vec![4, 6],
            library_open: true,
            library_page: 0,
            playback_history: Vec::new(),
            ..Default::default()
        };
        let parent = player.layout.parent;
        for mode in [1, 2, 0] {
            player.set_player_mode(&ctx, mode);
            assert_eq!(player.loaded, [Some(4), Some(6)]);
            assert_eq!(player.playing, [true, false]);
            assert_eq!(player.queue, vec![4, 6]);
            assert_eq!(player.layout.parent, parent);
            assert!(player.library_open);
        }
    }
    #[test]
    fn expanded_dj_moves_banks_below_and_restore_returns_them_to_sides() {
        let ctx = egui::Context::default();
        let mut player = Player::default();
        for (size, expanded, below) in [
            (egui::vec2(1920., 1080.), true, true),
            (egui::vec2(1440., 900.), true, true),
            (egui::vec2(1000., 760.), false, false),
        ] {
            for _ in 0..2 {
                let mut raw = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                raw.viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .maximized = Some(expanded);
                let mut output = ctx.run_ui(raw, |ui| player.group_ui(ui, 2));
                output.textures_delta.clear();
            }
            let bounds = |id: egui::Id| {
                let (rect, layer) = ctx
                    .data(|d| d.get_temp::<(egui::Rect, egui::LayerId)>(id.with("screen")))
                    .unwrap();
                ctx.layer_transform_to_global(layer).unwrap_or_default() * rect
            };
            for side in 0..2 {
                let bank = bounds(egui::Id::unique(("dj-side", side)));
                let transport = bounds(egui::Id::unique(("dj-transport", side)));
                assert!(
                    bank.right() <= size.x + 1. && bank.bottom() <= size.y + 1.,
                    "Bank escaped {size:?}: {bank:?}"
                );
                if below {
                    assert!(
                        bank.top() > transport.bottom(),
                        "Pads must sit below their deck"
                    );
                } else if side == 0 {
                    assert!(bank.right() < transport.left());
                } else {
                    assert!(bank.left() > transport.right());
                }
            }
            let browser = bounds(egui::Id::unique("dj-browser-screen"));
            assert!(
                browser.bottom() <= size.y + 1.,
                "Library escaped {size:?}: {browser:?}"
            );
            assert!(!player.run.auto);
        }
    }
    #[test]
    fn resized_mixer_controls_stay_inside_the_viewport() {
        let ctx = egui::Context::default();
        let mut player = Player::default();
        for size in [
            egui::vec2(2560., 1550.),
            egui::vec2(1200., 900.),
            egui::vec2(740., 480.),
            egui::vec2(540., 360.),
            egui::vec2(1000., 760.),
        ] {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let mut output = ctx.run_ui(raw, |ui| player.group_ui(ui, 2));
            output.textures_delta.clear();
            for id in [
                egui::Id::unique(("dj-transport", 0usize)),
                egui::Id::unique(("dj-transport", 1usize)),
                egui::Id::unique("dj-auto-fade-bounds"),
                egui::Id::unique(("dj-stems", 0usize)),
                egui::Id::unique(("dj-stems", 1usize)),
                egui::Id::unique("dj-browser-screen"),
            ] {
                let (rect, layer) = ctx
                    .data(|d| d.get_temp::<(egui::Rect, egui::LayerId)>(id.with("screen")))
                    .unwrap();
                let screen = ctx.layer_transform_to_global(layer).unwrap_or_default() * rect;
                assert!(
                    screen.left() >= -1.
                        && screen.top() >= -1.
                        && screen.right() <= size.x + 1.
                        && screen.bottom() <= size.y + 1.,
                    "{size:?}: control outside window {screen:?}"
                );
            }
        }
    }
    #[test]
    fn dj_long_titles_and_deep_folders_do_not_expand_outside_resized_window() {
        let ctx = egui::Context::default();
        let long = "Very long album artist folder and extended remix title ".repeat(12);
        let track = amp_core::Track {
            path: PathBuf::from("song.wav"),
            title: long.clone(),
            preparation: amp_core::Preparation {
                manual_cues: (1..=8)
                    .map(|number| amp_core::preparation::Cue {
                        id: format!("cue-{number}"),
                        label: long.clone(),
                        number: Some(number),
                        seconds: 12.,
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            },
        };
        let mut player = Player {
            library: Some(Library {
                tracks: vec![track.clone()],
                tree: Folder {
                    name: long.clone(),
                    folders: vec![Folder {
                        name: long,
                        folders: vec![],
                        tracks: vec![0],
                    }],
                    tracks: vec![],
                },
            }),
            loaded: [Some(0), Some(0)],
            loaded_snapshot: [Some(track.clone()), Some(track)],
            ..Default::default()
        };
        player.run.settings.playlists[0].name = "Long Smart Playlist name ".repeat(30);
        player.run.choice = player.run.settings.playlists[0].id.clone();
        for smart in [false, true] {
            player.dj_cues = [smart; 2];
            player.dj_browser = Default::default();
            if smart {
                player.dj_browser.select_smart(&player.run.choice);
            }
            for size in [
                egui::vec2(900., 600.),
                egui::vec2(1000., 760.),
                egui::vec2(740., 480.),
            ] {
                for _ in 0..2 {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ui| player.group_ui(ui, 2),
                    );
                    output.textures_delta.clear();
                    let rect = ctx
                        .data(|d| {
                            d.get_temp::<egui::Rect>(egui::Id::unique(("pane-bounds", 2usize)))
                        })
                        .unwrap();
                    let layer = ctx
                        .data(|d| {
                            d.get_temp::<egui::LayerId>(egui::Id::unique(("pane-layer", 2usize)))
                        })
                        .unwrap();
                    let screen = ctx.layer_transform_to_global(layer).unwrap_or_default() * rect;
                    assert!(
                        screen.right() <= size.x + 1. && screen.bottom() <= size.y + 1.,
                        "{size:?}: content escaped {screen:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn dj_library_drag_moves_boundary_and_fills_bottom_in_scaled_fullscreen() {
        let ctx = egui::Context::default();
        let mut player = Player::default();
        let size = egui::vec2(1920., 1080.);
        let bounds = |id: &str| {
            let (rect, layer) = ctx
                .data(|d| {
                    d.get_temp::<(egui::Rect, egui::LayerId)>(egui::Id::unique(id).with("bounds"))
                })
                .unwrap();
            ctx.layer_transform_to_global(layer).unwrap_or_default() * rect
        };
        let mut point = egui::Pos2::ZERO;
        let mut initial = 0.;
        let mut final_top = 0.;
        for frame in 0..7 {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            raw.viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .maximized = Some(true);
            if frame >= 2 {
                raw.events.push(egui::Event::PointerMoved(point));
                if frame == 2 || frame == 4 {
                    raw.events.push(egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 2,
                        modifiers: Default::default(),
                    });
                }
            }
            let mut output = ctx.run_ui(raw, |ui| player.group_ui(ui, 2));
            output.textures_delta.clear();
            let divider = bounds("dj-library-divider");
            if frame == 1 {
                point = divider.center();
                initial = divider.top();
            }
            if frame == 2 {
                point.y -= 170.;
            }
            if frame == 6 {
                final_top = divider.top();
                let (rect, layer) = ctx
                    .data(|d| {
                        d.get_temp::<(egui::Rect, egui::LayerId)>(
                            egui::Id::unique("dj-browser-screen").with("screen"),
                        )
                    })
                    .unwrap();
                let screen = ctx.layer_transform_to_global(layer).unwrap_or_default() * rect;
                assert!(
                    screen.bottom() > size.y - 35. && screen.bottom() <= size.y,
                    "Song list must fill bottom: {screen:?}"
                );
            }
        }
        assert!(
            final_top < initial - 50.,
            "Library divider did not move upward: {initial} -> {final_top}"
        );
    }
    #[test]
    fn enlarged_waveforms_and_library_remain_inside_the_window() {
        let ctx = egui::Context::default();
        let mut player = Player::default();
        ctx.data_mut(|d| {
            d.insert_persisted(egui::Id::unique("dj-wave-height"), 140_f32);
            d.insert_persisted(egui::Id::unique("dj-library-height"), 600_f32);
            d.insert_persisted(egui::Id::unique("dj-source-width"), 400_f32);
        });
        for size in [egui::vec2(740., 480.), egui::vec2(1920., 1080.)] {
            for _ in 0..2 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| player.group_ui(ui, 2),
                );
                output.textures_delta.clear();
                let rect = ctx
                    .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("pane-bounds", 2usize))))
                    .unwrap();
                let layer = ctx
                    .data(|d| d.get_temp::<egui::LayerId>(egui::Id::unique(("pane-layer", 2usize))))
                    .unwrap();
                let screen = ctx.layer_transform_to_global(layer).unwrap_or_default() * rect;
                assert!(
                    screen.right() <= size.x + 1. && screen.bottom() <= size.y + 1.,
                    "{size:?}: {screen:?}"
                );
            }
        }
    }
    #[test]
    fn dj_container_fits_a_short_viewport() {
        let ctx = egui::Context::default();
        let mut player = Player::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900., 600.),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw, |ui| player.group_ui(ui, 2));
        output.textures_delta.clear();
        let dj = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("pane-bounds", 2usize))))
            .unwrap();
        let layer = ctx
            .data(|d| d.get_temp::<egui::LayerId>(egui::Id::unique(("pane-layer", 2usize))))
            .unwrap();
        let screen = ctx.layer_transform_to_global(layer).unwrap_or_default() * dj;
        assert!(
            screen.bottom() <= 600. && screen.right() <= 900.,
            "DJ must fit viewport: {screen:?}"
        );
        let (browser, layer) = ctx
            .data(|d| {
                d.get_temp::<(egui::Rect, egui::LayerId)>(
                    egui::Id::unique("dj-browser-screen").with("screen"),
                )
            })
            .unwrap();
        let screen = ctx.layer_transform_to_global(layer).unwrap_or_default() * browser;
        assert!(
            screen.height() > 100. && screen.bottom() <= 600.,
            "Song browser must remain visible below decks: {screen:?}"
        );
    }
    #[test]
    fn dj_below_attached_library_has_finite_reachable_bounds() {
        let ctx = egui::Context::default();
        let mut player = Player {
            library_open: true,
            library_page: 0,
            playback_history: Vec::new(),
            dj_open: true,
            ..Default::default()
        };
        assert!(player.layout.join(2, 1));
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100., 900.),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw, |ui| player.group_ui(ui, 0));
        output.textures_delta.clear();
        let library = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("pane-bounds", 1usize))))
            .unwrap();
        let dj = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("pane-bounds", 2usize))))
            .unwrap();
        assert!(
            library.height() < 800.,
            "Library must not consume the scroll area's infinite height: {library:?}"
        );
        assert!(
            dj.top().is_finite() && dj.top() < 1200.,
            "DJ must follow Library within reachable scrolling distance: {dj:?}"
        );
        assert!(dj.top() >= library.bottom());
    }
    #[test]
    fn library_can_merge_into_dj_instead_of_player() {
        let ctx = egui::Context::default();
        let mut player = Player {
            library_open: true,
            library_page: 0,
            playback_history: Vec::new(),
            dj_open: true,
            ..Default::default()
        };
        player.layout.parent[1] = None;
        for press in [None, Some(true), Some(false)] {
            frame_neighbor(&ctx, &mut player, press, 2);
        }
        assert_eq!(player.layout.parent[1], Some(2));
        for press in [None, Some(true), Some(false)] {
            frame_neighbor(&ctx, &mut player, press, 2);
        }
        assert_eq!(player.layout.parent[1], None);
    }
    #[test]
    fn actual_magnet_and_detach_buttons_change_window_ownership() {
        let ctx = egui::Context::default();
        let mut player = Player {
            library_open: true,
            library_page: 0,
            playback_history: Vec::new(),
            ..Default::default()
        };
        player.layout.parent[1] = None;
        for press in [None, Some(true), Some(false)] {
            frame(&ctx, &mut player, press);
        }
        assert_eq!(player.layout.parent[1], Some(0));
        for press in [None, Some(true), Some(false)] {
            frame(&ctx, &mut player, press);
        }
        assert_eq!(player.layout.parent[1], None);
    }
}
