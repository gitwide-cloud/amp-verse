use super::*;
impl Player {
    pub(super) fn library_ui(&mut self, ui: &mut egui::Ui) {
        let blocked = self.live_mix.as_ref().is_some_and(|b| b.running())
            || self.run.auto
            || self.auto
            || self.auxiliary_playing()
            || self
                .audio
                .as_ref()
                .is_some_and(|a| (0..6).any(|d| a.playing(d)))
            || self.preparer.busy()
            || self.stem_panel.busy()
            || self.lab_open;

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
        self.preparer.preview_blocked = self.library_preview_blocked();
        self.preparer
            .show(ui.ctx(), &self.root, self.library.as_mut());
        ui.painter().rect_filled(
            ui.max_rect(),
            0.,
            theme::current(ui.ctx()).palette().background,
        );
        self.header(ui, "AMP Library", false);
        if self.workspace.enabled {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_sized([145., 32.], egui::Button::new("Add music folder…"))
                    .clicked()
                    && let Some(folder) = rfd::FileDialog::new()
                        .set_title("Add music (including subfolders)")
                        .pick_folder()
                {
                    self.add_music_source(folder);
                }
                if ui
                    .add_sized([145., 32.], egui::Button::new("Organise library"))
                    .clicked()
                {
                    self.library_page = 8;
                }
                self.remote_access_button(ui);
            });
        } else {
            self.remote_access_button(ui);
            let add_music = ui.button("Add music folder…");
            #[cfg(test)]
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::unique("library-add-music"),
                    (add_music.rect, add_music.enabled()),
                )
            });
            if add_music.clicked()
                && let Some(folder) = rfd::FileDialog::new()
                    .set_title("Add music to Library (includes subfolders)")
                    .pick_folder()
            {
                self.add_music_source(folder);
            }
            if ui
                .button("Find duplicates / Consolidate library…")
                .clicked()
            {
                self.library_page = 8;
            }
        }

        let height = (ui.available_height() - 64.).max(260.);
        egui::ScrollArea::vertical().id_salt("library-workflow-body").max_height((ui.available_height()-62.).max(80.)).auto_shrink([false,false]).show(ui,|ui| {
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(egui::vec2(176.,height - 70.),egui::Layout::top_down(egui::Align::Min),|ui| {
                egui::ScrollArea::vertical().id_salt("library-navigation").max_height((height-180.).max(160.)).show(ui,|ui|{
                for (page,icon,title) in [(0,icons::Icon::Folder,"Library"),(1,icons::Icon::Layers,"Smart Playlists"),(2,icons::Icon::List,"Playlists"),(3,icons::Icon::Device,"Devices"),(4,icons::Icon::History,"History"),(5,icons::Icon::Folder,"Sources"),(6,icons::Icon::Analyze,"Analyze Music"),(7,icons::Icon::Analyze,"Equalizer"),(8,icons::Icon::Folder,"Organise library"),(9,icons::Icon::List,"Tags & artwork"),(10,icons::Icon::List,"Podcasts")] {
                    if icons::button(ui,icon,title,self.library_page==page,egui::vec2(172.,46.)).clicked(){self.library_page=page;}
                }
                });
                ui.add_space(20.);ui.separator();
                let total=self.library.as_ref().map_or(0,|l|l.tracks.len());
                ui.label(RichText::new(if self.root.as_os_str().is_empty() {"Add a music folder"} else if self.scan.is_some() && self.library.is_none() {"Finding music…"} else if self.scan.is_some() {"Library ready · refreshing"} else {"Library ready"}).color(skin::signal(ui)));ui.weak(format!("{total} tracks"));
                ui.weak(if self.analyze_after_scan {"Finding analysis inputs…"} else if self.preparer.busy(){"Analysis running"}else{"No active analysis"});
            });
            ui.separator();
            ui.vertical(|ui| {ui.set_width(ui.available_width()-8.);
                match self.library_page {
                    0 => self.organized_music(ui,height),
                    1 => self.organized_smart(ui,height),
                    2 => self.organized_playlists(ui,height),
                    3 => {egui::ScrollArea::vertical().max_height(height-50.).show(ui,|ui| {self.remote_access_button(ui);ui.weak("Control Player / DJ from Android or a web browser; open Cast and song requests.");ui.separator();if self.outputs.show(ui,blocked) {
                        let prepared=self.loaded;
                        self.audio=None;self.right_sample_audio=None;
                        self.loaded=[None;6];self.playing=[false;6];
                        for (deck,index) in prepared.into_iter().enumerate() {if let Some(index)=index {self.load_deck(deck,index,false);}}
                    }
                    ui.separator();self.controllers.show(ui);ui.separator();ui.heading("AMP Cast devices");self.cast.show(ui, &self.root, self.library.is_some());});},
                    4 => {ui.heading("Playback history");ui.weak("Tracks started by this desktop session. A start does not mean the full track was played.");egui::ScrollArea::vertical().max_height(height-80.).show(ui,|ui|{for (deck,title) in self.playback_history.iter().rev(){ui.label(format!("Deck {}    {title}",deck+1));}if self.playback_history.is_empty(){ui.weak("Play a song to begin your session history.");}});},
                    5 => {ui.heading("Music sources");ui.label("Local folders and mounted network libraries");self.music_source_controls(ui);ui.separator();self.remote_access_button(ui);theme::selector(ui);},
                    7 => self.library_equalizer(ui),
                    9 => {if self.selected.is_none() && self.library.as_ref().is_some_and(|l|!l.tracks.is_empty()){self.selected=Some(0);}egui::ScrollArea::vertical().id_salt("tags-artwork-page").max_height((height-60.).max(180.)).show(ui,|ui|{
                        self.lookup.batch_controls(ui,&self.root,self.library.as_ref());
                        if let Some(path)=self.lookup.review_song.take(){self.selected=self.library.as_ref().and_then(|l|l.tracks.iter().position(|t|t.path==path));}
                        let track=self.selected.and_then(|i|self.library.as_ref()?.tracks.get(i));self.lookup.show(ui,&self.root,track);
                    });},
                    10 => self.podcasts.show(ui),
                    8 => {
                        let audio=&mut self.audio;
                        if let Some(destination)=self.organizer.show(ui,&self.root,blocked,||{if let Some(audio)=audio {for d in 0..6 {audio.stop(d);}}}) {self.connect_music_root(destination);}
                    },
                    _ => self.organized_analysis(ui),
                }
            });
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
        let status = ui
            .add(egui::Label::new(&self.status).truncate())
            .on_hover_text(&self.status);
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(egui::Id::unique("library-workflow-status"), status.rect)
        });
        #[cfg(not(test))]
        let _ = status;
    }
    pub(super) fn add_music_source(&mut self, folder: PathBuf) {
        if self.root.as_os_str().is_empty() {
            self.connect_music_root(folder);
            return;
        }
        match amp_core::music_sources::add(&self.root, &folder) {
            Ok(sources) => {
                self.source_roots = sources;
                self.scan = None;
                self.scan();
                self.status="Music source added. Existing songs stay available while new folders scan in the background.".into();
            }
            Err(error) => self.status = error,
        }
    }
    fn music_source_controls(&mut self, ui: &mut egui::Ui) {
        ui.strong("Main library");
        ui.label(self.root.display().to_string());
        ui.horizontal_wrapped(|ui| {
            if ui.button("Add music folder…").clicked()
                && let Some(folder) = rfd::FileDialog::new()
                    .set_title("Add another music source (includes subfolders)")
                    .pick_folder()
            {
                self.add_music_source(folder);
            }
            if ui
                .add_enabled(self.scan.is_none(), egui::Button::new("Rescan all sources"))
                .clicked()
            {
                self.scan();
            }
            if ui.button("Change main library…").clicked() {
                self.choose_folder();
            }
        });
        ui.weak("Added folders are combined with the main library. Music files stay in their original locations.");
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("additional-music-sources")
            .max_height(180.)
            .show(ui, |ui| {
                for source in &self.source_roots {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(source.root.display().to_string());
                        if ui.small_button("Remove source").clicked() {
                            remove = Some(source.id.clone());
                        }
                    });
                }
            });
        if let Some(id) = remove {
            match amp_core::music_sources::remove(&self.root, &id) {
                Ok(sources) => {
                    self.source_roots = sources;
                    self.scan = None;
                    self.scan();
                    self.status =
                        "Source removed from this library. Audio and preparation files were kept."
                            .into();
                }
                Err(error) => self.status = error,
            }
        }
        if self.scan.is_some() {
            ui.label("Scanning sources in the background…");
        }
        ui.label(&self.status);
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
                        self.show_editor.audition_blocked = self.auxiliary_playing()
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
        if let Some((root, playlist)) = self.show_editor.connect_import.take() {
            if let Ok(saved) = amp_core::show::Settings::load(&root) {
                self.connect_music_root(root);
                self.run.settings = saved;
                self.show_editor
                    .stage_import(playlist, &self.root, &self.run.settings);
            } else {
                self.status="Could not load the original library. Import kept in the current editing draft.".into();
            }
        }
        if self.show_editor.refresh_sources && self.scan.is_none() {
            self.show_editor.refresh_sources = false;
            self.scan();
        }
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
                        egui::Button::new(if self.queue.is_empty() {
                            "Create playlist"
                        } else {
                            "Create from queue"
                        }),
                    )
                    .clicked()
                    && let Err(error) = self.create_named_playlist(&name)
                {
                    self.status = error;
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
                        self.player_sequence.smart=None;
                        self.player_sequence.explicit_regular=true;
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
                    if let Some((_,p))=&mut self.playlist_snapshot {ui.checkbox(&mut p.companion_available,"Available in Companion").on_hover_text("Save playlist to publish this preference.");}
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
                            self.queue.remove(0);
                            self.player_sequence.smart=None;
                            self.player_sequence.explicit_regular=true;
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
                if self.root.as_os_str().is_empty() {
                    "Add a music folder above to begin."
                } else {
                    "Ready to analyze your library"
                }
            } else {
                &self.preparer.status
            });
            if self.preparer.busy() || self.analyze_after_scan {
                ui.spinner();
            }
            if crate::engine::path().is_none() {
                ui.weak("The optional analysis engine is not installed. Playback and manual editing still work.");
                if ui.button("Install analysis engine…").clicked() {
                    self.addons.selected = 5;
                    self.activate(ui, Some(skin::Module::Addons));
                }
            }
            ui.add_space(12.);
            if ui
                .add_enabled(
                    !self.preparer.busy()
                        && !self.analyze_after_scan
                        && !self.root.as_os_str().is_empty(),
                    egui::Button::new("Analyze library"),
                )
                .clicked()
            {
                self.request_library_analysis();
            }
            crate::preparation_speed::controls(ui);
            ui.weak("Analysis requires the optional engine add-on. Manual edits remain protected.");
        });
        ui.add_space(18.);
        skin::panel(ui, false, |ui| {
            ui.set_min_width(ui.available_width());
            self.stem_panel.library(ui, &self.root);
            if crate::stems::runtime().is_none() && ui.button("Open stem add-on setup…").clicked()
            {
                self.addons.selected = 6;
                self.activate(ui, Some(skin::Module::Addons));
            }
        });
        ui.add_space(18.);
        let blocked = self.run.auto
            || self.stem_panel.busy()
            || self
                .audio
                .as_ref()
                .is_some_and(|a| a.playing(0) || a.playing(1));
        skin::panel(ui, false, |ui| {
            self.preparer.catalog_ui(ui, &self.root, blocked)
        });
    }
    fn organized_music(&mut self, ui: &mut egui::Ui, height: f32) {
        if self.selected.is_none() && self.library.as_ref().is_some_and(|l| !l.tracks.is_empty()) {
            self.selected = Some(0);
        }
        let track = self
            .selected
            .and_then(|i| self.library.as_ref()?.tracks.get(i))
            .cloned();
        let prepare_height = (height * 0.4).clamp(150., 300.);
        let preparation = egui::CollapsingHeader::new("Prepare selected song")
            .id_salt("library-selected-preparation")
            .default_open(!self.workspace.enabled && height >= 400.)
            .show(ui, |ui| {
                skin::panel(ui, false, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("inline-prepare-panel")
                        .max_height(prepare_height)
                        .show(ui, |ui| {
                            if let Some(track) = track.as_ref() {
                                self.lookup.cover(ui, &self.root, track);
                                if ui.button("Find tags / album artwork...").clicked() {
                                    self.library_page = 9;
                                    self.lookup.query.clear();
                                }
                            }
                            self.preparer.prepare_panel(
                                ui,
                                &self.root,
                                track.as_ref(),
                                self.library.as_mut(),
                            );
                        });
                });
            });
        let preparation_used = preparation.header_response.rect.height()
            + preparation
                .body_response
                .as_ref()
                .map_or(0., |response| response.rect.height());
        if let Some((path, section)) = self.preparer.mix_section.take()
            && let Some(t) = self
                .library
                .as_ref()
                .and_then(|l| l.tracks.iter().find(|t| t.path == path))
        {
            let song = amp_core::show::song_id(t);
            if let Err(e) = self.execute_control(control::Action::CallSong {
                song,
                cue: Some(section),
                deck: control::Deck::Auto,
                execution: control::Execution::AutoFade,
            }) {
                self.status = e;
            } else {
                self.dj_open = true;
            }
        }
        let height = (height - preparation_used - 12.).max(180.);
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
                    let list_height = if self.queue.is_empty() {
                        (height - 120.).max(80.)
                    } else {
                        height * 0.34
                    };
                    let query=self.search.to_lowercase();
                    let filtered:Vec<usize>=self.library.as_ref().map(|lib|lib.tracks.iter().enumerate().filter(|(_,t)|t.path.starts_with(&self.folder)&&(query.is_empty()||format!("{} {} {} {}",t.title,t.path.display(),track_columns::cell(t,"artist"),track_columns::cell(t,"album")).to_lowercase().contains(&query))).map(|(i,_)|i).collect()).unwrap_or_default();
                    ui.weak(format!("{} matching tracks",filtered.len()));
                    let title_width=(ui.available_width()-210.).max(130.);
                    ui.horizontal(|ui|{ui.label("Queue");ui.add_sized([title_width,20.],egui::Label::new("Title"));ui.label("BPM    Key    Rating");});
                    egui::ScrollArea::vertical().auto_shrink([false,false]).id_salt("organized-tracks").max_height(list_height).min_scrolled_height(list_height).show_rows(ui,28.,filtered.len(),|ui,range|{
                        if let Some(lib)=&self.library {for position in range {let index=filtered[position];let track=&lib.tracks[index];
                            ui.horizontal(|ui|{
let mut included = self.queue.contains(&index);
if ui.checkbox(&mut included, "").changed() {
    if included {
        self.queue.push(index);
    } else {
        self.queue.retain(|&i| i != index);
        self.queue_selected = None;
    }
}
let row = ui.add_sized([title_width,24.],egui::Button::selectable(self.selected==Some(index), &track.title).truncate());
let row = row.interact(egui::Sense::click_and_drag());
#[cfg(test)]if position==0 {ui.ctx().data_mut(|data|data.insert_temp(egui::Id::unique("library-first-track"),row.rect));}
dragdrop::source(&row, amp_core::music_sources::audio_path(&self.root, &track.path));
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

                            });
                        }}
                    });
                    ui.horizontal_wrapped(|ui| {
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
                        if ui.add_enabled(self.library.is_some(), egui::Button::new("Add folder to queue…")).clicked()
                            && let Some(folder) = rfd::FileDialog::new()
                                .set_title("Add a music folder to the playlist queue")
                                .set_directory(self.root.join(&self.folder))
                                .pick_folder()
                        {
                            match self.root.canonicalize().and_then(|root| folder.canonicalize().map(|folder| (root, folder))) {
                                Ok((root, folder)) => {
                                    if let Some(relative) = amp_core::relative_folder(&folder, &root) {
                                        self.folder = relative;
                                        self.queue_library_folder();
                                    } else {
                                        self.status = "That folder is not indexed yet. Use Add music folder to include it, then add it to the queue.".into();
                                    }
                                }
                                Err(error) => self.status = format!("Could not open folder: {error}"),
                            }
                        }
                        if ui.add_enabled(self.library.is_some(), egui::Button::new("Add selected folder to queue")).clicked() {
                            self.queue_library_folder();
                        }
                    });
                    ui.add(egui::Label::new(&self.status).truncate()).on_hover_text(&self.status);
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
                if self.queue.is_empty() {
                    egui::CollapsingHeader::new("Playlist queue · 0 tracks")
                        .show(ui, |ui| self.organized_queue(ui, 100.));
                } else {
                    self.organized_queue(ui, (height * 0.28 - 54.).max(60.));
                }
            });
        });
    }
    fn queue_library_folder(&mut self) {
        let Some(library) = &self.library else {
            self.status = "Choose a music library first.".into();
            return;
        };
        let mut matching = 0;
        let mut added = 0;
        for (index, track) in library.tracks.iter().enumerate() {
            if amp_core::folder_contains(&track.path, &self.folder) {
                matching += 1;
                if !self.queue.contains(&index) {
                    self.queue.push(index);
                    added += 1;
                }
            }
        }
        self.status = if matching == 0 {
            "No indexed songs in that folder or its subfolders. Use Rescan to check for new music."
                .into()
        } else if added == 0 {
            format!("All {matching} songs from this folder are already in the playlist queue.")
        } else {
            format!(
                "Added {added} songs from this folder and its subfolders to the playlist queue."
            )
        };
    }
    fn organized_queue(&mut self, ui: &mut egui::Ui, height: f32) {
        let mut drop = None;
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
                            if let Some(track) = lib.tracks.get(index) {
                                let response = ui.add(
                                    egui::Button::selectable(
                                        self.queue_selected == Some(row),
                                        format!("{:02}     {}", row + 1, track.title),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click_and_drag()),
                                );
                                dragdrop::source(
                                    &response,
                                    amp_core::music_sources::audio_path(&self.root, &track.path),
                                );
                                if response.clicked() {
                                    self.queue_selected = Some(row);
                                }
                            }
                        }
                    }
                });
            let response = ui.interact(
                ui.min_rect(),
                ui.scope_id().with("playlist-drop"),
                egui::Sense::hover(),
            );
            drop = dragdrop::target(&response, skin::accent(ui));
        });
        if let Some(path) = drop {
            self.add_dropped_to_queue(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adding_folder_includes_descendants_and_reports_duplicates_and_empty_folders() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("Rock/Live")).unwrap();
        for name in ["Rock/one.mp3", "Rock/Live/two.mp3", "outside.mp3"] {
            std::fs::write(root.path().join(name), b"fixture").unwrap();
        }
        let mut player = Player {
            root: root.path().into(),
            library: Some(amp_core::scan(root.path()).unwrap()),
            folder: "Rock".into(),
            ..Default::default()
        };
        player.queue_library_folder();
        assert_eq!(player.queue.len(), 2);
        assert!(player.status.contains("Added 2"));
        player.queue_library_folder();
        assert_eq!(player.queue.len(), 2);
        assert!(player.status.contains("already"));
        player.folder = "Empty".into();
        player.queue_library_folder();
        assert_eq!(player.queue.len(), 2);
        assert!(player.status.contains("No indexed songs"));
    }
    #[test]
    fn empty_library_exposes_add_music_and_keeps_status_reachable_in_a_short_window() {
        let ctx = egui::Context::default();
        let mut player = Player {
            root: PathBuf::new(),
            status: "Add music to begin".into(),
            ..Default::default()
        };
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 460.),
                )),
                ..Default::default()
            },
            |ui| player.library_ui(ui),
        );
        output.textures_delta.clear();
        let (button, enabled) = ctx.data(|data| {
            data.get_temp::<(egui::Rect, bool)>(egui::Id::unique("library-add-music"))
                .unwrap()
        });
        let status = ctx.data(|data| {
            data.get_temp::<egui::Rect>(egui::Id::unique("library-workflow-status"))
                .unwrap()
        });
        assert!(enabled);
        assert!(button.min.y >= 0. && button.max.y < 460.);
        assert!(status.max.y <= 460., "status clipped at {status:?}");
    }
    #[test]
    fn short_library_starts_with_a_visible_song_list_and_optional_preparation() {
        let ctx = egui::Context::default();
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("Song.mp3"), b"fixture").unwrap();
        let mut player = Player {
            root: root.path().into(),
            library: Some(amp_core::scan(root.path()).unwrap()),
            ..Default::default()
        };
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 460.),
                )),
                ..Default::default()
            },
            |ui| player.library_ui(ui),
        );
        output.textures_delta.clear();
        let row = ctx.data(|data| {
            data.get_temp::<egui::Rect>(egui::Id::unique("library-first-track"))
                .unwrap()
        });
        let footer = ctx.data(|data| {
            data.get_temp::<egui::Rect>(egui::Id::unique("library-workflow-status"))
                .unwrap()
        });
        assert!(
            row.min.y >= 0. && row.max.y < footer.min.y,
            "song row hidden: {row:?}, footer {footer:?}"
        );
    }
    #[test]
    fn named_playlist_creation_preserves_queue_and_avoids_file_naming_and_overwrite_traps() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("song.mp3"), b"fixture").unwrap();
        let mut player = Player {
            root: root.path().into(),
            library: Some(amp_core::scan(root.path()).unwrap()),
            queue: vec![0, 0],
            ..Default::default()
        };
        player
            .create_named_playlist("Friday: night / favourites")
            .unwrap();
        let (path, list) = player.playlist_snapshot.clone().unwrap();
        assert!(path.starts_with(root.path().join("ampdj/playlists")));
        assert_eq!(list.name, "Friday: night / favourites");
        assert_eq!(
            list.tracks,
            vec![PathBuf::from("song.mp3"), PathBuf::from("song.mp3")]
        );
        assert_eq!(player.queue, vec![0, 0]);
        amp_core::catalog_db::compact(root.path()).unwrap();
        if path.is_file() {
            std::fs::remove_file(&path).unwrap();
        }
        assert!(
            player
                .create_playlist_at(path.clone(), "Overwrite")
                .unwrap_err()
                .contains("already exists")
        );
        assert_eq!(Playlist::load(&path, root.path()).unwrap().name, list.name);
        player
            .create_named_playlist("Friday: night / favourites")
            .unwrap();
        assert_ne!(player.playlist_snapshot.as_ref().unwrap().0, path);
        assert_eq!(player.queue, vec![0, 0]);
    }
    #[test]
    fn regular_playlist_create_add_save_and_restart_discovery() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(root.join("song.mp3"), b"fixture").unwrap();
        let folder = root.join("ampdj/playlists");
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
        restarted.accept_scan(crate::library_cache::scan(root).unwrap());
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
            loaded: [Some(3), None, None, None, None, None],
            playing: [true, false, false, false, false, false],
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
        for pressed in [true, false, false, false, false, false] {
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
        assert_eq!(player.loaded, [Some(3), None, None, None, None, None]);
        assert_eq!(player.playing, [true, false, false, false, false, false]);
        assert_eq!(player.queue, vec![3, 2, 3]);
    }
}
