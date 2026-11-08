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
mod docking;
mod lab;
mod mixer;
mod preparer;
mod skin;
mod theme;
mod waveform;
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
    playing: [bool; 2],
    volume: u8,
    queue: Vec<usize>,
    queue_selected: Option<usize>,
    auto: bool,
    library_open: bool,
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
            playing: [false; 2],
            volume: 75,
            queue: vec![],
            queue_selected: None,
            auto: false,
            library_open: std::env::var_os("AMP_DOCK_SMOKE").is_some(),
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
        match module {
            Some(skin::Module::Verse) => ui
                .ctx()
                .send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus),
            Some(skin::Module::Library) => {
                self.library_open = true;
                ui.ctx().send_viewport_cmd_to(
                    egui::ViewportId::from_hash_of("amp-library"),
                    egui::ViewportCommand::Focus,
                );
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
    fn poll(&mut self) {
        self.preparer.poll(&self.root, self.library.as_mut());
        if let Some(rx) = &self.scan {
            match rx.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(lib) => {
                            if let Some(audio) = &mut self.audio {
                                audio.stop(0);
                                audio.stop(1);
                            }
                            self.loaded = [None; 2];
                            self.playing = [false; 2];
                            self.selected = None;
                            self.folder.clear();
                            self.queue.clear();
                            self.queue_selected = None;
                            self.auto = false;
                            self.status = format!("{} tracks / offline ready", lib.tracks.len());
                            self.library = Some(lib);
                            if std::env::var_os("AMP_WAVEFORM_SMOKE").is_some() {
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
            audio.set_master(self.volume as f32 / 100.);
            if let Some(error) = audio.take_error() {
                self.status = error;
            }
        } else {
            self.playing = [false; 2];
        }
        if self.auto && !self.playing[0] {
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
        if deck == 0 && !play {
            self.auto = false;
        }
        if let Some(track) = self
            .library
            .as_ref()
            .and_then(|l| l.tracks.get(index))
            .cloned()
        {
            let result = self.ensure_audio().and_then(|_| {
                self.audio
                    .as_mut()
                    .unwrap()
                    .load(deck, &self.root, &track, play)
            });
            match result {
                Ok(()) => {
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
        if let Some(mut track) = self
            .library
            .as_ref()
            .and_then(|l| l.tracks.get(index))
            .cloned()
        {
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
                    self.loaded[deck] = Some(index);
                    self.status = format!("Deck {} cue {:.2}s", deck + 1, seconds);
                }
                Err(e) => self.status = e,
            }
        }
    }
    fn stop(&mut self, deck: usize) {
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
        let bpm = |d: usize| {
            self.loaded[d].and_then(|i| self.library.as_ref()?.tracks.get(i)?.preparation.bpm)
        };
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
        if !self.queue.is_empty() {
            let index = self.queue.remove(0);
            self.queue_selected = None;
            self.play(0, index);
        } else {
            self.auto = false;
            self.status = "Playlist finished".into();
        }
    }
    fn title(&self, deck: usize) -> &str {
        self.loaded[deck]
            .and_then(|i| self.library.as_ref()?.tracks.get(i))
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
            self.playing = [false; 2];
            self.selected = None;
            self.queue.clear();
            self.queue_selected = None;
            self.library = None;
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
                version: 1,
                name: "Playlist".into(),
                tracks: self
                    .queue
                    .iter()
                    .map(|&i| lib.tracks[i].path.clone())
                    .collect(),
            };
            self.status = list
                .save(&path)
                .map(|_| "Playlist saved".into())
                .unwrap_or_else(|e| e.to_string());
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
    fn player_ui(&mut self, ui: &mut egui::Ui) {
        skin::panel(ui, true, |ui| {
            ui.set_min_size(ui.available_size());
            self.header(ui, "AMP VERSE", true);
            ui.horizontal(|ui| {
                for (caption, action) in [
                    ("|<", 0),
                    (if self.playing[0] { "PAUSE" } else { "PLAY" }, 1),
                    ("STOP", 2),
                    (">|", 3),
                ] {
                    if skin::button(ui, caption, GOLD).clicked() {
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
                                if let Some(audio) = &self.audio {
                                    audio.set_fader(0.);
                                }
                            }
                            2 => self.stop(0),
                            _ => self.next(),
                        }
                    }
                }
                ui.add(egui::Slider::new(&mut self.volume, 0..=100).text("Master volume"));
            });
            skin::display(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let elapsed = self
                        .audio
                        .as_ref()
                        .map_or(0, |audio| audio.position(0) as u64);
                    ui.label(
                        RichText::new(format!("{:02}:{:02}", elapsed / 60, elapsed % 60))
                            .monospace()
                            .size(32.)
                            .color(skin::signal(ui)),
                    );
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(self.title(0))
                                .strong()
                                .size(19.)
                                .color(skin::signal(ui)),
                        );
                        let info = self.loaded[0]
                            .and_then(|i| self.library.as_ref()?.tracks.get(i))
                            .map(|t| {
                                format!(
                                    "{}   ·   Cue IN {:.2}s   ·   Cue OUT {}",
                                    t.path
                                        .extension()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_uppercase(),
                                    t.preparation.cue_in,
                                    t.preparation
                                        .cue_out
                                        .map(|s| format!("{s:.2}s"))
                                        .unwrap_or_else(|| "End".into())
                                )
                            })
                            .unwrap_or_else(|| {
                                "Open AMP Library from the title dropdown to choose music".into()
                            });
                        ui.label(info);
                    });
                });
            });
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(if self.playing[0] {
                        "PLAYING"
                    } else {
                        "STOPPED"
                    })
                    .monospace()
                    .color(skin::accent(ui)),
                );
                ui.separator();
                ui.label(&self.status);
            });
        });
    }
    fn library_ui(&mut self, ui: &mut egui::Ui) {
        self.preparer
            .show(ui.ctx(), &self.root, self.library.as_mut());
        skin::panel(ui, false, |ui| {
            ui.set_min_size(ui.available_size());
            self.header(ui, "AMP Library", false);
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
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Search Music...")
                        .desired_width(280.),
                );
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.preparer.busy() && self.selected.is_some(),
                        egui::Button::new("Prepare selected / cues..."),
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
                        egui::Button::new("Analyze library and save metadata"),
                    )
                    .clicked()
                    && let Some(lib) = &self.library
                {
                    self.preparer.batch(self.root.clone(), lib.tracks.clone());
                }
                ui.label(&self.preparer.status);
            });
            let content_height = (ui.available_height() - 120.).max(340.);
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
                                            "Name", "BPM", "Key", "Cues", "Cue IN", "Cue OUT",
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
                        ui.horizontal(|ui| {
                            if ui.button("Load...").clicked() {
                                self.load_playlist();
                            }
                            if ui
                                .add_enabled(!self.queue.is_empty(), egui::Button::new("Save..."))
                                .clicked()
                            {
                                self.save_playlist();
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
    fn dj_ui(&mut self, ui: &mut egui::Ui) {
        skin::panel(ui, true, |ui| {
            ui.set_min_size(ui.available_size());
            self.header(ui, "AMP DJ", true);
            ui.horizontal(|ui| {
                ui.weak("SCROLLING WAVEFORMS · cue markers / source time");
                ui.add(egui::Slider::new(&mut self.wave_span, 4. ..=64.).text("seconds visible"));
            });
            for d in 0..2 {
                let track = self.loaded[d].and_then(|i| self.library.as_ref()?.tracks.get(i));
                let position = self.audio.as_ref().map_or(0., |a| a.source_position(d));
                self.waveforms[d].show(ui, &self.root, track, position, self.wave_span, d);
            }
            egui::ScrollArea::vertical().id_salt("dj-controls-body").show(ui,|ui|{
                let deck_width=((ui.available_width()-198.)/2.).max(260.);
                ui.horizontal_top(|ui|{
                    self.deck_ui(ui,0,deck_width);
                    ui.allocate_ui_with_layout(egui::vec2(180.,0.),egui::Layout::top_down(egui::Align::Center),|ui|{
                        skin::panel(ui,true,|ui|{
                            ui.set_width(160.);ui.horizontal(|ui|{if ui.button("EQ").clicked(){self.eq_open=true;}
if ui.button("FX").clicked(){self.fx_open=true;}});
                            ui.label("CHANNELS");
                            ui.horizontal(|ui|{for d in 0..2{ui.vertical(|ui|{ui.label(if d==0{"A"}else{"B"});ui.horizontal(|ui|{ui.spacing_mut().slider_width=210.;if ui.add(egui::Slider::new(&mut self.channel_gain[d],0. ..=1.).vertical().show_value(false)).changed()&&let Some(a)=&self.audio{a.set_gain(d,self.channel_gain[d]);}skin::meter(ui,self.audio.as_ref().map_or(0.,|a|a.peak(d)),egui::vec2(19.,210.));});ui.label(format!("{}%",(self.channel_gain[d]*100.)as u32));});}});
                            ui.add_space(12.);
                            let fading=self.audio.as_ref().is_some_and(|a|a.fading());
                            let label=if fading{"CANCEL FADE"}else{"AUTO CROSSFADER"};
                            if ui.add_sized([160.,44.],egui::Button::new(RichText::new(label).strong().color(skin::accent(ui)))).on_hover_text("One-touch transition to the opposite deck; incoming audio starts when buffered. Press again to cancel.").clicked(){self.auto=false;self.auto_crossfade();}
                            ui.horizontal(|ui|{ui.label("A");if ui.add(egui::Slider::new(&mut self.fader,0. ..=1.).show_value(false)).changed()&&let Some(a)=&self.audio{a.set_fader(self.fader);}ui.label("B");});
                            ui.add(egui::DragValue::new(&mut self.fade_seconds).range(0.5..=30.).speed(0.5).prefix("Mix ").suffix(" s"));
                            ui.label(if fading{"MIXING"}else{"Manual / auto"});
                        });
                    });
                    self.deck_ui(ui,1,deck_width);
                });
                ui.horizontal(|ui|{ui.checkbox(&mut self.auto,"Queue Auto Play · Deck A");ui.add(egui::Slider::new(&mut self.volume,0..=100).text("Master"));if let Some(a)=&self.audio{ui.weak(format!("{} · {} Hz",a.device,a.sample_rate));}});
                ui.separator();
                ui.horizontal(|ui|{skin::label(ui,"LIBRARY");if ui.button("Choose folder...").clicked(){self.choose_folder();}ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search music"));if ui.button("Load playlist...").clicked(){self.load_playlist();}});
                let mut load=None;
                egui::ScrollArea::vertical().id_salt("dj-library").max_height(180.).min_scrolled_height(120.).show(ui,|ui|{
                    if let Some(lib)=&self.library{for(i,t)in lib.tracks.iter().enumerate(){if !t.title.to_lowercase().contains(&self.search.to_lowercase()){continue;}
                        ui.horizontal(|ui|{if ui.selectable_label(self.selected==Some(i),&t.title).clicked(){self.selected=Some(i);}ui.weak(format!("{} BPM · {}",t.preparation.bpm.map(|b|format!("{b:.1}")).unwrap_or_else(||"--".into()),t.preparation.harmonic_key.as_deref().unwrap_or("--")));if ui.button("Load A").clicked(){load=Some((0,i));}
if ui.button("Load B").clicked(){load=Some((1,i));}});
                    }}else{ui.weak("Choose your local or mounted NAS music folder. Prepared cues and BPM are read beside the music.");}
                });
                if let Some((d,i))=load{self.selected=Some(i);self.load_deck(d,i,false);}
                ui.label(RichText::new(&self.status).color(skin::accent(ui)));
                if let Some(a)=&self.audio{let under=a.controls.underruns.iter().map(|x|x.load(std::sync::atomic::Ordering::Relaxed)).sum::<u64>();if under>0{ui.label(format!("Audio buffer underruns: {under} frames"));}}
            });
        });
        self.dj_effects(ui.ctx());
    }
    fn deck_ui(&mut self, ui: &mut egui::Ui, deck: usize, width: f32) {
        let accent = if deck == 0 { ORANGE } else { CYAN };
        ui.allocate_ui_with_layout(egui::vec2(width,0.),egui::Layout::top_down(egui::Align::Center),|ui|{
            skin::panel(ui,true,|ui|{ui.set_width(width-18.);
                ui.horizontal(|ui|{skin::label(ui,if deck==0{"DECK A"}else{"DECK B"});let position=self.audio.as_ref().map_or(0.,|a|a.position(deck));ui.label(RichText::new(format!("{:02}:{:02}",position as u64/60,position as u64%60)).monospace().color(skin::signal(ui)));});
                skin::display(ui,|ui|{ui.set_min_width(ui.available_width());ui.label(RichText::new(self.title(deck)).strong().color(skin::signal(ui)));let track=self.loaded[deck].and_then(|i|self.library.as_ref()?.tracks.get(i));ui.label(track.map(|t|format!("{} BPM · {}",t.preparation.bpm.map(|b|format!("{:.1}",b*(1.+self.pitch[deck]as f64/100.))).unwrap_or_else(||"--".into()),t.preparation.harmonic_key.as_deref().unwrap_or("--"))).unwrap_or_else(||"Load a track from the library".into()));});
                ui.horizontal(|ui|{ui.vertical(|ui|{ui.label("PITCH");ui.spacing_mut().slider_width=235.;if ui.add(egui::Slider::new(&mut self.pitch[deck],-16. ..=16.).vertical().show_value(false)).changed()&&let Some(a)=&self.audio{a.set_rate(deck,1.+self.pitch[deck]/100.);}ui.label(format!("{:+.1}%",self.pitch[deck]));});let phase=ui.ctx().input(|i|i.time)as f32;skin::platter(ui,(width-94.).clamp(170.,290.),accent,self.playing[deck],phase);});
                ui.horizontal(|ui|{
                    if skin::button(ui,"CUE",accent).on_hover_text("Return to the prepared cue IN and pause").clicked()&&let Some(i)=self.loaded[deck]{self.load_deck(deck,i,false);}
                    if skin::button(ui,if self.playing[deck]{"PAUSE"}else{"PLAY"},accent).clicked(){if deck==0{self.auto=false;}self.toggle_deck(deck);}
                    if skin::button(ui,"SYNC",accent).on_hover_text("Match prepared BPM to the other deck. Changes pitch with speed; beat-phase sync/key lock are not available.").clicked(){self.sync_deck(deck);}
                    if ui.button("STOP").clicked(){self.stop(deck);}
                });
                ui.horizontal(|ui|{let looping=self.audio.as_ref().is_some_and(|a|a.controls.looping[deck].load(std::sync::atomic::Ordering::Relaxed));if ui.button(if looping{"LOOP OFF"}else if self.loaded[deck].and_then(|i|self.library.as_ref()?.tracks.get(i)?.preparation.bpm).is_some(){"LOOP 4 BEATS"}else{"LOOP 2 s"}).clicked(){let seconds=self.loaded[deck].and_then(|i|self.library.as_ref()?.tracks.get(i)?.preparation.bpm).map_or(2.,|bpm|4.*60./(bpm*(1.+self.pitch[deck]as f64/100.)));if let Some(a)=&self.audio&&let Err(e)=a.loop_toggle(deck,seconds as f32){self.status=e;}}
if ui.button("Pitch reset").clicked(){self.pitch[deck]=0.;if let Some(a)=&self.audio{a.set_rate(deck,1.);}}});
                let cues=self.loaded[deck].and_then(|i|self.library.as_ref().and_then(|l|l.tracks.get(i)).map(|t|(i,t.preparation.manual_cues.iter().chain(t.preparation.cues.iter().filter(|c|c.kind=="grid")).take(8).cloned().collect::<Vec<_>>())));
                if let Some((index,cues))=cues{ui.horizontal_wrapped(|ui|{for cue in cues{if ui.button(&cue.label).on_hover_text(format!("{:.2}s",cue.seconds)).clicked(){self.play_cue(deck,index,cue.seconds);}}});}
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
        self.poll();
        self.cast.poll();
        let ctx = ui.ctx().clone();
        let mut active = vec![egui::ViewportId::ROOT];
        for (open, name) in [
            (self.library_open, "amp-library"),
            (self.dj_open, "amp-dj"),
            (self.lab_open, "amp-lab-arrangement"),
            (self.cast_open, "amp-cast"),
            (self.addons_open, "amp-addons"),
        ] {
            if open {
                active.push(egui::ViewportId::from_hash_of(name));
            }
        }
        docking::retain(&ctx, &active);
        match self.screenshot_view.as_str() {
            "library" | "preparation" => self.library_ui(ui),
            "dj" => self.dj_ui(ui),
            "addons" => self.addons_ui(ui),
            "cast" => self.cast_ui(ui),
            "lab" => {
                let action = self.lab.show(ui, &self.root, self.library.as_ref());
                self.activate(ui, action);
            }
            _ => self.player_ui(ui),
        }
        if self.library_open {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("amp-library"),
                egui::ViewportBuilder::default()
                    .with_title("AMP VERSE / Library")
                    .with_decorations(false)
                    .with_icon(app_icon())
                    .with_inner_size([1100., 730.])
                    .with_min_inner_size([900., 680.]),
                |ui, _| {
                    if ui.ctx().input(|i| i.viewport().close_requested()) {
                        self.library_open = false;
                    }
                    self.library_ui(ui);
                },
            );
        }
        if self.dj_open {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("amp-dj"),
                egui::ViewportBuilder::default()
                    .with_title("AMP VERSE / DJ")
                    .with_decorations(false)
                    .with_icon(app_icon())
                    .with_inner_size([1000., 920.])
                    .with_min_inner_size([730., 620.]),
                |ui, _| {
                    if ui.ctx().input(|i| i.viewport().close_requested()) {
                        self.dj_open = false;
                    }
                    self.dj_ui(ui);
                },
            );
        }
        if self.lab_open {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("amp-lab-arrangement"),
                egui::ViewportBuilder::default()
                    .with_title("AMP Lab / Mix arrangement")
                    .with_decorations(false)
                    .with_icon(app_icon())
                    .with_inner_size([1100., 760.]),
                |ui, _| {
                    if ui.ctx().input(|i| i.viewport().close_requested()) {
                        self.lab_open = false;
                    }
                    let action = self.lab.show(ui, &self.root, self.library.as_ref());
                    self.activate(ui, action);
                },
            );
        }
        if self.cast_open {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("amp-cast"),
                egui::ViewportBuilder::default()
                    .with_title("AMP VERSE / Cast")
                    .with_decorations(false)
                    .with_icon(app_icon())
                    .with_inner_size([1120., 800.])
                    .with_min_inner_size([1020., 760.]),
                |ui, _| {
                    if ui.ctx().input(|i| i.viewport().close_requested()) {
                        self.cast_open = false;
                    }
                    self.cast_ui(ui);
                },
            );
        }
        if self.addons_open {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("amp-addons"),
                egui::ViewportBuilder::default()
                    .with_title("AMP VERSE / Plugin Manager")
                    .with_decorations(false)
                    .with_icon(app_icon())
                    .with_inner_size([1120., 760.])
                    .with_min_inner_size([1050., 720.]),
                |ui, _| {
                    if ui.ctx().input(|i| i.viewport().close_requested()) {
                        self.addons_open = false;
                    }
                    self.addons_ui(ui);
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
        "library" => [1100., 730.],
        "preparation" => [1200., 830.],
        "dj" => [1000., 920.],
        "lab" => [1100., 760.],
        "addons" => [1120., 760.],
        "cast" => [1120., 800.],
        _ => [760., 238.],
    };
    eframe::run_native(
        "AMP VERSE",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_decorations(false)
                .with_icon(app_icon())
                .with_inner_size(size)
                .with_min_inner_size(size),
            ..Default::default()
        },
        Box::new(|cc| {
            skin::configure(&cc.egui_ctx);
            Ok(Box::new(player))
        }),
    )
}
