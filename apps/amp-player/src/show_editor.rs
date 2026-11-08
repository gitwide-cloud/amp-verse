//! AMP Library edits a draft; only explicit Save publishes rules for future performances.
use amp_core::show::{EnergyPlan, Fallback, Settings, Style};
use eframe::egui;
#[derive(Default)]
pub struct Editor {
    pub open: bool,
    selected: usize,
    status: String,
    search: String,
    draft: Option<Settings>,
    root: std::path::PathBuf,
}
impl Editor {
    pub fn open_playlist(&mut self, id: &str, root: &std::path::Path, saved: &Settings) {
        self.draft = Some(saved.clone());
        self.root = root.into();
        self.selected = saved.playlists.iter().position(|p| p.id == id).unwrap_or(0);
        self.open = true;
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        root: &std::path::Path,
        saved: &mut Settings,
        library: Option<&amp_core::Library>,
    ) {
        if !self.open {
            return;
        }
        if self.draft.is_none() || self.root != root {
            self.draft = Some(saved.clone());
            self.root = root.into();
            self.selected = 0;
        }
        let settings = self.draft.as_mut().unwrap();
        let mut open = true;
        egui::Window::new("AMP Library · Smart Playlist rules").open(&mut open).default_width(780.).default_height(650.).vscroll(true).show(ctx,|ui|{
            ui.label("Prepare and save rules here. DJ uses saved rules; changes take effect when you next load a playlist.");
            egui::ComboBox::from_id_salt("edit-playlist").selected_text(settings.playlists.get(self.selected).map_or("No playlist",|p|p.name.as_str())).show_ui(ui,|ui|{
                for(i,p)in settings.playlists.iter().enumerate(){ui.selectable_value(&mut self.selected,i,&p.name);}
            });
            ui.horizontal(|ui|{
                if ui.add_enabled(settings.playlists.len()<1000,egui::Button::new("New Smart Playlist")).clicked(){
                    let mut p=Settings::default().playlists.pop().unwrap();p.id=format!("custom-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());p.name="New Smart Playlist".into();settings.playlists.push(p);self.selected=settings.playlists.len()-1;
                }
                if ui.button("Remove selected").clicked()&&self.selected<settings.playlists.len(){settings.playlists.remove(self.selected);self.selected=self.selected.min(settings.playlists.len().saturating_sub(1));}
                if ui.button("Save rules").clicked(){self.status=save_rules(root,settings,saved);}
                if ui.button("Reload saved rules").clicked(){match Settings::load(root){Ok(fresh)=>{*saved=fresh.clone();*settings=fresh;self.selected=0;self.status="Reloaded current shared configuration".into();},Err(e)=>self.status=e}}
            });
            if let Some(p)=settings.playlists.get_mut(self.selected){
                ui.horizontal(|ui|{ui.label("Name");ui.text_edit_singleline(&mut p.name);if ui.button("Copy control ID").clicked(){ui.ctx().copy_text(p.id.clone());}});
                ui.horizontal(|ui|{
                    egui::ComboBox::from_id_salt("mix-style").selected_text(format!("{:?}",p.style)).show_ui(ui,|ui|{for style in [Style::Hype,Style::Steady,Style::Chill]{ui.selectable_value(&mut p.style,style,format!("{style:?}"));}});
                    let mut targeted=p.target_energy.is_some();if ui.checkbox(&mut targeted,"Target energy").changed(){p.target_energy=targeted.then_some(5.);}
                    if let Some(e)=&mut p.target_energy{ui.add(egui::Slider::new(e,1. ..=10.));}
                    ui.add(egui::DragValue::new(&mut p.recent_repeat).range(0..=1000).prefix("Avoid last ").suffix(" songs"));
                });
                egui::ComboBox::from_id_salt("repeat-fallback").selected_text(match p.fallback{Fallback::Stop=>"Stop when all eligible songs are recent",Fallback::RelaxRecent=>"Continue with recently played music if needed"}).show_ui(ui,|ui|{
                    ui.selectable_value(&mut p.fallback,Fallback::Stop,"Stop when all eligible songs are recent");ui.selectable_value(&mut p.fallback,Fallback::RelaxRecent,"Continue with recently played music if needed");
                });
                let mut progressing=p.energy_progression.is_some();if ui.checkbox(&mut progressing,"Saved energy progression").changed(){p.energy_progression=progressing.then_some(EnergyPlan{target:3.,transitions:3,hold:true});}
                if let Some(plan)=&mut p.energy_progression{ui.horizontal(|ui|{ui.add(egui::Slider::new(&mut plan.target,1. ..=10.).text("Target"));ui.add(egui::DragValue::new(&mut plan.transitions).range(1..=100).suffix(" transitions"));ui.checkbox(&mut plan.hold,"Hold target");});}
                ui.checkbox(&mut p.use_manual_cues,"Prefer approved manual build/drop cues for Hype");
                source_ui(ui,root,library,p,&mut self.status);
                ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Find songs to allow, exclude, prefer or assign for recognition").desired_width(600.));
                if let Some(lib)=library{
                    let query=self.search.to_lowercase();let tracks:Vec<_>=lib.tracks.iter().filter(|t|t.title.to_lowercase().contains(&query)).collect();
                    if ui.button("Clear recognition assignment").clicked(){p.recognition_song=None;}
                    egui::ScrollArea::vertical().id_salt("song-rules").max_height(180.).show_rows(ui,25.,tracks.len(),|ui,rows|{
                        for i in rows{let t=tracks[i];let id=amp_core::show::song_id(t);
                            ui.push_id(&id,|ui|ui.horizontal(|ui|{
                                ui.add_sized([260.,20.],egui::Label::new(&t.title).truncate()).on_hover_text(t.path.display().to_string());
                                for(label,list,enabled)in[("Allow",&mut p.allowed,p.restrict_allowed),("Exclude",&mut p.excluded,true),("Prefer",&mut p.preferred,true)]{
                                    let mut checked=if label=="Allow"&&!p.restrict_allowed{true}else{list.contains(&id)};
                                    if ui.add_enabled(enabled,egui::Checkbox::new(&mut checked,label)).changed(){if checked{list.push(id.clone());}else{list.retain(|old|old!=&id);}}
                                }
                                let assigned=p.recognition_song.as_deref()==Some(id.as_str())||p.recognition_song.as_deref()==Some(t.title.as_str());
                                if ui.radio(assigned,"Recognition").clicked(){p.recognition_song=Some(id.clone());}
                            }));
                        }
                    });
                }
                egui::CollapsingHeader::new("Advanced selectors / stable IDs").show(ui,|ui|{
                    ui.weak(format!("Playlist ID: {}",p.id));
                    for(label,list)in[("Allowed song IDs",&mut p.allowed),("Excluded song IDs",&mut p.excluded),("Preferred song IDs",&mut p.preferred)]{
                        egui::CollapsingHeader::new(label).show(ui,|ui|{let mut text=list.join("\n");if ui.add(egui::TextEdit::multiline(&mut text).desired_rows(3)).changed(){*list=text.lines().map(str::trim).filter(|s|!s.is_empty()).map(str::to_owned).collect();}});
                    }
                    ui.horizontal(|ui|{ui.label("Assigned recognition song ID / exact title");let mut song=p.recognition_song.clone().unwrap_or_default();if ui.text_edit_singleline(&mut song).changed(){p.recognition_song=(!song.trim().is_empty()).then_some(song);}});
                });
            }
            if ui.button("Save Library configurations").clicked(){self.status=save_rules(root,settings,saved);}
            ui.weak(&self.status);ui.weak("Automatic section detection, scenes and hardware mappings remain separate milestones.");
        });
        self.open = open;
    }
}

fn save_rules(root: &std::path::Path, settings: &mut Settings, saved: &mut Settings) -> String {
    if root.as_os_str().is_empty() {
        return "Choose a music root first".into();
    }
    match settings.save(root) {
        Ok(()) => {
            if let Ok(fresh) = Settings::load(root) {
                *settings = fresh;
            }
            *saved = settings.clone();
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
    p.folders.clear();
    p.allowed.clear();
    p.restrict_folders = mode == SourceMode::Folders;
    p.restrict_allowed = mode != SourceMode::All;
}
fn import_source_playlist(
    p: &mut amp_core::show::SmartPlaylist,
    list: &amp_core::Playlist,
    library: &amp_core::Library,
) -> Result<usize, String> {
    let ids: Vec<_> = library
        .tracks
        .iter()
        .filter(|t| list.tracks.contains(&t.path))
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
        ui.horizontal(|ui|{
            if ui.add_enabled(library.is_some(),egui::Button::new("Browse source folder...")).clicked()
                && let Some(folder)=rfd::FileDialog::new().set_directory(root).pick_folder() {
                let relative=(|| {
                    let base=root.canonicalize().map_err(|e|e.to_string())?;
                    let selected=folder.canonicalize().map_err(|e|e.to_string())?;
                    selected.strip_prefix(base).map(std::path::Path::to_path_buf).map_err(|_|"That folder is outside the current music root. Choose its parent as your music root in AMP Library first.".to_owned())
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
    }
    if mode == SourceMode::Songs {
        ui.horizontal(|ui|{
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
    #[test]
    fn empty_source_selection_never_expands_to_entire_library() {
        let track = amp_core::Track {
            path: "Dance/Set/song.mp3".into(),
            title: "Song".into(),
            preparation: Default::default(),
        };
        let mut p = Settings::default().playlists.remove(0);
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
        let mut settings = Settings::default();
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
        let mut saved = Settings::default();
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
            editor.show(ui.ctx(), directory.path(), &mut saved, None)
        });
        output.textures_delta.clear();
        assert_eq!(saved.playlists[0].name, "Recognition");
        assert!(!directory.path().join(".ampverse/show.json").exists());
    }
}
