use super::*;
#[path = "library_cards.rs"]
mod cards;
/// Shared local-artwork renderer; two pending decodes, at most64 cached textures.
/// Falls back to a code-drawn tile without network requests.
pub(super) fn artwork(
    ui: &egui::Ui,
    rect: egui::Rect,
    root: &Path,
    track: Option<&amp_core::Track>,
    name: &str,
) {
    cards::artwork(ui, rect, root, track, name);
}
#[derive(Default)]
pub(super) struct PlaylistPreview {
    root: PathBuf,
    paths: Vec<PathBuf>,
    queue: Vec<usize>,
    snapshot: Option<(PathBuf, Playlist)>,
    selected: Option<usize>,
}
#[derive(Default)]
pub(super) struct LibraryBuilder {
    open: bool,
    root: PathBuf,
    checked: std::collections::BTreeSet<PathBuf>,
    folders: String,
    tracks: String,
    suggestions: Vec<PathBuf>,
    suggest_to_queue: bool,
    results: Option<LibraryResults>,
}
struct LibraryResults {
    generation: u64,
    folder: PathBuf,
    general: String,
    tracks: String,
    refreshed: std::time::Instant,
    indices: Vec<usize>,
}
fn matches_track(track: &amp_core::Track, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let text = format!(
        "{} {} {}",
        track.title,
        track.path.display(),
        track_columns::FIELDS
            .iter()
            .map(|(key, _, _, _)| track_columns::cell(track, key))
            .collect::<Vec<_>>()
            .join(" ")
    )
    .to_lowercase();
    query.split_whitespace().all(|word| text.contains(word))
}
fn folder_matches(folder: &Folder, path: &Path, query: &str) -> bool {
    path.to_string_lossy().to_lowercase().contains(query)
        || folder
            .folders
            .iter()
            .any(|child| folder_matches(child, &path.join(&child.name), query))
}
include!("playlist_builder.rs");
impl Player {
    pub(super) fn apply_disliked_playlist(&mut self, path: &Path, song: &Path, saved: &Playlist) {
        if self
            .playlist_preview
            .snapshot
            .as_ref()
            .is_some_and(|(p, _)| p == path)
        {
            self.playlist_preview.paths.retain(|p| p != song);
            if let Some(lib) = &self.library {
                self.playlist_preview
                    .queue
                    .retain(|i| lib.tracks.get(*i).is_some_and(|t| t.path != song));
            }
            self.playlist_preview.snapshot = Some((path.to_path_buf(), saved.clone()));
            self.playlist_preview.selected = None;
        }
    }

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
        if self.library_page == 2 {
            // Playlist actions live beside its heading; source management remains in Library / Sources.
        } else if self.workspace.enabled {
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
        let body_width = ui.available_width().max(0.);
        let nav_width=(body_width*0.16).clamp(176.,224.);
        let content_width=(body_width-nav_width-54.).max(0.);
        ui.horizontal_top(|ui| {
             cards::panel(ui, |ui| {ui.set_width(nav_width);ui.set_min_height((height-66.).max(220.));
                egui::ScrollArea::vertical().id_salt("library-navigation").max_height((height-180.).max(160.)).show(ui,|ui|{
                for (page,icon,title) in [(0,icons::Icon::Folder,"Library"),(1,icons::Icon::Layers,"Smart Playlists"),(2,icons::Icon::List,"Playlists"),(3,icons::Icon::Device,"Devices"),(4,icons::Icon::History,"History"),(5,icons::Icon::Folder,"Sources"),(6,icons::Icon::Analyze,"Analyze Music"),(11,icons::Icon::List,"Transcribe"),(7,icons::Icon::Analyze,"Equalizer"),(8,icons::Icon::Folder,"Organise library"),(9,icons::Icon::List,"Tags & artwork")] {
                    if cards::navigation(ui,icon,title,self.library_page==page,nav_width).clicked(){self.library_page=page;}
                }
                });
                ui.add_space(20.);ui.separator();
                egui::Frame::NONE.inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                let total=self.library.as_ref().map_or(0,|l|l.tracks.len());
                ui.label(RichText::new(if self.root.as_os_str().is_empty() {"Add a music folder"} else if self.scan.is_some() && self.library.is_none() {"Finding music…"} else if self.scan.is_some() {"Library ready · refreshing"} else {"Library ready"}).color(skin::signal(ui)));ui.weak(format!("{total} tracks"));
                ui.weak(if self.analyze_after_scan {"Finding analysis inputs…"} else if self.preparer.busy(){"Analysis running"}else{"No active analysis"});
                });
            });
            ui.separator();
            ui.allocate_ui_with_layout(egui::vec2(content_width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(content_width);
                ui.set_max_width(content_width);
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
                    10 => {if ui.button("Open AMP Podcasts").clicked(){self.workspace.enabled=true;self.workspace.pane=6;}ui.weak("Podcast discovery, subscriptions and downloads live in AMP Podcasts.");},
                    11 => {let track=self.selected.and_then(|i|self.library.as_ref()?.tracks.get(i));self.transcription.show(ui,&self.root,track);},
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
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(12, 6))
            .show(ui, |ui| {
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
            });
    }
    pub(super) fn add_music_source(&mut self, folder: PathBuf) {
        if self.root.as_os_str().is_empty() {
            self.connect_music_root(folder);
            return;
        }
        match amp_core::music_sources::add(&self.root, &folder) {
            Ok(sources) => {
                self.source_roots = sources;
                if let Err(error) =
                    crate::startup_index::remember(&self.root, Some(&self.source_roots))
                {
                    self.status = format!(
                        "Source choices could not be backed up to the startup index: {error}"
                    );
                }
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
                    if let Err(error) =
                        crate::startup_index::remember(&self.root, Some(&self.source_roots))
                    {
                        self.status = format!(
                            "Source choices could not be backed up to the startup index: {error}"
                        );
                    }
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
        let selected_prep = self.show_editor.selected_id().map(str::to_owned);
        if ui
            .add_enabled(
                selected_prep.is_some(),
                egui::Button::new("Prepare selected playlist stems…"),
            )
            .clicked()
            && let Some(id) = selected_prep
        {
            self.offer_smart_playlist_stems(&id);
        }
        let (live, cached) = self.playlist_processing_state();
        self.stem_panel.playlist_preparation_ui(ui, live, cached);
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
                            let detail = if self.show_editor.playlist_unsaved(&list.id) {
                                "Unsaved changes".into()
                            } else {
                                format!("{} · smart rules", show_editor::source_summary(&list))
                            };
                            let row = cards::collection(
                                ui,
                                &self.root,
                                None,
                                &list.name,
                                &detail,
                                selected.as_deref() == Some(list.id.as_str()),
                                width,
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
        if let Some(playlist) = self.show_editor.send_to_dj.take() {
            match self.execute_control(control::Action::LoadSmartPlaylist {
                playlist: playlist.clone(),
                execution: control::Execution::PlayNow,
                style: None,
                energy: None,
            }) {
                Ok(()) => {
                    self.smart_preview.stop();
                    if let Some(library) = &self.library {
                        self.dj_browser.select_saved_smart(
                            &playlist,
                            &self.root,
                            library,
                            &self.run.settings,
                            &self.queue,
                        );
                    }
                    self.activate(ui, Some(skin::Module::Dj));
                    self.status = "Smart Playlist sent to DJ · Auto Mix enabled".into();
                }
                Err(error) => self.status = error,
            }
        }
        if let Some(preview) = self.show_editor.preview.take() {
            self.smart_preview.stop();
            self.preparer.preview = Some(preview);
        }
    }
    fn organized_playlists(&mut self, ui: &mut egui::Ui, height: f32) {
        if self.playlist_preview.root != self.root {
            self.playlist_preview = Default::default();
        }
        self.refresh_playlist_preview();
        ui.horizontal_wrapped(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(46., 48.), egui::Sense::hover());
            icons::draw(ui, rect, icons::Icon::List, skin::accent(ui));
            ui.vertical(|ui| {
                ui.label(RichText::new("Playlists").size(30.).strong());
                ui.weak("Create, manage, and organize your playlists.");
            });
            ui.add_space(18.);
            if ui.add(cards::primary("+ New Playlist")).clicked() {
                self.new_playlist_name = Some(String::new());
            }
            if ui
                .add_sized([145., 38.], egui::Button::new("Import Playlist"))
                .clicked()
            {
                self.import_playlist_preview();
            }
            if ui
                .add_sized([145., 38.], egui::Button::new("Organize Library"))
                .clicked()
            {
                self.library_page = 8;
            }
        });
        if self.playlist_preview.snapshot.is_none() && !self.dj_browser.playlists.is_empty() {
            self.select_playlist_card(0);
        }
        if let Some(mut name) = self.new_playlist_name.clone() {
            cards::panel(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong("New playlist");
                    ui.add(
                        egui::TextEdit::singleline(&mut name)
                            .hint_text("Playlist name")
                            .desired_width(240.),
                    );
                    self.new_playlist_name = Some(name.clone());
                    if ui
                        .add_enabled(
                            !name.trim().is_empty(),
                            egui::Button::new("Create blank playlist"),
                        )
                        .clicked()
                        && let Err(error) = self.create_preview_playlist(&name)
                    {
                        self.status = error;
                    }
                    if ui.button("Cancel").clicked() {
                        self.new_playlist_name = None;
                    }
                });
            });
        }
        ui.add_space(12.);
        let narrow = ui.available_width() < 760.;
        if narrow {
            cards::panel(ui, |ui| {
                ui.strong(format!("Playlists ({})", self.dj_browser.playlists.len()));
                let mut selected = None;
                egui::ScrollArea::horizontal()
                    .id_salt("playlist-card-strip")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (index, (path, list)) in
                                self.dj_browser.playlists.iter().enumerate()
                            {
                                let track = self.library.as_ref().and_then(|lib| {
                                    list.tracks.iter().find_map(|path| {
                                        lib.tracks.iter().find(|t| &t.path == path)
                                    })
                                });
                                if cards::collection(
                                    ui,
                                    &self.root,
                                    track,
                                    &list.name,
                                    &format!("{} tracks", list.tracks.len()),
                                    self.playlist_preview
                                        .snapshot
                                        .as_ref()
                                        .is_some_and(|(p, _)| p == path),
                                    230.,
                                )
                                .clicked()
                                {
                                    selected = Some(index);
                                }
                            }
                            if self.dj_browser.playlists.is_empty() {
                                ui.weak("Create or import a playlist to begin.");
                            }
                        });
                    });
                if let Some(index) = selected {
                    self.select_playlist_card(index);
                }
            });
            ui.add_space(10.);
            self.playlist_detail_card(ui, (height - 235.).max(160.));
        } else {
            ui.horizontal_top(|ui| {
                let width = (ui.available_width() * 0.29).clamp(240., 330.);
                let mut selected = None;
                cards::panel(ui, |ui| {
                    ui.set_width(width);
                    ui.set_min_height((height - 140.).max(240.));
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Playlists ({})",
                                self.dj_browser.playlists.len()
                            ))
                            .size(19.)
                            .strong(),
                        );
                    });
                    ui.add_space(8.);
                    egui::ScrollArea::vertical()
                        .id_salt("playlist-collection-cards")
                        .max_height((height - 200.).max(160.))
                        .show(ui, |ui| {
                            for (index, (path, list)) in
                                self.dj_browser.playlists.iter().enumerate()
                            {
                                let track = self.library.as_ref().and_then(|lib| {
                                    list.tracks.iter().find_map(|path| {
                                        lib.tracks.iter().find(|t| &t.path == path)
                                    })
                                });
                                let response = cards::collection(
                                    ui,
                                    &self.root,
                                    track,
                                    &list.name,
                                    &format!("{} tracks", list.tracks.len()),
                                    self.playlist_preview
                                        .snapshot
                                        .as_ref()
                                        .is_some_and(|(p, _)| p == path),
                                    width,
                                );
                                if response.clicked() {
                                    selected = Some(index);
                                }
                                response.context_menu(|ui| {
                                    if ui.button("Open playlist").clicked() {
                                        selected = Some(index);
                                        ui.close();
                                    }
                                });
                            }
                            if self.dj_browser.playlists.is_empty() {
                                ui.add_space(20.);
                                ui.strong("Your playlists belong here");
                                ui.weak(
                                    "Create a blank playlist, then add music from your library.",
                                );
                            }
                        });
                });
                if let Some(index) = selected {
                    self.select_playlist_card(index);
                }
                ui.add_space(4.);
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 4.).max(220.));
                    self.playlist_detail_card(ui, (height - 140.).max(240.));
                });
            });
        }
        let (live, cached) = self.playlist_processing_state();
        self.stem_panel.playlist_preparation_ui(ui, live, cached);
    }
    fn import_playlist_preview(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("AMP playlist", &["json"])
            .pick_file()
        else {
            return;
        };
        match Playlist::load(&path, &self.root) {
            Ok(list) => {
                let index = if let Some(index) = self
                    .dj_browser
                    .playlists
                    .iter()
                    .position(|(p, _)| p == &path)
                {
                    self.dj_browser.playlists[index].1 = list;
                    index
                } else {
                    self.dj_browser.playlists.push((path, list));
                    self.dj_browser.playlists.len() - 1
                };
                self.select_playlist_card(index);
            }
            Err(error) => self.status = error.to_string(),
        }
    }
    fn select_playlist_card(&mut self, index: usize) {
        let Some((path, list)) = self.dj_browser.playlists.get(index).cloned() else {
            return;
        };
        self.library_builder.suggestions.clear();
        let Some(lib) = &self.library else {
            self.status = "Open the music library before loading a playlist.".into();
            return;
        };
        let indices: Option<Vec<_>> = list
            .tracks
            .iter()
            .map(|path| lib.tracks.iter().position(|t| &t.path == path))
            .collect();
        if let Some(indices) = indices {
            self.playlist_preview = PlaylistPreview {
                root: self.root.clone(),
                paths: list.tracks.clone(),
                queue: indices,
                snapshot: Some((path, list)),
                selected: None,
            };
        } else {
            self.playlist_preview = PlaylistPreview {
                root: self.root.clone(),
                paths: list.tracks.clone(),
                queue: vec![],
                snapshot: Some((path, list)),
                selected: None,
            };
            self.status = "This saved playlist has unavailable songs. Reconnect its source or rescan before playing or editing; no songs were removed.".into();
        }
    }
    fn playlist_detail_card(&mut self, ui: &mut egui::Ui, height: f32) {
        cards::panel(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let name = self
                .playlist_preview
                .snapshot
                .as_ref()
                .map_or("Playlist queue", |(_, p)| p.name.as_str())
                .to_owned();
            let track = self
                .library
                .as_ref()
                .and_then(|lib| {
                    self.playlist_preview
                        .queue
                        .first()
                        .and_then(|i| lib.tracks.get(*i))
                })
                .cloned();
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(92., 92.), egui::Sense::hover());
                cards::artwork(ui, rect, &self.root, track.as_ref(), &name);
                ui.vertical(|ui| {
                    ui.label(RichText::new(&name).size(27.).strong());
                    let duration = self.library.as_ref().and_then(|lib| {
                        cards::duration(
                            self.playlist_preview
                                .queue
                                .iter()
                                .filter_map(|i| lib.tracks.get(*i)),
                        )
                    });
                    ui.weak(format!(
                        "{} tracks{}",
                        self.playlist_preview.queue.len(),
                        duration.map_or_else(String::new, |time| format!(" · {time}"))
                    ));
                    ui.add_space(8.);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(
                                !self.playlist_preview.queue.is_empty(),
                                cards::primary("Play"),
                            )
                            .clicked()
                            && self.activate_playlist_preview()
                        {
                            self.play_playlist_queue();
                        }
                        if ui
                            .add_enabled(
                                self.playlist_preview.snapshot.is_some(),
                                egui::Button::new("+ Queue").min_size(egui::vec2(96., 38.)),
                            )
                            .on_hover_text(
                                "Add any missing songs from this saved playlist to the queue",
                            )
                            .clicked()
                        {
                            self.queue_preview_playlist();
                        }
                        ui.menu_button("•••", |ui| {
                            self.playlist_preview_options(ui);
                        });
                    });
                });
            });
            ui.add_space(10.);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !self.playlist_preview.queue.is_empty(),
                        egui::Button::new("Load to DJ").min_size(egui::vec2(112., 36.)),
                    )
                    .clicked()
                {
                    if !self.activate_playlist_preview() {
                        return;
                    }
                    self.run.load_regular_playlist(name.clone());
                    self.player_sequence.smart = None;
                    self.player_sequence.explicit_regular = true;
                    match self.execute_control(control::Action::PreloadNext) {
                        Ok(()) => {
                            self.status = format!("{name} loaded to DJ · ready to play");
                            self.activate(ui, Some(skin::Module::Dj));
                        }
                        Err(error) => self.status = error,
                    }
                }
                if ui
                    .add_enabled(
                        !self.playlist_preview.queue.is_empty(),
                        egui::Button::new("Save as…").min_size(egui::vec2(108., 36.)),
                    )
                    .clicked()
                {
                    self.edit_playlist_preview(|player| player.save_playlist_copy());
                }
                if ui
                    .add_enabled(
                        self.playlist_preview.queue.len() > 1,
                        egui::Button::new("Order for Mixing").min_size(egui::vec2(138., 36.)),
                    )
                    .clicked()
                {
                    self.edit_playlist_preview(|player| player.curate_playlist());
                }
                if ui
                    .add_enabled(
                        !self.playlist_preview.queue.is_empty(),
                        egui::Button::new("Find Similar").min_size(egui::vec2(116., 36.)),
                    )
                    .clicked()
                {
                    self.find_playlist_suggestions();
                }
                ui.menu_button("More", |ui| {
                    self.playlist_preview_options(ui);
                });
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add(cards::primary("Add music…")).clicked() {
                    self.library_builder.open = !self.library_builder.open;
                }
                if ui.button("Save playlist").clicked() {
                    self.edit_playlist_preview(|p| p.save_playlist());
                }
                ui.weak("Browse, check songs, then add. Suggestions never add automatically.");
            });
            ui.add_space(10.);
            ui.separator();
            if self.playlist_preview.queue.len() != self.playlist_preview.paths.len() {
                ui.colored_label(skin::accent(ui), format!("This playlist contains unavailable songs. Reconnect the source or rescan; all {} saved songs are retained.", self.playlist_preview.paths.len()));
            }
            ui.add(egui::Label::new(&self.status).wrap());
            self.playlist_preview_table(
                ui,
                if self.library_builder.open {
                    130.
                } else {
                    (height - 260.).max(130.)
                },
            );
            if self.library_builder.open {
                ui.separator();
                self.music_browser(ui, 390., true);
            }
            self.playlist_suggestions_ui(ui);
        });
    }
    fn create_preview_playlist(&mut self, name: &str) -> Result<(), String> {
        let snapshot = self.playlist_snapshot.clone();
        let selected = self.queue_selected;
        let active_queue = std::mem::take(&mut self.queue);
        let result = self.create_named_playlist(name);
        if result.is_ok() {
            self.playlist_preview = PlaylistPreview {
                root: self.root.clone(),
                paths: self
                    .library
                    .as_ref()
                    .map(|lib| {
                        self.queue
                            .iter()
                            .filter_map(|i| lib.tracks.get(*i).map(|t| t.path.clone()))
                            .collect()
                    })
                    .unwrap_or_default(),
                queue: self.queue.clone(),
                snapshot: self.playlist_snapshot.clone(),
                selected: None,
            };
        }
        self.queue = active_queue;
        self.playlist_snapshot = snapshot;
        self.queue_selected = selected;
        if result.is_ok() {
            self.library_builder.open = true;
        }
        result
    }
    fn playlist_preview_options(&mut self, ui: &mut egui::Ui) {
        if !self.refresh_playlist_preview() {
            ui.weak("Reconnect this playlist’s music source first.");
            return;
        }
        if ui.button("Save playlist changes").clicked() {
            self.edit_playlist_preview(|player| player.save_playlist());
            ui.close();
        }
        if ui.button("Add current queue to playlist").clicked() {
            self.playlist_preview
                .queue
                .extend(self.queue.iter().copied());
            ui.close();
        }
        if ui
            .add_enabled(
                !self.playlist_preview.queue.is_empty(),
                egui::Button::new("Prepare selected playlist stems…"),
            )
            .clicked()
        {
            let name = self
                .playlist_preview
                .snapshot
                .as_ref()
                .map_or("Playlist", |(_, p)| p.name.as_str())
                .to_owned();
            let paths = self
                .library
                .as_ref()
                .map(|lib| {
                    self.playlist_preview
                        .queue
                        .iter()
                        .filter_map(|i| lib.tracks.get(*i).map(|t| t.path.clone()))
                        .collect()
                })
                .unwrap_or_default();
            self.offer_playlist_stems(name, paths);
            ui.close();
        }
        if let Some((_, list)) = &mut self.playlist_preview.snapshot {
            ui.checkbox(&mut list.companion_available, "Available in Companion");
        }
        for (label, up) in [("Move selected up", true), ("Move selected down", false)] {
            if ui
                .add_enabled(
                    self.playlist_preview.selected.is_some(),
                    egui::Button::new(label),
                )
                .clicked()
                && let Some(i) = self
                    .playlist_preview
                    .selected
                    .filter(|i| *i < self.playlist_preview.queue.len())
            {
                let next = if up {
                    i.saturating_sub(1)
                } else {
                    (i + 1).min(self.playlist_preview.queue.len() - 1)
                };
                self.playlist_preview.queue.swap(i, next);
                self.playlist_preview.selected = Some(next);
                ui.close();
            }
        }
        if ui.button("Remove selected").clicked()
            && let Some(i) = self
                .playlist_preview
                .selected
                .take()
                .filter(|i| *i < self.playlist_preview.queue.len())
        {
            self.playlist_preview.queue.remove(i);
            ui.close();
        }
        if ui.button("Clear playlist").clicked() {
            self.playlist_preview.queue.clear();
            self.playlist_preview.selected = None;
            ui.close();
        }
        self.remember_playlist_preview_paths();
    }
    fn refresh_playlist_preview(&mut self) -> bool {
        if self.playlist_preview.root != self.root {
            return false;
        }
        let Some(lib) = &self.library else {
            self.playlist_preview.queue.clear();
            return false;
        };
        // The usual frame checks only this playlist, not the entire indexed library.
        // A rescan or reorder invalidates an index by its authoritative source path.
        if self.playlist_preview.paths.len() == self.playlist_preview.queue.len()
            && self
                .playlist_preview
                .paths
                .iter()
                .zip(&self.playlist_preview.queue)
                .all(|(path, index)| {
                    lib.tracks
                        .get(*index)
                        .is_some_and(|track| &track.path == path)
                })
        {
            return true;
        }
        let indices: std::collections::HashMap<_, _> = lib
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.path.as_path(), i))
            .collect();
        let resolved: Option<Vec<usize>> = self
            .playlist_preview
            .paths
            .iter()
            .map(|p| indices.get(p.as_path()).copied())
            .collect();
        if let Some(queue) = resolved {
            self.playlist_preview.queue = queue;
            true
        } else {
            self.playlist_preview.queue.clear();
            self.status="Rescan: this playlist has unavailable songs; playback and preparation were not changed.".into();
            false
        }
    }
    fn remember_playlist_preview_paths(&mut self) {
        if let Some(lib) = &self.library {
            self.playlist_preview.paths = self
                .playlist_preview
                .queue
                .iter()
                .filter_map(|i| lib.tracks.get(*i).map(|t| t.path.clone()))
                .collect();
        }
    }
    fn activate_playlist_preview(&mut self) -> bool {
        if !self.refresh_playlist_preview() {
            return false;
        }
        self.queue = self.playlist_preview.queue.clone();
        self.playlist_snapshot = self.playlist_preview.snapshot.clone();
        self.queue_selected = self.playlist_preview.selected;
        self.player_sequence.smart = None;
        self.player_sequence.explicit_regular = true;
        true
    }
    fn edit_playlist_preview(&mut self, edit: impl FnOnce(&mut Self)) {
        if !self.refresh_playlist_preview() {
            return;
        }
        std::mem::swap(&mut self.queue, &mut self.playlist_preview.queue);
        std::mem::swap(
            &mut self.playlist_snapshot,
            &mut self.playlist_preview.snapshot,
        );
        std::mem::swap(
            &mut self.queue_selected,
            &mut self.playlist_preview.selected,
        );
        edit(self);
        std::mem::swap(&mut self.queue, &mut self.playlist_preview.queue);
        std::mem::swap(
            &mut self.playlist_snapshot,
            &mut self.playlist_preview.snapshot,
        );
        std::mem::swap(
            &mut self.queue_selected,
            &mut self.playlist_preview.selected,
        );
        self.remember_playlist_preview_paths();
    }
    fn queue_preview_playlist(&mut self) {
        if !self.refresh_playlist_preview() {
            return;
        }
        let mut present = std::collections::HashMap::<usize, usize>::new();
        for index in &self.queue {
            *present.entry(*index).or_default() += 1;
        }
        let mut wanted = std::collections::HashMap::<usize, usize>::new();
        let mut added = 0;
        for index in &self.playlist_preview.queue {
            let count = wanted.entry(*index).or_default();
            *count += 1;
            if *count > present.get(index).copied().unwrap_or(0) {
                self.queue.push(*index);
                added += 1;
            }
        }
        self.status = format!("Added {added} songs from this playlist to the queue.");
    }
    fn playlist_preview_table(&mut self, ui: &mut egui::Ui, height: f32) {
        cards::track_header(ui, "playlist-preview");
        let rows = self.library.as_ref().map(|lib| {
            crate::track_sort::rows(
                ui.ctx(),
                "playlist-preview",
                self.library_generation,
                lib,
                &self.playlist_preview.queue,
            )
        });
        egui::ScrollArea::vertical()
            .id_salt("playlist-preview-tracks")
            .max_height(height)
            .show_rows(ui, 36., self.playlist_preview.queue.len(), |ui, range| {
                if let Some(lib) = &self.library {
                    for position in range {
                        let row = rows.as_ref().unwrap()[position];
                        if let Some(track) = self
                            .playlist_preview
                            .queue
                            .get(row)
                            .and_then(|i| lib.tracks.get(*i))
                        {
                            let response = cards::track_row(
                                ui,
                                row,
                                track,
                                self.playlist_preview.selected == Some(row),
                            );
                            dragdrop::source(
                                &response,
                                amp_core::music_sources::audio_path(&self.root, &track.path),
                            );
                            if response.clicked() {
                                self.playlist_preview.selected = Some(row);
                            }
                        }
                    }
                }
            });
        let response = ui.interact(
            ui.min_rect(),
            ui.scope_id().with("playlist-preview-drop"),
            egui::Sense::hover(),
        );
        if let Some(path) = dragdrop::target(&response, skin::accent(ui)) {
            let index = path.canonicalize().ok().and_then(|absolute| {
                let root = self.root.canonicalize().ok()?;
                let relative = absolute.strip_prefix(root).ok()?;
                self.library
                    .as_ref()?
                    .tracks
                    .iter()
                    .position(|t| t.path == relative)
            });
            if let Some(index) = index {
                if self.refresh_playlist_preview() {
                    self.playlist_preview.queue.push(index);
                    self.remember_playlist_preview_paths();
                }
            } else {
                self.status =
                    "Add this folder to the indexed Library before adding its songs to a playlist."
                        .into();
            }
        }
    }
    fn play_playlist_queue(&mut self) {
        self.selected = self.queue.first().copied();
        if let Some(index) = self.selected {
            self.queue.remove(0);
            self.player_sequence.smart = None;
            self.player_sequence.explicit_regular = true;
            self.play(0, index);
            self.auto = true;
            if let Some(audio) = &self.audio {
                audio.set_fader(0.);
            }
        }
    }
    fn save_playlist_copy(&mut self) {
        let suggested = self
            .playlist_snapshot
            .as_ref()
            .map_or("Playlist", |(_, list)| list.name.as_str());
        let Some(path) = rfd::FileDialog::new()
            .add_filter("AMP playlist", &["json"])
            .set_file_name(format!("{suggested} copy.json"))
            .save_file()
        else {
            return;
        };
        let tracks: Option<Vec<_>> = self
            .queue
            .iter()
            .map(|index| {
                self.library
                    .as_ref()?
                    .tracks
                    .get(*index)
                    .map(|track| track.path.clone())
            })
            .collect();
        let Some(tracks) = tracks else {
            self.status = "Wait for the library scan before saving these songs.".into();
            return;
        };
        let list = Playlist {
            version: 1,
            name: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            tracks,
            companion_available: self
                .playlist_snapshot
                .as_ref()
                .is_some_and(|(_, list)| list.companion_available),
            sync: None,
        };
        match list
            .save_in_library(&path, &self.root)
            .and_then(|()| Playlist::load(&path, &self.root))
        {
            Ok(saved) => {
                self.playlist_snapshot = Some((path.clone(), saved.clone()));
                if let Some(entry) = self
                    .dj_browser
                    .playlists
                    .iter_mut()
                    .find(|(p, _)| p == &path)
                {
                    entry.1 = saved;
                } else {
                    self.dj_browser.playlists.push((path, saved));
                }
                self.status = "Playlist copy saved.".into();
            }
            Err(error) => self.status = error.to_string(),
        }
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
            ui.heading("Analysis queue");
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
                    egui::Button::new("Start / resume analysis"),
                )
                .clicked()
            {
                self.request_library_analysis();
            }
            if self.preparer.failed() > 0 && ui.button("Retry failed songs").clicked() {
                self.preparer.retry_failed();
            }
            if self.preparer.queue_enabled && ui.button("Stop analysis queue").clicked() {
                if let Err(error) = crate::startup_index::remember(&self.root, None) {
                    self.status = format!("Could not save startup library path: {error}");
                }
                self.preparer.stop_queue();
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
                                if ui.button("Transcribe this recording…").clicked() {
                                    self.transcription.select_library(&self.root, track);
                                    self.library_page = 11;
                                }

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
        self.music_browser(ui, height, false);
        if !self.queue.is_empty() {
            self.organized_queue(ui, 130.);
        }
        self.playlist_suggestions_ui(ui);
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
            if self.library_page != 2 {
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
                                && let Some(i) =
                                    self.queue_selected.filter(|&i| i < self.queue.len())
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
            }
            cards::track_header(ui, "playlist-queue");
            let rows = self.library.as_ref().map(|lib| {
                crate::track_sort::rows(
                    ui.ctx(),
                    "playlist-queue",
                    self.library_generation,
                    lib,
                    &self.queue,
                )
            });
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .id_salt("organized-queue")
                .max_height(height)
                .min_scrolled_height(height.min(100.))
                .show_rows(ui, 36., self.queue.len(), |ui, range| {
                    if self.queue.is_empty() {
                        ui.add_space(16.);
                        ui.strong("Add tracks to this playlist");
                        ui.weak("Select songs in Library or drop a local audio file here.");
                    }
                    if let Some(lib) = &self.library {
                        for position in range {
                            let row = rows.as_ref().unwrap()[position];
                            if let Some(index) = self.queue.get(row).copied()
                                && let Some(track) = lib.tracks.get(index)
                            {
                                let response = cards::track_row(
                                    ui,
                                    row,
                                    track,
                                    self.queue_selected == Some(row),
                                );
                                dragdrop::source(
                                    &response,
                                    amp_core::music_sources::audio_path(&self.root, &track.path),
                                );
                                if response.clicked() {
                                    self.queue_selected = Some(row);
                                }
                                response.on_hover_text(format!(
                                    "{}\n{}",
                                    track.title,
                                    track.path.display()
                                ));
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
    fn card_fixture() -> (tempfile::TempDir, Player) {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..8 {
            std::fs::write(
                dir.path().join(format!("song-{index}.mp3")),
                b"layout fixture",
            )
            .unwrap();
        }
        let mut library = amp_core::scan(dir.path()).unwrap();
        for (index, track) in library.tracks.iter_mut().enumerate() {
            track.title = if index == 0 {
                "A long track title that must stay inside its table cell while preserving every action".into()
            } else {
                format!("Track {}", index + 1)
            };
            track.preparation.extensions.insert(
                "artist".into(),
                serde_json::json!(format!("Artist {}", index + 1)),
            );
            track.preparation.bpm = Some(118. + index as f64);
            track.preparation.harmonic_key = Some(format!("{}A", index + 1));
            track.preparation.analysis = Some(amp_core::preparation::AnalysisInfo {
                engine: "layout-fixture".into(),
                duration: 190. + index as f64,
                tempo_confidence: 0.8,
                key_confidence: 0.8,
                reviewed: false,
            });
        }
        let paths: Vec<_> = library.tracks.iter().map(|t| t.path.clone()).collect();
        let mut player = Player {
            root: dir.path().into(),
            library: Some(library),
            library_page: 2,
            ..Default::default()
        };
        for (name, tracks) in [
            ("Evening mix", paths.clone()),
            ("Favorites", paths[..4].to_vec()),
            ("Weekend", paths[4..].to_vec()),
        ] {
            player.dj_browser.playlists.push((
                dir.path().join(format!("{name}.json")),
                Playlist {
                    version: 1,
                    name: name.into(),
                    tracks,
                    companion_available: false,
                    sync: None,
                },
            ));
        }
        player.select_playlist_card(0);
        player.workspace.enabled = true;
        player.workspace.rendering = true;
        (dir, player)
    }
    #[test]
    fn actual_checkbox_and_queue_button_keep_selection_separate_from_playback() {
        let (_dir, mut player) = card_fixture();
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280., 800.));
        let draw = |player: &mut Player, events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| player.music_browser(ui, 650., false),
            );
            output.textures_delta.clear();
        };
        draw(&mut player, vec![]);
        let checkbox = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("library-first-checkbox")))
            .unwrap();
        for pressed in [true, false] {
            draw(
                &mut player,
                vec![
                    egui::Event::PointerMoved(checkbox.center()),
                    egui::Event::PointerButton {
                        pos: checkbox.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert_eq!(player.checked_library_indices(), vec![0]);
        assert!(player.queue.is_empty());
        // Repaint the newly enabled action before pressing it, as the native event loop does.
        draw(&mut player, vec![]);
        let add = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("library-add-checked-queue")))
            .unwrap();
        for pressed in [true, false] {
            draw(
                &mut player,
                vec![
                    egui::Event::PointerMoved(add.center()),
                    egui::Event::PointerButton {
                        pos: add.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert_eq!(player.queue, vec![0]);
        assert!(player.audio.is_none());
    }
    #[test]
    fn global_search_escapes_folder_scope_and_track_search_refines_it() {
        let (_dir, mut player) = card_fixture();
        player.folder = "No matching folder".into();
        player.library.as_mut().unwrap().tracks[0]
            .preparation
            .extensions
            .insert("artist".into(), serde_json::json!("Scope Sentinel"));
        player.search = "sentinel".into();
        assert_eq!(player.library_browser_results(), vec![0]);
        player.library_builder.tracks = "no such title".into();
        assert!(player.library_browser_results().is_empty());
        player.library_builder.tracks.clear();
        player.search.clear();
        assert!(player.library_browser_results().is_empty());
    }
    #[test]
    fn folder_additions_save_descendants_without_replacing_playback() {
        let (_dir, mut player) = card_fixture();
        player.queue = vec![7];
        player.create_preview_playlist("Folder builder").unwrap();
        player.add_folder_to_playlist();
        player.add_folder_to_playlist();
        assert_eq!(player.playlist_preview.queue.len(), 8);
        assert_eq!(player.queue, vec![7]);
        let (file, _) = player.playlist_preview.snapshot.as_ref().unwrap();
        assert_eq!(Playlist::load(file, &player.root).unwrap().tracks.len(), 8);
    }
    #[test]
    fn checked_songs_have_explicit_queue_and_playlist_destinations() {
        let (_dir, mut player) = card_fixture();
        player.queue = vec![7];
        player.create_preview_playlist("Selected songs").unwrap();
        let path = player.library.as_ref().unwrap().tracks[1].path.clone();
        player.library_builder.checked.insert(path.clone());
        assert!(player.playlist_preview.queue.is_empty());
        assert_eq!(player.queue, vec![7]);
        player.add_checked_to_playlist();
        player.add_checked_to_playlist();
        assert_eq!(player.playlist_preview.queue, vec![1]);
        assert_eq!(player.queue, vec![7]);
        let (file, _) = player.playlist_preview.snapshot.as_ref().unwrap();
        assert_eq!(
            Playlist::load(file, &player.root).unwrap().tracks,
            vec![path]
        );
        player.add_checked_to_queue();
        assert_eq!(player.queue, vec![7, 1]);
        player.library.as_mut().unwrap().tracks.reverse();
        assert_eq!(player.checked_library_indices(), vec![6]);
    }
    #[test]
    fn suggestions_are_review_only_until_an_explicit_add() {
        let (_dir, mut player) = card_fixture();
        for track in &mut player.library.as_mut().unwrap().tracks {
            track
                .preparation
                .extensions
                .insert("artist".into(), serde_json::json!("Shared artist"));
        }
        player.create_preview_playlist("Review first").unwrap();
        player.add_indices_to_playlist(&[0]);
        let before = player.playlist_preview.paths.clone();
        player.queue = vec![7];
        player.find_playlist_suggestions();
        assert!(!player.library_builder.suggestions.is_empty());
        assert_eq!(player.playlist_preview.paths, before);
        assert_eq!(player.queue, vec![7]);
        let (file, _) = player.playlist_preview.snapshot.as_ref().unwrap();
        assert_eq!(Playlist::load(file, &player.root).unwrap().tracks, before);
    }
    #[test]
    fn queue_find_similar_is_also_review_only() {
        let (_dir, mut player) = card_fixture();
        for track in &mut player.library.as_mut().unwrap().tracks {
            track
                .preparation
                .extensions
                .insert("artist".into(), serde_json::json!("Shared artist"));
        }
        player.queue = vec![0];
        let before = player.playlist_preview.paths.clone();
        player.expand_smart_list();
        assert_eq!(player.queue, vec![0]);
        assert_eq!(player.playlist_preview.paths, before);
        assert!(!player.library_builder.suggestions.is_empty());
        assert!(player.library_builder.suggest_to_queue);
    }
    #[test]
    fn library_search_matches_partial_metadata_not_only_title() {
        let (_dir, mut player) = card_fixture();
        let track = &mut player.library.as_mut().unwrap().tracks[0];
        track
            .preparation
            .extensions
            .insert("artist".into(), serde_json::json!("Earth Wind Fire"));
        track
            .preparation
            .extensions
            .insert("album".into(), serde_json::json!("Greatest Hits"));
        assert!(matches_track(track, "wind great"));
        assert!(!matches_track(track, "other artist"));
    }
    #[test]
    fn blank_playlist_does_not_capture_or_replace_the_playback_queue() {
        let (_dir, mut player) = card_fixture();
        player.queue = vec![7, 6];
        player.queue_selected = Some(1);
        player.create_preview_playlist("Blank builder").unwrap();
        assert!(
            player.playlist_preview.queue.is_empty(),
            "a new playlist must start blank"
        );
        assert_eq!(player.queue, vec![7, 6]);
        assert_eq!(player.queue_selected, Some(1));
    }
    #[test]
    fn playlist_edits_survive_the_next_refresh_without_touching_playback() {
        let (_dir, mut player) = card_fixture();
        player.queue = vec![7, 6];
        player.edit_playlist_preview(|p| {
            p.queue.remove(0);
        });
        let edited = player.playlist_preview.queue.clone();
        assert!(player.refresh_playlist_preview());
        assert_eq!(
            player.playlist_preview.queue, edited,
            "refresh must not undo an edit"
        );
        assert_eq!(player.queue, vec![7, 6]);
    }
    #[test]
    fn playlist_cards_browse_without_starting_audio_and_keep_duplicate_membership() {
        let (_dir, mut player) = card_fixture();
        let first = player.playlist_preview.queue[0];
        player.queue = vec![7, 6];
        player.queue_selected = Some(1);
        player.playlist_snapshot = Some(player.dj_browser.playlists[2].clone());
        let smart = amp_core::show::SmartPlaylist {
            id: "live-smart".into(),
            name: "Live Smart".into(),
            companion_available: false,
            source_library: None,
            source_root_hint: None,
            source_unavailable: false,
            genres: vec![],
            filters: vec![],
            style: amp_core::show::Style::default(),
            folders: vec![],
            playlist_sources: vec![],
            restrict_folders: false,
            allowed: vec![],
            restrict_allowed: false,
            excluded: vec![],
            preferred: vec![],
            recognition_song: None,
            target_energy: None,
            recent_repeat: 5,
            fallback: Default::default(),
            energy_progression: None,
            use_manual_cues: true,
        };
        player.player_sequence.smart = Some(smart);
        player.auto = true;
        let before_queue = player.queue.clone();
        let before_snapshot = serde_json::to_value(&player.playlist_snapshot).unwrap();
        let before_sequence = serde_json::to_value(&player.player_sequence).unwrap();
        let owner = player.playback_owner;
        player.select_playlist_card(1);
        player.select_playlist_card(0);
        assert_eq!(player.queue, before_queue);
        assert_eq!(player.queue_selected, Some(1));
        assert_eq!(
            serde_json::to_value(&player.playlist_snapshot).unwrap(),
            before_snapshot
        );
        assert_eq!(
            serde_json::to_value(&player.player_sequence).unwrap(),
            before_sequence
        );
        assert!(!player.player_sequence.explicit_regular);
        assert!(player.auto);
        assert_eq!(player.playback_owner, owner);
        player.dj_browser.playlists[0]
            .1
            .tracks
            .push(player.library.as_ref().unwrap().tracks[first].path.clone());
        player.select_playlist_card(0);
        assert_eq!(
            player
                .playlist_preview
                .queue
                .iter()
                .filter(|i| **i == first)
                .count(),
            2
        );
        player.queue_preview_playlist();
        assert_eq!(player.queue.iter().filter(|i| **i == first).count(), 2);
        assert_eq!(
            serde_json::to_value(&player.playlist_snapshot).unwrap(),
            before_snapshot
        );
        player.edit_playlist_preview(|p| {
            p.queue.remove(0);
        });
        let active = player.queue.clone();
        player.select_playlist_card(1);
        assert_eq!(player.queue, active);
        player.activate_playlist_preview();
        assert_eq!(player.queue, player.playlist_preview.queue);
        assert!(player.player_sequence.smart.is_none());
        assert!(player.player_sequence.explicit_regular);
        assert!(player.audio.is_none());
    }
    #[test]
    fn playlist_preview_resolves_paths_after_same_root_rescan_and_blocks_missing_sources() {
        let (_dir, mut player) = card_fixture();
        let paths = player.playlist_preview.paths.clone();
        player.queue = vec![1, 2];
        player.library.as_mut().unwrap().tracks.reverse();
        assert!(player.refresh_playlist_preview());
        let resolved: Vec<_> = player
            .playlist_preview
            .queue
            .iter()
            .map(|i| player.library.as_ref().unwrap().tracks[*i].path.clone())
            .collect();
        assert_eq!(resolved, paths);
        player.library.as_mut().unwrap().tracks.pop();
        let active = player.queue.clone();
        assert!(!player.activate_playlist_preview());
        assert_eq!(player.queue, active);
        assert_eq!(player.playlist_preview.paths, paths);
        player.queue_preview_playlist();
        assert_eq!(player.queue, active);
    }
    #[test]
    fn playlist_card_layouts_handle_narrow_and_wide_windows() {
        for size in [
            egui::vec2(320., 640.),
            egui::vec2(900., 760.),
            egui::vec2(1568., 930.),
        ] {
            let (_dir, mut player) = card_fixture();
            let ctx = egui::Context::default();
            theme::apply(&ctx, theme::Theme::ModernDark);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| player.library_ui(ui),
            );
            output.textures_delta.clear();
            assert!(player.queue.is_empty());
            assert_eq!(player.playlist_preview.queue.len(), 8);
            assert!(player.audio.is_none());
        }
    }
    #[test]
    fn export_playlist_card_preview_when_requested() {
        let Some(directory) = std::env::var_os("AMP_LIBRARY_CARD_PREVIEW") else {
            return;
        };
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let (_dir, mut player) = card_fixture();
        let dj = std::env::var("AMP_LIBRARY_CARD_PREVIEW_DJ").as_deref() == Ok("1");
        let player_view = std::env::var("AMP_LIBRARY_CARD_PREVIEW_PLAYER").as_deref() == Ok("1");
        if dj || player_view {
            player = Player::default();
            player.workspace.enabled = true;
            player.workspace.rendering = true;
        }
        let ctx = egui::Context::default();
        skin::configure(&ctx);
        theme::apply(&ctx, theme::Theme::ModernDark);
        let full_workspace = std::env::var("AMP_CARD_WORKSPACE").as_deref() == Ok("1");
        if full_workspace {
            player.workspace.pane = if player_view {
                0
            } else if dj {
                2
            } else {
                1
            };
            player.workspace.rendering = false;
        }
        let dimension = |key: &str, fallback: f32| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| v.is_finite() && (320. ..=4096.).contains(v))
                .unwrap_or(fallback)
        };
        let size = egui::vec2(
            dimension("AMP_CARD_PREVIEW_WIDTH", 1280.),
            dimension("AMP_CARD_PREVIEW_HEIGHT", 900.),
        );
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| {
                if full_workspace {
                    player.workspace_ui(ui);
                } else if player_view {
                    player.workspace_player(ui);
                } else if dj {
                    player.deck_workspace_ui(ui);
                } else {
                    player.library_ui(ui);
                }
            },
        );
        let mut textures = Vec::new();
        for (id, deltas) in output.textures_delta.set.drain() {
            let mut canvas: Option<image::RgbaImage> = None;
            for delta in deltas {
                let egui::ImageData::Color(image) = delta.image;
                let pixels: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect();
                let tile =
                    image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, pixels)
                        .unwrap();
                if let Some([x, y]) = delta.pos {
                    let target = canvas
                        .as_mut()
                        .expect("partial atlas update follows its allocation");
                    image::GenericImage::copy_from(target, &tile, x as u32, y as u32).unwrap();
                } else {
                    canvas = Some(tile);
                }
            }
            if let Some(canvas) = canvas {
                let name = format!("texture-{id:?}.png");
                canvas.save(directory.join(&name)).unwrap();
                textures.push(serde_json::json!({"id":format!("{id:?}"),"path":name}));
            }
        }
        let meshes:Vec<_>=ctx.tessellate(output.shapes,output.pixels_per_point).into_iter().filter_map(|shape|{
            let egui::epaint::Primitive::Mesh(mesh)=shape.primitive else{return None;};
            Some(serde_json::json!({"texture":format!("{:?}",mesh.texture_id),"clip":[shape.clip_rect.min.x,shape.clip_rect.min.y,shape.clip_rect.max.x,shape.clip_rect.max.y],"vertices":mesh.vertices.iter().map(|v|{let color=v.color.to_array();[v.pos.x,v.pos.y,v.uv.x,v.uv.y,color[0]as f32,color[1]as f32,color[2]as f32,color[3]as f32]}).collect::<Vec<_>>(),"indices":mesh.indices}))
        }).collect();
        std::fs::write(
            directory.join("scene.json"),
            serde_json::to_vec(
                &serde_json::json!({"size":[size.x,size.y],"textures":textures,"meshes":meshes}),
            )
            .unwrap(),
        )
        .unwrap();
    }
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
