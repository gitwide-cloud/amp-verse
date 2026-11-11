//! AMP Library edits a draft; only explicit Save publishes rules for future performances.
use amp_core::show::{EnergyPlan, Fallback, Settings, Style};
use eframe::egui;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlaylistAction {
    Edit,
    Duplicate,
    Resync,
    Delete,
}
pub fn playlist_menu(response: &egui::Response) -> Option<PlaylistAction> {
    let mut action = None;
    response.context_menu(|ui| {
        for (label, value) in [
            ("Edit in Library", PlaylistAction::Edit),
            ("Duplicate", PlaylistAction::Duplicate),
            ("Resync sources / rules", PlaylistAction::Resync),
        ] {
            if ui.button(label).clicked() {
                action = Some(value);
                ui.close();
            }
        }
        ui.separator();
        ui.menu_button("Delete...", |ui| {
            ui.weak("Remove this Smart Playlist, keeping music files.");
            if ui.button("Delete this Smart Playlist").clicked() {
                action = Some(PlaylistAction::Delete);
                ui.close();
            }
        });
    });
    action
}
#[derive(Default)]
pub struct Editor {
    pub open: bool,
    selected: usize,
    status: String,
    search: String,
    assignments_open: bool,
    draft: Option<Settings>,
    baseline: Option<Settings>,
    root: std::path::PathBuf,
    pub audition_blocked: bool,
    pub preview_active: bool,
    pub stop_preview: bool,
    pub refresh_sources: bool,
    pub connect_import: Option<(std::path::PathBuf, amp_core::show::SmartPlaylist)>,
    pub mix_request: Option<(amp_core::show::SmartPlaylist, Vec<usize>)>,
    pub preview_status: String,
    pub preview: Option<(std::path::PathBuf, f64)>,
    preview_order: Vec<String>,
    preview_paths: Vec<std::path::PathBuf>,
    source_labels: std::collections::HashMap<String, String>,
    existing_sources: Vec<amp_core::Playlist>,
    cache_bytes: Vec<u8>,
    conflict_review: Option<(Settings, Settings)>,
    review_scroll: bool,
}
impl Editor {
    pub fn open_playlist(&mut self, id: &str, root: &std::path::Path, saved: &Settings) {
        if self.draft.is_none() || self.root != root {
            self.conflict_review = None;
            self.review_scroll = false;
            self.draft = Some(saved.clone());
            self.baseline = Some(saved.clone());
            self.root = root.into();
            self.source_labels.clear();
            self.preview_order.clear();
            self.cache_bytes.clear();
            if saved.sync.is_none() && !root.join("ampdj/smart-playlists.json").exists() {
                for playlist in &mut self.draft.as_mut().unwrap().playlists {
                    set_source_mode(playlist, SourceMode::Folders);
                }
            }
            if let Some(path) = draft_path(root)
                && let Ok(bytes) = std::fs::read(path)
                && let Ok(cache) = serde_json::from_slice::<DraftCache>(&bytes)
                && cache.draft.validate().is_ok()
                && cache.baseline.validate().is_ok()
            {
                self.draft = Some(cache.draft);
                self.baseline = Some(cache.baseline);
                self.source_labels = cache.labels;
                self.status =
                    "Restored your local editing draft. Save Playlist publishes changes.".into();
            }
        }
        self.draft.as_mut().unwrap().resolve_sources(root);
        self.selected = self
            .draft
            .as_ref()
            .unwrap()
            .playlists
            .iter()
            .position(|p| p.id == id)
            .unwrap_or(0);
        self.open = true;
    }
    pub fn playlist_unsaved(&self, id: &str) -> bool {
        let draft = self
            .draft
            .as_ref()
            .and_then(|s| s.playlists.iter().find(|p| p.id == id));
        let base = self
            .baseline
            .as_ref()
            .and_then(|s| s.playlists.iter().find(|p| p.id == id));
        draft.is_some() && serde_json::to_value(draft).ok() != serde_json::to_value(base).ok()
    }
    pub fn has_unsaved(&self) -> bool {
        self.draft.as_ref().is_some_and(|draft| {
            self.baseline.as_ref().is_none_or(|base| {
                serde_json::to_value(draft).ok() != serde_json::to_value(base).ok()
            })
        })
    }
    pub fn refresh_saved(&mut self, saved: &Settings) {
        if self.has_unsaved() {
            return;
        }
        let selected = self.selected_id().map(str::to_owned);
        self.draft = Some(saved.clone());
        self.baseline = Some(saved.clone());
        self.selected = selected
            .and_then(|id| saved.playlists.iter().position(|p| p.id == id))
            .unwrap_or(0);
    }
    pub fn selected_id(&self) -> Option<&str> {
        self.draft
            .as_ref()?
            .playlists
            .get(self.selected)
            .map(|p| p.id.as_str())
    }
    fn cache_draft(&mut self) {
        let (Some(draft), Some(baseline), Some(path)) =
            (&self.draft, &self.baseline, draft_path(&self.root))
        else {
            return;
        };
        let cache = DraftCache {
            draft: draft.clone(),
            baseline: baseline.clone(),
            labels: self.source_labels.clone(),
        };
        if let Ok(bytes) = serde_json::to_vec(&cache)
            && bytes != self.cache_bytes
        {
            match save_draft_bytes(&path, &bytes) {
                Ok(()) => self.cache_bytes = bytes,
                Err(error) => {
                    self.status = format!("Draft remains open; could not keep local copy: {error}")
                }
            }
        }
    }
    pub fn new_playlist(&mut self, root: &std::path::Path, saved: &Settings) {
        self.open_playlist("", root, saved);
        let settings = self.draft.as_mut().unwrap();
        let mut playlist = Settings::default().playlists.pop().unwrap();
        playlist.id = format!(
            "custom-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        playlist.name = "New Smart Playlist".into();
        set_source_mode(&mut playlist, SourceMode::Folders);
        settings.playlists.push(playlist);
        self.selected = settings.playlists.len() - 1;
    }
    pub fn stage_import(
        &mut self,
        mut playlist: amp_core::show::SmartPlaylist,
        root: &std::path::Path,
        saved: &Settings,
    ) {
        self.open_playlist("", root, saved);
        playlist.source_unavailable = false;
        let settings = self.draft.as_mut().unwrap();
        if settings.playlists.len() >= 1000 {
            self.status = "This library already has 1,000 Smart Playlists.".into();
            return;
        }
        if settings.playlists.iter().any(|p| p.name == playlist.name) {
            playlist.name.push_str(" (imported)");
        }
        settings.playlists.push(playlist);
        self.selected = settings.playlists.len() - 1;
        self.status =
            "Connected to the imported music library. Save Playlist to use this import in DJ."
                .into();
        self.cache_draft();
    }
    pub fn playlists(&self, saved: &Settings) -> Vec<amp_core::show::SmartPlaylist> {
        self.draft.as_ref().unwrap_or(saved).playlists.clone()
    }
    pub fn set_existing_sources(&mut self, lists: Vec<amp_core::Playlist>) {
        self.existing_sources = lists;
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        root: &std::path::Path,
        saved: &mut Settings,
        library: Option<&amp_core::Library>,
    ) {
        self.open = true;
        if self.draft.is_none() || self.root != root {
            self.open_playlist("", root, saved);
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Export Smart Playlist…").on_hover_text("Export this draft's settings and source folders; no audio files").clicked()
                && let Some(p)=self.draft.as_ref().and_then(|s|s.playlists.get(self.selected)) {
                match amp_core::show::SmartPlaylistExport::encode(p,root) {
                    Ok(bytes)=> {
                        let name:String=p.name.chars().map(|c|if c.is_alphanumeric() || c==' ' || c=='-' {c}else{'_'}).take(80).collect();
                        if let Some(path)=rfd::FileDialog::new().add_filter("AMP Smart Playlist", &["ampsmart"]).set_file_name(format!("{name}.ampsmart")).save_file() {
                            self.status=match save_draft_bytes(&path,&bytes) {Ok(())=>format!("Exported {}. Contains settings only; music stays in its library.",path.display()),Err(e)=>format!("Could not export: {e}")};
                        }
                    }
                    Err(e)=>self.status=e,
                }
            }
            if ui.button("Import Smart Playlist…").clicked()
                && let Some(path)=rfd::FileDialog::new().add_filter("AMP Smart Playlist", &["ampsmart"]).pick_file() {
                let result=(|| {
                    use std::io::Read;
                    let mut bytes=Vec::new();
                    std::fs::File::open(&path).map_err(|e|e.to_string())?.take(amp_core::show::SmartPlaylistExport::MAX_BYTES+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
                    amp_core::show::SmartPlaylistExport::decode(&bytes,root)
                })();
                match result {
                    Ok(mut p)=> {
                        let settings=self.draft.as_mut().unwrap();
                        if settings.playlists.len()>=1000 {self.status="This library already has the maximum number of Smart Playlists.".into();return;}
                        if settings.playlists.iter().any(|old|old.name==p.name){p.name.push_str(" (imported)");}
                        self.status=if p.source_unavailable {"Settings imported. Original library is not selected; reconnect it in Library > Sources, or choose a replacement Music Source below. Save Playlist to keep this import.".into()}else{"Imported with matching library sources. Save Playlist to use it in DJ.".into()};
                        settings.playlists.push(p);self.selected=settings.playlists.len()-1;self.refresh_sources=true;
                    }
                    Err(e)=>self.status=e,
                }
            }
        });
        let dirty = self.has_unsaved();
        let settings = self.draft.as_mut().unwrap();
        let height = ui.available_height().max(180.);
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt("inline-smart-editor")
            .max_height(height)
            .auto_shrink([false, false]);
        if std::mem::take(&mut self.review_scroll) {
            scroll = scroll.vertical_scroll_offset(0.);
        }
        let area = scroll.show(ui, |ui| {
            let mut approve_review=false;
            let mut cancel_review=false;
            if let Some((proposed,shared))=&self.conflict_review {
                ui.colored_label(crate::ORANGE,"Shared rules changed. Review before saving your draft.");
                ui.add(egui::Label::new(&self.status).wrap());
                egui::CollapsingHeader::new("Compare saved and proposed sources").default_open(true).show(ui,|ui|{
                    for mine in &proposed.playlists { if let Some(theirs)=shared.playlists.iter().find(|p|p.id==mine.id) && serde_json::to_value(mine).ok()!=serde_json::to_value(theirs).ok(){ui.strong(&mine.name);ui.label(format!("Saved: {} · folders: {}",source_summary(theirs),theirs.folders.iter().map(|p|p.display().to_string()).collect::<Vec<_>>().join(", ")));ui.label(format!("Proposed: {} · folders: {}{}",source_summary(mine),mine.folders.iter().map(|p|p.display().to_string()).collect::<Vec<_>>().join(", "),if mine.restrict_allowed {format!(" · {} allowed songs",mine.allowed.len())}else{String::new()}));if let (Ok(local),Ok(saved))=(serde_json::to_value(mine),serde_json::to_value(theirs)){for (key,value) in local.as_object().unwrap(){if !["folders","restrict_folders","allowed","restrict_allowed","id"].contains(&key.as_str())&&*value!=saved[key]{ui.label(format!("{key}: saved {} → proposed {value}",saved[key]));}}}}}
                });
                approve_review=ui.button("Save reviewed draft changes").clicked();
                cancel_review=ui.button("Cancel review · keep both versions").clicked();
                ui.separator();
                ui.disable();
            }
            let mut remove = false;
            let mut duplicate = false;
            let mut save = false;
            let mut reload = false;
            if let Some(p) = settings.playlists.get_mut(self.selected) {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut p.name).font(egui::TextStyle::Heading).desired_width((ui.available_width()-55.).max(100.)));
                    ui.menu_button("...", |ui| {
                        ui.weak("Rename using the name field");
                        duplicate = ui.button("Duplicate").clicked();
                        ui.menu_button("Delete...", |ui| { if ui.button("Remove from draft").clicked() { remove = true; ui.close(); } });
                        ui.separator();
                        reload = ui.button("Discard drafts / reload saved").clicked();
                        if ui.button("Copy control ID").clicked() { ui.ctx().copy_text(p.id.clone()); ui.close(); }
                    });
                });
                if dirty { ui.colored_label(crate::ORANGE,"Unsaved changes · DJ still uses the saved version"); }
                ui.horizontal_wrapped(|ui|{let button=ui.button("Save Playlist");save|=button.clicked();#[cfg(test)]ui.ctx().data_mut(|data|data.insert_temp(egui::Id::unique("smart-top-save"),button.rect));if !self.status.is_empty(){ui.add(egui::Label::new(&self.status).wrap());}});
                ui.add_space(16.);
                ui.strong("Music sources");
                if p.source_unavailable {
                    ui.colored_label(crate::ORANGE,"Source library not connected · settings retained, playback paused for this playlist");
                    if let Some(hint)=&p.source_root_hint {
                        ui.label(format!("Original music library: {}",hint.display()));
                        if ui.add_enabled(!self.audition_blocked,egui::Button::new("Connect original library and keep import")).on_hover_text("Open the original music library; available while playback is stopped").clicked(){
                            match hint.is_dir().then(||amp_core::coordination::library_id(hint)).transpose() {
                                Ok(Some(id)) if p.source_library.as_ref()==Some(&id)=>self.connect_import=Some((hint.clone(),p.clone())),
                                _=>self.status="Original library is unavailable or its identity differs. Settings retained; choose its mounted folder through Sources, or reconnect to the current library below.".into(),
                            }
                        }
                    }
                    if ui.button("Use current library with these folder rules").on_hover_text("Explicitly reconnect these relative folders and song selections to the current library").clicked(){
                        p.source_library=amp_core::coordination::library_id(root).ok();p.source_root_hint=Some(root.to_path_buf());p.source_unavailable=false;self.refresh_sources=true;
                        self.status="Source reconnected to current library. Review the song count, then Save Playlist.".into();
                    }
                }
                if !p.source_unavailable && let Some(lib)=library {normalize_folder_spelling(p,&lib.tree);}
                let count = library.map(|lib| lib.tracks.iter().filter(|track|amp_core::show::eligible(p,track)).count());
                let source = source_summary(p);
                ui.add(egui::Label::new(format!("{} — {}", source, count.map_or("waiting for library scan".into(), |count|format!("{count} songs")))).wrap());
                ui.add_enabled_ui(!p.source_unavailable, |ui| {
                if let Some(lib)=library {
                    ui.label(format!("Indexed library: {} · {} songs",root.display(),lib.tracks.len()));
                    if let Some(relative)=library_folder_picker(ui,&lib.tree,p.folders.first()){
                        add_indexed_folder(p,relative.clone());
                        self.refresh_sources=true;
                        self.source_labels.insert(p.id.clone(),format!("Folder: {}",root.join(&relative).display()));
                        self.status="Folder added with all subfolders. Existing sources kept. Save Playlist to apply.".into();
                    }
                }
                ui.menu_button("Add Music Source", |ui| {
                    if ui.button("Select Folder...").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().set_directory(root).pick_folder() {
                            let mut selected=p.clone();
                            match select_folder(root, &folder, &mut selected) {
                                Ok(()) => { add_indexed_folder(p,selected.folders.remove(0)); self.refresh_sources=true; self.source_labels.insert(p.id.clone(),format!("Folder: {}",folder.display())); self.status = "Folder added with subfolders; existing sources kept. Save Playlist to apply.".into(); }
                                Err(error) => self.status = error,
                            }
                        }
                        ui.close();
                    }
                    if ui.add_enabled(library.is_some(),egui::Button::new("Import Playlist (M3U / M3U8 / AMP)...")).clicked() {
                        if let Some(file) = rfd::FileDialog::new().add_filter("Playlists", &["m3u","m3u8","json"]).pick_file() && let Some(lib) = library {
                            let mut selected=p.clone();
                            let result = if file.extension().is_some_and(|ext|ext.eq_ignore_ascii_case("json")) {
                                amp_core::Playlist::load(&file,root).map_err(|e|e.to_string()).and_then(|list|import_source_playlist(&mut selected,&list,lib).map(|count|(count,0)))
                            } else { import_m3u(&file,root,lib,&mut selected) };
                            match result {
                                Ok((count,missing)) => {add_playlist_source(p,file.file_stem().unwrap_or_default().to_string_lossy().into_owned(),selected.allowed);self.source_labels.insert(p.id.clone(),format!("Imported: {}",file.file_name().unwrap_or_default().to_string_lossy()));self.status=format!("Imported {count} songs; {missing} missing or outside the indexed library. Save Playlist to apply.");}
                                Err(error) => self.status = error,
                            }
                        }
                        ui.close();
                    }
                    ui.menu_button("Choose Existing Playlist", |ui| {
                        if self.existing_sources.is_empty() { ui.weak("Create or import a playlist in Library > Playlists first."); }
                        for list in &self.existing_sources {
                            if ui.button(format!("{} · {} songs",list.name,list.tracks.len())).clicked() && let Some(lib)=library {
                                let mut selected=p.clone();
                                match import_source_playlist(&mut selected,list,lib) {
                                    Ok(count)=>{add_playlist_source(p,list.name.clone(),selected.allowed);self.source_labels.insert(p.id.clone(),format!("Playlist: {}",list.name));self.status=format!("Selected {count} songs. Save Playlist to apply.");}
                                    Err(error)=>self.status=error,
                                }
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                    if ui.button("Use entire library").clicked() && confirm_source_change(p) { set_source_mode(p,SourceMode::All); self.source_labels.remove(&p.id); ui.close(); }
                });
                });
                let mut remove_folder=None;
                for (i,folder) in p.folders.iter().enumerate(){ui.horizontal_wrapped(|ui|{ui.label(format!("Folder: {} (includes subfolders)",if folder.as_os_str().is_empty(){root.display().to_string()}else{folder.display().to_string()}));if ui.small_button("Remove").clicked(){remove_folder=Some(i);}});}
                if let Some(i)=remove_folder {p.folders.remove(i);p.restrict_folders=true;}
                let mut remove_playlist=None;
                for (i,source) in p.playlist_sources.iter().enumerate(){ui.horizontal_wrapped(|ui|{ui.label(format!("Playlist: {} · {} songs (saved selection)",source.name,source.songs.len()));if ui.small_button("Remove").clicked(){remove_playlist=Some(i);}});}
                if let Some(i)=remove_playlist {p.playlist_sources.remove(i);}
                ui.weak("Sources are combined; overlapping songs appear once. Save Playlist to apply.");
                ui.add_space(18.);
                ui.strong("Mixing style");
                ui.horizontal(|ui| {
                    let width = ((ui.available_width()-12.)/3.).max(45.);
                    for (style,label,tip) in [(Style::Hype,"Hype","Prioritize builds and drops"),(Style::Steady,"Steady","Keep energy consistent"),(Style::Chill,"Chill","Gentle transitions and natural structure")] {
                        if ui.add_sized([width,36.],egui::Button::selectable(p.style==style,label)).on_hover_text(tip).clicked() { p.style=style; }
                    }
                });
                ui.add_space(16.);
                ui.strong("Target energy");
                ui.horizontal(|ui| {
                    if ui.selectable_label(p.target_energy.is_none(),"Auto").clicked() { p.target_energy=None; }
                    if ui.selectable_label(p.target_energy.is_some(),"Manual").clicked() { p.target_energy.get_or_insert(5.); }
                });
                if let Some(energy)=&mut p.target_energy { ui.add(egui::Slider::new(energy,1. ..=10.).text("Energy")); }
                ui.add_space(16.);
                egui::CollapsingHeader::new("Advanced Options").id_salt("smart-advanced").show(ui, |ui| {
                    egui::ComboBox::from_id_salt("repeat-fallback")
                        .selected_text(match p.fallback {
                            Fallback::Stop => "Stop when all eligible songs are recent",
                            Fallback::RelaxRecent => {
                                "Continue with recently played music if needed"
                            }
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut p.fallback,
                                Fallback::Stop,
                                "Stop when all eligible songs are recent",
                            );
                            ui.selectable_value(
                                &mut p.fallback,
                                Fallback::RelaxRecent,
                                "Continue with recently played music if needed",
                            );
                        });
                    let mut progressing = p.energy_progression.is_some();
                    if ui
                        .checkbox(&mut progressing, "Saved energy progression")
                        .changed()
                    {
                        p.energy_progression = progressing.then_some(EnergyPlan {
                            target: 3.,
                            transitions: 3,
                            hold: true,
                        });
                    }
                    if let Some(plan) = &mut p.energy_progression {
                        ui.horizontal_wrapped(|ui| {
                            ui.add(egui::Slider::new(&mut plan.target, 1. ..=10.).text("Target"));
                            ui.add(
                                egui::DragValue::new(&mut plan.transitions)
                                    .range(1..=100)
                                    .suffix(" transitions"),
                            );
                            ui.checkbox(&mut plan.hold, "Hold target");
                        });
                    }
                    ui.checkbox(
                        &mut p.use_manual_cues,
                        "Prefer approved manual build/drop cues for Hype",
                    );
                    let old_folders=p.folders.clone();
                    source_ui(ui, root, library, p, &mut self.status);
                    if p.folders!=old_folders{self.refresh_sources=true;}
                    if ui.button("Song exceptions / recognition...").clicked() {self.assignments_open=true;}
                    if self.assignments_open {
                        egui::Modal::new(egui::Id::unique("smart-song-assignments")).show(ui.ctx(), |ui| {
                            ui.set_width((ui.ctx().viewport_rect().width()-50.).clamp(380.,700.));
                            ui.horizontal(|ui| {ui.heading("Song exceptions & recognition");if ui.button("Close").clicked(){self.assignments_open=false;}});
                            ui.weak("Your chosen source already includes its songs. Only change exceptions here.");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text(
                                "Find songs to allow, exclude, prefer or assign for recognition",
                            )
                            .desired_width(ui.available_width()),
                    );
                    if let Some(lib) = library {
                        let query = self.search.to_lowercase();
                        let tracks: Vec<_> = lib
                            .tracks
                            .iter()
                            .filter(|t| t.title.to_lowercase().contains(&query))
                            .collect();
                        if ui.button("Clear recognition assignment").clicked() {
                            p.recognition_song = None;
                        }
                        egui::ScrollArea::vertical()
                            .id_salt("song-rules")
                            .max_height(220.)
                            .show_rows(ui, 25., tracks.len(), |ui, rows| {
                                for i in rows {
                                    let t = tracks[i];
                                    let id = amp_core::show::song_id(t);
                                    ui.push_id(&id, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.add_sized(
                                                [(ui.available_width() - 380.).clamp(80., 260.), 20.],
                                                egui::Label::new(&t.title).truncate(),
                                            )
                                            .on_hover_text(t.path.display().to_string());
                                            for (label, list, enabled) in [
                                                ("Allow", &mut p.allowed, p.restrict_allowed),
                                                ("Exclude", &mut p.excluded, true),
                                                ("Prefer", &mut p.preferred, true),
                                            ] {
                                                let mut checked =
                                                    if label == "Allow" && !p.restrict_allowed {
                                                        true
                                                    } else {
                                                        list.contains(&id)
                                                    };
                                                if ui
                                                    .add_enabled(
                                                        enabled,
                                                        egui::Checkbox::new(&mut checked, label),
                                                    )
                                                    .changed()
                                                {
                                                    if checked {
                                                        list.push(id.clone());
                                                    } else {
                                                        list.retain(|old| old != &id);
                                                    }
                                                }
                                            }
                                            let assigned = p.recognition_song.as_deref()
                                                == Some(id.as_str())
                                                || p.recognition_song.as_deref()
                                                    == Some(t.title.as_str());
                                            if ui.radio(assigned, "Demo").clicked() {
                                                p.recognition_song = Some(id.clone());
                                            }
                                        })
                                    });
                                }
                            });
                    }

                        });
                    }
                    egui::CollapsingHeader::new("Advanced selectors / stable IDs").show(ui, |ui| {
                        ui.weak(format!("Playlist ID: {}", p.id));
                        for (label, list) in [
                            ("Allowed song IDs", &mut p.allowed),
                            ("Excluded song IDs", &mut p.excluded),
                            ("Preferred song IDs", &mut p.preferred),
                        ] {
                            egui::CollapsingHeader::new(label).show(ui, |ui| {
                                let mut text = list.join("\n");
                                if ui
                                    .add(egui::TextEdit::multiline(&mut text).desired_rows(3))
                                    .changed()
                                {
                                    *list = text
                                        .lines()
                                        .map(str::trim)
                                        .filter(|s| !s.is_empty())
                                        .map(str::to_owned)
                                        .collect();
                                }
                            });
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Assigned recognition song ID / exact title");
                            let mut song = p.recognition_song.clone().unwrap_or_default();
                            if ui.text_edit_singleline(&mut song).changed() {
                                p.recognition_song = (!song.trim().is_empty()).then_some(song);
                            }
                        });
                    });

                });
                ui.add_space(18.);
                ui.horizontal_wrapped(|ui| {
                    save |= ui.add_sized([180.,38.],egui::Button::new(egui::RichText::new("Save Playlist").color(egui::Color32::BLACK)).fill(crate::skin::accent(ui)).wrap()).clicked();
                    if ui.add_enabled(self.preview_active || (library.is_some() && !self.audition_blocked),egui::Button::new(if self.preview_active {"Stop Preview"} else {"Preview Mix"}).min_size(egui::vec2(140.,38.))).on_hover_text("Short audio mix using this draft; available when live playback is stopped").clicked() {
                        if self.preview_active { self.stop_preview=true; }
                        else if let Some(lib)=library {
                            let order=preview_mix(p,lib);
                            self.preview_order=order.iter().map(|&index|lib.tracks[index].title.clone()).collect();
                            self.preview_paths=order.iter().map(|&index|lib.tracks[index].path.clone()).collect();
                            self.mix_request=Some((p.clone(),order));
                        }
                    }
                });
                if !self.preview_order.is_empty() {
                    ui.separator(); ui.strong("Mix preview · suggested order");
                    for (index,title) in self.preview_order.iter().enumerate() { let row=ui.add(egui::Label::new(format!("{}. {title}",index+1)).truncate().sense(egui::Sense::click_and_drag()));if let Some(path)=self.preview_paths.get(index){crate::dragdrop::source(&row,root.join(path));} }
                    if ui.add_enabled(!self.audition_blocked,egui::Button::new("Audition first track")).on_hover_text("Available when player, DJ, sampler and Auto Mix are stopped").clicked() && let Some(lib)=library && let Some(index)=preview_mix(p,lib).first().copied() {
                        self.preview=Some((lib.tracks[index].path.clone(),lib.tracks[index].preparation.cue_in));
                    }
                }
            } else { ui.weak("Create a Smart Playlist to choose its music and mixing style."); }
            if duplicate && let Some(original)=settings.playlists.get(self.selected).cloned() {
                let mut copy=original;copy.id=format!("custom-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());copy.name.push_str(" (copy)");settings.playlists.push(copy);self.selected=settings.playlists.len()-1;
            }
            if remove && self.selected<settings.playlists.len() { settings.playlists.remove(self.selected); self.selected=self.selected.min(settings.playlists.len().saturating_sub(1)); }
            if cancel_review {self.conflict_review=None;}
            if approve_review && let Some((mut proposed,_))=self.conflict_review.take(){self.status=save_rules(root,&mut proposed,saved);if self.status.starts_with("Saved."){*settings=proposed;self.baseline=Some(saved.clone());}}
            if save {
                self.conflict_review=None;
                match Settings::load(root){
                    Ok(current)=>{
                        let baseline=self.baseline.as_ref().unwrap_or(saved);
                        match merge_rules(baseline,settings,&current,false){
                            Ok(mut proposed)=>{self.status=save_rules(root,&mut proposed,saved);if self.status.starts_with("Saved."){*settings=proposed;self.baseline=Some(saved.clone());}},
                            Err(error)=>{self.status=error;if let Ok(proposed)=merge_rules(baseline,settings,&current,true){self.conflict_review=Some((proposed,current));self.review_scroll=true;}}
                        }
                    }
                    Err(error)=>self.status=error,
                }
            }
            if reload { match Settings::load(root) { Ok(fresh)=>{*saved=fresh.clone();*settings=fresh.clone();self.baseline=Some(fresh);self.selected=0;self.preview_order.clear();},Err(error)=>self.status=error } }
            if self.audition_blocked { ui.weak("Stop player / DJ playback before auditioning a mix. Saving and editing remain available."); }
            ui.add_space(8.);ui.add(egui::Label::new(&self.status).wrap());
            if !self.preview_status.is_empty() { ui.add(egui::Label::new(&self.preview_status).wrap()); }
        });
        ui.ctx().data_mut(|data| {
            data.insert_temp(egui::Id::unique("smart-editor-viewport"), area.inner_rect)
        });
        self.cache_draft();
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
struct DraftCache {
    draft: Settings,
    baseline: Settings,
    #[serde(default)]
    labels: std::collections::HashMap<String, String>,
}
fn draft_path(root: &std::path::Path) -> Option<std::path::PathBuf> {
    if cfg!(test) {
        return None;
    }
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hash);
    crate::theme::settings_path().and_then(|path| {
        path.parent()
            .map(|parent| parent.join(format!("smart-draft-{:x}.json", hash.finish())))
    })
}
fn save_draft_bytes(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("Missing draft folder"))?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}
fn source_summary(p: &amp_core::show::SmartPlaylist) -> String {
    if !p.playlist_sources.is_empty() || p.folders.len() > 1 {
        return format!(
            "{} folders + {} playlist sources",
            p.folders.len(),
            p.playlist_sources.len()
        );
    }
    if p.restrict_folders || !p.folders.is_empty() {
        if p.folders.is_empty() {
            "No music source selected".into()
        } else {
            format!(
                "Folder: {}{}",
                p.folders
                    .first()
                    .map(|path| if path.as_os_str().is_empty() {
                        "Entire music root".into()
                    } else {
                        path.display().to_string()
                    })
                    .unwrap_or_default(),
                if p.folders.len() > 1 {
                    format!(" + {} folders", p.folders.len() - 1)
                } else {
                    String::new()
                }
            )
        }
    } else if p.restrict_allowed || !p.allowed.is_empty() {
        "Playlist / selected songs".into()
    } else {
        "Entire library".into()
    }
}
fn confirm_source_change(p: &amp_core::show::SmartPlaylist) -> bool {
    p.allowed.is_empty() || rfd::MessageDialog::new().set_title("Change music source?").set_description("Replace the previous source song selection? Exclusions, preferred songs, recognition assignments and mixing settings will be kept.").set_buttons(rfd::MessageButtons::YesNo).show()==rfd::MessageDialogResult::Yes
}
fn prepare_additive_sources(p: &mut amp_core::show::SmartPlaylist) {
    if !p.allowed.is_empty() {
        p.playlist_sources.push(amp_core::show::PlaylistSource {
            name: "Selected songs".into(),
            songs: std::mem::take(&mut p.allowed),
        });
    }
    p.source_library = None;
    p.source_root_hint = None;
    p.source_unavailable = false;
    p.restrict_folders = true;
    p.restrict_allowed = false;
}
fn add_indexed_folder(p: &mut amp_core::show::SmartPlaylist, relative: std::path::PathBuf) {
    prepare_additive_sources(p);
    if !p
        .folders
        .iter()
        .any(|f| amp_core::folder_contains(f, &relative) && amp_core::folder_contains(&relative, f))
    {
        p.folders.push(relative);
    }
}
fn add_playlist_source(
    p: &mut amp_core::show::SmartPlaylist,
    name: String,
    mut songs: Vec<String>,
) {
    prepare_additive_sources(p);
    songs.sort();
    songs.dedup();
    if !p
        .playlist_sources
        .iter()
        .any(|source| source.name == name && source.songs == songs)
    {
        p.playlist_sources
            .push(amp_core::show::PlaylistSource { name, songs });
    }
}
fn select_indexed_folder(p: &mut amp_core::show::SmartPlaylist, relative: std::path::PathBuf) {
    set_source_mode(p, SourceMode::Folders);
    p.folders = vec![relative];
    p.restrict_allowed = false;
}
fn normalize_folder_spelling(p: &mut amp_core::show::SmartPlaylist, tree: &amp_core::Folder) {
    fn find(
        tree: &amp_core::Folder,
        path: &std::path::Path,
        wanted: &std::path::Path,
    ) -> Option<std::path::PathBuf> {
        if amp_core::folder_contains(path, wanted) && amp_core::folder_contains(wanted, path) {
            return Some(path.to_path_buf());
        }
        tree.folders.iter().find_map(|child| {
            let next = path.join(&child.name);
            amp_core::folder_contains(wanted, &next)
                .then(|| find(child, &next, wanted))
                .flatten()
        })
    }
    for folder in &mut p.folders {
        if let Some(actual) = find(tree, std::path::Path::new(""), folder) {
            *folder = actual;
        }
    }
    if !p.folders.is_empty() && p.allowed.is_empty() {
        p.restrict_allowed = false;
    }
}
fn folder_total(folder: &amp_core::Folder) -> usize {
    folder.tracks.len() + folder.folders.iter().map(folder_total).sum::<usize>()
}
fn source_folder_tree(
    ui: &mut egui::Ui,
    folder: &amp_core::Folder,
    path: &std::path::Path,
    selected: Option<&std::path::PathBuf>,
    choice: &mut Option<std::path::PathBuf>,
) {
    let label = format!("{} · {} songs", folder.name, folder_total(folder));
    let same = selected
        .is_some_and(|p| amp_core::folder_contains(p, path) && amp_core::folder_contains(path, p));
    let header = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        ui.make_persistent_id(("smart-folder-node", path)),
        path.as_os_str().is_empty(),
    );
    header
        .show_header(ui, |ui| {
            if ui.selectable_label(same, label).clicked() {
                *choice = Some(path.to_path_buf());
            }
        })
        .body(|ui| {
            for child in &folder.folders {
                source_folder_tree(ui, child, &path.join(&child.name), selected, choice);
            }
        });
}
fn library_folder_picker(
    ui: &mut egui::Ui,
    tree: &amp_core::Folder,
    selected: Option<&std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    let mut choice = None;
    let button = ui.button("Browse Library Folders…");
    egui::Popup::menu(&button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(340.);
            ui.weak("Select a folder to include every song in its subfolders.");
            egui::ScrollArea::both()
                .id_salt("smart-library-tree")
                .max_height(300.)
                .show(ui, |ui| {
                    source_folder_tree(ui, tree, std::path::Path::new(""), selected, &mut choice)
                });
            if choice.is_some() {
                ui.close();
            }
        });
    choice
}
fn select_folder(
    root: &std::path::Path,
    folder: &std::path::Path,
    p: &mut amp_core::show::SmartPlaylist,
) -> Result<(), String> {
    let base = root.canonicalize().map_err(|error| error.to_string())?;
    let selected = folder.canonicalize().map_err(|error| error.to_string())?;
    if !selected.is_dir() {
        return Err("Choose a music folder".into());
    }
    let relative=amp_core::relative_folder(&selected,&base).ok_or_else(||format!("That folder is outside the indexed library at {}. Your indexed songs are still available. Use Browse Library Folders to select them, or add this different folder through Library > Sources. Existing rules were kept.",root.display()))?;
    select_indexed_folder(p, relative);
    Ok(())
}
fn import_m3u(
    file: &std::path::Path,
    root: &std::path::Path,
    library: &amp_core::Library,
    p: &mut amp_core::show::SmartPlaylist,
) -> Result<(usize, usize), String> {
    if std::fs::metadata(file)
        .map_err(|error| error.to_string())?
        .len()
        > 4 * 1024 * 1024
    {
        return Err("Playlist is larger than 4 MB".into());
    }
    let bytes = std::fs::read(file).map_err(|error| error.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("Playlist is larger than 4 MB".into());
    }
    let text = String::from_utf8(bytes).map_err(|_| "Use a UTF-8 M3U8 playlist".to_owned())?;
    let base = root.canonicalize().map_err(|error| error.to_string())?;
    let parent = file.parent().ok_or("Playlist folder missing")?;
    let mut ids = Vec::new();
    let mut missing = 0;
    for entry in text
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let portable = std::path::PathBuf::from(entry.replace('\\', "/"));
        let candidate = if portable.is_absolute() {
            portable
        } else {
            parent.join(portable)
        };
        let found = candidate
            .canonicalize()
            .ok()
            .and_then(|absolute| amp_core::relative_folder(&absolute, &base))
            .and_then(|relative| {
                library.tracks.iter().find(|track| {
                    amp_core::folder_contains(&track.path, &relative)
                        && amp_core::folder_contains(&relative, &track.path)
                })
            });
        if let Some(track) = found {
            ids.push(amp_core::show::song_id(track));
        } else {
            missing += 1;
        }
    }
    if ids.is_empty() {
        return Err(format!(
            "No indexed songs found; {missing} missing or outside this library. Your current source was kept."
        ));
    }
    set_source_mode(p, SourceMode::Songs);
    p.allowed = ids;
    Ok((p.allowed.len(), missing))
}
fn preview_mix(p: &amp_core::show::SmartPlaylist, library: &amp_core::Library) -> Vec<usize> {
    let count = library
        .tracks
        .iter()
        .filter(|track| amp_core::show::eligible(p, track))
        .count()
        .min(8);
    let mut order = Vec::new();
    let mut recent = Vec::new();
    let mut profile = p.clone();
    profile.recent_repeat = count.max(1);
    profile.fallback = Fallback::Stop;
    for _ in 0..count {
        let current = order.last().copied();
        if let Some(index) = amp_core::show::choose(
            &profile,
            &library.tracks,
            current,
            &recent,
            p.target_energy,
            0,
        ) {
            recent.push(amp_core::show::song_id(&library.tracks[index]));
            order.push(index);
        } else {
            break;
        }
    }
    order
}
/// Three-way edits preserve unrelated shared changes; source selectors form one atomic group.
fn merge_rules(
    base: &Settings,
    draft: &Settings,
    current: &Settings,
    reviewed: bool,
) -> Result<Settings, String> {
    use serde_json::Value;
    let mut merged = current.clone();
    let fields = [
        "folders",
        "restrict_folders",
        "allowed",
        "restrict_allowed",
        "source_library",
        "source_root_hint",
        "playlist_sources",
    ];
    let mut conflicts = Vec::new();
    for old in &base.playlists {
        if !draft.playlists.iter().any(|p| p.id == old.id)
            && let Some(index) = merged.playlists.iter().position(|p| p.id == old.id)
        {
            if serde_json::to_value(&merged.playlists[index]).ok() != serde_json::to_value(old).ok()
                && !reviewed
            {
                conflicts.push(format!(
                    "{}: deleted locally and edited on shared library",
                    old.name
                ));
            } else {
                merged.playlists.remove(index);
            }
        }
    }
    for mine in &draft.playlists {
        let prior = base.playlists.iter().find(|p| p.id == mine.id);
        let index = merged.playlists.iter().position(|p| p.id == mine.id);
        let Some(prior) = prior else {
            if let Some(index) = index {
                if serde_json::to_value(&merged.playlists[index]).ok()
                    != serde_json::to_value(mine).ok()
                {
                    if reviewed {
                        merged.playlists[index] = mine.clone();
                    } else {
                        conflicts.push(format!("{}: same ID added by another device", mine.name));
                    }
                }
            } else {
                merged.playlists.push(mine.clone());
            }
            continue;
        };
        let old = serde_json::to_value(prior).map_err(|e| e.to_string())?;
        let local = serde_json::to_value(mine).map_err(|e| e.to_string())?;
        let Some(index) = index else {
            if old != local {
                if reviewed {
                    merged.playlists.push(mine.clone());
                } else {
                    conflicts.push(format!("{}: removed on shared library", mine.name));
                }
            }
            continue;
        };
        let mut shared =
            serde_json::to_value(&merged.playlists[index]).map_err(|e| e.to_string())?;
        let group = |value: &Value| {
            fields
                .iter()
                .map(|key| value[*key].clone())
                .collect::<Vec<_>>()
        };
        if group(&local) != group(&old) {
            if group(&shared) != group(&old) && group(&shared) != group(&local) && !reviewed {
                conflicts.push(format!(
                    "{}: music source changed in both versions",
                    mine.name
                ));
            } else {
                for key in fields {
                    shared[key] = local[key].clone();
                }
            }
        }
        for (key, value) in local.as_object().ok_or("Invalid playlist")? {
            if fields.contains(&key.as_str()) || key == "id" {
                continue;
            }
            if *value != old[key] {
                if shared[key] != old[key] && shared[key] != *value && !reviewed {
                    conflicts.push(format!("{}: {key} changed in both versions", mine.name));
                } else {
                    shared[key] = value.clone();
                }
            }
        }
        merged.playlists[index] = serde_json::from_value(shared).map_err(|e| e.to_string())?;
    }
    if !conflicts.is_empty() {
        return Err(format!(
            "Conflict: {}. Your draft and shared rules were retained; review the proposed changes above.",
            conflicts.join("; ")
        ));
    }
    merged.validate()?;
    Ok(merged)
}
fn save_rules(root: &std::path::Path, settings: &mut Settings, saved: &mut Settings) -> String {
    if root.as_os_str().is_empty() {
        return "Choose a music root first".into();
    }
    let current = root.join("ampdj/smart-playlists.json");
    if current.is_file() {
        let backup = root.join("ampdj/backups");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        if let Err(error) = std::fs::create_dir_all(&backup).and_then(|()| {
            std::fs::copy(&current, backup.join(format!("show-{stamp}.json"))).map(|_| ())
        }) {
            return format!("Could not back up existing rules: {error}");
        }
    }
    match settings.save(root) {
        Ok(()) => {
            if let Ok(fresh) = Settings::load(root) {
                *settings = fresh;
            }
            *saved = settings.clone();
            if !cfg!(test)
                && let Err(error) = amp_core::storage::export_rules(root)
            {
                return format!("Shared rules saved; Documents copy could not be written: {error}");
            }
            "Saved. Active performances keep their loaded rules.".into()
        }
        Err(error) => error,
    }
}
#[derive(Clone, Copy, PartialEq)]
enum SourceMode {
    All,
    Folders,
    Songs,
}
fn set_source_mode(p: &mut amp_core::show::SmartPlaylist, mode: SourceMode) {
    p.source_library = None;
    p.source_root_hint = None;
    p.source_unavailable = false;
    p.folders.clear();
    p.playlist_sources.clear();
    p.allowed.clear();
    p.restrict_folders = mode == SourceMode::Folders;
    p.restrict_allowed = mode != SourceMode::All;
}
fn import_source_playlist(
    p: &mut amp_core::show::SmartPlaylist,
    list: &amp_core::Playlist,
    library: &amp_core::Library,
) -> Result<usize, String> {
    let ids: Vec<_> = list
        .tracks
        .iter()
        .filter_map(|path| library.tracks.iter().find(|track| &track.path == path))
        .map(amp_core::show::song_id)
        .collect();
    if ids.is_empty() {
        return Err("No songs from that playlist are in the loaded library. Add its music folder in AMP Library first.".into());
    }
    set_source_mode(p, SourceMode::Songs);
    p.allowed = ids;
    Ok(p.allowed.len())
}
fn source_ui(
    ui: &mut egui::Ui,
    root: &std::path::Path,
    library: Option<&amp_core::Library>,
    p: &mut amp_core::show::SmartPlaylist,
    status: &mut String,
) {
    ui.separator();
    ui.strong("Music sources for this mix");
    ui.label(format!(
        "Music root: {}",
        if root.as_os_str().is_empty() {
            "Choose a music folder in AMP Library first".into()
        } else {
            root.display().to_string()
        }
    ));
    let old = if p.restrict_folders {
        SourceMode::Folders
    } else if p.restrict_allowed || !p.allowed.is_empty() {
        SourceMode::Songs
    } else if !p.folders.is_empty() {
        SourceMode::Folders
    } else {
        SourceMode::All
    };
    let mut mode = old;
    ui.horizontal_wrapped(|ui| {
        ui.radio_value(&mut mode, SourceMode::All, "Entire library");
        ui.radio_value(&mut mode, SourceMode::Folders, "Selected folders");
        ui.radio_value(&mut mode, SourceMode::Songs, "Selected songs / playlist");
    });
    if mode != old {
        set_source_mode(p, mode);
    }
    if mode == SourceMode::Folders {
        ui.horizontal_wrapped(|ui|{
            if ui.add_enabled(!root.as_os_str().is_empty(),egui::Button::new("Browse source folder...")).clicked()
                && let Some(folder)=rfd::FileDialog::new().set_directory(root).pick_folder() {
                let relative=(|| {
                    let base=root.canonicalize().map_err(|e|e.to_string())?;
                    let selected=folder.canonicalize().map_err(|e|e.to_string())?;
                    amp_core::relative_folder(&selected,&base).ok_or_else(||"That folder is outside the current music root. Use Browse Library Folders to choose an indexed folder, or change the music root in Library > Sources.".to_owned())
                })();
                match relative {Ok(path)=>{if !p.folders.contains(&path){p.folders.push(path);}*status="Source folder selected, including its subfolders. Save to apply.".into();},Err(e)=>*status=e}
            }
            if ui.button("Clear folders").clicked(){p.folders.clear();}
        });
        let mut folders = std::collections::BTreeSet::new();
        folders.insert(std::path::PathBuf::new());
        if let Some(lib) = library {
            for t in &lib.tracks {
                let mut parent = t.path.parent();
                while let Some(folder) = parent {
                    if !folder.as_os_str().is_empty() {
                        folders.insert(folder.to_path_buf());
                    }
                    parent = folder.parent();
                }
            }
        }
        folders.extend(p.folders.iter().cloned());
        egui::ScrollArea::vertical()
            .id_salt("smart-source-folders")
            .max_height(110.)
            .show(ui, |ui| {
                for folder in folders {
                    let mut selected = p.folders.contains(&folder);
                    let label = if folder.as_os_str().is_empty() {
                        "Entire music root (including subfolders)".into()
                    } else {
                        folder.display().to_string()
                    };
                    if ui.checkbox(&mut selected, label).changed() {
                        if selected {
                            p.folders.push(folder);
                        } else {
                            p.folders.retain(|old| old != &folder);
                        }
                    }
                }
            });
        // Retain the older schema's empty-allow safety too, so older readers cannot
        // turn an explicitly empty folder selection into the entire library.
        p.restrict_allowed = p.folders.is_empty();
        ui.weak("Folder sources include subfolders. No folders selected means no songs.");
        if library.is_none() {
            ui.weak("You can choose folders now. Scan the music root in Library > Sources to load their songs before building the mix.");
        }
    }
    if mode == SourceMode::Songs {
        ui.horizontal_wrapped(|ui|{
            if ui.add_enabled(library.is_some(),egui::Button::new("Import source playlist...")).clicked()
                && let Some(file)=rfd::FileDialog::new().set_directory(root).add_filter("AMP Playlist", &["json"]).pick_file() {
                *status=match amp_core::Playlist::load(&file,root).map_err(|e|e.to_string()).and_then(|list|import_source_playlist(p,&list,library.unwrap())) {
                    Ok(count)=>format!("Selected {count} library songs from the playlist. Membership is copied into this draft; Save to apply."),Err(e)=>e,
                };
            }
            if ui.button("Clear song selection").clicked(){p.allowed.clear();}
        });
        ui.weak("Use Allow below to choose songs. An empty selection mixes nothing. Imported playlist membership is a saved snapshot.");
    }
    if let Some(lib) = library {
        let count = lib
            .tracks
            .iter()
            .filter(|t| amp_core::show::eligible(p, t))
            .count();
        ui.strong(format!(
            "{count} of {} songs qualify for this mix",
            lib.tracks.len()
        ));
    }
    ui.separator();
}
#[cfg(test)]
mod tests {
    use super::*;
    fn test_settings() -> Settings {
        let mut settings = Settings::default();
        let mut second = settings.playlists[0].clone();
        second.id = "second".into();
        second.name = "Second".into();
        settings.playlists.push(second);
        settings
    }
    #[test]
    fn choosing_music_root_includes_all_nested_songs_and_keeps_exclusions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Dance/Set/Deep")).unwrap();
        for f in ["one.mp3", "Dance/two.mp3", "Dance/Set/Deep/three.mp3"] {
            std::fs::write(dir.path().join(f), b"music").unwrap();
        }
        let lib = amp_core::scan(dir.path()).unwrap();
        assert_eq!(folder_total(&lib.tree), 3);
        let mut p = test_settings().playlists.remove(0);
        p.allowed = vec!["stale-id".into()];
        p.restrict_allowed = true;
        select_folder(dir.path(), dir.path(), &mut p).unwrap();
        assert!(p.allowed.is_empty());
        assert_eq!(
            lib.tracks
                .iter()
                .filter(|t| amp_core::show::eligible(&p, t))
                .count(),
            3
        );
        p.restrict_allowed = true; // legacy folder draft safety flag
        assert_eq!(
            lib.tracks
                .iter()
                .filter(|t| amp_core::show::eligible(&p, t))
                .count(),
            3
        );
        p.excluded.push(amp_core::show::song_id(
            lib.tracks
                .iter()
                .find(|t| t.path == std::path::Path::new("one.mp3"))
                .unwrap(),
        ));
        assert_eq!(
            lib.tracks
                .iter()
                .filter(|t| amp_core::show::eligible(&p, t))
                .count(),
            2
        );
        select_folder(dir.path(), &dir.path().join("Dance"), &mut p).unwrap();
        assert_eq!(
            lib.tracks
                .iter()
                .filter(|t| amp_core::show::eligible(&p, t))
                .count(),
            2
        );
        let before = serde_json::to_value(&p).unwrap();
        let other = tempfile::tempdir().unwrap();
        assert!(select_folder(dir.path(), other.path(), &mut p).is_err());
        assert_eq!(serde_json::to_value(&p).unwrap(), before);
    }
    #[test]
    #[cfg(windows)]
    fn windows_folder_case_does_not_turn_a_valid_source_into_zero_songs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Music/Deep")).unwrap();
        std::fs::write(dir.path().join("Music/Deep/song.mp3"), b"music").unwrap();
        let lib = amp_core::scan(dir.path()).unwrap();
        let mut p = test_settings().playlists.remove(0);
        select_folder(dir.path(), &dir.path().join("MUSIC"), &mut p).unwrap();
        normalize_folder_spelling(&mut p, &lib.tree);
        assert_eq!(p.folders, vec![std::path::PathBuf::from("Music")]);
        assert_eq!(
            lib.tracks
                .iter()
                .filter(|t| amp_core::show::eligible(&p, t))
                .count(),
            1
        );
        assert!(!amp_core::folder_contains(
            std::path::Path::new("MusicOld/a.mp3"),
            std::path::Path::new("Music")
        ));
    }
    #[test]
    #[ignore = "reads user's real NAS library; requires AMP_SMART_SOURCE_ROOT"]
    fn actual_music_root_retains_thousands_of_eligible_songs() {
        let root = std::path::PathBuf::from(std::env::var_os("AMP_SMART_SOURCE_ROOT").unwrap());
        let lib = amp_core::scan(&root).unwrap();
        assert!(lib.tracks.len() > 4000, "{} indexed", lib.tracks.len());
        let mut p = test_settings().playlists.remove(0);
        p.excluded.clear();
        select_folder(&root, &root, &mut p).unwrap();
        let count = lib
            .tracks
            .iter()
            .filter(|t| amp_core::show::eligible(&p, t))
            .count();
        assert_eq!(count, lib.tracks.len());
        assert_eq!(folder_total(&lib.tree), count);
        println!("Music root selection includes {count} songs recursively");
    }
    #[test]
    fn top_save_button_publishes_the_draft_even_with_a_lower_save_button() {
        let root = tempfile::tempdir().unwrap();
        test_settings().save(root.path()).unwrap();
        let mut saved = Settings::load(root.path()).unwrap();
        let mut editor = Editor::default();
        editor.open_playlist("demo", root.path(), &saved);
        editor.draft.as_mut().unwrap().playlists[0].name = "Saved by top button".into();
        let context = egui::Context::default();
        let mut draw = |position: egui::Pos2, pressed: Option<bool>| {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900., 700.),
                )),
                ..Default::default()
            };
            raw.events.push(egui::Event::PointerMoved(position));
            if let Some(pressed) = pressed {
                raw.events.push(egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                });
            }
            let mut output =
                context.run_ui(raw, |ui| editor.show(ui, root.path(), &mut saved, None));
            output.textures_delta.clear();
        };
        draw(egui::pos2(0., 0.), None);
        let target = context
            .data(|data| data.get_temp::<egui::Rect>(egui::Id::unique("smart-top-save")))
            .unwrap()
            .center();
        draw(target, None);
        draw(target, Some(true));
        draw(target, Some(false));
        assert_eq!(
            Settings::load(root.path()).unwrap().playlists[0].name,
            "Saved by top button"
        );
        assert!(!editor.has_unsaved());
    }
    #[test]
    fn stale_draft_merges_unrelated_updates_but_requires_source_review() {
        let root = tempfile::tempdir().unwrap();
        let base = test_settings();
        let mut shared = base.clone();
        shared.playlists[0].restrict_allowed = true;
        shared.playlists[0].use_manual_cues = false;
        assert!(save_rules(root.path(), &mut shared, &mut base.clone()).starts_with("Saved."));
        let current = Settings::load(root.path()).unwrap();
        let mut draft = base.clone();
        set_source_mode(&mut draft.playlists[0], SourceMode::Folders);
        draft.playlists[0].folders.push("Demo".into());
        draft.playlists[0].restrict_allowed = false;
        let before = std::fs::read(root.path().join("ampdj/smart-playlists.json")).unwrap();
        assert!(
            merge_rules(&base, &draft, &current, false)
                .err()
                .unwrap()
                .contains("music source")
        );
        assert_eq!(
            std::fs::read(root.path().join("ampdj/smart-playlists.json")).unwrap(),
            before
        );
        let mut reviewed = merge_rules(&base, &draft, &current, true).unwrap();
        assert!(
            !reviewed.playlists[0].use_manual_cues,
            "Independent shared edit must remain"
        );
        assert_eq!(
            reviewed.sync.as_ref().unwrap().revision,
            current.sync.as_ref().unwrap().revision
        );
        let mut saved = current.clone();
        assert!(save_rules(root.path(), &mut reviewed, &mut saved).starts_with("Saved."));
        let song = amp_core::Track {
            path: "Demo/song.mp3".into(),
            title: "Song".into(),
            preparation: Default::default(),
        };
        assert!(amp_core::show::eligible(&saved.playlists[0], &song));
        let mut outside = song;
        outside.path = "Outside/song.mp3".into();
        assert!(!amp_core::show::eligible(&saved.playlists[0], &outside));
        assert_eq!(saved.playlists[0].id, base.playlists[0].id);
    }
    #[test]
    fn reviewed_rules_cannot_overwrite_a_newer_shared_revision() {
        let root = tempfile::tempdir().unwrap();
        let mut shared = test_settings();
        let mut saved = shared.clone();
        assert!(save_rules(root.path(), &mut shared, &mut saved).starts_with("Saved."));
        let baseline = shared.clone();
        let mut draft = baseline.clone();
        draft.playlists[0].name = "Local edit".into();
        let mut proposed = merge_rules(&baseline, &draft, &shared, true).unwrap();
        shared.playlists[1].name = "Other device edit".into();
        assert!(save_rules(root.path(), &mut shared, &mut saved).starts_with("Saved."));
        assert!(!save_rules(root.path(), &mut proposed, &mut saved).starts_with("Saved."));
        assert_eq!(
            Settings::load(root.path()).unwrap().playlists[1].name,
            "Other device edit"
        );
    }
    #[test]
    fn simple_folder_and_m3u_sources_include_music_without_song_checkboxes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("Demo/Subfolder")).unwrap();
        std::fs::write(root.join("Demo/a.mp3"), b"a").unwrap();
        std::fs::write(root.join("Demo/Subfolder/b.mp3"), b"b").unwrap();
        std::fs::write(root.join("outside.mp3"), b"c").unwrap();
        let library = amp_core::scan(root).unwrap();
        let mut profile = test_settings().playlists.remove(0);
        let original_id = profile.id.clone();
        profile.preferred = vec!["keep-preference".into()];
        profile.recognition_song = Some("keep-assignment".into());
        select_folder(root, &root.join("Demo"), &mut profile).unwrap();
        assert_eq!(
            library
                .tracks
                .iter()
                .filter(|track| amp_core::show::eligible(&profile, track))
                .count(),
            2
        );
        assert!(profile.allowed.is_empty());
        assert!(!profile.restrict_allowed);
        let playlist = root.join("Demo/mix.m3u8");
        std::fs::write(
            &playlist,
            "\u{feff}#EXTM3U\nSubfolder/b.mp3\nmissing.mp3\na.mp3\n",
        )
        .unwrap();
        let (count, missing) = import_m3u(&playlist, root, &library, &mut profile).unwrap();
        assert_eq!((count, missing), (2, 1));
        let ordered: Vec<_> = profile
            .allowed
            .iter()
            .map(|id| {
                library
                    .tracks
                    .iter()
                    .find(|track| amp_core::show::song_id(track) == *id)
                    .unwrap()
                    .path
                    .clone()
            })
            .collect();
        assert_eq!(
            ordered,
            vec![
                std::path::PathBuf::from("Demo/Subfolder/b.mp3"),
                std::path::PathBuf::from("Demo/a.mp3")
            ]
        );
        assert_eq!(profile.id, original_id);
        assert_eq!(profile.preferred, vec!["keep-preference"]);
        assert_eq!(profile.recognition_song.as_deref(), Some("keep-assignment"));
        let before = serde_json::to_value(&profile).unwrap();
        let preview = preview_mix(&profile, &library);
        assert!(!preview.is_empty());
        assert_eq!(serde_json::to_value(&profile).unwrap(), before);
        std::fs::write(&playlist, "missing.mp3").unwrap();
        assert!(import_m3u(&playlist, root, &library, &mut profile).is_err());
        assert_eq!(serde_json::to_value(&profile).unwrap(), before);
    }
    #[test]
    fn local_draft_persistence_and_new_playlist_keep_ids_and_saved_rules() {
        let dir = tempfile::tempdir().unwrap();
        let saved = test_settings();
        let mut editor = Editor::default();
        editor.new_playlist(dir.path(), &saved);
        let new = editor.draft.as_ref().unwrap().playlists.last().unwrap();
        assert!(new.restrict_folders && new.folders.is_empty());
        assert!(new.restrict_allowed);
        assert_eq!(saved.playlists.len(), 2);
        let mut draft = saved.clone();
        draft.playlists[0].name = "Local name".into();
        draft.playlists[0].style = Style::Chill;
        draft.playlists[0].target_energy = Some(3.);
        let cache = DraftCache {
            draft: draft.clone(),
            baseline: saved.clone(),
            labels: Default::default(),
        };
        let path = dir.path().join("local/draft.json");
        save_draft_bytes(&path, &serde_json::to_vec(&cache).unwrap()).unwrap();
        save_draft_bytes(&path, &serde_json::to_vec(&cache).unwrap()).unwrap();
        let restored: DraftCache = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(restored.draft.playlists[0].id, saved.playlists[0].id);
        assert_eq!(restored.draft.playlists[0].name, "Local name");
        assert_eq!(restored.draft.playlists[0].style, Style::Chill);
        assert_eq!(restored.draft.playlists[0].target_energy, Some(3.));
        assert!(!dir.path().join("ampdj/smart-playlists.json").exists());
    }
    #[test]
    fn empty_source_selection_never_expands_to_entire_library() {
        let track = amp_core::Track {
            path: "Dance/Set/song.mp3".into(),
            title: "Song".into(),
            preparation: Default::default(),
        };
        let mut p = test_settings().playlists.remove(0);
        set_source_mode(&mut p, SourceMode::Folders);
        assert!(!amp_core::show::eligible(&p, &track));
        p.folders.push("Dance".into());
        p.restrict_allowed = false;
        assert!(amp_core::show::eligible(&p, &track));
        p.folders = vec!["Other".into()];
        assert!(!amp_core::show::eligible(&p, &track));
        set_source_mode(&mut p, SourceMode::Songs);
        assert!(!amp_core::show::eligible(&p, &track));
        p.allowed.push(amp_core::show::song_id(&track));
        assert!(amp_core::show::eligible(&p, &track));
    }
    #[test]
    fn imported_playlist_limits_mix_and_persists_sources() {
        let directory = tempfile::tempdir().unwrap();
        let track = |path: &str| amp_core::Track {
            path: path.into(),
            title: path.into(),
            preparation: Default::default(),
        };
        let lib = amp_core::Library {
            tracks: vec![track("Dance/a.mp3"), track("Other/b.mp3")],
            tree: amp_core::Folder {
                name: "Music".into(),
                folders: vec![],
                tracks: vec![0, 1],
            },
        };
        let list = amp_core::Playlist {
            sync: None,
            version: 1,
            name: "Source".into(),
            tracks: vec!["Dance/a.mp3".into()],
        };
        let mut settings = test_settings();
        assert_eq!(
            import_source_playlist(&mut settings.playlists[0], &list, &lib).unwrap(),
            1
        );
        assert!(amp_core::show::eligible(
            &settings.playlists[0],
            &lib.tracks[0]
        ));
        assert!(!amp_core::show::eligible(
            &settings.playlists[0],
            &lib.tracks[1]
        ));
        set_source_mode(&mut settings.playlists[1], SourceMode::Folders);
        settings.save(directory.path()).unwrap();
        let loaded = Settings::load(directory.path()).unwrap();
        assert_eq!(loaded.playlists[0].allowed, settings.playlists[0].allowed);
        assert!(!amp_core::show::eligible(
            &loaded.playlists[1],
            &lib.tracks[0]
        ));
    }
    #[test]
    fn unsaved_rule_draft_does_not_change_performance_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let mut saved = test_settings();
        let mut draft = saved.clone();
        draft.playlists[0].name = "Unsaved change".into();
        let mut editor = Editor {
            open: true,
            draft: Some(draft),
            root: directory.path().into(),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            editor.show(ui, directory.path(), &mut saved, None)
        });
        output.textures_delta.clear();
        assert_eq!(saved.playlists[0].name, "Demo");
        assert!(!directory.path().join("ampdj/smart-playlists.json").exists());
    }
    #[test]
    fn switching_inline_playlists_keeps_unsaved_drafts_and_viewport_constrained() {
        let root = tempfile::tempdir().unwrap();
        let mut saved = test_settings();
        let mut editor = Editor::default();
        let first = saved.playlists[0].id.clone();
        let second = saved.playlists[1].id.clone();
        editor.open_playlist(&first, root.path(), &saved);
        editor.draft.as_mut().unwrap().playlists[0].name = "Keep my draft".into();
        editor.open_playlist(&second, root.path(), &saved);
        assert_eq!(editor.selected_id(), Some(second.as_str()));
        assert_eq!(
            editor.draft.as_ref().unwrap().playlists[0].name,
            "Keep my draft"
        );
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(620., 500.));
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| editor.show(ui, root.path(), &mut saved, None),
        );
        output.textures_delta.clear();
        let viewport = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("smart-editor-viewport")))
            .unwrap();
        assert!(
            screen.contains_rect(viewport),
            "Inline editor must fit parent viewport: {viewport:?}"
        );
        assert_eq!(saved.playlists[0].name, "Demo");
    }
}

#[cfg(test)]
mod additive_source_tests {
    use super::*;
    fn track(path: &str) -> amp_core::Track {
        amp_core::Track {
            path: path.into(),
            title: path.into(),
            preparation: Default::default(),
        }
    }
    #[test]
    fn folders_append_deduplicate_and_remain_recursive() {
        let mut p = Settings::default().playlists.remove(0);
        add_indexed_folder(&mut p, "House".into());
        add_indexed_folder(&mut p, "Disco".into());
        add_indexed_folder(&mut p, "House".into());
        assert_eq!(p.folders.len(), 2);
        assert!(amp_core::show::eligible(&p, &track("House/Deep/a.mp3")));
        assert!(amp_core::show::eligible(&p, &track("Disco/b.mp3")));
        assert!(!amp_core::show::eligible(&p, &track("Other/c.mp3")));
        p.folders.clear();
        assert!(!amp_core::show::eligible(&p, &track("Other/c.mp3")));
    }
    #[test]
    fn folders_and_playlist_selections_are_a_union_with_global_exclusions() {
        let mut p = Settings::default().playlists.remove(0);
        let original = track("Other/original.mp3");
        let extra = track("Extra/a.mp3");
        p.allowed = vec![amp_core::show::song_id(&original)];
        p.restrict_allowed = true;
        add_indexed_folder(&mut p, "House".into());
        add_playlist_source(
            &mut p,
            "Extras".into(),
            vec![amp_core::show::song_id(&extra)],
        );
        assert!(amp_core::show::eligible(&p, &original));
        assert!(amp_core::show::eligible(&p, &extra));
        assert!(amp_core::show::eligible(&p, &track("House/a.mp3")));
        p.excluded.push(amp_core::show::song_id(&extra));
        assert!(!amp_core::show::eligible(&p, &extra));
        p.playlist_sources.clear();
        assert!(!amp_core::show::eligible(&p, &original));
    }
    #[test]
    fn mixed_sources_roundtrip_in_portable_export() {
        let root = tempfile::tempdir().unwrap();
        let mut p = Settings::default().playlists.remove(0);
        add_indexed_folder(&mut p, "House".into());
        add_indexed_folder(&mut p, "Disco".into());
        add_playlist_source(
            &mut p,
            "Extras".into(),
            vec![amp_core::show::song_id(&track("Extra/a.mp3"))],
        );
        let bytes = amp_core::show::SmartPlaylistExport::encode(&p, root.path()).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["version"], 2);
        let imported = amp_core::show::SmartPlaylistExport::decode(&bytes, root.path()).unwrap();
        assert_eq!(imported.folders, p.folders);
        assert_eq!(imported.playlist_sources.len(), 1);
        assert!(amp_core::show::eligible(&imported, &track("Extra/a.mp3")));
    }
}
