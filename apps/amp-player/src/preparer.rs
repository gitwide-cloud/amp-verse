//! Desktop preparation review and background library analysis.
use amp_core::{
    Library, Preparation, Track, analysis,
    preparation::{self, Cue},
};
use eframe::egui;
use std::{path::PathBuf, sync::mpsc};
struct Update {
    root: PathBuf,
    path: PathBuf,
    result: Result<Preparation, String>,
    selected: bool,
}
#[derive(Default)]
pub struct Preparer {
    pub open: bool,
    pub preview: Option<(PathBuf, f64)>,
    review_selected: std::collections::HashSet<String>,
    editing_revision: Option<Vec<u8>>,
    root: PathBuf,
    path: PathBuf,
    title: String,
    draft: Preparation,
    job: Option<mpsc::Receiver<Update>>,
    pub status: String,
    total: usize,
    done: usize,
    failures: usize,
}
impl Preparer {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn edit(&mut self, root: &std::path::Path, track: &Track) {
        if self.busy() {
            return;
        }
        self.root = root.to_path_buf();
        self.path = track.path.clone();
        self.title = track.title.clone();
        match preparation::load_with_revision(&root.join(&track.path)) {
            Ok((p, revision)) => {
                self.draft = p;
                self.editing_revision = revision;
            }
            Err(e) => {
                self.status = e;
                return;
            }
        }
        if let Some(title) = self.draft.extensions.get("title").and_then(|v| v.as_str()) {
            self.title = title.into();
        }
        self.review_selected.clear();
        self.open = true;
    }
    pub fn batch(&mut self, root: PathBuf, tracks: Vec<Track>) {
        if self.busy() {
            return;
        }
        self.total = tracks.len();
        self.done = 0;
        self.failures = 0;
        self.status = format!("Preparing {} tracks...", self.total);
        self.start(root, tracks, false);
    }
    fn start(&mut self, root: PathBuf, tracks: Vec<Track>, selected: bool) {
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        std::thread::spawn(move || {
            for track in tracks {
                let result = (|| {
                    let path = amp_core::resolve(&root, &track.path).map_err(|e| e.to_string())?;
                    let (snapshot, sidecar_before) = preparation::load_with_revision(&path)?;
                    let existing = if selected {
                        track.preparation.clone()
                    } else {
                        snapshot
                    };
                    let result = analysis::analyze(&path, &existing)?;
                    if !selected {
                        if sidecar_before != amp_core::preparation::revision(&path) {
                            return Err(
                                "Preparation was edited during analysis; kept the newer metadata"
                                    .into(),
                            );
                        }
                        preparation::save_if_unchanged(
                            &root,
                            &track.path,
                            &result,
                            sidecar_before.as_deref(),
                        )?;
                    }
                    Ok(result)
                })();
                if tx
                    .send(Update {
                        root: root.clone(),
                        path: track.path,
                        result,
                        selected,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
    }
    pub fn poll(&mut self, root: &std::path::Path, library: Option<&mut Library>) {
        let mut updates = Vec::new();
        let mut finished = false;
        if let Some(rx) = &self.job {
            loop {
                match rx.try_recv() {
                    Ok(update) => updates.push(update),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
        }
        let mut library = library;
        for update in updates {
            self.done += 1;
            match update.result {
                Ok(p) => {
                    if update.selected {
                        self.draft = p;
                        self.status =
                            "Analysis ready for review. Save to publish metadata beside the song."
                                .into();
                    } else {
                        if update.root == root
                            && let Some(lib) = library.as_mut()
                            && let Some(t) = lib.tracks.iter_mut().find(|t| t.path == update.path)
                        {
                            t.preparation = p;
                        }
                        self.status = format!(
                            "Prepared {}/{}: {}",
                            self.done,
                            self.total,
                            update.path.display()
                        );
                    }
                }
                Err(e) => {
                    self.failures += 1;
                    self.status = format!("{}: {e}", update.path.display());
                }
            }
        }
        if finished {
            if self.total > 1 {
                self.status = format!(
                    "Finished: {} prepared, {} failed ({} tracks)",
                    self.done.saturating_sub(self.failures),
                    self.failures,
                    self.total
                );
            }
            self.job = None;
        }
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        root: &std::path::Path,
        library: Option<&mut Library>,
    ) {
        if !self.open {
            return;
        }
        let mut open = true;
        let mut library = library;
        egui::Window::new("AMP Preparation / key, tempo and cues").open(&mut open).default_width(700.).default_height(650.).vscroll(true).show(ctx,|ui|{
        ui.strong(&self.title);ui.label("Independent heuristic analyzer: review estimates before using them in a DJ set.");
        ui.add_enabled_ui(!self.busy(),|ui|{ui.horizontal(|ui|{
            if ui.button("Analyze track").clicked(){self.total=1;self.done=0;self.failures=0;let track=Track{path:self.path.clone(),title:self.title.clone(),preparation:self.draft.clone()};self.status="Analyzing decoded audio...".into();self.start(self.root.clone(),vec![track],true);}
            if ui.button("Save approved metadata").clicked(){
            if preparation::revision(&self.root.join(&self.path))!=self.editing_revision {self.status="Metadata changed on disk; reopen this track before saving".into();return;}
self.draft.schema_version=Some(1);self.draft.manual_override=true;if let Some(a)=&mut self.draft.analysis{a.reviewed=true;}match preparation::save_if_unchanged(&self.root,&self.path,&self.draft,self.editing_revision.as_deref()){Ok(())=>{if self.root==root&&let Some(lib)=library.as_mut()&&let Some(t)=lib.tracks.iter_mut().find(|t|t.path==self.path){t.preparation=self.draft.clone();if let Some(title)=self.draft.extensions.get("title").and_then(|v|v.as_str()){t.title=title.into();}}self.editing_revision=preparation::revision(&self.root.join(&self.path));self.status="Approved metadata saved in .ampverse. Original audio unchanged.".into();},Err(e)=>self.status=e}}
        });});
        let changes=amp_core::review::changes(&self.draft);
        if !changes.is_empty() {
            ui.separator();ui.strong("Analysis recommendations — review before applying");
            egui::ScrollArea::vertical().id_salt("review-changes").max_height(150.).show(ui,|ui|{
                for (field,before,after) in &changes {
                    let mut selected=self.review_selected.contains(field);
                    ui.horizontal(|ui|{if ui.checkbox(&mut selected,field).changed(){if selected{self.review_selected.insert(field.clone());}else{self.review_selected.remove(field);}}
                    ui.label(format!("{} → {}",before.to_string().chars().take(60).collect::<String>(),after.to_string().chars().take(60).collect::<String>()));});
                    egui::CollapsingHeader::new(format!("Exact {field} values")).show(ui,|ui|{ui.label("Existing");ui.code(serde_json::to_string_pretty(before).unwrap_or_default());ui.label("Suggested");ui.code(serde_json::to_string_pretty(after).unwrap_or_default());});
                }
            });
            ui.horizontal_wrapped(|ui|{
                for (label,accept,all) in [("Accept selected",true,false),("Accept all",true,true),("Reject selected",false,false),("Keep existing / dismiss all",false,true)] {
                    if ui.button(label).clicked(){let fields=if all {changes.iter().map(|c|c.0.clone()).collect()}else{self.review_selected.iter().cloned().collect::<Vec<_>>()};match amp_core::review::decide(&self.draft,&fields,accept){Ok(p)=>{self.draft=p;self.review_selected.clear();self.status="Review decision is staged. Save approved metadata to publish it.".into();},Err(e)=>self.status=e}}
                }
                if ui.button("Preview suggested IN").clicked()&&let Some(p)=amp_core::review::proposal(&self.draft){self.preview=Some((self.path.clone(),p.cue_in));}
            });
        }
        ui.add_enabled_ui(!self.busy(),|ui|{
        egui::CollapsingHeader::new("Library tags (portable metadata)").show(ui,|ui| {
            for field in ["title","artist","album","genre","comment"] {
                ui.horizontal(|ui| {
                    ui.label(field);
                    let mut value=self.draft.extensions.get(field).and_then(|v|v.as_str()).unwrap_or(if field=="title" {&self.title} else {""}).to_owned();
                    if ui.text_edit_singleline(&mut value).changed() {
                        if value.trim().is_empty() {self.draft.extensions.remove(field);} else {self.draft.extensions.insert(field.into(),serde_json::json!(value));}
                    }
                });
            }
        });
        ui.weak("Saved and manually corrected values are protected. Use review decisions to approve replacements.");
        ui.horizontal(|ui|{ui.label("Cue IN (s)");if ui.add(egui::DragValue::new(&mut self.draft.cue_in).speed(0.01).range(0. ..=21600.)).changed(){self.draft.manual_override=true;}ui.label("Cue OUT (s)");let mut end=self.draft.cue_out.unwrap_or(0.);if ui.add(egui::DragValue::new(&mut end).speed(0.01).range(0. ..=21600.)).changed(){self.draft.cue_out=if end>0.{Some(end)}else{None};self.draft.manual_override=true;}});
        ui.horizontal(|ui|{ui.label("BPM");let mut bpm=self.draft.bpm.unwrap_or(0.);if ui.add(egui::DragValue::new(&mut bpm).speed(0.1).range(0. ..=300.)).changed(){self.draft.bpm=if bpm>0.{Some(bpm)}else{None};self.draft.manual_override=true;}ui.label("Grid offset (s)");let mut offset=self.draft.beat_offset.unwrap_or(0.);if ui.add(egui::DragValue::new(&mut offset).speed(0.01).range(0. ..=21600.)).changed(){self.draft.beat_offset=Some(offset);self.draft.manual_override=true;}});
        ui.horizontal(|ui|{ui.label("Key");let mut key=self.draft.key.clone().unwrap_or_default();if ui.add(egui::TextEdit::singleline(&mut key).desired_width(140.)).changed(){self.draft.key=if key.is_empty(){None}else{Some(key)};self.draft.manual_override=true;}ui.label("Harmonic code");let mut code=self.draft.harmonic_key.clone().unwrap_or_default();if ui.add(egui::TextEdit::singleline(&mut code).desired_width(70.)).changed(){self.draft.harmonic_key=if code.is_empty(){None}else{Some(code)};self.draft.manual_override=true;}});
        ui.horizontal(|ui| {
            ui.label("Rating");
            let mut rating = self.draft.extensions.get("rating").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            if ui.add(egui::Slider::new(&mut rating, 0..=5)).changed() { self.draft.extensions.insert("rating".into(), serde_json::json!(rating)); }
            let mut known = amp_core::smart::energy(&self.draft).is_some();
            if ui.checkbox(&mut known, "Energy known").changed() {
                if known { self.draft.extensions.insert("energy".into(), serde_json::json!(0.5)); }
                else { self.draft.extensions.remove("energy"); }
            }
            if known {
                let mut energy = amp_core::smart::energy(&self.draft).unwrap_or(0.5);
                if ui.add(egui::Slider::new(&mut energy, 0. ..=1.).text("Energy")).changed() { self.draft.extensions.insert("energy".into(), serde_json::json!(energy)); }
            }
        });
        ui.horizontal(|ui|{
            let mut rate=self.draft.extensions.get("playback_rate").and_then(|v|v.as_f64()).unwrap_or(1.);
            if ui.add(egui::Slider::new(&mut rate,0.84..=1.16).text("Playback rate")).changed() {self.draft.extensions.insert("playback_rate".into(),serde_json::json!(rate));}
            ui.weak("Applied when loaded; changing speed also changes audible pitch (no key lock).");
        });
        ui.weak("Key edits correct saved metadata; they do not transpose the audio. Energy is a relative level estimate or your manual value.");
        if let Some(a)=&self.draft.analysis{ui.label(format!("{:.2}s / tempo score {:.2} / key margin {:.2} / {}",a.duration,a.tempo_confidence,a.key_confidence,a.engine));}
        ui.separator();ui.strong("Manual cues (preserved across re-analysis)");
        let mut remove=None;egui::ScrollArea::vertical().id_salt("manual-cues").max_height(160.).show(ui,|ui|{for(i,cue)in self.draft.manual_cues.iter_mut().enumerate(){ui.push_id(i,|ui|{ui.horizontal(|ui|{let mut number=cue.number.unwrap_or((i+1) as u16);if ui.add(egui::DragValue::new(&mut number).range(1..=128).prefix("Cue ")).changed(){cue.number=Some(number);}
ui.add(egui::TextEdit::singleline(&mut cue.label).desired_width(220.));ui.add(egui::DragValue::new(&mut cue.seconds).speed(0.01).range(0. ..=21600.).suffix(" s"));if ui.button("Remove").clicked(){remove=Some(i);}});});}});if let Some(i)=remove{self.draft.manual_cues.remove(i);}
        if ui.button("Add manual cue").clicked(){self.draft.manual_cues.push(Cue{id:format!("manual-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()),label:"Cue".into(),seconds:self.draft.cue_in,kind:"hotcue".into(),..Default::default()});}
        amp_core::preparation::assign_numbers(&mut self.draft);
        egui::CollapsingHeader::new(format!("Generated cue candidates ({})",self.draft.cues.len())).show(ui,|ui|{egui::ScrollArea::vertical().max_height(120.).show(ui,|ui|{for cue in &self.draft.cues{ui.label(format!("{:.2}s   {}",cue.seconds,cue.label));}});});
        });
        ui.label(&self.status);ui.weak(format!("Metadata: {}",amp_core::preparation::portable(&self.root.join(&self.path)).display()));
    });
        self.open = open;
    }
}
