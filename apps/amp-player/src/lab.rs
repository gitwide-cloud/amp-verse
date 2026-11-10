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
    editor: crate::lab_editor::Editor,
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
            let previous = self.tab;
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.tab, 0, "Waveform editor");
                ui.selectable_value(&mut self.tab, 1, "Mix timeline");
            });
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
            if ui.button("Waveform editor").clicked() {
                self.tab = 0;
            }
            if ui.button("Save arrangement...").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("AMP arrangement", &["json"])
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
                    .add_filter("AMP arrangement", &["json"])
                    .pick_file()
            {
                match std::fs::read(path)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| {
                        serde_json::from_slice::<Vec<Clip>>(&bytes).map_err(|e| e.to_string())
                    }) {
                    Ok(clips) => self.clips = clips,
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
        {
            let mut preparation = track.preparation.clone();
            if preparation.cue_out.is_none() {
                preparation.cue_out = amp_core::resolve(root, &track.path)
                    .ok()
                    .and_then(|path| {
                        amp_core::command("ffprobe")
                            .args([
                                "-v",
                                "error",
                                "-show_entries",
                                "format=duration",
                                "-of",
                                "default=nw=1:nk=1",
                            ])
                            .arg(path)
                            .output()
                            .ok()
                    })
                    .filter(|o| o.status.success())
                    .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok());
            }
            let start = self
                .clips
                .iter()
                .filter_map(|c| c.duration().ok().map(|d| c.start + d))
                .fold(0., f64::max);
            self.clips.push(Clip {
                path: track.path.clone(),
                start,
                preparation,
                gain_db: 0.,
            });
        }
        let extent = self
            .clips
            .iter()
            .filter_map(|c| c.duration().ok().map(|d| c.start + d))
            .fold(1., f64::max);
        let mut remove = None;
        egui::ScrollArea::vertical()
            .max_height(460.)
            .show(ui, |ui| {
                for (i, clip) in self.clips.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(clip.path.display().to_string())
                                            .strong(),
                                    )
                                    .truncate(),
                                );
                                if ui.button("Remove").clicked() {
                                    remove = Some(i);
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Start (s)");
                                ui.add(
                                    egui::DragValue::new(&mut clip.start)
                                        .range(0. ..=864000.)
                                        .speed(0.1),
                                );
                                ui.label("Cue IN");
                                ui.add(
                                    egui::DragValue::new(&mut clip.preparation.cue_in)
                                        .range(0. ..=864000.)
                                        .speed(0.1),
                                );
                                ui.label("Cue OUT");
                                let end = clip.preparation.cue_out.get_or_insert(0.);
                                ui.add(egui::DragValue::new(end).range(0. ..=864000.).speed(0.1));
                                ui.add(
                                    egui::DragValue::new(&mut clip.gain_db)
                                        .range(-60. ..=12.)
                                        .suffix(" dB"),
                                );
                            });
                            let (r, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 30.),
                                egui::Sense::hover(),
                            );
                            ui.painter()
                                .rect_filled(r, 2., ui.visuals().extreme_bg_color);
                            if let Ok(duration) = clip.duration() {
                                let mut bar = r;
                                bar.min.x += r.width() * (clip.start / extent) as f32;
                                bar.max.x = r.left()
                                    + r.width() * ((clip.start + duration) / extent) as f32;
                                ui.painter()
                                    .rect_filled(bar, 2., ui.visuals().selection.bg_fill);
                                ui.painter().text(
                                    bar.center(),
                                    egui::Align2::CENTER_CENTER,
                                    format!("{:.2}s to {:.2}s", clip.start, clip.start + duration),
                                    egui::FontId::monospace(12.),
                                    ui.visuals().text_color(),
                                );
                            }
                        });
                    });
                }
            });
        if let Some(i) = remove {
            self.clips.remove(i);
        }
        ui.label(format!(
            "Timeline length: {extent:.2} seconds / bars show clip placement, not waveforms"
        ));
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
        ui.weak("320 kbps MP3 / limiter / existing files never overwritten. Individual exports remove leading timeline silence. Beat sync, fades and waveform editing of this arrangement are not yet available.");
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
