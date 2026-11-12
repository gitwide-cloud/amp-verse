//! Review first; all hashing, copying, and verification run off the UI thread.
use super::*;
use amp_core::consolidate::{Plan, Report};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
enum Event {
    Progress(String),
    Scanned(Result<Box<Plan>, String>),
    Finished(Result<Report, String>),
}
#[derive(Default)]
pub struct Organizer {
    root: PathBuf,
    destination: PathBuf,
    plan: Option<Plan>,
    report: Option<Report>,
    moving: bool,
    rx: Option<mpsc::Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    status: String,
    confirmed: bool,
}
impl Drop for Organizer {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl Organizer {
    pub fn moving(&self) -> bool {
        self.moving && self.rx.is_some()
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        root: &Path,
        blocked: bool,
        mut before_move: impl FnMut(),
    ) -> Option<PathBuf> {
        let ctx = ui.ctx().clone();
        let mut finished = false;
        if let Some(rx) = &self.rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    Event::Progress(status) => self.status = status,
                    Event::Scanned(result) => {
                        match result {
                            Ok(plan) => {
                                self.status = format!(
                                    "{} songs · {} unique files · {} extra exact copies",
                                    plan.files.len(),
                                    plan.groups.len(),
                                    plan.files.len() - plan.groups.len()
                                );
                                self.plan = Some(*plan);
                            }
                            Err(e) => self.status = e,
                        }
                        finished = true;
                    }
                    Event::Finished(result) => {
                        match result {
                            Ok(report) => {
                                self.status = format!(
                                    "{} originals moved. {} duplicate or changed originals retained.",
                                    report.moved, report.retained
                                );
                                self.report = Some(report);
                            }
                            Err(e) => {
                                self.status = format!(
                                    "Consolidation stopped: {e} Completed files are kept; unverified originals were not removed. Retry this preview to continue."
                                )
                            }
                        }
                        finished = true;
                    }
                }
            }
        }
        if finished {
            self.rx = None;
            self.moving = false;
        }
        let busy = self.rx.is_some();
        let mut connect = None;
        ui.heading("Organise library");
        egui::ScrollArea::vertical().id_salt("library-organizer-body").max_height((ui.available_height()-40.).max(180.)).show(ui,|ui|{
            ui.label("Find exact duplicate files, choose a keeper, then move unique music into one base directory.");
            ui.weak("Different encodings, edits, or files with different tags are kept as separate songs. Nothing is deleted just for matching names.");
            ui.label(format!("Current library: {}",root.display()));
            if ui.add_enabled(!busy,egui::Button::new("Scan all music sources for duplicates")).clicked(){
                self.root=root.into();self.plan=None;self.report=None;self.confirmed=false;self.cancel=Arc::new(AtomicBool::new(false));
                let root=root.to_path_buf();let cancel=self.cancel.clone();let (tx,rx)=mpsc::channel();self.rx=Some(rx);let ctx=ctx.clone();
                std::thread::spawn(move||{let result=amp_core::consolidate::scan(&root,&cancel,|status|{let _=tx.send(Event::Progress(status));ctx.request_repaint();});let _=tx.send(Event::Scanned(result.map(Box::new)));ctx.request_repaint();});
            }
            if ui.add_enabled(!busy,egui::Button::new("Resume previous consolidation…")).clicked()
                && let Some(path)=rfd::FileDialog::new().set_title("Choose the recorded consolidation destination").pick_folder() {
                match amp_core::consolidate::resume_plan(&path) {
                    Ok(plan)=>{self.root=if root.canonicalize().is_ok_and(|r|r==plan.root) {root.into()}else{plan.root.clone()};self.destination=path;self.plan=Some(plan);self.report=None;self.confirmed=false;self.status="Recovery preview loaded. Use the original source library and confirm the same move to resume.".into();}
                    Err(error)=>self.status=error,
                }
            }
            ui.horizontal(|ui|{if ui.add_enabled(!busy,egui::Button::new("Choose empty destination folder…")).clicked() && let Some(path)=rfd::FileDialog::new().set_title("Choose a separate, empty base directory").pick_folder(){self.destination=path;self.confirmed=false;}ui.label(self.destination.display().to_string());});
            ui.weak("Subfolders are retained under Library / Sources inside that base folder, so different songs with the same filename cannot overwrite each other.");
            ui.separator();ui.label(&self.status);
            if busy{ui.horizontal(|ui|{ui.spinner();if ui.button("Cancel").clicked(){self.cancel.store(true,Ordering::Relaxed);}});}
            if let Some(plan)=&mut self.plan {
                let mut organize=plan.files.first().is_some_and(|f|f.organize_names);
                if ui.add_enabled(!busy&&!self.destination.join("ampdj/consolidation.ampdb").exists(),egui::Checkbox::new(&mut organize,"Organize as Artist / Album / Title (rename files)")).changed(){for file in &mut plan.files{file.organize_names=organize;}self.confirmed=false;}
                ui.weak("Renaming uses saved or embedded tags, with a short content ID to prevent filename collisions. Preview below before moving.");
                let bytes:u64=plan.groups.iter().map(|g|plan.files[g.keeper].bytes).sum();
                ui.strong(format!("Move preview: {} unique songs · {:.2} GiB",plan.groups.len(),bytes as f64/1073741824.));
                ui.weak("A checksum-verified copy, preparation, playlist remap and recovery journal are written before any original is removed. Extra duplicate copies and their metadata stay in their source folders.");
                egui::ScrollArea::vertical().id_salt("duplicate-groups").max_height(270.).show(ui,|ui|{
                    let mut any=false;
                    for group in plan.groups.iter_mut().filter(|g|g.members.len()>1){any=true;
                        let title=plan.files[group.keeper].original.file_name().unwrap_or_default().to_string_lossy();
                        egui::CollapsingHeader::new(format!("{} copies · {title}",group.members.len())).id_salt(&plan.files[group.keeper].hash).show(ui,|ui|{
                            for &index in &group.members {
                                let entry=&plan.files[index];
                                ui.add_enabled_ui(!busy,|ui|{ui.radio_value(&mut group.keeper,index,entry.original.display().to_string());});
                                ui.weak(format!("{} saved cues · {} · {}",entry.preparation.manual_cues.len()+entry.preparation.cues.len(),if entry.preparation.manual_override{"Manual edits"}else{"Automatic / unedited"},if entry.preparation.analysis.is_some(){"Analyzed"}else{"No analysis"}));
                            }
                        });
                    }
                    if !any{ui.label("No exact duplicates found. All scanned songs are unique files.");}
                    egui::CollapsingHeader::new("All proposed moves").show(ui,|ui|{for group in &plan.groups{ui.label(format!("{} → {}",plan.files[group.keeper].original.display(),amp_core::consolidate::target_relative(&plan.files[group.keeper]).display()));}});
                });
                ui.add_enabled_ui(!busy,|ui|{ui.checkbox(&mut self.confirmed,"Move the selected originals after verification; keep extra duplicates for review");});
            }
            if blocked{ui.colored_label(ORANGE,"Stop playback, Auto DJ, analysis and stem jobs; close Lab before applying moves. Other devices and servers should not be using these source folders during the move.");}
            let ready=!busy && !blocked && self.confirmed && self.plan.as_ref().is_some_and(|p|!p.files.is_empty()) && !self.destination.as_os_str().is_empty() && self.root==root && self.report.is_none();
            if ui.add_enabled(ready,egui::Button::new("Move unique songs to destination")).clicked(){
                before_move();self.moving=true;let plan=self.plan.clone().unwrap();let destination=self.destination.clone();self.cancel=Arc::new(AtomicBool::new(false));let cancel=self.cancel.clone();let (tx,rx)=mpsc::channel();self.rx=Some(rx);let ctx=ctx.clone();
                std::thread::spawn(move||{let result=amp_core::consolidate::execute(&plan,&destination,&cancel,|status|{let _=tx.send(Event::Progress(status));ctx.request_repaint();},stems::relocate_cached);let _=tx.send(Event::Finished(result));ctx.request_repaint();});
            }
            if let Some(report)=&self.report {
                if ui.button("Use consolidated library").clicked(){connect=Some(report.destination.clone());}
                egui::ScrollArea::vertical().id_salt("consolidate-warnings").max_height(120.).show(ui,|ui|{for warning in &report.warnings{ui.label(warning);}});
            }
        });
        connect
    }
}
