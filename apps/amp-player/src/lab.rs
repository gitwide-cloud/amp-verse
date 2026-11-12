#[path = "lab_timeline.rs"]
mod timeline;
use amp_core::{
    Library,
    arrangement::{Clip, render},
};
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
};
#[derive(Default)]
pub struct Arrangement {
    clips: Vec<Clip>,
    timeline: timeline::Timeline,
    editor: crate::lab_editor::Editor,
    youtube: crate::lab_youtube::Panel,
    tab: usize,
    demo_loaded: bool,
    status: String,
    job: Option<mpsc::Receiver<Result<String, String>>>,
}
impl Arrangement {
    pub fn take_sample(&mut self) -> Option<crate::sample_editor::Delivery> {
        self.editor.take_sample()
    }
    pub(super) fn stop_preview(&mut self) {
        self.editor.stop();
        self.timeline.stop();
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        root: &Path,
        library: Option<&Library>,
    ) -> Option<crate::skin::Module> {
        crate::skin::panel(ui, false, |ui| {
            ui.set_min_size(ui.available_size());
            let action = crate::skin::heading(ui, "AMP Lab", true);
            self.editor.poll(ui.ctx());
            if let Some(path) = self.youtube.poll(ui.ctx()) {
                self.tab = 0;
                self.timeline.stop();
                let loaded = self.editor.load_path(path);
                self.youtube.opened(loaded);
            }
            let previous = self.tab;
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.tab, 0, "Waveform editor");
                ui.selectable_value(&mut self.tab, 1, "Mix timeline");
            });
            if previous == 1 && self.tab != 1 {
                self.timeline.stop();
            }
            if previous == 0 && self.tab != 0 {
                self.editor.stop();
            }
            if !self.demo_loaded
                && std::env::var("AMP_SCREENSHOT_VIEW").is_ok_and(|s| s == "lab")
                && let Some(track) = library.and_then(|l| l.tracks.first())
                && let Ok(path) = amp_core::resolve(root, &track.path)
            {
                self.editor.load_path(path);
                self.demo_loaded = true;
            }
            self.youtube.show(ui);
            egui::ScrollArea::vertical()
                .id_salt("lab-workspace")
                .max_height(ui.available_height())
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.tab == 0 {
                        self.editor.show(ui, root, library);
                    } else {
                        self.content(ui, root, library);
                    }
                });
            action
        })
    }
    fn content(&mut self, ui: &mut egui::Ui, root: &Path, library: Option<&Library>) {
        if let Some(rx) = &self.job
            && let Ok(result) = rx.try_recv()
        {
            self.status = result.unwrap_or_else(|e| e);
            self.job = None;
        }
        ui.heading("AMP Lab / Mix arrangement");
        ui.label("Arrange songs using cue IN/OUT and timeline starts. Overlaps mix together; source files stay unchanged.");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Save arrangement...").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("AMP mix project", &["ampmix", "json"])
                    .save_file()
            {
                self.status = serde_json::to_vec_pretty(&self.clips)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| std::fs::write(path, bytes).map_err(|e| e.to_string()))
                    .map(|_| "Arrangement saved".into())
                    .unwrap_or_else(|e| e);
            }
            if ui.button("Load arrangement...").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("AMP mix project", &["ampmix", "json"])
                    .pick_file()
            {
                match std::fs::read(path)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| {
                        serde_json::from_slice::<Vec<Clip>>(&bytes).map_err(|e| e.to_string())
                    }) {
                    Ok(clips) if clips.iter().all(|c| c.duration().is_ok() && c.lane < 64) => {
                        self.clips = clips;
                        self.timeline.reset();
                    }
                    Ok(_) => {
                        self.status = "Invalid mix project; existing arrangement retained".into()
                    }
                    Err(e) => self.status = e,
                }
            }
        });
        ui.separator();
        if library.is_none() {
            ui.label("Choose your music folder in AMP Library or AMP DJ first.");
        }
        let mut add = None;
        egui::ComboBox::from_id_salt("arrange-add")
            .selected_text("Add from music library...")
            .show_ui(ui, |ui| {
                if let Some(lib) = library {
                    for (i, t) in lib.tracks.iter().enumerate() {
                        if ui.button(&t.title).clicked() {
                            add = Some(i);
                        }
                    }
                }
            });
        if let Some(i) = add
            && let Some(track) = library.and_then(|l| l.tracks.get(i))
            && let Ok(path) = amp_core::resolve(root, &track.path)
        {
            let start = self
                .clips
                .iter()
                .filter_map(|c| c.duration().ok().map(|d| c.start + d))
                .fold(0., f64::max);
            self.timeline.import(path, self.clips.len() % 2, start);
        }
        self.timeline_ui(ui, root);
        ui.add_enabled_ui(self.job.is_none() && !self.clips.is_empty(), |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Export one combined MP3...").clicked()
                    && let Some(output) = rfd::FileDialog::new()
                        .add_filter("MP3", &["mp3"])
                        .save_file()
                {
                    self.export(root.to_path_buf(), output, false);
                }
                if ui.button("Export individual MP3s...").clicked()
                    && let Some(folder) = rfd::FileDialog::new().pick_folder()
                {
                    self.export(root.to_path_buf(), folder, true);
                }
            });
        });
        ui.label(if self.job.is_some() {
            "Exporting..."
        } else {
            &self.status
        });
        ui.weak("320 kbps MP3 / limiter / existing files never overwritten. Individual exports remove leading timeline silence. Drag clips to move; drag edges to trim. Overlaps sum together; use fades to shape transitions. Automatic beat sync is not applied.");
    }
    fn export(&mut self, root: PathBuf, output: PathBuf, individual: bool) {
        let clips = self.clips.clone();
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        std::thread::spawn(move || {
            let result = (|| {
                if individual {
                    for (i, c) in clips.iter().enumerate() {
                        let mut c = c.clone();
                        c.start = 0.;
                        render(&root, &[c], &output.join(format!("track-{:02}.mp3", i + 1)))?;
                    }
                } else {
                    render(&root, &clips, &output)?;
                }
                Ok("Export complete".into())
            })();
            let _ = tx.send(result);
        });
    }
}
