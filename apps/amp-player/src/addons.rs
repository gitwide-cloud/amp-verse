mod catalog;
// Built-in add-on catalog; no remote code loading.
use crate::skin::{self, GOLD, Module};
use eframe::egui::{self, RichText};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

struct Entry {
    id: &'static str,
    name: &'static str,
    kind: &'static str,
    summary: &'static str,
    features: &'static [&'static str],
}
const ENTRIES: [Entry; 8] = [
    Entry {
        id: "player",
        name: "AMP VERSE",
        kind: "Included",
        summary: "Compact base player and module launcher.",
        features: &[
            "Local playback and network library access",
            "Top-bar module dropdown",
            "Independent Library and DJ windows",
        ],
    },
    Entry {
        id: "library",
        name: "AMP Library",
        kind: "Included",
        summary: "Music folders, preparation data and playlists.",
        features: &[
            "Folder tree and file search",
            "Prepared cue IN/OUT and BPM metadata",
            "Save/load playlists; add and remove queued tracks",
        ],
    },
    Entry {
        id: "dj",
        name: "AMP DJ",
        kind: "Included",
        summary: "Six quick tracks, two control decks, manual mixing and Auto DJ.",
        features: &[
            "Two independent local playback decks",
            "Prepared cue IN/OUT",
            "Prepared Auto DJ and automatic crossfade",
            "Hot cues and sound-effect sampler",
        ],
    },
    Entry {
        id: "cast",
        name: "AMP Cast",
        kind: "Companion",
        summary: "Native Cast dashboard and independent headless server.",
        features: &[
            "Native dashboard: connect, queue, listeners, start/stop",
            "One MP3 encoder shared by listeners",
            "Runs independently from the base player",
            "Server requires separate startup and FFmpeg",
        ],
    },
    Entry {
        id: "lab",
        name: "AMP Lab",
        kind: "Included",
        summary: "Waveform editing, samples and multitrack mix arrangements.",
        features: &[
            "Waveform trim handles, fades, gain and normalization",
            "Original/processed preview and selection MP3 export",
            "Cue-aligned arrangement with combined or individual export",
            "Per-clip stems, three-band EQ, sweep filter and fades",
        ],
    },
    Entry {
        id: "analysis",
        name: "AMP Library Engine",
        kind: "Optional",
        summary: "Installable BPM, key, energy, waveform and cue analysis.",
        features: &[
            "Separate executable; absent from base playback",
            "Shared-library leases and protected manual metadata",
            "Runs only when analysis is requested",
            "Requires FFmpeg; install a build matching your OS",
        ],
    },
    Entry {
        id: "stems",
        name: "AMP Stem Separator",
        kind: "Optional",
        summary: "Separate active DJ tracks in memory, without saved stem audio.",
        features: &[
            "Local native separation; music is never uploaded",
            "Per-deck levels, mute and solo on a shared playback clock",
            "Hardware-based live stem limits with bounded RAM and cancellation",
            "Original audio and saved cues remain unchanged",
        ],
    },
    Entry {
        id: "podcasts",
        name: "AMP Podcasts",
        kind: "Optional",
        summary: "Discover, browse, follow and download public podcasts.",
        features: &[
            "Listen before subscribing through AMP Player",
            "Persistent subscriptions, progress and offline episodes",
            "Uninstall keeps your podcast data",
        ],
    },
];

pub struct Manager {
    updater: crate::app_update::Updater,
    pub selected: usize,
    download: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
    podcast_download: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
    search: String,
    filter: catalog::Filter,
    local: [catalog::Local; 8],
    catalog: Option<catalog::Catalog>,
    catalog_job: Option<std::sync::mpsc::Receiver<Result<catalog::Catalog, String>>>,
    catalog_status: String,
    seen: std::collections::BTreeSet<String>,
    initialized: bool,
    detail_tab: usize,
    lab: Option<PathBuf>,
    status: String,
    icon: Option<egui::TextureHandle>,
}
impl Default for Manager {
    fn default() -> Self {
        Self {
            updater: Default::default(),
            selected: 5,
            download: None,
            search: String::new(),
            podcast_download: None,
            filter: catalog::Filter::All,
            local: std::array::from_fn(|_| Default::default()),
            catalog: None,
            catalog_job: None,
            catalog_status: "Not checked".into(),
            seen: Default::default(),
            initialized: false,
            detail_tab: 0,
            lab: std::env::var_os("AMP_LAB_EXE")
                .map(PathBuf::from)
                .filter(|p| p.is_file()),
            status: "Choose an add-on to view its details and actions.".into(),
            icon: None,
        }
    }
}
impl Manager {
    fn launch_lab(&mut self) {
        let Some(path) = &self.lab else { return };
        let mut command = Command::new(path);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        self.status=match command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
            Ok(_)=>"Opened the external editor. Open your audio file in that app; file handoff is not implemented yet.".into(),
            Err(e)=>format!("Could not open editor: {e}"),
        };
    }
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Module> {
        if !self.initialized {
            self.initialized = true;
            self.local = catalog::inventory();
            self.seen = catalog::seen();
            self.check_catalog();
        }
        if let Some(rx) = &self.catalog_job {
            match rx.try_recv() {
                Ok(Ok(c)) => {
                    self.catalog = Some(c);
                    self.catalog_job = None;
                    self.catalog_status = "Checked published releases".into();
                }
                Ok(Err(e)) => {
                    self.catalog_job = None;
                    self.catalog_status = format!("Check failed · {e}");
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.catalog_job = None;
                    self.catalog_status = "Check interrupted · Retry".into();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ui
                    .ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
        // Presence changes immediately after an install/remove, without losing selection.
        let fresh = catalog::inventory();
        self.local = fresh;

        if let Some(rx) = &self.podcast_download {
            if let Ok(result) = rx.try_recv() {
                self.status = result.map_or_else(
                    |e| e,
                    |_| "AMP Podcasts installed; user data preserved.".into(),
                );
                self.podcast_download = None;
            } else {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
            }
        }

        if let Some(receiver) = &self.download {
            match receiver.try_recv() {
                Ok(result) => {
                    self.status = match result {
                        Ok(()) => {
                            "Analysis add-on downloaded and installed. Ready to analyze.".into()
                        }
                        Err(e) => format!("Download failed: {e}"),
                    };
                    self.download = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.status = "Download stopped unexpectedly. Please retry.".into();
                    self.download = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
        }
        if self.icon.is_none() {
            let icon = crate::app_icon();
            self.icon = Some(ui.ctx().load_texture(
                "addon-brand",
                egui::ColorImage::from_rgba_unmultiplied(
                    [icon.width as usize, icon.height as usize],
                    &icon.rgba,
                ),
                egui::TextureOptions::LINEAR,
            ));
        }
        let mut action = None;
        let counts = self.counts();
        skin::panel(ui, false, |ui| {
            ui.horizontal_wrapped(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(46., 46.), egui::Sense::hover());
                crate::icons::draw(ui, rect, crate::icons::Icon::Puzzle, skin::accent(ui));
                ui.vertical(|ui| {
                    ui.heading("Add-ons Manager");
                    ui.label("Extend AMP Verse with optional modules, tools, and services.");
                });
                for (i, label) in ["Installed", "Updates", "New", "Available"]
                    .iter()
                    .enumerate()
                {
                    egui::Frame::group(ui.style())
                        .inner_margin(10.)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "{}  {}",
                                    label,
                                    if i == 1 && self.catalog.is_none() {
                                        "—".into()
                                    } else {
                                        counts[[1, 2, 3, 5][i]].to_string()
                                    }
                                ))
                                .strong()
                                .color(status_color(i, ui)),
                            );
                        });
                }
            });
        });
        ui.add_space(10.);
        let wide = ui.available_width() >= 1100.;
        let height = (ui.available_height() - 85.).max(260.);
        if wide {
            let width = ui.available_width();
            ui.horizontal_top(|ui| {
                skin::panel(ui, false, |ui| {
                    ui.set_width(170.);
                    ui.set_min_height(height);
                    self.sidebar(ui, &counts);
                });
                let list_width = ((width - 210.) * 0.48).max(340.);
                skin::panel(ui, false, |ui| {
                    ui.set_width(list_width);
                    ui.set_min_height(height);
                    self.list(ui, height - 25.);
                });
                skin::panel(ui, false, |ui| {
                    ui.set_width(ui.available_width().max(280.) - 16.);
                    ui.set_min_height(height);
                    egui::ScrollArea::vertical()
                        .id_salt("addon-details")
                        .max_height(height - 24.)
                        .show(ui, |ui| {
                            self.details(ui, &mut action);
                        });
                });
            });
        } else {
            ui.horizontal_wrapped(|ui| {
                for f in catalog::Filter::ALL {
                    ui.selectable_value(&mut self.filter, f, f.label());
                }
                if ui
                    .add_enabled(
                        self.catalog_job.is_none(),
                        egui::Button::new("Check for updates"),
                    )
                    .clicked()
                {
                    self.check_catalog();
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("addons-compact")
                .show(ui, |ui| {
                    skin::panel(ui, false, |ui| {
                        ui.set_width(ui.available_width() - 16.);
                        self.list(ui, 260.);
                    });
                    ui.add_space(8.);
                    skin::panel(ui, false, |ui| {
                        ui.set_width(ui.available_width() - 16.);
                        self.details(ui, &mut action);
                    });
                });
        }
        ui.add_space(6.);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!("AMP VERSE  {}", env!("CARGO_PKG_VERSION")))
                    .strong()
                    .color(skin::signal(ui)),
            );
            ui.separator();
            ui.label(format!(
                "Installed: {}   |   Updates: {}   |   New: {}   |   Available: {}",
                counts[1],
                if self.catalog.is_some() {
                    counts[2].to_string()
                } else {
                    "Not checked".into()
                },
                counts[3],
                counts[5]
            ));
            ui.separator();
            ui.weak(&self.catalog_status);
        });
        if !self.status.is_empty() {
            ui.label(RichText::new(&self.status).color(skin::accent(ui)));
        }
        action
    }
    fn check_catalog(&mut self) {
        if self.catalog_job.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.catalog_job = Some(rx);
        self.catalog_status = "Checking published releases…".into();
        std::thread::spawn(move || {
            let _ = tx.send(catalog::fetch());
        });
    }
    fn release(&self, i: usize) -> Option<&catalog::Release> {
        self.catalog.as_ref().and_then(|c| c.releases[i].as_ref())
    }
    fn is_update(&self, i: usize) -> bool {
        catalog::update(&self.local[i], self.release(i))
    }
    fn is_new(&self, i: usize) -> bool {
        !self.local[i].installed && !self.seen.contains(ENTRIES[i].id)
    }
    fn counts(&self) -> [usize; 6] {
        let mut result = [0; 6];
        for (i, l) in self.local.iter().enumerate() {
            for (n, f) in catalog::Filter::ALL.iter().enumerate() {
                if f.matches(i, l, self.is_update(i), self.is_new(i)) {
                    result[n] += 1;
                }
            }
        }
        result
    }
    fn sidebar(&mut self, ui: &mut egui::Ui, counts: &[usize; 6]) {
        skin::label(ui, "ADD-ONS");
        ui.separator();
        for (i, f) in catalog::Filter::ALL.iter().enumerate() {
            if ui
                .add_sized(
                    [ui.available_width(), 42.],
                    egui::Button::new(format!(
                        "{}  ({})",
                        f.label(),
                        if *f == catalog::Filter::Updates && self.catalog.is_none() {
                            "—".into()
                        } else {
                            counts[i].to_string()
                        }
                    ))
                    .selected(self.filter == *f),
                )
                .clicked()
            {
                self.filter = *f;
            }
        }
        ui.add_space(15.);
        ui.separator();
        ui.strong("Sources");
        ui.hyperlink_to(
            "GitHub Releases",
            "https://github.com/gitwide-cloud/amp-verse/releases",
        );
        ui.weak("Only existing AMP Verse add-ons are listed.");
        ui.add_space(18.);
        if ui
            .add_enabled(
                self.catalog_job.is_none(),
                egui::Button::new(if self.catalog_job.is_some() {
                    "Checking…"
                } else {
                    "Check for updates"
                }),
            )
            .clicked()
        {
            self.check_catalog();
        }
        ui.add_space(12.);
        egui::CollapsingHeader::new("Desktop app update").show(ui, |ui| {
            self.updater.show(ui);
        });
    }
    fn list(&mut self, ui: &mut egui::Ui, height: f32) {
        ui.heading("ADD-ONS");
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("Search add-ons…")
                .desired_width(ui.available_width()),
        );
        ui.add_space(8.);
        let search = self.search.to_lowercase();
        let mut visible = 0;
        egui::ScrollArea::vertical()
            .id_salt("addon-list")
            .max_height((height - 65.).max(120.))
            .show(ui, |ui| {
                for (i, entry) in ENTRIES.iter().enumerate() {
                    if !self
                        .filter
                        .matches(i, &self.local[i], self.is_update(i), self.is_new(i))
                        || !format!("{} {}", entry.name, entry.summary)
                            .to_lowercase()
                            .contains(&search)
                    {
                        continue;
                    }
                    visible += 1;
                    let selected = self.selected == i;
                    let color = skin::accent(ui);
                    let frame = egui::Frame::new()
                        .fill(if selected {
                            color.gamma_multiply(0.13)
                        } else {
                            ui.visuals().faint_bg_color
                        })
                        .stroke(egui::Stroke::new(
                            if selected { 2. } else { 1. },
                            if selected {
                                color
                            } else {
                                ui.visuals().widgets.noninteractive.bg_stroke.color
                            },
                        ))
                        .corner_radius(9.)
                        .inner_margin(10.);
                    let row = frame.show(ui, |ui| {
                        ui.set_width(ui.available_width() - 20.);
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(43., 43.), egui::Sense::hover());
                            crate::icons::draw(ui, rect, entry_icon(i), color);
                            ui.vertical(|ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(entry.name).strong().size(18.))
                                        .selectable(false),
                                );
                                self.badges(ui, i);
                            });
                        });
                        ui.add(egui::Label::new(entry.summary).wrap().selectable(false));
                        ui.add(
                            egui::Label::new(
                                self.local[i]
                                    .version
                                    .as_deref()
                                    .map(|v| format!("v{v}"))
                                    .unwrap_or_else(|| {
                                        if self.local[i].installed {
                                            "Installed version unavailable".into()
                                        } else {
                                            entry.kind.into()
                                        }
                                    }),
                            )
                            .selectable(false),
                        );
                    });
                    let response = ui.interact(
                        row.response.rect,
                        ui.id().with(("addon-row", i)),
                        egui::Sense::click(),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            ui.is_enabled(),
                            entry.name,
                        )
                    });
                    if response.clicked() {
                        self.selected = i;
                        self.detail_tab = 0;
                    }
                    if response.hovered() {
                        ui.painter().rect_stroke(
                            row.response.rect,
                            9.,
                            egui::Stroke::new(1.5, color),
                            egui::StrokeKind::Inside,
                        );
                    }
                    ui.add_space(7.);
                }
                if visible == 0 {
                    ui.label("No matching add-ons.");
                    ui.weak("Try another filter or search.");
                }
            });
    }
    fn badges(&self, ui: &mut egui::Ui, i: usize) {
        ui.horizontal_wrapped(|ui| {
            badge(
                ui,
                if self.local[i].installed {
                    "✓ Installed"
                } else {
                    "+ Available"
                },
                if self.local[i].installed {
                    status_color(0, ui)
                } else {
                    ui.visuals().text_color()
                },
            );
            if self.is_update(i) {
                badge(ui, "↑ Update", status_color(1, ui));
            }
            if self.is_new(i) {
                badge(ui, "★ New", status_color(2, ui));
            }
        });
    }
    fn details(&mut self, ui: &mut egui::Ui, action: &mut Option<Module>) {
        let entry = &ENTRIES[self.selected];
        ui.horizontal_wrapped(|ui| {
            if self.selected == 5 {
                if let Some(icon) = &self.icon {
                    ui.image((icon.id(), egui::vec2(54., 54.)));
                }
            } else {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(48., 48.), egui::Sense::hover());
                crate::icons::draw(ui, rect, entry_icon(self.selected), skin::accent(ui));
            }
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(entry.name)
                        .strong()
                        .size(24.)
                        .color(skin::accent(ui)),
                );
                self.badges(ui, self.selected);
            });
        });
        ui.label(entry.kind);
        ui.add_space(8.);
        ui.label(entry.summary);
        ui.add_space(10.);
        ui.horizontal_wrapped(|ui| {
            for (i, label) in ["Overview", "Details", "Change log"].iter().enumerate() {
                ui.selectable_value(&mut self.detail_tab, i, *label);
            }
        });
        ui.separator();
        match self.detail_tab {
            0 => {
                for feature in entry.features {
                    ui.label(format!("✓ {feature}"));
                }
            }
            1 => {
                ui.label(format!(
                    "Installed: {}",
                    if self.local[self.selected].installed {
                        "Yes"
                    } else {
                        "No"
                    }
                ));
                ui.label(format!(
                    "Installed version: {}",
                    self.local[self.selected]
                        .version
                        .as_deref()
                        .unwrap_or("Not recorded")
                ));
                ui.label(format!(
                    "Published compatible version: {}",
                    self.release(self.selected)
                        .map(|r| r.version.as_str())
                        .unwrap_or("Not established")
                ));
                ui.weak("Included modules update with the desktop app. Presence is separate from whether a module is running.");
            }
            _ => {
                if let Some(r) = self.release(self.selected) {
                    ui.label(&r.notes);
                    ui.hyperlink_to("View release on GitHub", &r.url);
                } else {
                    ui.weak("No compatible release notes loaded for this add-on.");
                    ui.hyperlink_to(
                        "GitHub Releases",
                        "https://github.com/gitwide-cloud/amp-verse/releases",
                    );
                }
            }
        }
        ui.add_space(12.);
        if self.is_new(self.selected) && ui.button("Mark as seen").clicked() {
            self.seen.insert(entry.id.into());
            catalog::save_seen(&self.seen);
        }
        if self.is_update(self.selected) {
            ui.label(
                RichText::new("↑ Update available")
                    .strong()
                    .color(status_color(1, ui)),
            );
            if self.selected <= 4 {
                self.updater.show(ui);
            }
        } else if self.local[self.selected].installed {
            ui.weak(if self.local[self.selected].version.is_none() {
                "Installed · update comparison unavailable (version not recorded)"
            } else if self.catalog_job.is_some() {
                "Installed · checking for updates…"
            } else if self.catalog.is_none() {
                "Installed · updates not checked"
            } else if self.release(self.selected).is_none() {
                "Installed · no compatible published update found"
            } else {
                "Installed · no newer compatible release found"
            });
        }
        ui.separator();
        match self.selected {
            0..=2 => {
                ui.label(
                    RichText::new(format!(
                        "Included in this build / {}",
                        env!("CARGO_PKG_VERSION")
                    ))
                    .color(skin::signal(ui)),
                );
                if skin::button(ui, "OPEN", GOLD).clicked() {
                    *action = Some(match self.selected {
                        0 => Module::Verse,
                        1 => Module::Library,
                        _ => Module::Dj,
                    });
                }
            }
            3 => {
                let companion = std::env::current_exe()
                    .ok()
                    .and_then(|p| {
                        p.parent().map(|p| {
                            p.join(if cfg!(windows) {
                                "amp-cast.exe"
                            } else {
                                "amp-cast"
                            })
                        })
                    })
                    .is_some_and(|p| p.is_file());
                ui.label(if companion {
                    "Companion executable present beside AMP Verse"
                } else {
                    "Companion executable not found beside AMP Verse"
                });
                if skin::button(ui, "OPEN NATIVE CAST", GOLD).clicked() {
                    *action = Some(Module::Cast);
                }
                ui.hyperlink_to("Open browser listener", "http://127.0.0.1:8787");
                ui.hyperlink_to(
                    "Download AMP Verse builds",
                    "https://github.com/gitwide-cloud/amp-verse/actions",
                );
                ui.weak("The server must already be running. Presence does not imply an active station.");
            }
            4 => {
                if skin::button(ui, "OPEN AMP LAB", GOLD).clicked() {
                    *action = Some(Module::LabSetup);
                }
                ui.weak(
                    "Integrated waveform editor included. External editor connection is optional.",
                );
                ui.hyperlink_to(
                    "Download MP3 Lab releases",
                    "https://github.com/gitwide-cloud/mp3-lab/releases",
                );
                if ui.button("Connect installed editor...").clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_file()
                {
                    self.lab = Some(path);
                    self.status =
                        "Editor selected for this session. Click Open editor to launch it.".into();
                }
                if let Some(path) = &self.lab {
                    ui.add(egui::Label::new(path.display().to_string()).truncate());
                }
                if ui
                    .add_enabled(
                        self.lab.as_ref().is_some_and(|p| p.is_file()),
                        egui::Button::new("Open editor"),
                    )
                    .clicked()
                {
                    self.launch_lab();
                }
                ui.weak("The connected executable stays separate. Use AMP_LAB_EXE to configure it at startup; picker connections last for this session.");
            }
            7 => {
                ui.label(if crate::podcast_plugin::installed() {
                    "AMP Podcasts installed"
                } else {
                    "Not installed"
                });
                let mut enabled = crate::podcast_plugin::enabled();
                if ui.checkbox(&mut enabled, "Enable AMP Podcasts").changed() {
                    self.status = crate::podcast_plugin::set_enabled(enabled)
                        .err()
                        .unwrap_or_default();
                }
                if ui
                    .add_enabled(
                        self.podcast_download.is_none(),
                        egui::Button::new("Install / Check for updates from GitHub"),
                    )
                    .clicked()
                {
                    let (tx, rx) = std::sync::mpsc::channel();
                    self.podcast_download = Some(rx);
                    std::thread::spawn(move || {
                        let _ = tx.send(crate::podcast_plugin::download());
                    });
                }
                if ui.button("Install downloaded podcast package...").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("AMP Podcasts", &["ampplugin"])
                        .pick_file()
                {
                    self.status = std::fs::read(path)
                        .map_err(|e| e.to_string())
                        .and_then(|b| crate::podcast_plugin::install(&b))
                        .map_or_else(|e| e, |_| "AMP Podcasts installed".into());
                }
                if ui
                    .add_enabled(
                        crate::podcast_plugin::enabled(),
                        egui::Button::new("Open AMP Podcasts"),
                    )
                    .clicked()
                {
                    *action = Some(Module::Podcasts);
                }
                if ui.button("Uninstall (keep all podcast data)").clicked() {
                    self.status = crate::podcast_plugin::uninstall().map_or_else(
                        |e| e,
                        |_| "Uninstalled; subscriptions and downloads retained".into(),
                    );
                }
                ui.hyperlink_to(
                    "GitHub releases",
                    "https://github.com/gitwide-cloud/amp-verse/releases",
                );
            }
            6 => {
                ui.label(if crate::onnx_stem_worker::configured() {
                    "Live stem engine installed"
                } else {
                    "Live stem engine not installed"
                });
                if skin::button(ui, "OPEN AMP DJ", GOLD).clicked() {
                    *action = Some(Module::Dj);
                }
                ui.weak("Open Stems in DJ or use the Lab stem controls. Moving a stem knob starts separation; zero returns to original audio and stops separation. No separate stem audio is saved.");
                ui.hyperlink_to("Live stem setup guide", "https://github.com/gitwide-cloud/amp-verse/blob/feature/on-demand-stems-20261122/docs/LIVE-STEMS.md");
            }
            _ => {
                let installed = crate::engine::path();
                ui.label(if installed.is_some() {
                    "Analysis add-on installed"
                } else {
                    "Analysis add-on not installed"
                });
                if ui
                    .add_enabled(
                        self.download.is_none(),
                        egui::Button::new(if self.download.is_some() {
                            "Downloading..."
                        } else {
                            "Download & install analysis engine"
                        }),
                    )
                    .clicked()
                {
                    let (sender, receiver) = std::sync::mpsc::channel();
                    self.download = Some(receiver);
                    self.status = "Downloading the engine from GitHub Releases...".into();
                    std::thread::spawn(move || {
                        let _ = sender.send(crate::engine_download::download());
                    });
                }
                ui.hyperlink_to(
                    "Open releases in browser",
                    "https://github.com/gitwide-cloud/amp-verse/releases",
                );
                if ui.button("Install downloaded engine...").clicked()
                    && let Some(source) = rfd::FileDialog::new().pick_file()
                {
                    self.status =
                        match std::env::current_exe()
                            .map_err(|e| e.to_string())
                            .and_then(|exe| {
                                let directory = exe
                                    .parent()
                                    .ok_or("Missing application directory")?
                                    .join("addons");
                                std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                                let target = directory.join(if cfg!(windows) {
                                    "amp-engine.exe"
                                } else {
                                    "amp-engine"
                                });
                                if source.canonicalize().ok() != target.canonicalize().ok() {
                                    std::fs::copy(source, &target).map_err(|e| e.to_string())?;
                                }
                                #[cfg(unix)]
                                {
                                    use std::os::unix::fs::PermissionsExt;
                                    std::fs::set_permissions(
                                        directory.join("amp-engine"),
                                        std::fs::Permissions::from_mode(0o755),
                                    )
                                    .map_err(|e| e.to_string())?;
                                }
                                Ok(())
                            }) {
                            Ok(()) => {
                                "Analysis add-on installed. Library analysis is available.".into()
                            }
                            Err(e) => format!("Could not install analysis add-on: {e}"),
                        };
                }
                if let Some(path) = installed {
                    ui.add(egui::Label::new(path.display().to_string()).truncate());
                    let local = std::env::current_exe()
                        .ok()
                        .and_then(|p| {
                            p.parent().map(|p| {
                                p.join("addons").join(if cfg!(windows) {
                                    "amp-engine.exe"
                                } else {
                                    "amp-engine"
                                })
                            })
                        })
                        .is_some_and(|p| p == path);
                    if local && ui.button("Remove analysis add-on").clicked() {
                        self.status=match std::fs::remove_file(path) {Ok(())=>"Analysis add-on removed. Prepared playback and manual editing remain available.".into(),Err(e)=>format!("Could not remove add-on: {e}")};
                    }
                }
                ui.weak("Downloads start only when requested. SHA-256 is checked before installation. Analysis starts separately.");
            }
        }
    }
}
fn entry_icon(i: usize) -> crate::icons::Icon {
    use crate::icons::Icon;
    match i {
        0 => Icon::Play,
        1 => Icon::Folder,
        2 => Icon::Headphones,
        3 => Icon::Cast,
        4 => Icon::Flask,
        5 => Icon::Analyze,
        6 => Icon::Layers,
        _ => Icon::Microphone,
    }
}
fn status_color(i: usize, ui: &egui::Ui) -> egui::Color32 {
    match i {
        0 => skin::signal(ui),
        1 => skin::accent(ui),
        2 => egui::Color32::from_rgb(70, 175, 255),
        _ => ui.visuals().text_color(),
    }
}
fn badge(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.14))
        .stroke(egui::Stroke::new(1., color))
        .corner_radius(6.)
        .inner_margin(egui::Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(text).color(color).strong()).selectable(false));
        });
}

#[cfg(test)]
mod manager_tests {
    use super::*;
    #[test]
    fn existing_catalog_renders_status_and_actions_without_horizontal_overflow() {
        for width in [900., 1280., 1600.] {
            let ctx = egui::Context::default();
            let mut manager = Manager {
                initialized: true,
                ..Default::default()
            };
            let mut right = 0.;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 1100.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    manager.show(ui);
                    right = ui.min_rect().right();
                },
            );
            output.textures_delta.clear();
            assert!(
                right <= width + 1.,
                "horizontal overflow {right} at {width}"
            );
            let counts = manager.counts();
            assert_eq!(counts[1] + counts[5], ENTRIES.len());
            assert_eq!(counts[2], 0);
            assert_eq!(counts[3], counts[5]);
            fn collect(s: &egui::Shape, t: &mut Vec<String>) {
                match s {
                    egui::Shape::Text(x) => t.push(x.galley.text().into()),
                    egui::Shape::Vec(v) => {
                        for s in v {
                            collect(s, t)
                        }
                    }
                    _ => {}
                }
            }
            let mut text = Vec::new();
            for s in output.shapes {
                collect(&s.shape, &mut text);
            }
            for label in [
                "Add-ons Manager",
                "ADD-ONS",
                "AMP Library Engine",
                "Overview",
                "Change log",
            ] {
                assert!(
                    text.iter().any(|s| s == label),
                    "missing {label} at {width}: {text:?}"
                );
            }
            assert!(!text.iter().any(|s| s.contains("Available components")));
        }
    }
}
