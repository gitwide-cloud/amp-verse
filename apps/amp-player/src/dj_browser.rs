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
    folder: PathBuf,
    smart: String,
    search: String,
    pub(super) playlists: Vec<(PathBuf, Playlist)>,
    playlist: Option<usize>,
    destination: usize,
    columns: track_columns::Layout,
    details: Option<amp_core::Track>,
    stem_status: std::collections::HashMap<PathBuf, (std::time::Instant, bool)>,
    stem_rx: Option<std::sync::mpsc::Receiver<Vec<(PathBuf, bool)>>>,
}
impl Browser {
    pub(super) fn selected_smart(&self) -> Option<&str> {
        (self.source == Source::Smart && !self.smart.is_empty()).then_some(self.smart.as_str())
    }
    pub(super) fn select_smart(&mut self, id: &str) {
        self.source = Source::Smart;
        self.smart = id.into();
    }
}
fn auto_mix_toggle(ui: &mut egui::Ui, enabled: &mut bool) -> egui::Response {
    let label = if *enabled {
        "Auto Mix: ON"
    } else {
        "Auto Mix: OFF"
    };
    ui.toggle_value(enabled, label)
        .on_hover_text("Start or stop automatic mixing from the selected source")
}
fn playlist_indices(library: &Library, playlist: &Playlist) -> Vec<usize> {
    playlist
        .tracks
        .iter()
        .filter_map(|path| library.tracks.iter().position(|t| &t.path == path))
        .collect()
}
impl Player {
    pub(super) fn dj_library_ui(&mut self, ui: &mut egui::Ui) {
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
                        amp_core::music_sources::audio_path(&self.root, &track.path)
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
                            let path = amp_core::music_sources::audio_path(&self.root, &track.path);
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

        let mut auto = self.run.auto;
        let mut toggle = false;
        let mut queue_drop = None;
        let mut open_playlist = false;
        let mut load = None;
        let mut request = None;
        let requests = ui
            .horizontal(|ui| {
                skin::label(ui, "DJ Library");
                ui.menu_button("Columns", |ui| {save_columns |= self.dj_browser.columns.menu(ui);});

                if ui.small_button("Refresh playlists").clicked() {
                    match amp_core::show::Settings::load(&self.root) {
                        Ok(settings) => {
                            self.run.settings = settings;
                            self.dj_browser.stem_status.clear();
                            self.status = "Saved Smart Playlists refreshed. Loaded decks keep their current mix.".into();
                        }
                        Err(error) => self.status = format!("Could not refresh Smart Playlists: {error}"),
                    }
                }

                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!(
                            "Song requests ({}) · click to view",
                            self.run.requests.len()
                        ))
                        .weak(),
                    )
                    .sense(egui::Sense::click()),
                )
            })
            .inner;
        let mut cancel_request = None;
        egui::Popup::menu(&requests).show(|ui| {
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
        let mut edit_smart = false;
        let mut context_edit = None;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(sidebar_width, height - 30.),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(sidebar_width);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    toggle = auto_mix_toggle(ui, &mut auto).changed();
                    ui.separator();
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
                                for profile in &self.run.settings.playlists {
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
                                if let Some(lib) = &self.library {
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
                let Some(lib) = &self.library else {
                    ui.weak(if self.scan.is_some() {
                        "Scanning your music folders. Tracks will appear when the scan finishes."
                    } else {
                        "Add music sources in AMP Library."
                    });
                    return;
                };
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
                        .map(|(_, p)| playlist_indices(lib, p))
                        .unwrap_or_else(|| self.queue.clone()),
                    Source::Smart => {
                        if let Some(profile) = self
                            .run
                            .settings
                            .playlists
                            .iter()
                            .find(|p| p.id == self.dj_browser.smart)
                        {
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [(ui.available_width() - 400.).max(120.), 26.],
                                    egui::Label::new(egui::RichText::new(&profile.name).strong())
                                        .truncate(),
                                );
                                load_smart = ui.button("Load for Auto DJ").clicked();
                                edit_smart = ui.button("Edit sources / rules in Library").clicked();
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
                            let eligible: Vec<_> = lib
                                .tracks
                                .iter()
                                .enumerate()
                                .filter(|(_, t)| amp_core::show::eligible(profile, t))
                                .map(|(i, _)| i)
                                .collect();
                            eligible
                        } else {
                            ui.weak("Choose a saved Smart Playlist on the left.");
                            Vec::new()
                        }
                    }
                };
                let query = self.dj_browser.search.to_lowercase();
                indices.retain(|&i| {
                    lib.tracks.get(i).is_some_and(|t| {
                        t.title.to_lowercase().contains(&query)
                            || t.path.to_string_lossy().to_lowercase().contains(&query)
                            || ["artist", "album", "genre"].iter().any(|key| {
                                track_columns::cell(t, key).to_lowercase().contains(&query)
                            })
                    })
                });
                ui.weak(format!("{} tracks", indices.len()));
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
                                                    &self.root, &t.path,
                                                );
                                                let cached = self.dj_browser.stem_status.get(&path);
                                                if cached.is_none_or(|(at, _)| {
                                                    at.elapsed().as_secs() > 30
                                                }) {
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
                                                        self.selected == Some(i),
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
                                                        .sense(egui::Sense::click()),
                                                )
                                            };
                                            let response = response.on_hover_text(&value);
                                            if column.key == "title" {
                                                crate::dragdrop::source(
                                                    &response,
                                                    amp_core::music_sources::audio_path(
                                                        &self.root, &t.path,
                                                    ),
                                                );
                                            }
                                            if response.clicked() {
                                                self.selected = Some(i);
                                            }
                                            if response.double_clicked() {
                                                load = Some((self.dj_browser.destination, i));
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
                                                    load = Some((deck, i));
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
                if indices.is_empty() {
                    ui.weak("No tracks in this source. Prepare and edit playlists in AMP Library.");
                }
            });
        });
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
        if edit_smart {
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
        if toggle {
            if auto && self.dj_browser.source != Source::Smart && !self.run.locked {
                // Snapshot the chosen browser source for this session only. Never
                // silently fall back to a different Smart Playlist or all folders.
                if let Some(library) = &self.library {
                    let mut profile = amp_core::show::Settings::default().playlists.remove(0);
                    profile.id = "browser-session".into();
                    profile.name = "DJ browser source".into();
                    if self.dj_browser.source == Source::Folders {
                        profile.restrict_folders = true;
                        profile.folders = vec![self.dj_browser.folder.clone()];
                    } else {
                        let indices = self
                            .dj_browser
                            .playlist
                            .and_then(|i| self.dj_browser.playlists.get(i))
                            .map(|(_, list)| playlist_indices(library, list))
                            .unwrap_or_else(|| self.queue.clone());
                        profile.restrict_allowed = true;
                        profile.allowed = indices
                            .iter()
                            .filter_map(|&i| library.tracks.get(i))
                            .map(amp_core::show::song_id)
                            .collect();
                    }
                    self.run.playlist = Some(profile.id.clone());
                    self.run.profile = Some(profile);
                    self.run.preloaded = None;
                }
            }
            let action = if auto
                && self.dj_browser.source == Source::Smart
                && !self.dj_browser.smart.is_empty()
            {
                self.run.choice = self.dj_browser.smart.clone();
                control::Action::LoadSmartPlaylist {
                    playlist: self.dj_browser.smart.clone(),
                    execution: control::Execution::PlayNow,
                    style: None,
                    energy: None,
                }
            } else {
                control::Action::AutoDj { enabled: auto }
            };
            if let Err(e) = self.execute_control(action) {
                self.status = format!("Auto Mix could not start: {e}");
            }
        }
        if let Some((song, within)) = request
            && let Err(error) = self.execute_control(control::Action::RequestSong { song, within })
        {
            self.status = error;
        }
        if let Some((deck, index)) = load {
            self.selected = Some(index);
            if let Some(track) = self
                .library
                .as_ref()
                .and_then(|library| library.tracks.get(index))
            {
                let path = amp_core::music_sources::audio_path(&self.root, &track.path);
                if let Err(error) = self.safe_load_path(self.rack.live[deck], path, None) {
                    self.status = error;
                }
            }
            self.dj_tab = 0;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auto_mix_button_can_be_clicked_on_and_off() {
        let ctx = egui::Context::default();
        let mut enabled = false;
        let mut rect = egui::Rect::NOTHING;
        let mut changed = false;
        let mut frame = |events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    let response = auto_mix_toggle(ui, &mut enabled);
                    rect = response.rect;
                    changed = response.changed();
                },
            );
            output.textures_delta.clear();
            (rect, enabled, changed)
        };
        let (rect, _, _) = frame(vec![]);
        for expected in [true, false] {
            let position = rect.center();
            frame(vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ]);
            let (_, enabled, changed) = frame(vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }]);
            assert_eq!(enabled, expected);
            assert!(changed);
        }
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
        assert_eq!(playlist_indices(&library, &playlist), vec![1, 0, 1]);
        assert_eq!(library.tracks[0].title, "A");
    }
}
