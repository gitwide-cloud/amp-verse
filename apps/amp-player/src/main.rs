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
mod dj_browser;
mod docking;
mod engine;
mod lab;
mod mixer;
mod performance;
mod preparer;
mod sampler;
mod show_editor;
mod skin;
mod theme;
mod waveform;
mod window_layout;
use skin::{CYAN, GOLD, ORANGE};

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
    scan: Option<mpsc::Receiver<Result<Library, String>>>,
    selected: Option<usize>,
    folder: PathBuf,
    search: String,
    audio: Option<mixer::Engine>,
    sampler: sampler::Sampler,
    dj_browser: dj_browser::Browser,
    dj_tab: usize,
    player_mode: u8,
    mini_on_top: bool,
    run: performance::Run,
    show_editor: show_editor::Editor,
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
    eq_open: bool,
    fx_open: bool,
    eq: [[f32; 3]; 2],
    filter: [f32; 2],
    loaded: [Option<usize>; 2],
    loaded_snapshot: [Option<amp_core::Track>; 2],
    playing: [bool; 2],
    volume: u8,
    playlist_snapshot: Option<(PathBuf, Playlist)>,
    queue: Vec<usize>,
    queue_selected: Option<usize>,
    auto: bool,
    shuffle: bool,
    repeat_one: bool,
    library_open: bool,
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
            selected: None,
            folder: PathBuf::new(),
            search: String::new(),
            audio: None,
            sampler: Default::default(),
            dj_browser: Default::default(),
            dj_tab: 0,
            player_mode: 0,
            mini_on_top: false,
            run: Default::default(),
            show_editor: Default::default(),
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
            fade_seconds: 8.,
            eq_open: false,
            fx_open: false,
            eq: [[0.; 3]; 2],
            filter: [22000.; 2],
            loaded: [None; 2],
            loaded_snapshot: [None, None],
            playing: [false; 2],
            volume: 75,
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
            library_open: std::env::var_os("AMP_DOCK_SMOKE").is_some()
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
        let root = self.root.clone();
        let (tx, rx) = mpsc::channel();
        self.scan = Some(rx);
        self.status = "Scanning music folders...".into();
        std::thread::spawn(move || {
            let _ = tx.send(amp_core::scan(&root).map_err(|e| e.to_string()));
        });
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
    fn poll(&mut self) {
        if let Some(engine) = &mut self.audio {
            self.sampler.poll(engine);
        }
        self.preparer.poll(&self.root, self.library.as_mut());
        if let Some(rx) = &self.scan {
            match rx.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(lib) => {
                            self.accept_library_scan(lib);
                            match amp_core::show::Settings::load(&self.root) {
                                Ok(settings) => self.run.settings = settings,
                                Err(e) => self.status = e,
                            }
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
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.scan = None;
                    self.status = "Scan interrupted".into();
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
                    self.loaded_snapshot[deck] = Some(track.clone());
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
            self.root = root;
            let pref = library_preference();
            if let Some(parent) = pref.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(pref, self.root.to_string_lossy().as_bytes());
            self.scan();
        }
    }
    fn save_playlist(&mut self) {
        if let Some(lib) = &self.library
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("AMP playlist", &["json"])
                .save_file()
        {
            let list = Playlist {
                sync: self
                    .playlist_snapshot
                    .as_ref()
                    .filter(|(p, _)| p == &path)
                    .and_then(|(_, list)| list.sync.clone()),
                version: 1,
                name: "Playlist".into(),
                tracks: self
                    .queue
                    .iter()
                    .map(|&i| lib.tracks[i].path.clone())
                    .collect(),
            };
            match list.save_in_library(&path, &self.root) {
                Ok(()) => {
                    self.status = "Playlist saved".into();
                    if let Ok(saved) = Playlist::load(&path, &self.root) {
                        self.playlist_snapshot = Some((path, saved));
                    }
                }
                Err(e) => self.status = e.to_string(),
            }
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
        let response =
            egui::CollapsingHeader::new(RichText::new(&folder.name).color(skin::accent(ui)))
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
            let preferred = |p| {
                if p == 0 {
                    135.
                } else if p == 2 {
                    600.
                } else {
                    680.
                }
            };
            let minimum_height: f32 = panes.iter().map(|&p| preferred(p) + 8.).sum();
            let available = ui.available_size().max(egui::vec2(1., 1.));
            let scale = (available.x / 900.)
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
            let mut bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, logical);
            egui::Scene::new()
                .sense(egui::Sense::hover())
                .zoom_range(scale..=scale)
                .max_inner_size(logical)
                .show(ui, &mut bounds, |ui| self.group_contents(ui, host));
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
                        d.insert_temp(egui::Id::unique(("pane-bounds", host)), ui.min_rect())
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
                    if ui
                        .add_sized(
                            [58., 48.],
                            egui::Button::new(RichText::new(caption).color(skin::accent(ui))),
                        )
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
                if ui.button("EQ").clicked() {
                    self.eq_open = true;
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
                egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal),
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
                egui::ViewportCommand::WindowLevel(if self.mini_on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                }),
            );
        }
    }
    fn player_view_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Player view", |ui| {
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
                .checkbox(&mut self.mini_on_top, "Keep small player on top")
                .changed()
                && self.player_mode != 0
            {
                ui.ctx().send_viewport_cmd_to(
                    egui::ViewportId::ROOT,
                    egui::ViewportCommand::WindowLevel(if self.mini_on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    }),
                );
            }
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
        let mut bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, design);
        egui::Scene::new()
            .sense(egui::Sense::hover())
            .zoom_range(scale..=scale)
            .max_inner_size(design)
            .show(ui, &mut bounds, |ui| {
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
                                            if let Some(i) = self
                                                .selected
                                                .or_else(|| self.queue.first().copied())
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
    fn library_ui(&mut self, ui: &mut egui::Ui) {
        self.show_editor.show(
            ui.ctx(),
            &self.root,
            &mut self.run.settings,
            self.library.as_ref(),
        );

        self.preparer
            .show(ui.ctx(), &self.root, self.library.as_mut());
        skin::panel(ui, false, |ui| {
            ui.set_min_size(ui.available_size());
            self.header(ui, "AMP Library", false);
            ui.horizontal(|ui| {
                if ui.button("Smart Playlist rules / defaults…").clicked() {
                    self.show_editor.open = true;
                }
                ui.weak(&self.control_status);
            });
            egui::CollapsingHeader::new("Companion / show-control connection").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Listen address");
                    ui.text_edit_singleline(&mut self.control_bind);
                    ui.label("Optional password");
                    ui.add(egui::TextEdit::singleline(&mut self.control_password).password(true));
                });
                ui.horizontal(|ui| {
                    if ui.button("Start / restart control").clicked() {
                        self.start_control(ui.ctx());
                    }
                    if ui.button("Stop control connection").clicked() {
                        self.control = None;
                        self.control_restart = None;
                        self.control_status =
                            "Show control stopped; audio continues locally".into();
                    }
                });
            });

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.scan.is_none(),
                        egui::Button::new("Choose music folder..."),
                    )
                    .clicked()
                {
                    self.choose_folder();
                }
                if ui
                    .add_enabled(
                        self.scan.is_none() && !self.root.as_os_str().is_empty(),
                        egui::Button::new("Rescan"),
                    )
                    .clicked()
                {
                    self.scan();
                }
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Search songs or folders…")
                        .desired_width((ui.available_width() - 70.).max(200.)),
                );
                if ui.button("Clear").clicked() {
                    self.search.clear();
                }
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.preparer.busy() && self.selected.is_some(),
                        egui::Button::new("Edit song…"),
                    )
                    .clicked()
                    && let Some(t) = self
                        .selected
                        .and_then(|i| self.library.as_ref()?.tracks.get(i))
                {
                    self.preparer.edit(&self.root, t);
                }
                if ui
                    .add_enabled(
                        !self.preparer.busy() && self.library.is_some(),
                        egui::Button::new("Analyze music"),
                    )
                    .clicked()
                    && let Some(lib) = &self.library
                {
                    self.preparer.batch(self.root.clone(), lib.tracks.clone());
                }
                ui.label(&self.preparer.status);
            });
            egui::CollapsingHeader::new("Analysis settings").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Parallel songs");
                    ui.add_enabled(
                        !self.preparer.busy(),
                        egui::DragValue::new(&mut self.preparer.workers)
                            .range(0..=amp_core::batch::cpu_count().min(32)),
                    );
                    let count = amp_core::batch::workers(self.preparer.workers, usize::MAX);
                    ui.weak(format!(
                        "{} = {count} songs at once · {} logical CPUs",
                        if self.preparer.workers == 0 {
                            "Auto"
                        } else {
                            "Selected"
                        },
                        amp_core::batch::cpu_count()
                    ));
                });
            });
            let content_height = (ui.available_height() - 150.).max(300.);
            ui.horizontal_top(|ui| {
                let sidebar_width = 210.;
                skin::panel(ui, false, |ui| {
                    ui.set_width(sidebar_width);
                    ui.set_min_height(content_height);
                    skin::label(ui, "Music folders");
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("folders")
                        .max_height(content_height - 95.)
                        .show(ui, |ui| {
                            if let Some(lib) = &self.library {
                                Self::folder_tree(ui, &lib.tree, Path::new(""), &mut self.folder);
                            } else {
                                ui.weak("Choose a local or mounted NAS folder.");
                            }
                        });
                    ui.separator();
                    ui.label(RichText::new("FOLDERS + PLAYLISTS").color(skin::accent(ui)));
                });
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    skin::panel(ui, false, |ui| {
                        ui.set_min_width(ui.available_width());
                        skin::label(
                            ui,
                            if self.folder.as_os_str().is_empty() {
                                "Music / All files"
                            } else {
                                self.folder.to_str().unwrap_or("Music")
                            },
                        );
                        let mut play = None;
                        egui::ScrollArea::both()
                            .id_salt("files")
                            .auto_shrink([false, false])
                            .max_height(content_height * 0.45)
                            .min_scrolled_height(content_height * 0.45)
                            .show(ui, |ui| {
                                egui::Grid::new("track-table")
                                    .striped(true)
                                    .min_col_width(64.)
                                    .show(ui, |ui| {
                                        for h in [
                                            "Mix", "Name", "BPM", "Key", "Cues", "Cue IN", "Cue OUT", "Rating", "Energy",
                                            "Type",
                                        ] {
                                            ui.label(
                                                RichText::new(h).strong().color(skin::accent(ui)),
                                            );
                                        }
                                        ui.end_row();
                                        if let Some(lib) = &self.library {
                                            for (i, track) in lib.tracks.iter().enumerate() {
                                                if !track.path.starts_with(&self.folder)
                                                    || !track
                                                        .title
                                                        .to_lowercase()
                                                        .contains(&self.search.to_lowercase())
                                                {
                                                    continue;
                                                }
                                                let mut included=self.queue.contains(&i);
                                                if ui.checkbox(&mut included, "").changed() {if included {self.queue.push(i);} else {self.queue.retain(|&q|q!=i);self.queue_selected=None;}}
                                                let response = ui.selectable_label(
                                                    self.selected == Some(i),
                                                    &track.title,
                                                );
                                                if response.clicked() {
                                                    self.selected = Some(i);
                                                }
                                                if response.double_clicked() {
                                                    self.selected = Some(i);
                                                    play = Some(i);
                                                }
                                                ui.label(
                                                    track
                                                        .preparation
                                                        .bpm
                                                        .map(|x| format!("{x:.1}"))
                                                        .unwrap_or_else(|| "--".into()),
                                                );
                                                ui.label(
                                                    track
                                                        .preparation
                                                        .harmonic_key
                                                        .as_deref()
                                                        .or(track.preparation.key.as_deref())
                                                        .unwrap_or("--"),
                                                );
                                                ui.label(
                                                    (track.preparation.cues.len()
                                                        + track.preparation.manual_cues.len())
                                                    .to_string(),
                                                );
                                                ui.label(format!(
                                                    "{:.2}s",
                                                    track.preparation.cue_in
                                                ));
                                                ui.label(
                                                    track
                                                        .preparation
                                                        .cue_out
                                                        .map(|x| format!("{x:.2}s"))
                                                        .unwrap_or_else(|| "End".into()),
                                                );
                                                ui.label(track.preparation.extensions.get("rating").and_then(|v|v.as_u64()).map(|n|format!("{n}/5")).unwrap_or_else(||"--".into()));
                                                ui.label(amp_core::smart::energy(&track.preparation).map(|e|format!("{:.0}%",e*100.)).unwrap_or_else(||"--".into()));
                                                ui.label(
                                                    track
                                                        .path
                                                        .extension()
                                                        .unwrap_or_default()
                                                        .to_string_lossy()
                                                        .to_uppercase(),
                                                );
                                                ui.end_row();
                                            }
                                        }
                                    });
                            });
                        if let Some(i) = play {
                            self.play(0, i);
                            if let Some(audio) = &self.audio {
                                audio.set_fader(0.);
                            }
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Add folder to playlist").clicked() && let Some(lib)=&self.library {
                                for (i,t) in lib.tracks.iter().enumerate() {if t.path.starts_with(&self.folder) && !self.queue.contains(&i) {self.queue.push(i);}}
                            }
                            if ui
                                .add_enabled(
                                    self.selected.is_some(),
                                    egui::Button::new("Add to playlist"),
                                )
                                .clicked()
                                && let Some(i) = self.selected
                            {
                                self.queue.push(i);
                            }
                            if ui
                                .add_enabled(
                                    self.selected.is_some(),
                                    egui::Button::new("Play selected"),
                                )
                                .clicked()
                                && let Some(i) = self.selected
                            {
                                self.play(0, i);
                            }
                        });
                    });
                    skin::panel(ui, false, |ui| {
                        ui.set_min_width(ui.available_width());
                        skin::label(ui, &format!("Playlist / {} queued", self.queue.len()));
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Load...").clicked() {
                                self.load_playlist();
                            }
                            if ui
                                .add_enabled(!self.queue.is_empty(), egui::Button::new("Save..."))
                                .clicked()
                            {
                                self.save_playlist();
                            }
                            if ui.add_enabled(!self.queue.is_empty(),egui::Button::new("Find similar songs")).on_hover_text("Add up to 20 analyzed neighbors from the current library, keeping the first song's energy range.").clicked() {self.expand_smart_list();}
                            if ui.add_enabled(self.queue.len()>1,egui::Button::new("Smart List · order for mixing")).on_hover_text("Keeps the first song as the energy anchor; orders by harmonic key, BPM and energy. Save as a normal playlist.").clicked() {self.curate_playlist();}
                            for (caption,up) in [("Up",true),("Down",false)] {
                                if ui.add_enabled(self.queue_selected.is_some(),egui::Button::new(caption)).clicked() && let Some(i)=self.queue_selected {
                                    let next=if up {i.saturating_sub(1)} else {(i+1).min(self.queue.len().saturating_sub(1))};
                                    if i<self.queue.len() {self.queue.swap(i,next);self.queue_selected=Some(next);}
                                }
                            }
                            if ui.button("Clear").clicked() {
                                self.queue.clear();
                                self.queue_selected = None;
                            }
                            if ui
                                .add_enabled(
                                    self.queue_selected.is_some(),
                                    egui::Button::new("Remove selected"),
                                )
                                .clicked()
                                && let Some(i) = self.queue_selected.take()
                                && i < self.queue.len()
                            {
                                self.queue.remove(i);
                            }
                        });
                        egui::ScrollArea::vertical()
                            .id_salt("playlist")
                            .auto_shrink([false, false])
                            .max_height(content_height * 0.27)
                            .min_scrolled_height(content_height * 0.27)
                            .show(ui, |ui| {
                                if let Some(lib) = &self.library {
                                    for (row, &i) in self.queue.iter().enumerate() {
                                        if ui
                                            .selectable_label(
                                                self.queue_selected == Some(row),
                                                format!("{:02}   {}", row + 1, lib.tracks[i].title),
                                            )
                                            .clicked()
                                        {
                                            self.queue_selected = Some(row);
                                        }
                                    }
                                }
                                if self.queue.is_empty() {
                                    ui.weak("Select files above and add them to your playlist.");
                                }
                            });
                    });
                });
            });
            let track = self
                .selected
                .and_then(|i| self.library.as_ref()?.tracks.get(i));
            let position = if self.selected == self.loaded[0] {
                self.audio.as_ref().map_or(0., |a| a.source_position(0))
            } else {
                track.map_or(0., |t| t.preparation.cue_in)
            };
            self.preview_wave
                .show(ui, &self.root, track, position, 32., 0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} files",
                        self.library.as_ref().map_or(0, |l| l.tracks.len())
                    ))
                    .color(skin::accent(ui)),
                );
                ui.separator();
                ui.label(self.root.display().to_string());
            });
        });
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
        let mut selected = self.run.choice.clone();
        let mut context_edit = None;
        ui.horizontal_wrapped(|ui| {
            let selector = egui::ComboBox::from_id_salt("performance-playlist")
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
                        if show_editor::edit_menu(&row) {
                            context_edit = Some(p.id.clone());
                        }
                    }
                });
            if !selected.is_empty() && show_editor::edit_menu(&selector.response) {
                context_edit = Some(selected.clone());
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
            if ui.button("Start Auto DJ").clicked() {
                let result = self.execute_control(control::Action::LoadSmartPlaylist {
                    playlist: selected.clone(),
                    execution: control::Execution::PlayNow,
                    style: None,
                    energy: None,
                });
                if let Err(e) = result {
                    self.status = e;
                }
            }
            if ui.button("Mix Next").clicked()
                && let Err(e) = self.execute_control(control::Action::MixNext { immediate: true })
            {
                self.status = e;
            }
            ui.checkbox(&mut self.run.crossfade, "Auto crossfade");
            if ui
                .checkbox(&mut self.run.locked, "Manual DJ Lock")
                .changed()
                && self.run.locked
            {
                self.run.cancel_automation();
            }
            ui.label(format!("{:?}", self.run.style))
                .on_hover_text(format!(
                    "Energy: {} · {}",
                    self.run
                        .target
                        .map(|e| format!("{e:.1}"))
                        .unwrap_or_else(|| "follow current".into()),
                    if self.run.auto {
                        "Auto DJ"
                    } else {
                        "Manual / loaded"
                    }
                ));
        });
        self.run.choice = selected;
        if let Some(id) = context_edit {
            self.edit_smart_in_library(ui, &id);
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
                sampler::Action::Trigger(pad) => engine.sample_trigger(pad),
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
        skin::panel(ui, true, |ui| {
            ui.set_min_width(ui.available_width());
            self.header(ui, "AMP DJ", true);
            ui.horizontal(|ui| {
                for (tab, label) in ["Mixer", "Music library", "Sampler"]
                    .into_iter()
                    .enumerate()
                {
                    ui.selectable_value(&mut self.dj_tab, tab, label);
                }
            });
            if self.dj_tab != 0 {
                egui::ScrollArea::both()
                    .id_salt("dj-selected-tool")
                    .max_height(ui.available_height())
                    .show(ui, |ui| {
                        if self.dj_tab == 1 {
                            self.dj_library_ui(ui);
                        } else {
                            self.sampler_ui(ui);
                        }
                    });
                return;
            }
            self.performance_controls(ui);
            for d in 0..2 {
                let track = self.loaded_track(d).cloned();
                let position = self.audio.as_ref().map_or(0., |a| a.source_position(d));
                self.waveforms[d].height = Some(40.);
                self.waveforms[d].show(ui, &self.root, track.as_ref(), position, self.wave_span, d);
            }
            let height = (ui.available_height() - 34.).max(230.);
            let deck_width = ((ui.available_width() - 178.) / 2.).max(250.);
            ui.horizontal_top(|ui| {
                self.deck_ui(ui, 0, deck_width, height);
                ui.allocate_ui_with_layout(
                    egui::vec2(160., 0.),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        skin::panel(ui, true, |ui| {
                            ui.set_width(140.);
                            ui.horizontal(|ui| {
                                if ui.button("EQ").clicked() {
                                    self.eq_open = true;
                                }
                                if ui.button("FX").clicked() {
                                    self.fx_open = true;
                                }
                            });
                            let meter_height = (height - 220.).clamp(60., 110.);
                            ui.horizontal(|ui| {
                                for d in 0..2 {
                                    ui.vertical(|ui| {
                                        ui.label(if d == 0 { "A" } else { "B" });
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().slider_width = meter_height;
                                            if ui
                                                .add(
                                                    egui::Slider::new(
                                                        &mut self.channel_gain[d],
                                                        0. ..=1.,
                                                    )
                                                    .vertical()
                                                    .show_value(false),
                                                )
                                                .changed()
                                                && let Some(a) = &self.audio
                                            {
                                                a.set_gain(d, self.channel_gain[d]);
                                            }
                                            skin::meter(
                                                ui,
                                                self.audio.as_ref().map_or(0., |a| a.peak(d)),
                                                egui::vec2(16., meter_height),
                                            );
                                        });
                                    });
                                }
                            });
                            let fading = self.audio.as_ref().is_some_and(|a| a.fading());
                            let fade = ui.add_sized(
                                [140., 42.],
                                egui::Button::new(if fading {
                                    "Cancel mix"
                                } else {
                                    "Auto crossfade"
                                })
                                .wrap(),
                            );
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::unique("dj-auto-fade-bounds"), fade.rect);
                                d.insert_temp(
                                    egui::Id::unique("dj-auto-fade-bounds").with("screen"),
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
                                        egui::Slider::new(&mut self.fader, 0. ..=1.)
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
                            ui.add(
                                egui::DragValue::new(&mut self.fade_seconds)
                                    .range(0.5..=30.)
                                    .speed(0.5)
                                    .prefix("Mix ")
                                    .suffix(" s"),
                            );
                            egui::CollapsingHeader::new("Waveform zoom").show(ui, |ui| {
                                ui.add(
                                    egui::Slider::new(&mut self.wave_span, 4. ..=64.)
                                        .show_value(true),
                                );
                            });
                        });
                    },
                );
                self.deck_ui(ui, 1, deck_width, height);
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
        });
    }
    fn deck_ui(&mut self, ui: &mut egui::Ui, deck: usize, width: f32, height: f32) {
        let accent = if deck == 0 { ORANGE } else { CYAN };
        let platter_side = (height - 190.).clamp(80., 170.);
        ui.allocate_ui_with_layout(egui::vec2(width,0.),egui::Layout::top_down(egui::Align::Center),|ui|{
            skin::panel(ui,true,|ui|{ui.set_width(width-18.);
                ui.horizontal(|ui|{skin::label(ui,if deck==0{"DECK A"}else{"DECK B"});let position=self.audio.as_ref().map_or(0.,|a|a.position(deck));ui.label(RichText::new(format!("{:02}:{:02}",position as u64/60,position as u64%60)).monospace().color(skin::signal(ui)));});
                skin::display(ui,|ui|{ui.set_min_width(ui.available_width());ui.label(RichText::new(self.title(deck)).strong().color(skin::signal(ui)));let track=self.loaded_track(deck);ui.label(track.map(|t|format!("{} BPM · {}",t.preparation.bpm.map(|b|format!("{:.1}",b*(1.+self.pitch[deck]as f64/100.))).unwrap_or_else(||"--".into()),t.preparation.harmonic_key.as_deref().unwrap_or("--"))).unwrap_or_else(||"Load a track from the library".into()));});
                ui.horizontal(|ui|{ui.vertical(|ui|{ui.label("PITCH");ui.spacing_mut().slider_width=platter_side;if ui.add(egui::Slider::new(&mut self.pitch[deck],-16. ..=16.).vertical().show_value(false)).changed()&&let Some(a)=&self.audio{a.set_rate(deck,1.+self.pitch[deck]/100.);}ui.label(format!("{:+.1}%",self.pitch[deck]));});let phase=ui.ctx().input(|i|i.time)as f32;skin::platter(ui,platter_side,accent,self.playing[deck],phase);});
                let transport=ui.horizontal(|ui|{
                    if skin::button(ui,"CUE",accent).on_hover_text("Return to the prepared cue IN and pause").clicked()&&let Some(i)=self.loaded[deck]{self.load_deck(deck,i,false);}
                    if skin::button(ui,if self.playing[deck]{"PAUSE"}else{"PLAY"},accent).clicked(){if deck==0{self.auto=false;}self.toggle_deck(deck);}
                    if skin::button(ui,"SYNC",accent).on_hover_text("Match prepared BPM to the other deck. Changes pitch with speed; beat-phase sync/key lock are not available.").clicked(){self.sync_deck(deck);}
                    if ui.button("STOP").clicked(){self.stop(deck);}
                });
                ui.ctx().data_mut(|d|{d.insert_temp(egui::Id::unique(("dj-transport",deck)),transport.response.rect);d.insert_temp(egui::Id::unique(("dj-transport",deck)).with("screen"),(transport.response.rect,ui.layer_id()));});
                let cues=self.loaded[deck].and_then(|index|self.loaded_track(deck).map(|t|(index,t.preparation.manual_cues.iter().chain(t.preparation.cues.iter().filter(|c|c.kind=="grid")).take(8).cloned().collect::<Vec<_>>())));
                ui.horizontal_wrapped(|ui| {
                    let looping=self.audio.as_ref().is_some_and(|a|a.controls.looping[deck].load(std::sync::atomic::Ordering::Relaxed));
                    if ui.button(if looping{"Loop off"}else{"Loop"}).on_hover_text("Loop four prepared beats, or two seconds without BPM").clicked(){
                        let seconds=self.loaded_track(deck).and_then(|t|t.preparation.bpm).map_or(2.,|bpm|4.*60./(bpm*(1.+self.pitch[deck]as f64/100.)));
                        if let Some(a)=&self.audio&&let Err(e)=a.loop_toggle(deck,seconds as f32){self.status=e;}
                    }
                    if ui.button("Pitch reset").clicked(){self.pitch[deck]=0.;if let Some(a)=&self.audio{a.set_rate(deck,1.);}}
                    ui.menu_button("Hot cues",|ui| {
                        if let Some((index,cues))=&cues {for cue in cues {if ui.button(&cue.label).clicked(){self.play_cue(deck,*index,cue.seconds);ui.close();}}}
                        else {ui.weak("Load a prepared track first");}
                    });
                });
            });
        });
    }
    fn dj_effects(&mut self, ctx: &egui::Context) {
        if self.eq_open {
            let mut open = true;
            egui::Window::new("AMP DJ · Channel EQ")
                .open(&mut open)
                .show(ctx, |ui| {
                    for d in 0..2 {
                        ui.strong(if d == 0 { "Deck A" } else { "Deck B" });
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
                        if ui.button(format!("Flat {}", d + 1)).clicked() {
                            self.eq[d] = [0.; 3];
                            if let Some(a) = &self.audio {
                                for b in 0..3 {
                                    a.set_eq(d, b, 0.);
                                }
                            }
                        }
                    }
                });
            self.eq_open = open;
        }
        if self.fx_open {
            let mut open = true;
            egui::Window::new("AMP DJ · Filter FX").open(&mut open).show(ctx,|ui|{for d in 0..2{if ui.add(egui::Slider::new(&mut self.filter[d],200. ..=22000.).logarithmic(true).text(if d==0{"A low-pass"}else{"B low-pass"}).suffix(" Hz")).changed()&&let Some(a)=&self.audio{a.set_filter(d,self.filter[d]);}
if ui.button(format!("Bypass {}",d+1)).clicked(){self.filter[d]=22000.;if let Some(a)=&self.audio{a.set_filter(d,22000.);}}}ui.weak("First FX: channel low-pass filter. Additional effects and MIDI/OSC remain future work.");});
            self.fx_open = open;
        }
    }
    fn cast_ui(&mut self, ui: &mut egui::Ui) {
        skin::panel(ui, true, |ui| {
            ui.set_min_size(ui.available_size());
            self.header(ui, "AMP Cast", true);
            self.cast.show(ui);
        });
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
            "rules" => {
                self.show_editor.open = true;
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
        "library" => [1100., 830.],
        "preparation" | "rules" => [1200., 830.],
        "dj" => [1000., 920.],
        "dj-window" => [900., 600.],
        "compact" => [520., 210.],
        "mini" => [620., 100.],
        "dj-library" => [1100., 430.],
        "sampler" => [800., 850.],
        "lab" => [1100., 760.],
        "addons" => [1120., 760.],
        "cast" => [1120., 800.],
        _ => [1100., 170.],
    };
    eframe::run_native(
        "AMP VERSE",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_decorations(false)
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
    fn resized_mixer_controls_stay_inside_the_viewport() {
        let ctx = egui::Context::default();
        let mut player = Player::default();
        for size in [
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
        assert!(
            dj.height() <= 600.,
            "DJ container must fit the native viewport: {dj:?}"
        );
        for id in [
            egui::Id::unique(("dj-transport", 0usize)),
            egui::Id::unique(("dj-transport", 1usize)),
            egui::Id::unique("dj-auto-fade-bounds"),
        ] {
            let control = ctx.data(|d| d.get_temp::<egui::Rect>(id)).unwrap();
            assert!(
                control.bottom() <= 600. && control.right() <= 900.,
                "Core DJ controls must remain visible: {control:?}"
            );
        }
    }
    #[test]
    fn dj_below_attached_library_has_finite_reachable_bounds() {
        let ctx = egui::Context::default();
        let mut player = Player {
            library_open: true,
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
