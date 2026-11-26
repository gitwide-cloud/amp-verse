// Shared Library and normal-playlist picker. Selection never owns playback.
impl Player {
    fn library_browser_results(&mut self) -> Vec<usize> {
        let fresh = self.library_builder.results.as_ref().is_some_and(|r| {
            r.generation == self.library_generation
                && r.folder == self.folder
                && r.general == self.search
                && r.tracks == self.library_builder.tracks
                && r.refreshed.elapsed() < Duration::from_secs(1)
        });
        if !fresh {
            let indices = self
                .library
                .as_ref()
                .map(|lib| {
                    lib.tracks
                        .iter()
                        .enumerate()
                        .filter(|(_, t)| {
                            (!self.search.trim().is_empty() || t.path.starts_with(&self.folder))
                                && matches_track(t, &self.search)
                                && matches_track(t, &self.library_builder.tracks)
                        })
                        .map(|(i, _)| i)
                        .collect()
                })
                .unwrap_or_default();
            self.library_builder.results = Some(LibraryResults {
                generation: self.library_generation,
                folder: self.folder.clone(),
                general: self.search.clone(),
                tracks: self.library_builder.tracks.clone(),
                refreshed: std::time::Instant::now(),
                indices,
            });
        }
        self.library_builder
            .results
            .as_ref()
            .map(|r| r.indices.clone())
            .unwrap_or_default()
    }
    fn checked_library_indices(&self) -> Vec<usize> {
        self.library
            .as_ref()
            .map(|lib| {
                lib.tracks
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| self.library_builder.checked.contains(&t.path))
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default()
    }
    fn add_checked_to_queue(&mut self) {
        let indices = self.checked_library_indices();
        let mut added = 0;
        for index in indices {
            if !self.queue.contains(&index) {
                self.queue.push(index);
                added += 1;
            }
        }
        self.status = format!("Added {added} checked songs to playback queue; playback unchanged.");
    }
    fn add_indices_to_playlist(&mut self, indices: &[usize]) {
        if self.playlist_preview.snapshot.is_none() || !self.refresh_playlist_preview() {
            self.status =
                "Choose or create a playlist first. Reconnect any missing source before editing."
                    .into();
            return;
        }
        let mut added = 0;
        for &index in indices {
            if self
                .library
                .as_ref()
                .is_some_and(|lib| lib.tracks.get(index).is_some())
                && !self.playlist_preview.queue.contains(&index)
            {
                self.playlist_preview.queue.push(index);
                added += 1;
            }
        }
        self.remember_playlist_preview_paths();
        self.edit_playlist_preview(|p| p.save_playlist());
        self.status = format!("Added {added} songs to playlist · saved. Existing songs kept once.");
    }
    fn add_checked_to_playlist(&mut self) {
        let indices = self.checked_library_indices();
        self.add_indices_to_playlist(&indices);
    }
    fn add_folder_to_playlist(&mut self) {
        let indices = self
            .library
            .as_ref()
            .map(|lib| {
                lib.tracks
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.path.starts_with(&self.folder))
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.add_indices_to_playlist(&indices);
    }
    fn filtered_folder_tree(
        ui: &mut egui::Ui,
        folder: &Folder,
        path: &Path,
        selected: &mut PathBuf,
        query: &str,
    ) {
        if query.is_empty() {
            Self::folder_tree(ui, folder, path, selected);
            return;
        }
        if !folder_matches(folder, path, query) {
            return;
        }
        let label = if path.as_os_str().is_empty() {
            "All music".to_string()
        } else {
            folder.name.clone()
        };
        if ui.selectable_label(*selected == path, label).clicked() {
            *selected = path.into();
        }
        ui.indent(path, |ui| {
            for child in &folder.folders {
                Self::filtered_folder_tree(ui, child, &path.join(&child.name), selected, query);
            }
        });
    }
    fn music_browser(&mut self, ui: &mut egui::Ui, height: f32, editing: bool) {
        if self.library_builder.root != self.root {
            self.library_builder = LibraryBuilder {
                root: self.root.clone(),
                open: editing,
                ..Default::default()
            };
        }
        ui.horizontal(|ui| {
            ui.label("Search library");
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Artist, title, album, genre, filename or any part…")
                    .desired_width((ui.available_width() - 80.).max(120.)),
            );
            if ui.button("Clear").clicked() {
                self.search.clear();
                self.library_builder.tracks.clear();
                self.library_builder.folders.clear();
                self.folder = PathBuf::new();
            }
        });
        let mut filtered = self.library_browser_results();
        let checked = self.checked_library_indices().len();
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{checked} checked · {} matching", filtered.len()));
            if ui.button("Check results").clicked()
                && let Some(lib) = &self.library
            {
                for &i in &filtered {
                    self.library_builder
                        .checked
                        .insert(lib.tracks[i].path.clone());
                }
            }
            if ui.button("Clear checks").clicked() {
                self.library_builder.checked.clear();
            }
            let queue_action =
                ui.add_enabled(checked > 0, egui::Button::new("Add checked to queue"));
            #[cfg(test)]
            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    egui::Id::unique("library-add-checked-queue"),
                    queue_action.rect,
                )
            });
            if queue_action.clicked() {
                self.add_checked_to_queue();
            }
            if editing {
                if ui
                    .add_enabled(checked > 0, egui::Button::new("Add checked to playlist"))
                    .clicked()
                {
                    self.add_checked_to_playlist();
                }
            } else {
                ui.menu_button("Add checked to playlist…", |ui| {
                    if checked == 0 {
                        ui.weak("Check songs first.");
                    }
                    let lists: Vec<_> = self
                        .dj_browser
                        .playlists
                        .iter()
                        .enumerate()
                        .map(|(i, (_, p))| (i, p.name.clone()))
                        .collect();
                    for (i, name) in lists {
                        if ui
                            .add_enabled(checked > 0, egui::Button::new(name))
                            .clicked()
                        {
                            self.select_playlist_card(i);
                            self.add_checked_to_playlist();
                            ui.close();
                        }
                    }
                    if ui.button("Create blank playlist…").clicked() {
                        self.library_page = 2;
                        self.new_playlist_name = Some(String::new());
                        ui.close();
                    }
                });
            }
        });
        let width = ui.available_width();
        let folders_width = (width * 0.24).clamp(130., 230.);
        let mut preview = None;
        let mut play = None;
        let mut edit = None;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.selected.is_some(), egui::Button::new("Play selected"))
                .clicked()
            {
                play = self.selected;
            }
            if ui
                .add_enabled(
                    self.selected.is_some(),
                    egui::Button::new("Edit song details / cues"),
                )
                .clicked()
            {
                edit = self.selected;
            }
        });
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(egui::vec2(folders_width, height-100.), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.horizontal(|ui| { ui.strong("Folders"); });
                ui.add(egui::TextEdit::singleline(&mut self.library_builder.folders).hint_text("Search folders…").desired_width(ui.available_width()));
                if ui.selectable_label(self.folder.as_os_str().is_empty(), "All music").clicked() { self.folder = PathBuf::new(); }
                egui::ScrollArea::vertical().id_salt(("builder-folders",editing)).max_height((height-190.).max(80.)).show(ui, |ui| {
                    if let Some(lib) = &self.library { Self::filtered_folder_tree(ui, &lib.tree, Path::new(""), &mut self.folder, &self.library_builder.folders.trim().to_lowercase()); }
                });
                if editing {
                    if ui.button("Add folder to playlist").on_hover_text("Add this folder and all subfolders, ignoring search filters").clicked() { self.add_folder_to_playlist(); }
                } else if ui.button("Add folder to queue").clicked() { self.queue_library_folder(); }
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width((width-folders_width-25.).max(160.));
                ui.horizontal(|ui| {
                    ui.strong("Tracks");
                    ui.add(egui::TextEdit::singleline(&mut self.library_builder.tracks).hint_text("Search tracks in this folder…").desired_width((ui.available_width()-12.).max(100.)));
                });
                ui.weak(format!("Folder: {}", if self.folder.as_os_str().is_empty() { "All music".into() } else { self.folder.display().to_string() }));
                egui::ScrollArea::both().id_salt(("builder-tracks",editing)).max_height((height-160.).max(100.)).auto_shrink([false,false]).show(ui, |ui| {
                    let row_width = ui.available_width().max(780.);
                    let title_width = (row_width*0.29).max(190.);
                    ui.horizontal(|ui| {
                        ui.add_sized([28.,20.],egui::Label::new("✓"));
                        for (key,label,w) in [("title","Title",title_width),("artist","Artist",155.),("album","Album / Genre",180.),("length","Length / BPM / Key",155.)] { crate::track_sort::button(ui,"library-picker",key,label,w); }
                    });
                    if let Some(lib)=&self.library {
                        let order=crate::track_sort::rows(ui.ctx(),"library-picker",self.library_generation,lib,&filtered);
                        filtered=order.iter().map(|row|filtered[*row]).collect();
                    }
                    egui::ScrollArea::vertical().id_salt(("builder-rows",editing)).max_height((height-195.).max(80.)).show_rows(ui, 50., filtered.len(), |ui, range| {
                        if let Some(lib) = &self.library { for position in range {
                            let index=filtered[position]; let track=&lib.tracks[index];
                            ui.horizontal(|ui| {
                                let mut marked=self.library_builder.checked.contains(&track.path);
                                let selection = ui.checkbox(&mut marked, "").on_hover_text("Select song; does not change playback or playlist");
                                #[cfg(test)] if position==0 { ui.ctx().data_mut(|d|d.insert_temp(egui::Id::unique("library-first-checkbox"),selection.rect)); }
                                if selection.changed() {
                                    if marked { self.library_builder.checked.insert(track.path.clone()); } else { self.library_builder.checked.remove(&track.path); }
                                }
                                let row=ui.add_sized([title_width,44.],egui::Button::selectable(self.selected==Some(index),track_columns::cell(track,"title")).truncate().sense(egui::Sense::click_and_drag()));
                                #[cfg(test)] if position==0 { ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("library-first-track"),row.rect)); }
                                dragdrop::source(&row, amp_core::music_sources::audio_path(&self.root,&track.path));
                                row.clone().on_hover_text(format!("{}\nArtist: {}\nAlbum: {}\nGenre: {}\nFile: {}",track.title,track_columns::cell(track,"artist"),track_columns::cell(track,"album"),track_columns::cell(track,"genre"),track.path.display()));
                                if row.clicked() { self.selected=Some(index); }
                                if row.double_clicked() { if editing { preview=Some(index); } else { play=Some(index); } }
                                ui.add_sized([155.,44.],egui::Label::new(track_columns::cell(track,"artist")).truncate());
                                ui.add_sized([180.,44.],egui::Label::new(format!("{}\n{}",track_columns::cell(track,"album"),track_columns::cell(track,"genre"))).truncate());
                                ui.add_sized([155.,44.],egui::Label::new(format!("{} · {} · {}",track_columns::cell(track,"length"),track_columns::cell(track,"bpm"),track_columns::cell(track,"key"))).truncate());
                                if ui.button("Preview").clicked() { preview=Some(index); }
                            });
                        }}
                    });
                });
            });
        });
        if let Some(track) = edit.and_then(|i| self.library.as_ref()?.tracks.get(i)) {
            self.preparer.edit(&self.root, track);
        }
        if let Some(index) = preview {
            self.preview_library_song(index);
        }
        if let Some(index) = play {
            self.play(0, index);
            if let Some(a) = &self.audio {
                a.set_fader(0.);
            }
        }
        ui.horizontal_wrapped(|ui| {
            if self.smart_preview.active() && ui.button("Stop preview").clicked() {
                self.smart_preview.stop();
            }
            ui.label(&self.status);
        });
    }
    fn preview_library_song(&mut self, index: usize) {
        if self.library_preview_blocked() {
            self.status =
                "Pause deck playback before previewing; your live mix was not changed.".into();
            return;
        }
        let Some(track) = self
            .library
            .as_ref()
            .and_then(|lib| lib.tracks.get(index))
            .cloned()
        else {
            return;
        };
        let Some(profile) = self.run.settings.playlists.first() else {
            return;
        };
        if let Err(error) =
            self.smart_preview
                .start(&self.root, vec![track], profile, self.volume as f32 / 100.)
        {
            self.status = error;
        } else {
            self.status = "Previewing song · playlist and queue unchanged".into();
        }
    }
    fn find_playlist_suggestions(&mut self) {
        if !self.refresh_playlist_preview() {
            return;
        }
        self.find_suggestions(self.playlist_preview.queue.clone(), false);
    }
    pub(super) fn find_suggestions(&mut self, seeds: Vec<usize>, to_queue: bool) {
        self.library_builder.suggest_to_queue = to_queue;
        self.library_builder.suggestions.clear();
        let Some(lib) = &self.library else {
            return;
        };
        if seeds.is_empty() {
            self.status = "Add a song before requesting similar songs.".into();
            return;
        }
        let seed_set: std::collections::HashSet<_> = seeds.iter().copied().collect();
        let seed_artists: std::collections::HashSet<_> = seeds
            .iter()
            .filter_map(|i| lib.tracks.get(*i))
            .take(100)
            .map(|t| track_columns::cell(t, "artist").to_lowercase())
            .filter(|a| !a.is_empty())
            .collect();
        let seed_genres: std::collections::HashSet<_> = seeds
            .iter()
            .filter_map(|i| lib.tracks.get(*i))
            .take(100)
            .flat_map(|t| amp_core::genres::values(&t.preparation))
            .map(|g| g.to_lowercase())
            .collect();
        let preparations: Vec<_> = lib.tracks.iter().map(|t| &t.preparation).collect();
        let prepared = amp_core::smart::curate(
            &preparations,
            &seeds[..seeds.len().min(100)],
            seeds.len().min(100).saturating_add(20),
        )
        .unwrap_or_default();
        let mut candidates: Vec<_> = prepared
            .into_iter()
            .filter(|i| !seed_set.contains(i))
            .map(|i| (i, 100usize))
            .collect();
        for (i, track) in lib.tracks.iter().enumerate() {
            if seed_set.contains(&i) || candidates.iter().any(|(index, _)| *index == i) {
                continue;
            }
            let artist = track_columns::cell(track, "artist").to_lowercase();
            let genres = amp_core::genres::values(&track.preparation);
            let same_artist = !artist.is_empty() && seed_artists.contains(&artist);
            let same_genre = genres
                .iter()
                .any(|g| seed_genres.contains(&g.to_lowercase()));
            let score = usize::from(same_artist) * 30 + usize::from(same_genre) * 10;
            if score > 0 {
                candidates.push((i, score));
            }
        }
        candidates.sort_by_key(|(i, score)| (std::cmp::Reverse(*score), *i));
        self.library_builder.suggestions = candidates
            .into_iter()
            .take(20)
            .map(|(i, _)| lib.tracks[i].path.clone())
            .collect();
        self.status = format!(
            "{} suggestions · based on prepared mixing data or matching artist/genre · nothing added",
            self.library_builder.suggestions.len()
        );
    }
    fn playlist_suggestions_ui(&mut self, ui: &mut egui::Ui) {
        if self.library_builder.suggestions.is_empty() {
            return;
        }
        ui.separator();
        ui.strong(if self.library_builder.suggest_to_queue {
            "Suggested songs for playback queue · preview, then choose Add"
        } else {
            "Suggested songs for playlist · preview, then choose Add"
        });
        let mut add = None;
        let mut preview = None;
        egui::ScrollArea::vertical()
            .id_salt("playlist-suggestions")
            .max_height(220.)
            .show(ui, |ui| {
                if let Some(lib) = &self.library {
                    for path in &self.library_builder.suggestions {
                        if let Some((index, track)) =
                            lib.tracks.iter().enumerate().find(|(_, t)| &t.path == path)
                        {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(format!(
                                    "{} — {} · {} · {}",
                                    track_columns::cell(track, "title"),
                                    track_columns::cell(track, "artist"),
                                    track_columns::cell(track, "album"),
                                    track_columns::cell(track, "genre")
                                ));
                                if ui.button("Preview").clicked() {
                                    preview = Some(index);
                                }
                                if ui.button("Add").clicked() {
                                    add = Some(index);
                                }
                            });
                        }
                    }
                }
            });
        if let Some(index) = preview {
            self.preview_library_song(index);
        }
        if let Some(index) = add {
            if self.library_builder.suggest_to_queue {
                if !self.queue.contains(&index) {
                    self.queue.push(index);
                }
                self.status = "Suggestion added to playback queue.".into();
            } else {
                self.add_indices_to_playlist(&[index]);
            }
            if let Some(lib) = &self.library {
                let path = &lib.tracks[index].path;
                self.library_builder.suggestions.retain(|p| p != path);
            }
        }
        if self.smart_preview.active() && ui.button("Stop preview").clicked() {
            self.smart_preview.stop();
        }
    }
}
