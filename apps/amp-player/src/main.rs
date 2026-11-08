#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use amp_core::{Deck, Folder, Library, Playlist};
use eframe::egui::{self, RichText};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};
mod skin;
use skin::{CYAN, GOLD, GREEN, ORANGE};

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
    decks: [Deck; 2],
    loaded: [Option<usize>; 2],
    playing: [bool; 2],
    volume: u8,
    queue: Vec<usize>,
    queue_selected: Option<usize>,
    auto: bool,
    library_open: bool,
    dj_open: bool,
    status: String,
    screenshot_frames: u32,
    screenshot_view: String,
}
impl Default for Player {
    fn default() -> Self {
        Self {
            root: std::env::args_os()
                .nth(1)
                .map(PathBuf::from)
                .unwrap_or_default(),
            library: None,
            scan: None,
            selected: None,
            folder: PathBuf::new(),
            search: String::new(),
            decks: Default::default(),
            loaded: [None; 2],
            playing: [false; 2],
            volume: 75,
            queue: vec![],
            queue_selected: None,
            auto: false,
            library_open: false,
            dj_open: false,
            status: "Choose your music folder in Library".into(),
            screenshot_frames: 0,
            screenshot_view: std::env::var("AMP_SCREENSHOT_VIEW").unwrap_or_default(),
        }
    }
}
impl Player {
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
        if let Some(rx) = &self.scan {
            match rx.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(lib) => {
                            for deck in &mut self.decks {
                                deck.stop();
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
        self.playing = [self.decks[0].playing(), self.decks[1].playing()];
        if self.auto && !self.playing[0] {
            self.next();
        }
    }
    fn play(&mut self, deck: usize, index: usize) {
        if let Some(track) = self.library.as_ref().and_then(|l| l.tracks.get(index)) {
            match self.decks[deck].play(&self.root, track, self.volume) {
                Ok(()) => {
                    self.loaded[deck] = Some(index);
                    self.playing[deck] = true;
                    self.status = format!("Deck {} / {}", deck + 1, track.title);
                }
                Err(e) => self.status = e,
            }
        }
    }
    fn stop(&mut self, deck: usize) {
        self.decks[deck].stop();
        self.playing[deck] = false;
        if deck == 0 {
            self.auto = false;
        }
        self.status = format!("Deck {} stopped", deck + 1);
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
            for deck in &mut self.decks {
                deck.stop();
            }
            self.auto = false;
            self.loaded = [None; 2];
            self.playing = [false; 2];
            self.selected = None;
            self.queue.clear();
            self.queue_selected = None;
            self.library = None;
            self.root = root;
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
        let response = egui::CollapsingHeader::new(RichText::new(&folder.name).color(GOLD))
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
            skin::heading(ui, "Player", true);
            skin::panel(ui, false, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let state = if self.playing[0] {
                        "PLAYING"
                    } else {
                        "STOPPED"
                    };
                    ui.label(RichText::new(state).monospace().size(25.).color(GREEN));
                    ui.separator();
                    ui.label(RichText::new(self.title(0)).size(18.).color(GREEN));
                });
                ui.label(RichText::new(&self.status).color(egui::Color32::LIGHT_GRAY));
            });
            ui.horizontal(|ui| {
                if skin::button(ui, "PLAY", GOLD).clicked()
                    && let Some(i) = self.selected.or(self.loaded[0])
                {
                    self.play(0, i);
                }
                if skin::button(ui, "STOP", GOLD).clicked() {
                    self.stop(0);
                }
                if skin::button(ui, "NEXT", GOLD).clicked() {
                    self.next();
                }
                ui.add(egui::Slider::new(&mut self.volume, 0..=100).text("Next play volume"));
            });
            ui.horizontal(|ui| {
                if skin::button(ui, "LIBRARY", GOLD).clicked() {
                    self.library_open = true;
                }
                if skin::button(ui, "AMP DJ", GOLD).clicked() {
                    self.dj_open = true;
                }
                ui.hyperlink_to(
                    "AMP Lab downloads",
                    "https://github.com/gitwide-cloud/mp3-lab/releases",
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak("0.1 / LOCAL AUDIO");
                });
            });
        });
    }
    fn library_ui(&mut self, ui: &mut egui::Ui) {
        skin::panel(ui, false, |ui| {
            ui.set_min_size(ui.available_size());
            skin::heading(ui, "Library", false);
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
            let content_height = (ui.available_height() - 85.).max(340.);
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
                    ui.label(RichText::new("FOLDERS + PLAYLISTS").color(GOLD));
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
                                        for h in ["Name", "BPM", "Cue IN", "Cue OUT", "Type"] {
                                            ui.label(RichText::new(h).strong().color(GOLD));
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
                    .color(GOLD),
                );
                ui.separator();
                ui.label(self.root.display().to_string());
            });
        });
    }
    fn dj_ui(&mut self, ui: &mut egui::Ui) {
        skin::panel(ui, true, |ui| {
            ui.set_min_size(ui.available_size());
            skin::heading(ui, "AMP DJ", true);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("PREPARED AUDIO / TWO DECKS")
                        .monospace()
                        .color(GREEN),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{} QUEUED", self.queue.len())).color(GOLD));
                });
            });
            let width = ((ui.available_width() - 45.) / 2.).max(210.);
            let phase = ui.ctx().input(|i| i.time) as f32;
            ui.columns(2, |columns| {
                for (deck, column) in columns.iter_mut().enumerate() {
                    let accent = if deck == 0 { ORANGE } else { CYAN };
                    skin::panel(column, true, |ui| {
                        ui.set_min_width(width - 25.);
                        ui.label(
                            RichText::new(if deck == 0 { "DECK A" } else { "DECK B" })
                                .strong()
                                .size(22.)
                                .color(accent),
                        );
                        ui.label(RichText::new(self.title(deck)).color(GREEN));
                        ui.label(if self.playing[deck] {
                            "PLAYING"
                        } else {
                            "STOPPED"
                        });
                        skin::platter(
                            ui,
                            (width - 32.).min(300.),
                            accent,
                            self.playing[deck],
                            phase,
                        );
                        let track = self.loaded[deck]
                            .or(self.selected)
                            .and_then(|i| self.library.as_ref()?.tracks.get(i));
                        let cues = track
                            .map(|t| {
                                format!(
                                    "IN {:.2}s / OUT {} / BPM {}",
                                    t.preparation.cue_in,
                                    t.preparation
                                        .cue_out
                                        .map(|v| format!("{v:.2}s"))
                                        .unwrap_or_else(|| "End".into()),
                                    t.preparation
                                        .bpm
                                        .map(|v| format!("{v:.1}"))
                                        .unwrap_or_else(|| "--".into())
                                )
                            })
                            .unwrap_or_else(|| "Select a track in Library".into());
                        ui.label(RichText::new(cues).monospace().color(accent));
                        ui.horizontal(|ui| {
                            if skin::button(ui, "PLAY / CUE", accent).clicked()
                                && let Some(i) = self.selected.or(self.loaded[deck])
                            {
                                self.play(deck, i);
                            }
                            if skin::button(ui, "STOP", accent).clicked() {
                                self.stop(deck);
                            }
                        });
                    });
                }
            });
            skin::panel(ui, false, |ui| {
                ui.checkbox(
                    &mut self.auto,
                    "Prepared Auto DJ / sequential playlist on Deck A",
                );
                ui.label("Prepared cue IN/OUT is active. Crossfade, sync, pitch and FX are future milestones.");
            });
            ui.label(RichText::new(&self.status).color(GOLD));
        });
    }
}
impl eframe::App for Player {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.poll();
        let ctx = ui.ctx().clone();
        match self.screenshot_view.as_str() {
            "library" => self.library_ui(ui),
            "dj" => self.dj_ui(ui),
            _ => self.player_ui(ui),
        }
        if self.library_open {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("amp-library"),
                egui::ViewportBuilder::default()
                    .with_title("AMP VERSE / Library")
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
                    .with_inner_size([820., 660.])
                    .with_min_inner_size([730., 620.]),
                |ui, _| {
                    if ui.ctx().input(|i| i.viewport().close_requested()) {
                        self.dj_open = false;
                    }
                    self.dj_ui(ui);
                },
            );
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
        "dj" => [820., 660.],
        _ => [700., 255.],
    };
    eframe::run_native(
        "AMP VERSE",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
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
