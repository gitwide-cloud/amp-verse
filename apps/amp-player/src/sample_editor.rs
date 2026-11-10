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
    isolated: bool,
    parts: [bool; 3],
    job: Option<mpsc::Receiver<Result<PathBuf, String>>>,
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
        ui.checkbox(&mut self.isolated, "Isolate stems");
        if self.isolated {
            ui.horizontal(|ui| {
                for (i, name) in ["Vocals", "Instruments", "Beat"].iter().enumerate() {
                    ui.checkbox(&mut self.parts[i], *name);
                }
            });
            ui.weak("First preview separates this clip locally using the optional stem add-on.");
        }
        let length = end - start;
        let valid = input.is_some()
            && length > 0.
            && length <= 30.
            && (!self.isolated
                || (self.parts.iter().any(|p| *p) && crate::stems::runtime().is_some()));
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
            let parts = self.isolated.then_some(self.parts);
            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            self.status = "Rendering sample…".into();
            std::thread::spawn(move || {
                let _ = tx.send(render(&root, &source, start, length, parts, &filter));
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
fn run(command: &mut std::process::Command) -> Result<(), String> {
    let result = command.output().map_err(|e| e.to_string())?;
    if result.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&result.stderr)
            .chars()
            .take(2000)
            .collect())
    }
}
fn render(
    root: &Path,
    source: &Path,
    start: f64,
    length: f64,
    parts: Option<[bool; 3]>,
    filter: &str,
) -> Result<PathBuf, String> {
    if !start.is_finite()
        || start < 0.
        || !length.is_finite()
        || length <= 0.
        || length > 30.
        || parts.is_some_and(|p| !p.iter().any(|v| *v))
    {
        return Err("Choose a valid selection up to 30 seconds and at least one stem".into());
    }
    let stage = tempfile::tempdir().map_err(|e| e.to_string())?;
    let clip = stage.path().join("clip.wav");
    run(amp_core::command("ffmpeg")
        .args(["-nostdin", "-v", "error", "-ss"])
        .arg(start.to_string())
        .arg("-i")
        .arg(source)
        .arg("-t")
        .arg(length.to_string())
        .args(["-vn", "-ar", "48000", "-ac", "2", "-c:a", "pcm_s16le"])
        .arg(&clip))?;
    let inputs = if let Some(parts) = parts {
        crate::stems::separate_clip(&stage.path().join("stems"), &clip)?
            .into_iter()
            .zip(parts)
            .filter_map(|(p, on)| on.then_some(p))
            .collect::<Vec<_>>()
    } else {
        vec![clip]
    };
    let directory = if root.as_os_str().is_empty() {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .ok_or("Choose a music library first")?
            .join("Documents/ampdj/samples")
    } else {
        root.join("ampdj/samples")
    };
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let pending = tempfile::Builder::new()
        .suffix(".wav")
        .tempfile_in(&directory)
        .map_err(|e| e.to_string())?;
    let mut command = amp_core::command("ffmpeg");
    command.args(["-nostdin", "-v", "error", "-y"]);
    for input in &inputs {
        command.arg("-i").arg(input);
    }
    let mix = (0..inputs.len())
        .map(|i| format!("[{i}:a]"))
        .collect::<String>();
    command
        .arg("-filter_complex")
        .arg(format!(
            "{mix}amix=inputs={}:normalize=0,{filter}[out]",
            inputs.len()
        ))
        .args([
            "-map",
            "[out]",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(pending.path());
    run(&mut command)?;
    let output = directory.join(format!("sample-{}.wav", amp_core::coordination::token()));
    pending
        .persist_noclobber(&output)
        .map_err(|e| e.to_string())?;
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
    fn clip_export_preserves_source_and_respects_selection() {
        let root = tempfile::tempdir().unwrap();
        let source = fixture(root.path());
        let original = std::fs::read(&source).unwrap();
        let output = render(root.path(), &source, 0.5, 0.75, None, "anull").unwrap();
        assert!(output.starts_with(root.path().join("ampdj/samples")));
        assert!((audio_duration(&output) - 0.75).abs() < 0.001);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert!(render(root.path(), &source, 0., 31., None, "anull").is_err());
        assert!(render(root.path(), &source, 0., 1., Some([false; 3]), "anull").is_err());
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
            Some([true, false, false]),
            "anull",
        )
        .unwrap();
        assert!((audio_duration(&output) - 1.).abs() < 0.001);
    }
}
