use super::*;
impl Player {
    pub(super) fn library_ui(&mut self, ui: &mut egui::Ui) {
        if theme::current(ui.ctx()) != theme::Theme::Classic {
            let style = ui.style_mut();
            style
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(16.));
            style
                .text_styles
                .insert(egui::TextStyle::Button, egui::FontId::proportional(15.));
            style
                .text_styles
                .insert(egui::TextStyle::Small, egui::FontId::proportional(13.));
            style
                .text_styles
                .insert(egui::TextStyle::Heading, egui::FontId::proportional(24.));
        }
        self.preparer
            .show(ui.ctx(), &self.root, self.library.as_mut());
        ui.painter().rect_filled(
            ui.max_rect(),
            0.,
            theme::current(ui.ctx()).palette().background,
        );
        self.header(ui, "AMP Library", false);
        let height = (ui.available_height() - 64.).max(350.);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(egui::vec2(176.,height - 70.),egui::Layout::top_down(egui::Align::Min),|ui| {
                for (page,icon,title) in [(0,icons::Icon::Folder,"Library"),(1,icons::Icon::Layers,"Smart Playlists"),(2,icons::Icon::List,"Playlists"),(3,icons::Icon::Device,"Devices"),(4,icons::Icon::History,"History"),(5,icons::Icon::Folder,"Sources"),(6,icons::Icon::Analyze,"Analyze Music"),(7,icons::Icon::Analyze,"Equalizer")] {
                    if icons::button(ui,icon,title,self.library_page==page,egui::vec2(172.,46.)).clicked(){self.library_page=page;}
                }
                ui.add_space(20.);ui.separator();
                let total=self.library.as_ref().map_or(0,|l|l.tracks.len());
                ui.label(RichText::new("Library ready").color(skin::signal(ui)));ui.weak(format!("{total} tracks"));
                ui.weak(if self.preparer.busy(){"Analysis running"}else{"No active analysis"});
            });
            ui.separator();
            ui.vertical(|ui| {ui.set_width(ui.available_width()-8.);
                match self.library_page {
                    0 => self.organized_music(ui,height),
                    1 => self.organized_smart(ui,height),
                    2 => self.organized_playlists(ui,height),
                    3 => {ui.heading("Devices");ui.weak("Find AMP Cast servers on your LAN or connect by address.");self.cast.show(ui);},
                    4 => {ui.heading("Playback history");ui.weak("Tracks started by this desktop session. A start does not mean the full track was played.");egui::ScrollArea::vertical().max_height(height-80.).show(ui,|ui|{for (deck,title) in self.playback_history.iter().rev(){ui.label(format!("Deck {}    {title}",deck+1));}if self.playback_history.is_empty(){ui.weak("Play a song to begin your session history.");}});},
                    5 => {ui.heading("Music sources");ui.label("Local folders and mounted network libraries");ui.label(self.root.display().to_string());ui.horizontal(|ui|{if ui.button("Browse music folder").clicked(){self.choose_folder();}
if ui.add_enabled(self.scan.is_none(),egui::Button::new("Rescan")).clicked(){self.scan();}});ui.separator();ui.heading("Remote control");ui.label(&self.control_status);ui.label("Listen address");ui.text_edit_singleline(&mut self.control_bind);ui.label("Optional password");ui.add(egui::TextEdit::singleline(&mut self.control_password).password(true));ui.horizontal(|ui|{if ui.button("Start / restart control").clicked(){self.start_control(ui.ctx());}
if ui.button("Stop control").clicked(){self.control=None;self.control_restart=None;self.control_status="Control stopped".into();}});theme::selector(ui);},
                    7 => self.library_equalizer(ui),
                    _ => self.organized_analysis(ui),
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.colored_label(
                skin::accent(ui),
                format!(
                    "{} files",
                    self.library.as_ref().map_or(0, |l| l.tracks.len())
                ),
            );
            ui.separator();
            ui.add(egui::Label::new(self.root.display().to_string()).truncate());
        });
    }
    fn organized_smart(&mut self, ui: &mut egui::Ui, height: f32) {
        ui.heading("Smart Playlists");
        ui.weak("Choose music. Choose a style. Save.");
        ui.add_space(12.);
        ui.horizontal_top(|ui| {
            let width = (ui.available_width() * 0.25).clamp(180., 260.);
            let selected = self.show_editor.selected_id().map(str::to_owned);
            let mut action = None;
            skin::panel(ui, false, |ui| {
                ui.set_width(width);
                ui.set_min_height((height - 140.).max(200.));
                ui.strong("My Smart Playlists");
                if ui
                    .add_sized([width, 34.], egui::Button::new("+ New Playlist"))
                    .clicked()
                {
                    self.show_editor
                        .new_playlist(&self.root, &self.run.settings);
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("smart-playlist-shelf")
                    .max_height((height - 200.).max(160.))
                    .show(ui, |ui| {
                        for list in self.show_editor.playlists(&self.run.settings) {
                            let row = ui.add_sized(
                                [width, 36.],
                                egui::Button::selectable(
                                    selected.as_deref() == Some(list.id.as_str()),
                                    &list.name,
                                )
                                .right_text("")
                                .truncate(),
                            );
                            if row.clicked() {
                                self.show_editor.open_playlist(
                                    &list.id,
                                    &self.root,
                                    &self.run.settings,
                                );
                            }
                            if let Some(value) = show_editor::playlist_menu(&row) {
                                action = Some((list.id.clone(), value));
                            }
                        }
                    });
            });
            if let Some((id, value)) = action {
                self.smart_playlist_action(ui, &id, value);
            }
            ui.allocate_ui_with_layout(
                egui::vec2(
                    (ui.available_width() - 8.).max(200.),
                    (height - 80.).max(220.),
                ),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    skin::panel(ui, false, |ui| {
                        ui.set_min_width(ui.available_width());
                        self.show_editor.preview_active = self.smart_preview.active();
                        self.show_editor.preview_status = self.smart_preview.status.clone();
                        self.show_editor.audition_blocked = self.run.locked
                            || self.run.auto
                            || self.audio.as_ref().is_some_and(|audio| {
                                audio.playing(0)
                                    || audio.playing(1)
                                    || audio.controls.sample_playing.iter().any(|voice| {
                                        voice.load(std::sync::atomic::Ordering::Relaxed)
                                    })
                            });
                        self.show_editor.set_existing_sources(
                            self.dj_browser
                                .playlists
                                .iter()
                                .map(|(_, list)| list.clone())
                                .collect(),
                        );
                        self.show_editor.show(
                            ui,
                            &self.root,
                            &mut self.run.settings,
                            self.library.as_ref(),
                        );
                    });
                },
            );
        });
        if self.show_editor.stop_preview {
            self.smart_preview.stop();
            self.show_editor.stop_preview = false;
        }
        if let Some((profile, indices)) = self.show_editor.mix_request.take() {
            if self.show_editor.audition_blocked {
                self.status = "Stop live playback before previewing a mix".into();
            } else if let Some(library) = &self.library {
                let tracks = indices
                    .iter()
                    .filter_map(|&index| library.tracks.get(index))
                    .cloned()
                    .collect();
                if let Err(error) = self.smart_preview.start(
                    &self.root,
                    tracks,
                    &profile,
                    self.volume as f32 / 100.,
                ) {
                    self.smart_preview.status = error;
                }
            }
        }
        if let Some(preview) = self.show_editor.preview.take() {
            self.smart_preview.stop();
            self.preparer.preview = Some(preview);
        }
    }
    fn organized_playlists(&mut self, ui: &mut egui::Ui, height: f32) {
        ui.heading("Playlists");
        ui.weak("Create, manage and organize your playlists.");
        if let Some((path, list)) = &self.playlist_snapshot {
            if let Some(entry) = self
                .dj_browser
                .playlists
                .iter_mut()
                .find(|(p, _)| p == path)
            {
                entry.1 = list.clone();
            } else {
                self.dj_browser.playlists.push((path.clone(), list.clone()));
            }
        }
        ui.horizontal(|ui| {
            if ui.button("New playlist").clicked() {
                self.new_playlist_name = Some(String::new());
            }
            if ui.button("Import playlist").clicked() {
                self.load_playlist();
            }
        });
        if let Some(mut name) = self.new_playlist_name.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label("Playlist name");
                ui.add(
                    egui::TextEdit::singleline(&mut name)
                        .hint_text("e.g. Friday night")
                        .desired_width(220.),
                );
                self.new_playlist_name = Some(name.clone());
                if ui
                    .add_enabled(
                        !name.trim().is_empty(),
                        egui::Button::new("Create playlist..."),
                    )
                    .clicked()
                {
                    let directory = self.root.join(".ampverse/playlists");
                    match std::fs::create_dir_all(&directory) {
                        Ok(()) => {
                            let filename: String = name
                                .chars()
                                .map(|c| {
                                    if c.is_control() || "<>:\"/\\|?*".contains(c) {
                                        '_'
                                    } else {
                                        c
                                    }
                                })
                                .collect();
                            if let Some(path) = rfd::FileDialog::new()
                                .set_directory(directory)
                                .add_filter("AMP playlist", &["json"])
                                .set_file_name(format!("{}.json", filename.trim()))
                                .save_file()
                                && let Err(error) = self.create_playlist_at(path, &name)
                            {
                                self.status = error;
                            }
                        }
                        Err(error) => {
                            self.status = format!("Could not create playlist folder: {error}")
                        }
                    }
                }
                if ui.button("Cancel").clicked() {
                    self.new_playlist_name = None;
                }
            });
        }
        ui.add_space(12.);
        ui.horizontal_top(|ui| {
            let width = (ui.available_width() * 0.32).max(190.);
            let mut chosen = None;
            skin::panel(ui, false, |ui| {
                ui.set_width(width);
                ui.set_min_height(height - 125.);
                ui.strong("Opened playlists");
                egui::ScrollArea::vertical()
                    .max_height(height - 180.)
                    .show(ui, |ui| {
                        for (i, (path, list)) in self.dj_browser.playlists.iter().enumerate() {
                            let selected = self
                                .playlist_snapshot
                                .as_ref()
                                .is_some_and(|(p, _)| p == path);
                            if icons::button(
                                ui,
                                icons::Icon::List,
                                &list.name,
                                selected,
                                egui::vec2(width, 46.),
                            )
                            .clicked()
                            {
                                chosen = Some(i);
                            }
                            ui.weak(format!("{} tracks", list.tracks.len()));
                        }
                        if self.dj_browser.playlists.is_empty() {
                            ui.weak("Import a saved playlist or save your current queue.");
                        }
                    });
            });
            if let Some(i) = chosen {
                let (path, list) = self.dj_browser.playlists[i].clone();
                if let Some(lib) = &self.library {
                    let indices: Option<Vec<_>> = list
                        .tracks
                        .iter()
                        .map(|p| lib.tracks.iter().position(|t| &t.path == p))
                        .collect();
                    if let Some(indices) = indices {
                        self.queue = indices;
                        self.queue_selected = None;
                        self.playlist_snapshot = Some((path, list));
                    } else {
                        self.status =
                            "Rescan: this playlist has tracks outside the indexed library".into();
                    }
                }
            }
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - 4.);
                skin::panel(ui, false, |ui| {
                    ui.heading(
                        self.playlist_snapshot
                            .as_ref()
                            .map_or("New playlist", |(_, p)| p.name.as_str()),
                    );
                    ui.weak(format!("{} tracks", self.queue.len()));
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Add songs from Library").clicked() { self.library_page = 0; self.status = "Select songs with the library checkboxes, or use Add folder. Then save the playlist queue.".into(); }
                        if ui.add_enabled(self.playlist_snapshot.is_some() || !self.queue.is_empty(),egui::Button::new("Save playlist")).clicked() { self.save_playlist(); }
                    });
                    if ui
                        .add_enabled(!self.queue.is_empty(), egui::Button::new("Play playlist"))
                        .clicked()
                    {
                        self.selected = self.queue.first().copied();
                        if let Some(index) = self.selected {
                            self.play(0, index);
                            self.auto = true;
                            if let Some(a) = &self.audio {
                                a.set_fader(0.);
                            }
                        }
                    }
                });
                self.organized_queue(ui, (height - 300.).max(100.));
            });
        });
    }
    fn organized_analysis(&mut self, ui: &mut egui::Ui) {
        ui.heading("Analyze Music");
        ui.weak("Prepare BPM, key, beatgrids, waveforms and cue suggestions.");
        ui.add_space(18.);
        let total = self.library.as_ref().map_or(0, |l| l.tracks.len());
        let analyzed = self.library.as_ref().map_or(0, |l| {
            l.tracks
                .iter()
                .filter(|t| t.preparation.analysis.is_some())
                .count()
        });
        ui.columns(3, |columns| {
            for (column, label, value) in [
                (0, "Total tracks", total),
                (1, "With analysis", analyzed),
                (2, "Missing analysis", total - analyzed),
            ] {
                skin::panel(&mut columns[column], false, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(RichText::new(value.to_string()).size(30.).strong());
                    ui.weak(label);
                });
            }
        });
        ui.add_space(18.);
        skin::panel(ui, false, |ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("Analysis progress");
            let (done, total) = self.preparer.progress();
            if total > 0 {
                ui.add(
                    egui::ProgressBar::new(done as f32 / total as f32)
                        .text(format!("{done} / {total} jobs returned")),
                );
            }
            ui.label(if self.preparer.status.is_empty() {
                "Ready to analyze your library"
            } else {
                &self.preparer.status
            });
            if self.preparer.busy() {
                ui.spinner();
            }
            ui.add_space(12.);
            if ui
                .add_enabled(
                    !self.preparer.busy() && self.library.is_some(),
                    egui::Button::new("Analyze library"),
                )
                .clicked()
                && let Some(lib) = &self.library
            {
                self.preparer.batch(self.root.clone(), lib.tracks.clone());
            }
            ui.label("Parallel songs (0 = automatic)");
            ui.add_enabled(
                !self.preparer.busy(),
                egui::DragValue::new(&mut self.preparer.workers)
                    .range(0..=amp_core::batch::cpu_count().min(32)),
            );
            ui.weak("Analysis requires the optional engine add-on. Manual edits remain protected.");
        });
    }
    fn organized_music(&mut self, ui: &mut egui::Ui, height: f32) {
        ui.horizontal(|ui| {
            let (search_icon, _) =
                ui.allocate_exact_size(egui::vec2(24., 24.), egui::Sense::hover());
            icons::draw(ui, search_icon, icons::Icon::Search, skin::accent(ui));
            ui.add_sized(
                [ui.available_width(), 38.],
                egui::TextEdit::singleline(&mut self.search).hint_text("Search music and folders…"),
            );
        });
        ui.add_space(12.);
        ui.horizontal_top(|ui| {
            let folder_width = (ui.available_width() * 0.29).clamp(180., 290.);
            skin::panel(ui, false, |ui| {
                ui.set_width(folder_width);
                ui.set_min_height(height - 80.);
                ui.heading("Folders");
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("organized-folders")
                    .max_height(height - 140.)
                    .show(ui, |ui| {
                        if let Some(lib) = &self.library {
                            Self::folder_tree(ui, &lib.tree, Path::new(""), &mut self.folder);
                        } else {
                            ui.weak("Add a music folder in Sources.");
                        }
                    });
            });
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - 4.);
                skin::panel(ui, false, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.heading("Tracks");
                    ui.weak(if self.folder.as_os_str().is_empty() {
                        "Music / All files"
                    } else {
                        self.folder.to_str().unwrap_or("Music")
                    });
                    let mut play = None;
                    let mut edit = None;
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .id_salt("organized-tracks")
                        .max_height(height * 0.34)
                        .min_scrolled_height(height * 0.34)
                        .show(ui, |ui| {
                            egui::Grid::new("organized-track-grid")
                                .striped(true)
                                .spacing([16., 12.])
                                .show(ui, |ui| {
                                    for label in ["Queue", "Title", "BPM", "Key", "Rating"] {
                                        ui.strong(label);
                                    }
                                    ui.end_row();
                                    if let Some(lib) = &self.library {
                                        for (index, track) in lib.tracks.iter().enumerate() {
                                            if !track.path.starts_with(&self.folder)
                                                || !format!(
                                                    "{} {}",
                                                    track.title,
                                                    track.path.display()
                                                )
                                                .to_lowercase()
                                                .contains(&self.search.to_lowercase())
                                            {
                                                continue;
                                            }
                                            let mut included = self.queue.contains(&index);
                                            if ui.checkbox(&mut included, "").changed() {
                                                if included {
                                                    self.queue.push(index);
                                                } else {
                                                    self.queue.retain(|&i| i != index);
                                                    self.queue_selected = None;
                                                }
                                            }
                                            let row = ui.selectable_label(
                                                self.selected == Some(index),
                                                &track.title,
                                            );
                                            if row.clicked() {
                                                self.selected = Some(index);
                                            }
                                            if row.double_clicked() {
                                                play = Some(index);
                                            }
                                            row.context_menu(|ui| {
                                                if ui
                                                    .button("Edit cues, key, BPM and rating")
                                                    .clicked()
                                                {
                                                    edit = Some(index);
                                                    ui.close();
                                                }
                                            });
                                            ui.label(
                                                track
                                                    .preparation
                                                    .bpm
                                                    .map(|v| format!("{v:.1}"))
                                                    .unwrap_or_else(|| "—".into()),
                                            );
                                            ui.label(
                                                track
                                                    .preparation
                                                    .harmonic_key
                                                    .as_deref()
                                                    .or(track.preparation.key.as_deref())
                                                    .unwrap_or("—"),
                                            );
                                            ui.label(
                                                track
                                                    .preparation
                                                    .extensions
                                                    .get("rating")
                                                    .and_then(|v| v.as_u64())
                                                    .map(|v| format!("{v}/5"))
                                                    .unwrap_or_else(|| "—".into()),
                                            );
                                            ui.end_row();
                                        }
                                    }
                                });
                        });
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(self.selected.is_some(), egui::Button::new("Play"))
                            .clicked()
                        {
                            play = self.selected;
                        }
                        if ui
                            .add_enabled(self.selected.is_some(), egui::Button::new("Edit song"))
                            .clicked()
                        {
                            edit = self.selected;
                        }
                        if ui.button("Add folder").clicked()
                            && let Some(lib) = &self.library
                        {
                            for (i, t) in lib.tracks.iter().enumerate() {
                                if t.path.starts_with(&self.folder) && !self.queue.contains(&i) {
                                    self.queue.push(i);
                                }
                            }
                        }
                    });
                    egui::CollapsingHeader::new("Selected track waveform").show(ui, |ui| {
                        let track = self
                            .selected
                            .and_then(|i| self.library.as_ref()?.tracks.get(i));
                        let position = track.map_or(0., |t| t.preparation.cue_in);
                        self.preview_wave
                            .show(ui, &self.root, track, position, 32., 0);
                    });
                    if let Some(i) = play {
                        self.play(0, i);
                        if let Some(a) = &self.audio {
                            a.set_fader(0.);
                        }
                    }
                    if let Some(track) = edit.and_then(|i| self.library.as_ref()?.tracks.get(i)) {
                        self.preparer.edit(&self.root, track);
                    }
                });
                self.organized_queue(ui, (height * 0.28 - 54.).max(60.));
            });
        });
    }
    fn organized_queue(&mut self, ui: &mut egui::Ui, height: f32) {
        skin::panel(ui, false, |ui| {
            ui.set_min_width(ui.available_width());
            ui.heading(format!("Playlist Queue · {} tracks", self.queue.len()));
            ui.horizontal_wrapped(|ui| {
                if ui.button("Load").clicked() {
                    self.load_playlist();
                }
                if ui
                    .add_enabled(
                        self.playlist_snapshot.is_some() || !self.queue.is_empty(),
                        egui::Button::new("Save"),
                    )
                    .clicked()
                {
                    self.save_playlist();
                }
                if ui
                    .add_enabled(self.queue.len() > 1, egui::Button::new("Order for mixing"))
                    .clicked()
                {
                    self.curate_playlist();
                }
                if ui
                    .add_enabled(!self.queue.is_empty(), egui::Button::new("Find similar"))
                    .clicked()
                {
                    self.expand_smart_list();
                }
                ui.menu_button("More", |ui| {
                    for (name, up) in [("Move up", true), ("Move down", false)] {
                        if ui.button(name).clicked()
                            && let Some(i) = self.queue_selected.filter(|&i| i < self.queue.len())
                        {
                            let next = if up {
                                i.saturating_sub(1)
                            } else {
                                (i + 1).min(self.queue.len() - 1)
                            };
                            self.queue.swap(i, next);
                            self.queue_selected = Some(next);
                            ui.close();
                        }
                    }
                    if ui.button("Remove selected").clicked()
                        && let Some(i) =
                            self.queue_selected.take().filter(|&i| i < self.queue.len())
                    {
                        self.queue.remove(i);
                        ui.close();
                    }
                    if ui.button("Clear queue").clicked() {
                        self.queue.clear();
                        self.queue_selected = None;
                        ui.close();
                    }
                });
            });
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .id_salt("organized-queue")
                .max_height(height)
                .min_scrolled_height(height.min(100.))
                .show(ui, |ui| {
                    if self.queue.is_empty() {
                        ui.add_space(18.);
                        ui.label(RichText::new("Add tracks to the queue").size(20.).strong());
                        ui.weak("Select tracks using the checkboxes in your library.");
                    }
                    if let Some(lib) = &self.library {
                        for (row, &index) in self.queue.iter().enumerate() {
                            if let Some(track) = lib.tracks.get(index)
                                && ui
                                    .selectable_label(
                                        self.queue_selected == Some(row),
                                        format!("{:02}     {}", row + 1, track.title),
                                    )
                                    .clicked()
                            {
                                self.queue_selected = Some(row);
                            }
                        }
                    }
                });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regular_playlist_create_add_save_and_restart_discovery() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(root.join("song.mp3"), b"fixture").unwrap();
        let folder = root.join(".ampverse/playlists");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("Friday.json");
        let mut player = Player {
            root: root.into(),
            ..Default::default()
        };
        player
            .create_playlist_at(path.clone(), "Friday night")
            .unwrap();
        let empty = Playlist::load(&path, root).unwrap();
        assert!(empty.tracks.is_empty());
        assert_eq!(empty.name, "Friday night");
        assert!(
            player
                .create_playlist_at(path.clone(), "Overwrite")
                .is_err()
        );
        player.accept_library_scan(amp_core::scan(root).unwrap());
        player.queue = vec![0, 0];
        player.save_playlist();
        let saved = Playlist::load(&path, root).unwrap();
        assert_eq!(saved.name, "Friday night");
        assert_eq!(saved.tracks, vec![PathBuf::from("song.mp3"); 2]);
        assert_eq!(std::fs::read(root.join("song.mp3")).unwrap(), b"fixture");
        let mut restarted = Player {
            root: root.into(),
            ..Default::default()
        };
        restarted.accept_library_scan(amp_core::scan(root).unwrap());
        assert!(
            restarted
                .dj_browser
                .playlists
                .iter()
                .any(|(p, list)| p == &path
                    && list.name == "Friday night"
                    && list.tracks.len() == 2)
        );
    }

    #[test]
    fn library_navigation_changes_page_without_touching_playback_or_queue() {
        let ctx = egui::Context::default();
        theme::apply(&ctx, theme::Theme::ModernDark);
        let mut player = Player {
            loaded: [Some(3), None],
            playing: [true, false],
            queue: vec![3, 2, 3],
            ..Default::default()
        };
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280., 860.));
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| player.library_ui(ui),
        );
        output.textures_delta.clear();
        let rect = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("navigation-button", "Sources"))))
            .unwrap();
        for pressed in [true, false] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events: vec![
                        egui::Event::PointerMoved(rect.center()),
                        egui::Event::PointerButton {
                            pos: rect.center(),
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| player.library_ui(ui),
            );
            output.textures_delta.clear();
        }
        assert_eq!(player.library_page, 5);
        assert_eq!(player.loaded, [Some(3), None]);
        assert_eq!(player.playing, [true, false]);
        assert_eq!(player.queue, vec![3, 2, 3]);
    }
}
