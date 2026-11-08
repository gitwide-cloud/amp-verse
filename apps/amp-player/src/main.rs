#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use amp_core::{Deck, Folder, Library, Playlist};
use eframe::egui::{self, Color32, RichText};
use std::{path::PathBuf, sync::mpsc, time::Duration};

struct Player {
    root: PathBuf,
    library: Option<Library>,
    scan: Option<mpsc::Receiver<Result<Library, String>>>,
    selected: Option<usize>,
    decks: [Deck; 2],
    loaded: [Option<usize>; 2],
    volume: u8,
    queue: Vec<usize>,
    auto: bool,
    library_open: bool,
    dj_open: bool,
    status: String,
    screenshot_frames: u32,
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
            decks: Default::default(),
            loaded: [None; 2],
            volume: 75,
            queue: vec![],
            auto: false,
            library_open: false,
            dj_open: false,
            status: "Choose your music folder".into(),
            screenshot_frames: 0,
        }
    }
}
impl Player {
    fn scan(&mut self) {
        let root = self.root.clone();
        let (tx, rx) = mpsc::channel();
        self.scan = Some(rx);
        self.status = "Scanning folders…".into();
        std::thread::spawn(move || {
            let _ = tx.send(amp_core::scan(&root).map_err(|e| e.to_string()));
        });
    }
    fn play(&mut self, deck: usize, index: usize) {
        if let Some(track) = self.library.as_ref().and_then(|l| l.tracks.get(index)) {
            match self.decks[deck].play(&self.root, track, self.volume) {
                Ok(()) => {
                    self.loaded[deck] = Some(index);
                    self.status = format!("Deck {} · {}", deck + 1, track.title);
                }
                Err(e) => self.status = e,
            }
        }
    }
    fn next(&mut self) {
        if !self.queue.is_empty() {
            let index = self.queue.remove(0);
            self.play(0, index);
        } else {
            self.auto = false;
            self.status = "Queue finished".into();
        }
    }
    fn tree(ui: &mut egui::Ui, folder: &Folder, library: &Library, selected: &mut Option<usize>) {
        ui.collapsing(&folder.name, |ui| {
            for sub in &folder.folders {
                Self::tree(ui, sub, library, selected);
            }
            for &i in &folder.tracks {
                if ui
                    .selectable_label(*selected == Some(i), &library.tracks[i].title)
                    .clicked()
                {
                    *selected = Some(i);
                }
            }
        });
    }
}
impl eframe::App for Player {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(rx) = &self.scan {
            match rx.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(lib) => {
                            for deck in &mut self.decks {
                                deck.stop();
                            }
                            self.loaded = [None; 2];
                            self.selected = None;
                            self.queue.clear();
                            self.auto = false;
                            self.status = format!("{} tracks · offline ready", lib.tracks.len());
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
        let playing = self.decks[0].playing();
        let _ = self.decks[1].playing();
        if self.auto && !playing {
            self.next();
        }
        egui::Frame::new()
            .fill(Color32::from_rgb(39, 43, 52))
            .inner_margin(10)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("AMP⚡VERSE")
                            .strong()
                            .color(Color32::from_rgb(245, 211, 80)),
                    );
                    ui.label("PLAYER  /  0.1 prototype");
                });
                egui::Frame::new()
                    .fill(Color32::from_rgb(7, 16, 12))
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        let title = self.loaded[0]
                            .and_then(|i| self.library.as_ref()?.tracks.get(i))
                            .map(|t| t.title.as_str())
                            .unwrap_or("READY · LOCAL AUDIO");
                        ui.label(
                            RichText::new(title)
                                .monospace()
                                .size(22.)
                                .color(Color32::from_rgb(126, 245, 88)),
                        );
                        ui.label(RichText::new(&self.status).color(Color32::LIGHT_GREEN));
                    });
                ui.horizontal(|ui| {
                    if ui.button("▶ PLAY").clicked()
                        && let Some(i) = self.selected
                    {
                        self.play(0, i);
                    }
                    if ui.button("■ STOP").clicked() {
                        self.auto = false;
                        self.decks[0].stop();
                        self.status = "Stopped".into();
                    }
                    if ui.button("NEXT ▶|").clicked() {
                        self.next();
                    }
                    ui.add(egui::Slider::new(&mut self.volume, 0..=100).text("Next play volume"));
                });
                ui.horizontal(|ui| {
                    if ui.button("FOLDER LIBRARY").clicked() {
                        self.library_open = true;
                    }
                    if ui.button("AMP DJ").clicked() {
                        self.dj_open = true;
                    }
                    ui.hyperlink_to(
                        "AMP Lab downloads ↗",
                        "https://github.com/gitwide-cloud/mp3-lab/releases",
                    );
                });
            });
        let mut library_open = self.library_open;
        egui::Window::new("AMP LIBRARY · Folders & Playlists")
            .open(&mut library_open)
            .default_width(500.)
            .show(&ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.scan.is_none(), egui::Button::new("Choose folder…"))
                        .clicked()
                        && let Some(root) = rfd::FileDialog::new().pick_folder()
                    {
                        self.root = root;
                        self.library = None;
                        self.scan();
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
                    ui.label(self.root.display().to_string());
                });
                egui::ScrollArea::vertical()
                    .max_height(240.)
                    .show(ui, |ui| {
                        if let Some(lib) = &self.library {
                            Self::tree(ui, &lib.tree, lib, &mut self.selected);
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button("Add selected to playlist").clicked()
                        && let Some(i) = self.selected
                    {
                        self.queue.push(i);
                    }
                    if ui.button("Save playlist…").clicked()
                        && let Some(lib) = &self.library
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
                    if ui.button("Load playlist…").clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_file()
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
                                    } else {
                                        self.status =
                                            "Rescan: playlist includes an unindexed track".into();
                                    }
                                }
                            }
                            Err(e) => self.status = e.to_string(),
                        }
                    }
                });
                ui.label(format!("Playlist · {} queued tracks", self.queue.len()));
                if let Some(lib) = &self.library {
                    for &i in &self.queue {
                        ui.label(&lib.tracks[i].title);
                    }
                }
                if ui.button("Clear playlist").clicked() {
                    self.queue.clear();
                }
            });
        self.library_open = library_open;
        let mut dj_open = self.dj_open;
        egui::Window::new("AMP DJ · Two-deck prototype")
            .open(&mut dj_open)
            .show(&ctx, |ui| {
                for deck in 0..2 {
                    ui.horizontal(|ui| {
                        ui.label(format!("DECK {}", deck + 1));
                        if ui.button("Play selected").clicked()
                            && let Some(i) = self.selected
                        {
                            self.play(deck, i);
                        }
                        if ui.button("Stop").clicked() {
                            self.decks[deck].stop();
                            if deck == 0 {
                                self.auto = false;
                            }
                        }
                        let title = self.loaded[deck]
                            .and_then(|i| self.library.as_ref()?.tracks.get(i))
                            .map(|t| t.title.as_str())
                            .unwrap_or("Empty");
                        ui.label(title);
                    });
                }
                ui.checkbox(
                    &mut self.auto,
                    "Prepared Auto DJ · sequential playlist on deck 1",
                );
                ui.label("Uses cue IN/OUT from .amp.json sidecars. No beat sync or crossfade yet.");
            });
        self.dj_open = dj_open;
        if let Some(path) = std::env::var_os("AMP_SCREENSHOT") {
            use eframe::icon_data::IconDataExt;
            if self.scan.is_none() {
                self.screenshot_frames += 1;
                if self.screenshot_frames == 10 {
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
            }
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}
fn main() -> eframe::Result {
    let mut player = Player::default();
    if !player.root.as_os_str().is_empty() {
        player.scan();
    }
    eframe::run_native(
        "AMP⚡VERSE",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([600., 210.])
                .with_min_inner_size([540., 200.]),
            ..Default::default()
        },
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(player))
        }),
    )
}
