//! Non-destructive clip rendering and direct sampler handoff.
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::mpsc,
};
pub struct Delivery {
    pub path: PathBuf,
    pub pad: usize,
    pub right: bool,
    pub name: String,
    pub looping: bool,
}
#[derive(Default)]
pub struct Editor {
    name: String,
    pad: usize,
    right: bool,
    looping: bool,

    job: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    target: Option<Delivery>,
    delivery: Option<Delivery>,
    preview: Option<Child>,
    preview_file: Option<PathBuf>,
    status: String,
}
impl Drop for Editor {
    fn drop(&mut self) {
        self.stop();
    }
}
impl Editor {
    pub fn stop(&mut self) {
        if let Some(c) = self.cancel.take() {
            c.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(mut child) = self.preview.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(path) = self.preview_file.take() {
            let _ = std::fs::remove_file(path);
        }
    }
    pub fn take(&mut self) -> Option<Delivery> {
        self.delivery.take()
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok(result) => {
                    self.job = None;
                    match result {
                        Ok(path) => {
                            if let Some(mut target) = self.target.take() {
                                target.path = path;
                                self.status = format!(
                                    "Sent to sampler {} · pad {}",
                                    if target.right { "B" } else { "A" },
                                    target.pad + 1
                                );
                                self.delivery = Some(target);
                            } else {
                                self.stop();
                                match amp_core::command("ffplay")
                                    .args(["-nodisp", "-autoexit", "-loglevel", "error"])
                                    .arg(&path)
                                    .stdout(Stdio::null())
                                    .stderr(Stdio::null())
                                    .spawn()
                                {
                                    Ok(child) => {
                                        self.preview = Some(child);
                                        self.preview_file = Some(path);
                                        self.status = "Previewing sample".into();
                                    }
                                    Err(e) => self.status = e.to_string(),
                                }
                            }
                        }
                        Err(e) => {
                            self.target = None;
                            self.status = e;
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.job = None;
                    self.target = None;
                    self.status = "Sample rendering interrupted".into();
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(150))
                }
            }
        }
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        root: &Path,
        input: Option<&Path>,
        start: f64,
        end: f64,
        filter: String,
        processing: amp_core::lab_processing::Processing,
    ) {
        ui.heading("Quick sample editor");
        ui.label("Use the waveform IN / OUT selection above, then preview or send to a pad.");
        ui.horizontal_wrapped(|ui| {
            ui.label("Sample name");
            ui.text_edit_singleline(&mut self.name);
            ui.selectable_value(&mut self.right, false, "Sampler A");
            ui.selectable_value(&mut self.right, true, "Sampler B");
            egui::ComboBox::from_id_salt("sample-destination-pad")
                .selected_text(format!("Pad {}", self.pad + 1))
                .show_ui(ui, |ui| {
                    for pad in 0..9 {
                        ui.selectable_value(&mut self.pad, pad, format!("Pad {}", pad + 1));
                    }
                });
            ui.checkbox(&mut self.looping, "Loop");
        });
        ui.weak("Uses the waveform editor’s stem, EQ and filter settings above.");
        let length = end - start;
        let valid = input.is_some()
            && length > 0.
            && length <= 30.
            && (!processing.stems_changed() || crate::onnx_stem_worker::configured());
        if length > 30. {
            ui.weak("Select up to 30 seconds for a sampler pad.");
        }
        let mut send = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    valid && self.job.is_none(),
                    egui::Button::new("Preview sample"),
                )
                .clicked()
            {
                send = Some(false);
            }
            if ui
                .add_enabled(
                    valid && self.job.is_none(),
                    egui::Button::new("Export to sampler"),
                )
                .clicked()
            {
                send = Some(true);
            }
            if ui.button("Stop preview").clicked() {
                self.stop();
            }
        });
        if let Some(to_pad) = send {
            self.stop();
            let name = if self.name.trim().is_empty() {
                input
                    .unwrap()
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else {
                self.name.trim().into()
            };
            self.target = to_pad.then(|| Delivery {
                path: PathBuf::new(),
                pad: self.pad,
                right: self.right,
                name,
                looping: self.looping,
            });
            let source = input.unwrap().to_path_buf();
            let root = root.to_path_buf();

            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            self.status = "Rendering sample…".into();
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            self.cancel = Some(cancel.clone());
            std::thread::spawn(move || {
                let result =
                    render_with_cancel(&root, &source, start, length, processing, &filter, cancel);
                if let Err(e) = tx.send(result)
                    && let Ok(path) = e.0
                {
                    let _ = std::fs::remove_file(path);
                }
            });
        }
        if self.job.is_some() {
            ui.spinner();
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(150));
        }
        ui.label(&self.status);
    }
}

#[cfg(test)]
fn run(command: &mut std::process::Command) -> Result<(), String> {
    let p = command.output().map_err(|e| e.to_string())?;
    if p.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&p.stderr).into_owned())
    }
}
#[cfg(test)]
fn render(
    root: &Path,
    source: &Path,
    start: f64,
    length: f64,
    p: amp_core::lab_processing::Processing,
    filter: &str,
) -> Result<PathBuf, String> {
    render_with_cancel(
        root,
        source,
        start,
        length,
        p,
        filter,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
}
fn render_with_cancel(
    root: &Path,
    source: &Path,
    start: f64,
    length: f64,
    processing: amp_core::lab_processing::Processing,
    filter: &str,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<PathBuf, String> {
    if !start.is_finite()
        || start < 0.
        || !length.is_finite()
        || length <= 0.
        || length > 30.
        || processing.validate().is_err()
    {
        return Err("Choose a valid selection up to 30 seconds and at least one stem".into());
    }
    let directory = if root.as_os_str().is_empty() {
        std::env::var_os("AMP_DOCUMENTS_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .map(|home| PathBuf::from(home).join("Documents"))
            })
            .ok_or("Choose a music library first")?
            .join("ampdj/samples")
    } else {
        root.join("ampdj/samples")
    };
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let output = directory.join(format!("sample-{}.wav", amp_core::coordination::token()));
    crate::lab_audio::render(source, start, length, processing, filter, &output, cancel)?;

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn audio_duration(path: &Path) -> f64 {
        let r = amp_core::command("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(path)
            .output()
            .unwrap();
        assert!(r.status.success());
        String::from_utf8(r.stdout).unwrap().trim().parse().unwrap()
    }
    fn fixture(root: &Path) -> PathBuf {
        let path = root.join("source.wav");
        run(amp_core::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-ar",
                "48000",
                "-ac",
                "2",
            ])
            .arg(&path))
        .unwrap();
        path
    }
    #[test]
    #[ignore = "requires FFmpeg and ffprobe; explicitly run by codec CI"]
    fn clip_export_preserves_source_and_respects_selection() {
        let root = tempfile::tempdir().unwrap();
        let source = fixture(root.path());
        let original = std::fs::read(&source).unwrap();
        let output = render(root.path(), &source, 0.5, 0.75, Default::default(), "anull").unwrap();
        assert!(output.starts_with(root.path().join("ampdj/samples")));
        assert!((audio_duration(&output) - 0.75).abs() < 0.001);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert!(render(root.path(), &source, 0., 31., Default::default(), "anull").is_err());
        assert!(
            render(
                root.path(),
                &source,
                0.,
                1.,
                amp_core::lab_processing::Processing {
                    filter: f32::NAN,
                    ..Default::default()
                },
                "anull"
            )
            .is_err()
        );
    }
    #[test]
    #[ignore = "requires installed stem runtime and model"]
    fn isolated_clip_exports_playable_audio() {
        let root = tempfile::tempdir().unwrap();
        let source = fixture(root.path());
        let output = render(
            root.path(),
            &source,
            0.25,
            1.,
            amp_core::lab_processing::Processing {
                stems: [0., -1., -1.],
                ..Default::default()
            },
            "anull",
        )
        .unwrap();
        assert!((audio_duration(&output) - 1.).abs() < 0.001);
    }
}
