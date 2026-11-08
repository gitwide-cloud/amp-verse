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
                if ui.button("Reload saved rules").clicked(){*settings=saved.clone();self.selected=0;self.status="Reloaded saved configuration".into();}
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
                let mut folders=p.folders.iter().map(|p|p.to_string_lossy()).collect::<Vec<_>>().join("\n");
                ui.label("Source folders relative to your music root (one per line; blank uses all folders)");
                if ui.add(egui::TextEdit::multiline(&mut folders).desired_rows(2)).changed(){p.folders=folders.lines().map(str::trim).filter(|s|!s.is_empty()).map(std::path::PathBuf::from).collect();}
                if !p.allowed.is_empty(){p.restrict_allowed=true;}
                if ui.checkbox(&mut p.restrict_allowed,"Only allow selected songs").changed()&&!p.restrict_allowed{p.allowed.clear();}
                ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Find songs to allow, exclude, prefer or assign for recognition").desired_width(600.));
                if let Some(lib)=library{
                    let query=self.search.to_lowercase();let tracks:Vec<_>=lib.tracks.iter().filter(|t|t.title.to_lowercase().contains(&query)).collect();
                    if ui.button("Clear recognition assignment").clicked(){p.recognition_song=None;}
                    egui::ScrollArea::vertical().id_salt("song-rules").max_height(180.).show_rows(ui,25.,tracks.len(),|ui,rows|{
                        for i in rows{let t=tracks[i];let id=amp_core::show::song_id(t);
                            ui.push_id(&id,|ui|ui.horizontal(|ui|{
                                ui.add_sized([260.,20.],egui::Label::new(&t.title).truncate());
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
            if ui.button("Save Library configurations").clicked(){self.status=if root.as_os_str().is_empty(){"Choose a music root first".into()}else{match settings.save(root){Ok(())=>{*saved=settings.clone();"Saved. Active performances keep their loaded rules.".into()},Err(e)=>e}};}
            ui.weak(&self.status);ui.weak("Automatic section detection, scenes and hardware mappings remain separate milestones.");
        });
        self.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
