use super::*;

#[derive(Default, PartialEq, Clone, Copy)]
enum Source {
    Smart,
    Playlists,
    #[default]
    Folders,
}
#[derive(Default)]
pub struct Browser {
    source: Source,
    path_index: std::collections::HashMap<PathBuf, usize>,
    folder: PathBuf,
    smart: String,
    search: String,
    search_cache: Search,
    pub(super) playlists: Vec<(PathBuf, Playlist)>,
    playlist: Option<usize>,
    destination: usize,
    columns: track_columns::Layout,
    details: Option<amp_core::Track>,
    stem_status: std::collections::HashMap<PathBuf, (std::time::Instant, bool)>,
    stem_rx: Option<std::sync::mpsc::Receiver<Vec<(PathBuf, bool)>>>,
    membership: Membership,
    snapshot: Option<std::sync::Arc<Library>>,
    snapshot_root: PathBuf,
    settings_snapshot: Option<std::sync::Arc<amp_core::show::Settings>>,
    queue_snapshot: Vec<usize>,
    selected_path: Option<PathBuf>,
    refresh_generation: u64,
}
#[derive(Clone, PartialEq, Eq)]
struct MatchKey {
    rules: Vec<u8>,
    generation: u64,
}
struct MatchResult {
    key: MatchKey,
    indices: Vec<usize>,
}
#[derive(Default)]
struct Membership {
    cached: Option<MatchResult>,
    job: Option<std::sync::mpsc::Receiver<MatchResult>>,
}
#[derive(Default)]
struct Search {
    query: String,
    input: Vec<usize>,
    matching: Vec<usize>,
}
impl Search {
    fn filter(&mut self, input: Vec<usize>, query: &str, library: &Library) -> Vec<usize> {
        if query.is_empty() {
            return input;
        }
        if self.query == query && self.input == input {
            return self.matching.clone();
        }
        self.matching = input
            .iter()
            .copied()
            .filter(|&i| {
                library.tracks.get(i).is_some_and(|t| {
                    t.title.to_lowercase().contains(query)
                        || t.path.to_string_lossy().to_lowercase().contains(query)
                        || ["artist", "album", "genre"]
                            .iter()
                            .any(|key| track_columns::cell(t, key).to_lowercase().contains(query))
                })
            })
            .collect();
        self.query = query.into();
        self.input = input;
        self.matching.clone()
    }
}
impl Membership {
    fn indices(
        &mut self,
        profile: &amp_core::show::SmartPlaylist,
        library: &Library,
        generation: u64,
    ) -> Option<Vec<usize>> {
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok(result) => {
                    self.cached = Some(result);
                    self.job = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.job = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let key = MatchKey {
            rules: serde_json::to_vec(profile).unwrap_or_default(),
            generation,
        };
        let matching = self.cached.as_ref().filter(|result| result.key == key);
        let ready = matching.map(|result| result.indices.clone());
        if self.job.is_none() && matching.is_none() {
            let tracks = library.tracks.clone();
            let profile = profile.clone();
            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            std::thread::spawn(move || {
                let indices = tracks
                    .iter()
                    .enumerate()
                    .filter(|(_, track)| amp_core::show::eligible(&profile, track))
                    .map(|(index, _)| index)
                    .collect();
                let _ = tx.send(MatchResult { key, indices });
            });
        }
        ready
    }
}
impl Browser {
    fn capture(
        &mut self,
        root: &Path,
        library: &Library,
        settings: &amp_core::show::Settings,
        queue: &[usize],
    ) {
        self.path_index = track_positions(library);
        self.snapshot_root = root.to_path_buf();
        self.snapshot = Some(std::sync::Arc::new(library.clone()));
        self.settings_snapshot = Some(std::sync::Arc::new(settings.clone()));
        self.queue_snapshot = queue.to_vec();
        self.refresh_generation = self.refresh_generation.wrapping_add(1);
        self.membership = Membership::default();
        self.search_cache = Search::default();
        self.stem_status.clear();
        self.stem_rx = None;
    }
    pub(super) fn selected_smart(&self) -> Option<&str> {
        (self.source == Source::Smart && !self.smart.is_empty()).then_some(self.smart.as_str())
    }
    pub(super) fn select_smart(&mut self, id: &str) {
        self.source = Source::Smart;
        self.smart = id.into();
    }
}
fn track_positions(library: &Library) -> std::collections::HashMap<PathBuf, usize> {
    let mut index = std::collections::HashMap::new();
    for (i, track) in library.tracks.iter().enumerate() {
        index.entry(track.path.clone()).or_insert(i);
    }
    index
}
fn playlist_indices(
    index: &std::collections::HashMap<PathBuf, usize>,
    playlist: &Playlist,
) -> Vec<usize> {
    playlist
        .tracks
        .iter()
        .filter_map(|path| index.get(path).copied())
        .collect()
}
impl Player {
    pub(super) fn dj_library_ui(&mut self, ui: &mut egui::Ui) {
        if self.dj_browser.snapshot.is_none()
            && let Some(library) = &self.library
        {
            self.dj_browser
                .capture(&self.root, library, &self.run.settings, &self.queue);
        }
        let library_snapshot = self.dj_browser.snapshot.clone();
        let snapshot_root = self.dj_browser.snapshot_root.clone();
        let settings_snapshot = self.dj_browser.settings_snapshot.clone();
        let mut refresh = false;
        if let Some(rx) = &self.dj_browser.stem_rx
            && let Ok(results) = rx.try_recv()
        {
            for (path, complete) in results {
                self.dj_browser
                    .stem_status
                    .insert(path, (std::time::Instant::now(), complete));
            }
            self.dj_browser.stem_rx = None;
        }
        let mut stem_checks = Vec::new();
        let mut save_columns = false;
        if let Some(track) = self.dj_browser.details.clone() {
            let mut open = true;
            egui::Window::new("Track metadata and analysis")
                .id(egui::Id::unique("dj-track-details"))
                .open(&mut open)
                .resizable(true)
                .default_width(620.)
                .show(ui.ctx(), |ui| {
                    ui.heading(&track.title);
                    ui.label(
                        amp_core::music_sources::audio_path(&snapshot_root, &track.path)
                            .display()
                            .to_string(),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(600.)
                        .show(ui, |ui| {
                            for (key, label, _, _) in track_columns::FIELDS {
                                if *key != "stems" {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.strong(*label);
                                        ui.label(track_columns::cell(&track, key));
                                    });
                                }
                            }
                            let path =
                                amp_core::music_sources::audio_path(&snapshot_root, &track.path);
                            ui.label(format!(
                                "Stems: {}",
                                self.dj_browser.stem_status.get(&path).map_or(
                                    "Checking…",
                                    |(_, ready)| if *ready { "Complete" } else { "Not prepared" }
                                )
                            ));
                            if !self.dj_browser.stem_status.contains_key(&path) {
                                stem_checks.push(path);
                            }
                            ui.separator();
                            ui.strong("All stored tags and analysis fields");
                            fn fields(ui: &mut egui::Ui, name: &str, value: &serde_json::Value) {
                                match value {
                                    serde_json::Value::Object(map) => {
                                        egui::CollapsingHeader::new(name)
                                            .id_salt(name)
                                            .default_open(name == "Metadata")
                                            .show(ui, |ui| {
                                                for (key, value) in map {
                                                    fields(ui, key, value);
                                                }
                                            });
                                    }
                                    _ => {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.strong(name);
                                            ui.label(track_columns::display(value));
                                        });
                                    }
                                }
                            }
                            if let Ok(value) = serde_json::to_value(&track.preparation) {
                                fields(ui, "Metadata", &value);
                            }
                        });
                });
            if !open {
                self.dj_browser.details = None;
            }
        }

        let height = ui
            .available_height()
            .max(if self.rack.six { 80. } else { 180. });
        let sidebar_id = egui::Id::unique("dj-source-width");
        let sidebar_max = (ui.available_width() * 0.35).clamp(150., 400.);
        let mut sidebar_width = ui
            .ctx()
            .data_mut(|d| d.get_persisted::<f32>(sidebar_id))
            .unwrap_or(195.)
            .clamp(150., sidebar_max);

        let mut queue_drop = None;
        let mut open_playlist = false;
        let mut load = None;
        let mut request = None;
        let requests = ui
            .horizontal(|ui| {
                skin::label(ui, "DJ Library");
                self.remote_access_button(ui);
                ui.menu_button("Columns", |ui| {
                    save_columns |= self.dj_browser.columns.menu(ui);
                });

                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!(
                            "Song requests ({}) · click to view",
                            self.run.requests.len()
                        ))
                        .weak(),
                    )
                    .sense(egui::Sense::click_and_drag()),
                )
            })
            .inner;
        let mut cancel_request = None;
        egui::Popup::menu(&requests).show(|ui| {
            if let Some(control) = &self.control {
                let address = control
                    .address
                    .replace("0.0.0.0", "127.0.0.1")
                    .replace("[::]", "127.0.0.1");
                ui.hyperlink_to(
                    "Open song-request web page",
                    format!("http://{address}/requests"),
                );
                ui.weak(
                    "On your phone, use this PC’s LAN address with the same port and /requests.",
                );
                ui.separator();
            }
            if self.run.requests.is_empty() {
                ui.label("No pending requests. Right-click a song to request it.");
            }
            for (slot, id) in self.run.requests.iter().enumerate() {
                let title = self
                    .library
                    .as_ref()
                    .and_then(|l| l.tracks.iter().find(|t| amp_core::show::song_id(t) == *id))
                    .map_or(id.as_str(), |t| t.title.as_str());
                let prepared = self.run.preloaded.is_some_and(|(_, index)| {
                    self.library
                        .as_ref()
                        .and_then(|l| l.tracks.get(index))
                        .is_some_and(|t| amp_core::show::song_id(t) == *id)
                });
                ui.horizontal(|ui| {
                    ui.label(format!("{} · {title}", slot + 1));
                    if ui
                        .add_enabled(!prepared, egui::Button::new("Remove request"))
                        .on_hover_text("An already prepared handoff is kept intact")
                        .clicked()
                    {
                        cancel_request = Some(id.clone());
                    }
                });
            }
        });
        if let Some(id) = cancel_request {
            self.run.requests.retain(|s| *s != id);
        }
        let mut load_smart = false;
        let mut matching_smart = false;
        let mut edit_smart = false;
        let mut context_edit = None;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(sidebar_width, height - 30.),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(sidebar_width);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    ui.selectable_value(
                        &mut self.dj_browser.source,
                        Source::Smart,
                        "Smart Playlists",
                    );
                    ui.selectable_value(
                        &mut self.dj_browser.source,
                        Source::Playlists,
                        "Playlists",
                    );
                    ui.selectable_value(&mut self.dj_browser.source, Source::Folders, "Folders");
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("dj-source-sidebar")
                        .max_height((height - 160.).max(70.))
                        .show(ui, |ui| match self.dj_browser.source {
                            Source::Smart => {
                                for profile in settings_snapshot
                                    .as_ref()
                                    .map(|settings| settings.playlists.as_slice())
                                    .unwrap_or(&[])
                                {
                                    let row = ui.selectable_value(
                                        &mut self.dj_browser.smart,
                                        profile.id.clone(),
                                        &profile.name,
                                    );
                                    if let Some(action) = show_editor::playlist_menu(&row) {
                                        context_edit = Some((profile.id.clone(), action));
                                    }
                                }
                            }
                            Source::Playlists => {
                                if ui
                                    .selectable_label(
                                        self.dj_browser.playlist.is_none(),
                                        "Current playlist / queue",
                                    )
                                    .clicked()
                                {
                                    self.dj_browser.playlist = None;
                                }
                                for (i, (_, list)) in self.dj_browser.playlists.iter().enumerate() {
                                    if ui
                                        .selectable_label(
                                            self.dj_browser.playlist == Some(i),
                                            &list.name,
                                        )
                                        .clicked()
                                    {
                                        self.dj_browser.playlist = Some(i);
                                    }
                                }
                                open_playlist = ui.button("Open saved playlist...").clicked();
                            }
                            Source::Folders => {
                                if ui
                                    .selectable_label(
                                        self.dj_browser.folder.as_os_str().is_empty(),
                                        "All music",
                                    )
                                    .clicked()
                                {
                                    self.dj_browser.folder.clear();
                                }
                                if let Some(lib) = &library_snapshot {
                                    Self::folder_tree(
                                        ui,
                                        &lib.tree,
                                        Path::new(""),
                                        &mut self.dj_browser.folder,
                                    );
                                }
                            }
                        });
                },
            );
            window_layout::splitter(
                ui,
                "dj-source-divider",
                &mut sidebar_width,
                150. ..=sidebar_max,
                true,
                1.,
                height - 30.,
            );
            ui.ctx()
                .data_mut(|d| d.insert_persisted(sidebar_id, sidebar_width));
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                ui.add(
                    egui::TextEdit::singleline(&mut self.dj_browser.search)
                        .desired_width(ui.available_width())
                        .hint_text("Search this source"),
                );
                let Some(lib) = &library_snapshot else {
                    ui.weak(if self.scan.is_some() {
                        "Scanning your music folders. Tracks will appear when the scan finishes."
                    } else {
                        "Add music sources in AMP Library."
                    });
                    return;
                };
                if self.dj_browser.source != Source::Smart {
                    ui.horizontal(|ui| {
                        edit_smart = ui.button("Edit sources / rules in Library").clicked();
                        refresh |= ui
                            .button("Refresh")
                            .on_hover_text("Update this list from saved library data")
                            .clicked();
                    });
                }
                let mut indices: Vec<usize> = match self.dj_browser.source {
                    Source::Folders => lib
                        .tracks
                        .iter()
                        .enumerate()
                        .filter(|(_, t)| t.path.starts_with(&self.dj_browser.folder))
                        .map(|(i, _)| i)
                        .collect(),
                    Source::Playlists => self
                        .dj_browser
                        .playlist
                        .and_then(|i| self.dj_browser.playlists.get(i))
                        .map(|(_, p)| playlist_indices(&self.dj_browser.path_index, p))
                        .unwrap_or_else(|| self.dj_browser.queue_snapshot.clone()),
                    Source::Smart => {
                        if let Some(profile) = settings_snapshot.as_ref().and_then(|settings| {
                            settings
                                .playlists
                                .iter()
                                .find(|p| p.id == self.dj_browser.smart)
                        }) {
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [(ui.available_width() - 485.).max(40.), 26.],
                                    egui::Label::new(egui::RichText::new(&profile.name).strong())
                                        .truncate(),
                                );
                                load_smart = ui.button("Load for Auto DJ").clicked();
                                edit_smart = ui.button("Edit sources / rules in Library").clicked();
                                refresh |= ui
                                    .button("Refresh")
                                    .on_hover_text(
                                        "Update this list from saved rules and library data",
                                    )
                                    .clicked();
                            });
                            if self.show_editor.playlist_unsaved(&profile.id) {
                                ui.colored_label(
                                    ORANGE,
                                    "Unsaved source changes: DJ uses the saved version.",
                                );
                                if ui.button("Open draft in Library to save").clicked() {
                                    edit_smart = true;
                                }
                            }
                            match self.dj_browser.membership.indices(
                                profile,
                                lib,
                                self.dj_browser.refresh_generation,
                            ) {
                                Some(indices) => indices,
                                None => {
                                    matching_smart = true;
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label("Matching saved playlist rules…");
                                    });
                                    Vec::new()
                                }
                            }
                        } else {
                            ui.weak("Choose a saved Smart Playlist on the left.");
                            ui.horizontal(|ui| {
                                edit_smart = ui.button("Edit sources / rules in Library").clicked();
                                refresh |= ui.button("Refresh").clicked();
                            });
                            Vec::new()
                        }
                    }
                };
                let query = self.dj_browser.search.to_lowercase();
                indices = self.dj_browser.search_cache.filter(indices, &query, lib);
                if !matching_smart {
                    ui.weak(format!("{} tracks", indices.len()));
                }
                egui::ScrollArea::horizontal()
                    .id_salt("dj-columns-horizontal")
                    .show(ui, |ui| {
                        let width: f32 = self
                            .dj_browser
                            .columns
                            .columns
                            .iter()
                            .filter(|c| c.visible)
                            .map(|c| c.width + ui.spacing().item_spacing.x)
                            .sum::<f32>()
                            + 120.;
                        ui.set_min_width(width);
                        ui.horizontal(|ui| {
                            ui.add_sized([28., 24.], egui::Label::new("#"));
                            for column in self
                                .dj_browser
                                .columns
                                .columns
                                .iter_mut()
                                .filter(|c| c.visible)
                            {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(column.width, 24.),
                                    egui::Sense::hover(),
                                );
                                ui.painter_at(rect).text(
                                    rect.left_center(),
                                    egui::Align2::LEFT_CENTER,
                                    track_columns::label(&column.key),
                                    egui::FontId::proportional(14.),
                                    ui.visuals().text_color(),
                                );
                                let edge = egui::Rect::from_min_max(
                                    egui::pos2(rect.right() - 5., rect.top()),
                                    rect.right_bottom(),
                                );
                                let handle = ui
                                    .interact(
                                        edge,
                                        ui.scope_id().with((&column.key, "resize")),
                                        egui::Sense::drag(),
                                    )
                                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                                ui.painter().line_segment(
                                    [edge.center_top(), edge.center_bottom()],
                                    egui::Stroke::new(1., ui.visuals().weak_text_color()),
                                );
                                if handle.dragged() {
                                    column.width = (column.width
                                        + ui.input(|i| i.pointer.delta().x))
                                    .clamp(45., 800.);
                                }
                                save_columns |= handle.drag_stopped();
                            }
                            ui.label("Load");
                        });
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .id_salt("dj-source-tracks")
                            .max_height((height - if self.rack.six { 110. } else { 155. }).max(34.))
                            .show_rows(ui, 34., indices.len(), |ui, range| {
                                for row in range {
                                    let i = indices[row];
                                    let t = &lib.tracks[i];
                                    ui.horizontal(|ui| {
                                        ui.add_sized(
                                            [28., 30.],
                                            egui::Label::new((row + 1).to_string()),
                                        );
                                        for column in self
                                            .dj_browser
                                            .columns
                                            .columns
                                            .iter()
                                            .filter(|c| c.visible)
                                        {
                                            let value = if column.key == "stems" {
                                                let path = amp_core::music_sources::audio_path(
                                                    &snapshot_root,
                                                    &t.path,
                                                );
                                                let cached = self.dj_browser.stem_status.get(&path);
                                                if cached.is_none() {
                                                    stem_checks.push(path);
                                                }
                                                cached
                                                    .map_or("Checking…", |(_, ready)| {
                                                        if *ready {
                                                            "Complete"
                                                        } else {
                                                            "Not prepared"
                                                        }
                                                    })
                                                    .into()
                                            } else {
                                                track_columns::cell(t, &column.key)
                                            };
                                            let response = if column.key == "title" {
                                                ui.add_sized(
                                                    [column.width, 30.],
                                                    egui::Button::selectable(
                                                        self.dj_browser.selected_path.as_ref()
                                                            == Some(&t.path),
                                                        &value,
                                                    )
                                                    .truncate()
                                                    .sense(egui::Sense::click_and_drag()),
                                                )
                                            } else {
                                                ui.add_sized(
                                                    [column.width, 30.],
                                                    egui::Label::new(&value)
                                                        .truncate()
                                                        .sense(egui::Sense::click_and_drag()),
                                                )
                                            };
                                            let response = response.on_hover_text(&value);
                                            #[cfg(test)]
                                            if row == 0 && column.key == "title" {
                                                ui.ctx().data_mut(|data| {
                                                    data.insert_temp(
                                                        egui::Id::unique("dj-browser-first-track"),
                                                        (response.interact_rect, response.layer_id),
                                                    )
                                                });
                                            }
                                            {
                                                crate::dragdrop::source(
                                                    &response,
                                                    amp_core::music_sources::audio_path(
                                                        &snapshot_root,
                                                        &t.path,
                                                    ),
                                                );
                                            }
                                            if response.clicked() {
                                                self.dj_browser.selected_path =
                                                    Some(t.path.clone());
                                                self.selected =
                                                    self.library.as_ref().and_then(|library| {
                                                        library
                                                            .tracks
                                                            .iter()
                                                            .position(|track| track.path == t.path)
                                                    });
                                            }
                                            if response.double_clicked() {
                                                load = Some((
                                                    self.dj_browser.destination,
                                                    t.path.clone(),
                                                ));
                                            }
                                            response.context_menu(|ui| {
                                                if ui
                                                    .button("Track details / all metadata…")
                                                    .clicked()
                                                {
                                                    self.dj_browser.details = Some(t.clone());
                                                    ui.close();
                                                }
                                                ui.separator();
                                                ui.label("Request in this live Smart Playlist");
                                                for within in 1..=5 {
                                                    if ui
                                                        .button(format!(
                                                            "Play within {within} songs"
                                                        ))
                                                        .clicked()
                                                    {
                                                        request = Some((
                                                            amp_core::show::song_id(t),
                                                            within,
                                                        ));
                                                        ui.close();
                                                    }
                                                }
                                            });
                                        }
                                        ui.horizontal(|ui| {
                                            ui.set_min_width(68.);
                                            for (deck, label, color) in
                                                [(0, "A", ORANGE), (1, "B", CYAN)]
                                            {
                                                if ui
                                                    .add_sized(
                                                        [30., 24.],
                                                        egui::Button::new(
                                                            egui::RichText::new(label)
                                                                .strong()
                                                                .color(color),
                                                        ),
                                                    )
                                                    .clicked()
                                                {
                                                    self.dj_browser.destination = deck;
                                                    load = Some((deck, t.path.clone()));
                                                }
                                            }
                                        });
                                    });
                                }
                            });
                    });
                let target = ui.add_sized(
                    [ui.available_width(), 24.],
                    egui::Button::new(if self.run.auto {
                        "Drop music here > Auto Mix candidates"
                    } else {
                        "Drop music here > Playlist queue"
                    })
                    .sense(egui::Sense::hover()),
                );
                queue_drop = dragdrop::target(&target, GOLD);
                if indices.is_empty() && !matching_smart {
                    ui.weak("No tracks in this source. Prepare and edit playlists in AMP Library.");
                }
            });
        });
        if refresh {
            match amp_core::show::Settings::load(&self.root) {
                Ok(settings) => {
                    self.run.settings = settings;
                    for (path, playlist) in &mut self.dj_browser.playlists {
                        if let Ok(saved) = Playlist::load(path, &self.root) {
                            *playlist = saved;
                        }
                    }
                    stem_checks.clear();
                    if let Some(library) = &self.library {
                        self.dj_browser.capture(
                            &self.root,
                            library,
                            &self.run.settings,
                            &self.queue,
                        );
                    }
                    self.status =
                        "DJ list refreshed. Loaded decks and playback are unchanged.".into();
                }
                Err(error) => self.status = format!("Could not refresh DJ list: {error}"),
            }
        }
        if save_columns && let Err(error) = self.dj_browser.columns.save() {
            self.status = format!("Could not save columns: {error}");
        }
        if self.dj_browser.stem_rx.is_none() && !stem_checks.is_empty() {
            stem_checks.sort();
            stem_checks.dedup();
            let (tx, rx) = std::sync::mpsc::channel();
            self.dj_browser.stem_rx = Some(rx);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let result = stem_checks
                    .into_iter()
                    .map(|path| {
                        let ready = stems::ready(&path).is_some();
                        (path, ready)
                    })
                    .collect();
                let _ = tx.send(result);
                ctx.request_repaint();
            });
        }
        if let Some(path) = queue_drop {
            self.add_dropped_to_queue(path);
        }
        if open_playlist
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("AMP playlist", &["json"])
                .pick_file()
        {
            match Playlist::load(&path, &self.root) {
                Ok(list) => {
                    let index = self
                        .dj_browser
                        .playlists
                        .iter()
                        .position(|(p, _)| p == &path);
                    if let Some(i) = index {
                        self.dj_browser.playlists[i].1 = list;
                        self.dj_browser.playlist = Some(i);
                    } else {
                        self.dj_browser.playlists.push((path, list));
                        self.dj_browser.playlist = Some(self.dj_browser.playlists.len() - 1);
                    }
                }
                Err(e) => self.status = e.to_string(),
            }
        }
        if edit_smart && self.dj_browser.source != Source::Smart {
            self.workspace.pane = 1;
            self.library_open = true;
            self.library_page = 5;
        } else if edit_smart {
            context_edit = Some((
                self.dj_browser.smart.clone(),
                show_editor::PlaylistAction::Edit,
            ));
        }
        if let Some((id, action)) = context_edit {
            self.smart_playlist_action(ui, &id, action);
        }
        if load_smart {
            self.run.choice = self.dj_browser.smart.clone();
            if let Err(e) = self.execute_control(control::Action::LoadSmartPlaylist {
                playlist: self.dj_browser.smart.clone(),
                execution: control::Execution::LoadOnly,
                style: None,
                energy: None,
            }) {
                self.status = e;
            }
        }
        if let Some((song, within)) = request
            && let Err(error) = self.execute_control(control::Action::RequestSong { song, within })
        {
            self.status = error;
        }
        if let Some((deck, track_path)) = load {
            self.selected = self.library.as_ref().and_then(|library| {
                library
                    .tracks
                    .iter()
                    .position(|track| track.path == track_path)
            });
            let path = amp_core::music_sources::audio_path(&snapshot_root, &track_path);
            if let Err(error) = self.safe_load_path(self.rack.live[deck], path, None) {
                self.status = error;
            }
            self.dj_tab = 0;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smart_membership_reuses_matches_and_invalidates_rules_and_analysis() {
        let mut profile = amp_core::show::Settings::default().playlists.remove(0);
        profile.genres = vec!["Rock".into()];
        let mut library = Library {
            tracks: (0..14000)
                .map(|index| {
                    let mut preparation = amp_core::Preparation::default();
                    preparation.extensions.insert(
                        "genre".into(),
                        serde_json::json!(if index % 2 == 0 { "Rock" } else { "R&B" }),
                    );
                    amp_core::Track {
                        path: format!("songs/{index}.mp3").into(),
                        title: format!("Track {index}"),
                        preparation,
                    }
                })
                .collect(),
            tree: Folder {
                name: "Music".into(),
                folders: vec![],
                tracks: vec![],
            },
        };
        fn wait(
            cache: &mut Membership,
            p: &amp_core::show::SmartPlaylist,
            l: &Library,
            g: u64,
        ) -> Vec<usize> {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(indices) = cache.indices(p, l, g) {
                    return indices;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "membership worker timed out"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let mut cache = Membership::default();
        assert!(cache.indices(&profile, &library, 1).is_none());
        let matched = wait(&mut cache, &profile, &library, 1);
        assert_eq!(matched.len(), 7000);
        assert!(matched.iter().all(|i| i % 2 == 0));
        assert!(cache.job.is_none());
        assert_eq!(cache.indices(&profile, &library, 1), Some(matched));
        assert!(cache.job.is_none(), "redraw must reuse matching results");
        profile.genres = vec!["R&B".into()];
        assert!(
            cache.indices(&profile, &library, 1).is_none(),
            "do not show previous rules' rows"
        );
        assert!(
            wait(&mut cache, &profile, &library, 1)
                .iter()
                .all(|i| i % 2 == 1)
        );
        library.tracks[0]
            .preparation
            .extensions
            .insert("genre".into(), serde_json::json!("R&B"));
        assert!(cache.indices(&profile, &library, 2).is_none());
        assert_eq!(wait(&mut cache, &profile, &library, 2).len(), 7001);
    }

    #[test]
    fn displayed_snapshot_stays_fixed_until_explicit_refresh_and_keeps_paths() {
        let mut library = Library {
            tracks: vec![amp_core::Track {
                path: "a.mp3".into(),
                title: "Original".into(),
                preparation: Default::default(),
            }],
            tree: Folder {
                name: "Music".into(),
                folders: vec![],
                tracks: vec![0],
            },
        };
        let mut settings = amp_core::show::Settings::default();
        let mut second = library.tracks[0].clone();
        second.path = "b.mp3".into();
        second.title = "Second".into();
        library.tracks.push(second);
        let mut browser = Browser::default();
        browser.capture(Path::new("music"), &library, &settings, &[0]);
        assert_eq!(
            browser
                .search_cache
                .filter(vec![0, 1], "original", browser.snapshot.as_ref().unwrap()),
            vec![0]
        );
        let generation = browser.refresh_generation;
        let profile = settings.playlists[0].clone();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if browser
                .membership
                .indices(&profile, browser.snapshot.as_ref().unwrap(), generation)
                .is_some()
            {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        library.tracks[0].title = "Changed by analysis".into();
        settings.playlists[0].name = "Changed rules".into();
        library.tracks.reverse();
        assert_eq!(
            browser.snapshot.as_ref().unwrap().tracks[0].title,
            "Original"
        );
        assert_ne!(
            browser.settings_snapshot.as_ref().unwrap().playlists[0].name,
            settings.playlists[0].name
        );
        assert_eq!(
            browser
                .search_cache
                .filter(vec![0, 1], "original", browser.snapshot.as_ref().unwrap()),
            vec![0],
            "Search must remain stable until explicit refresh"
        );
        for _ in 0..5 {
            assert!(
                browser
                    .membership
                    .indices(&profile, browser.snapshot.as_ref().unwrap(), generation)
                    .is_some()
            );
            assert!(browser.membership.job.is_none());
        }
        browser.capture(Path::new("music"), &library, &settings, &[]);
        assert!(
            browser
                .search_cache
                .filter(vec![0, 1], "original", browser.snapshot.as_ref().unwrap())
                .is_empty()
        );
        assert_eq!(
            browser
                .search_cache
                .filter(vec![0, 1], "analysis", browser.snapshot.as_ref().unwrap()),
            vec![1]
        );
        assert_eq!(browser.path_index.get(Path::new("a.mp3")), Some(&1));
        assert_eq!(browser.refresh_generation, generation + 1);
        assert_eq!(
            browser.snapshot.as_ref().unwrap().tracks[1].title,
            "Changed by analysis"
        );
        assert_eq!(
            browser.snapshot.as_ref().unwrap().tracks[1].path,
            PathBuf::from("a.mp3")
        );
        assert!(browser.queue_snapshot.is_empty());
    }
    #[test]
    fn saved_playlist_keeps_order_and_duplicates_without_changing_library() {
        let library = Library {
            tracks: vec![
                amp_core::Track {
                    path: "a.mp3".into(),
                    title: "A".into(),
                    preparation: Default::default(),
                },
                amp_core::Track {
                    path: "b.mp3".into(),
                    title: "B".into(),
                    preparation: Default::default(),
                },
            ],
            tree: Folder {
                name: "Music".into(),
                folders: vec![],
                tracks: vec![0, 1],
            },
        };
        let playlist = Playlist {
            companion_available: false,
            sync: None,
            version: 1,
            name: "Show".into(),
            tracks: vec!["b.mp3".into(), "a.mp3".into(), "b.mp3".into()],
        };
        assert_eq!(
            playlist_indices(&track_positions(&library), &playlist),
            vec![1, 0, 1]
        );
        assert_eq!(library.tracks[0].title, "A");
    }
}
