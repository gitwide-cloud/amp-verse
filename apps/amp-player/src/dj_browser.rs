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
}
impl Browser {
    pub(super) fn select_smart(&mut self, id: &str) {
        self.source = Source::Smart;
        self.smart = id.into();
    }
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
        let height = ui.available_height().clamp(240., 420.);
        skin::label(ui, "DJ LIBRARY");
        let mut auto = self.run.auto;
        let mut toggle = false;
        let mut queue_drop = None;
        let mut open_playlist = false;
        let mut load = None;
        let mut load_smart = false;
        let mut edit_smart = false;
        let mut context_edit = None;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(195., height - 30.),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(195.);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    let label = if auto {
                        "Auto Mix: ON"
                    } else {
                        "Auto Mix: OFF"
                    };
                    toggle = ui.toggle_value(&mut auto, label).changed();
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
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                ui.add(
                    egui::TextEdit::singleline(&mut self.dj_browser.search)
                        .desired_width(ui.available_width())
                        .hint_text("Search this source"),
                );
                let Some(lib) = &self.library else {
                    ui.weak("Add music sources in AMP Library.");
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
                                ui.colored_label(ORANGE,"Unsaved source changes: DJ uses the saved version.");
                                if ui.button("Open draft in Library to save").clicked(){edit_smart=true;}
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
                    })
                });
                ui.weak(format!("{} tracks", indices.len()));
                egui::ScrollArea::vertical()
                    .id_salt("dj-source-tracks")
                    .max_height((height - 125.).max(80.))
                    .min_scrolled_height((height - 125.).max(80.))
                    .show_rows(ui, 28., indices.len(), |ui, range| {
                        for row in range {
                            let i = indices[row];
                            let t = &lib.tracks[i];
                            ui.horizontal(|ui| {
                                let row=ui.add_sized(
                                        [(ui.available_width() - 300.).max(140.), 24.],
                                        egui::Button::selectable(
                                            self.selected == Some(i),
                                            &t.title,
                                        )
                                        .right_text("")
                                        .truncate().sense(egui::Sense::click_and_drag()),
                                    )
;
                                crate::dragdrop::source(&row,self.root.join(&t.path));
                                if row.clicked() {self.selected=Some(i);}
                                if row.double_clicked() {load=Some((self.dj_browser.destination,i));}
                                ui.weak(format!(
                                    "{} BPM · {}",
                                    t.preparation
                                        .bpm
                                        .map(|b| format!("{b:.1}"))
                                        .unwrap_or_else(|| "--".into()),
                                    t.preparation.harmonic_key.as_deref().unwrap_or("--")
                                ));
                                ui.label("Load →");
                                for (deck,label,color) in [(0,"A",ORANGE),(1,"B",CYAN)] {
                                    if ui.add_sized([30.,24.],egui::Button::new(egui::RichText::new(label).strong().color(color)).selected(self.dj_browser.destination==deck)).on_hover_text(format!("Prepare on Deck {label}; playing decks stage the replacement")).clicked(){self.dj_browser.destination=deck;load=Some((deck,i));}
                                }
                            });
                        }
                    });
                let target=ui.add_sized([ui.available_width(),24.],egui::Button::new(if self.run.auto {"Drop music here → Auto Mix candidates"}else{"Drop music here → Playlist queue"}).sense(egui::Sense::hover()));
                queue_drop=dragdrop::target(&target,GOLD);
                if indices.is_empty() {
                    ui.weak("No tracks in this source. Prepare and edit playlists in AMP Library.");
                }
            });
        });
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
                    let mut profile = amp_core::show::Settings::default().playlists.remove(7);
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
        if let Some((deck, index)) = load {
            self.selected = Some(index);
            if let Some(track) = self
                .library
                .as_ref()
                .and_then(|library| library.tracks.get(index))
            {
                let path = self.root.join(&track.path);
                if let Err(error) = self.safe_load_path(deck, path, None) {
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
            sync: None,
            version: 1,
            name: "Show".into(),
            tracks: vec!["b.mp3".into(), "a.mp3".into(), "b.mp3".into()],
        };
        assert_eq!(playlist_indices(&library, &playlist), vec![1, 0, 1]);
        assert_eq!(library.tracks[0].title, "A");
    }
}
