//! Opt-in recorder for authorized local input and monitor sources.
//! FFmpeg's device capture is intentionally explicit; no service-stream extraction.
use eframe::egui;
use std::{
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Instant,
};

pub(super) struct Recorder {
    source: String,
    child: Option<Child>,
    output: Option<PathBuf>,
    started: Option<Instant>,
    status: String,
    completed: Option<PathBuf>,
}
impl Default for Recorder {
    fn default() -> Self {
        Self {
            source: if cfg!(target_os = "linux") {
                "default".into()
            } else {
                String::new()
            },
            child: None,
            output: None,
            started: None,
            status: "Idle".into(),
            completed: None,
        }
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        if self.child.is_some() {
            self.finish();
        }
    }
}
impl Recorder {
    pub(super) fn take_completed(&mut self) -> Option<PathBuf> {
        self.completed.take()
    }
    fn start(&mut self) {
        if self.child.is_some() { return; }
        if self.source.trim().is_empty() {
            self.status = "Enter an audio capture device/source first".into();
            return;
        }
        let Some(output) = rfd::FileDialog::new()
            .add_filter("WAV", &["wav"])
            .set_file_name("amp-lab-recording.wav")
            .save_file() else { return; };
        if output.exists() {
            self.status = "Choose a new file name: recordings never overwrite existing audio".into();
            return;
        }
        let mut cmd = amp_core::command("ffmpeg");
        cmd.args(["-hide_banner", "-loglevel", "error", "-n", "-y"]);
        #[cfg(target_os = "windows")]
        cmd.args(["-f", "dshow", "-i"])
            .arg(format!("audio={}", self.source.trim()));
        #[cfg(target_os = "linux")]
        cmd.args(["-f", "pulse", "-i"]).arg(self.source.trim());
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            self.status = "Recording is currently supported on Windows and Linux only".into();
            return;
        }
        cmd.args(["-vn", "-ac", "2", "-ar", "48000", "-c:a", "pcm_s24le"])
            .arg(&output)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match cmd.spawn() {
            Ok(child) => {
                self.child = Some(child);
                self.output = Some(output);
                self.started = Some(Instant::now());
                self.status = "Recording (24-bit WAV, 48 kHz)".into();
            }
            Err(e) => self.status = format!("Could not start FFmpeg: {e}"),
        }
    }
    fn finish(&mut self) {
        let Some(mut child) = self.child.take() else { return; };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(b"q\n");
            let _ = stdin.flush();
        }
        let result = child.wait();
        self.started = None;
        if result.is_ok_and(|s| s.success()) {
            if let Some(path) = self.output.take() {
                if std::fs::metadata(&path).is_ok_and(|m| m.len() > 44) {
                    self.status = format!("Recorded {}", path.display());
                    self.completed = Some(path);
                    return;
                }
            }
        }
        self.status = "Capture failed or source unavailable. Check input device and FFmpeg.".into();
        self.output = None;
    }
    fn refresh(&mut self) {
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(_)) => {
                    self.child = None;
                    self.started = None;
                    self.output = None;
                    self.status = "Capture ended unexpectedly; check source name and audio permissions".into();
                }
                Ok(None) => {}
                Err(e) => self.status = format!("Recorder status error: {e}"),
            }
        }
    }
    pub(super) fn show(&mut self, ui: &mut egui::Ui) {
        self.refresh();
        egui::CollapsingHeader::new("Record audio (authorized sources)")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Capture from a locally configured audio input, then edit in AMP Lab. Record only audio you have permission to capture.");
                #[cfg(target_os = "windows")]
                ui.label("Windows: enter the DirectShow audio device name (for example, Stereo Mix if your sound driver provides it). FFmpeg -list_devices true -f dshow -i dummy lists devices. Universal WASAPI loopback is not yet implemented.");
                #[cfg(target_os = "linux")]
                ui.label("Linux: enter a PulseAudio/PipeWire source. For desktop output, use the appropriate monitor source from 'pactl list short sources'; 'default' normally records your default input.");
                #[cfg(not(any(target_os = "windows", target_os = "linux")))]
                ui.label("This version supports Windows and Linux capture only.");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Source:");
                    ui.add_enabled(self.child.is_none(), egui::TextEdit::singleline(&mut self.source).desired_width(260.));
                    if self.child.is_none() {
                        if ui.button("● Record to WAV...").clicked() { self.start(); }
                    } else if ui.button("■ Stop and open in editor").clicked() {
                        self.finish();
                    }
                });
                if let Some(start) = self.started {
                    ui.strong(format!("REC  {:02}:{:02}", start.elapsed().as_secs()/60, start.elapsed().as_secs()%60));
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                }
                ui.label(&self.status);
                ui.weak("Saved losslessly first. Existing AMP Lab trim, fades, normalization, preview and 256 kbps MP3 export apply after recording.");
            });
    }
}
