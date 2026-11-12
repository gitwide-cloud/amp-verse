//! Desktop editing foundation adapted from gitwide-cloud/mp3-lab (MIT).
//! Source: d4a0f4a7c3fc8f990ca5b2f774ea63d4cabe4755; see third-party/mp3-lab/PROVENANCE.md.
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

#[derive(Clone, Copy, PartialEq)]
struct EditState {
    start: f64,
    end: f64,
    fade_in: f64,
    fade_out: f64,
    gain: f64,
    normalize: bool,
}
#[derive(Default)]
pub(super) struct Editor {
    sample: crate::sample_editor::Editor,
    saved_edit: Option<EditState>,
    export_edit: Option<(PathBuf, EditState)>,
    input: Option<PathBuf>,
    duration: f64,
    start: f64,
    end: f64,
    fade_in: f64,
    fade_out: f64,
    gain: f64,
    normalize: bool,
    player: Option<Child>,
    exporting: Option<Receiver<Result<PathBuf, String>>>,
    status: String,
    waveform: Vec<[f32; 2]>,
    wave_job: Option<Receiver<Result<Vec<[f32; 2]>, String>>>,
    peak_db: Option<f64>,
    preview_job: Option<Receiver<Result<PathBuf, String>>>,
    preview_path: Option<PathBuf>,
    started: Option<std::time::Instant>,
    offset: f64,
    preview_start: f64,
    preview_end: f64,
    paused: bool,
    repeat: bool,
    volume: f32,
    initialized: bool,
    library_search: String,
}
impl Drop for Editor {
    fn drop(&mut self) {
        self.stop();
    }
}
impl Editor {
    pub fn take_sample(&mut self) -> Option<crate::sample_editor::Delivery> {
        self.sample.take()
    }
    pub(super) fn stop(&mut self) {
        self.sample.stop();
        self.kill_player();
        self.preview_job = None;
        self.started = None;
        self.offset = 0.0;
        self.paused = false;
        if let Some(p) = self.preview_path.take() {
            let _ = std::fs::remove_file(p);
        }
    }
    fn kill_player(&mut self) {
        if let Some(mut p) = self.player.take() {
            let _ = p.kill();
            let _ = p.wait();
        }
    }
    fn elapsed(&self) -> f64 {
        self.offset
            + self
                .started
                .map(|s| s.elapsed().as_secs_f64())
                .unwrap_or(0.0)
    }
    fn pause(&mut self) {
        if self.player.is_some() {
            self.offset = self.elapsed();
            self.started = None;
            self.kill_player();
            self.paused = true;
            self.status = "Paused".into();
        } else if self.paused {
            self.resume();
        }
    }
    fn resume(&mut self) {
        let Some(path) = &self.preview_path else {
            return;
        };
        match command("ffplay")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nodisp",
                "-autoexit",
                "-volume",
            ])
            .arg(format!("{:.0}", self.volume))
            .arg("-ss")
            .arg(format!("{:.5}", self.offset))
            .arg("-i")
            .arg(path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(p) => {
                self.player = Some(p);
                self.started = Some(std::time::Instant::now());
                self.paused = false;
                self.status = "Playing preview".into();
            }
            Err(e) => self.status = format!("Playback unavailable: {e}"),
        }
    }
    fn seek_preview(&mut self, delta: f64) {
        if self.preview_path.is_some() {
            let playing = self.player.is_some();
            self.offset = (self.elapsed() + delta)
                .clamp(0.0, (self.preview_end - self.preview_start - 0.01).max(0.0));
            self.started = None;
            self.kill_player();
            if playing {
                self.resume();
            }
        }
    }
    fn length(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }
    fn clamp(&mut self) {
        self.start = self.start.clamp(0.0, self.duration);
        self.end = self.end.clamp(self.start, self.duration);
        self.fade_in = self.fade_in.clamp(0.0, self.length());
        self.fade_out = self
            .fade_out
            .clamp(0.0, (self.length() - self.fade_in).max(0.0));
    }
    fn open(&mut self) {
        let Some(p) = rfd::FileDialog::new()
            .add_filter(
                "Audio",
                &["mp3", "wav", "flac", "m4a", "aac", "ogg", "opus"],
            )
            .pick_file()
        else {
            return;
        };
        self.load_path(p);
    }
    fn edit_state(&self) -> EditState {
        EditState {
            start: self.start,
            end: self.end,
            fade_in: self.fade_in,
            fade_out: self.fade_out,
            gain: self.gain,
            normalize: self.normalize,
        }
    }
    fn unsaved(&self) -> bool {
        self.input.is_some() && self.saved_edit != Some(self.edit_state())
    }
    pub(super) fn load_path(&mut self, p: PathBuf) -> bool {
        if self.exporting.is_some() {
            self.status = "Wait for the current export before opening another file.".into();
            return false;
        }
        let approved = !self.unsaved() || rfd::MessageDialog::new()
            .set_title("Unsaved AMP Lab edits")
            .set_description("Replace the current edits? Choose No to keep editing and export first. Downloaded MP3s remain saved and can be opened later.")
            .set_buttons(rfd::MessageButtons::YesNo).show() == rfd::MessageDialogResult::Yes;
        self.load_path_with_permission(p, approved)
    }
    fn load_path_with_permission(&mut self, p: PathBuf, approved: bool) -> bool {
        if self.exporting.is_some() || !approved {
            self.status =
                "Current edits retained. Export first, then open the downloaded MP3 when ready."
                    .into();
            return false;
        }
        match duration(&p) {
            Ok(d) if d > 0.0 && d.is_finite() => {
                self.stop();
                self.duration = d;
                self.start = 0.0;
                self.end = d;
                self.fade_in = 0.0;
                self.fade_out = 0.0;
                self.gain = 0.0;
                self.normalize = false;
                self.input = Some(p.clone());
                self.saved_edit = Some(self.edit_state());
                self.status = "Reading waveform…".into();
                self.waveform.clear();
                self.peak_db = None;
                let (tx, rx) = mpsc::channel();
                self.wave_job = Some(rx);
                thread::spawn(move || {
                    let _ = tx.send(read_waveform(&p, d));
                });
                true
            }
            Ok(_) => {
                self.status = "Invalid track duration".into();
                false
            }
            Err(e) => {
                self.status = e;
                false
            }
        }
    }
    fn filter(&self) -> String {
        let mut f = vec!["asetpts=PTS-STARTPTS".to_string()];
        if self.normalize {
            f.push("loudnorm=I=-16:TP=-3:LRA=11".into())
        }
        if self.gain.abs() > 0.001 {
            f.push(format!("volume={:.2}dB", self.gain))
        }
        if self.fade_in > 0.0 {
            f.push(format!("afade=t=in:st=0:d={:.4}", self.fade_in))
        }
        if self.fade_out > 0.0 {
            f.push(format!(
                "afade=t=out:st={:.4}:d={:.4}",
                self.length() - self.fade_out,
                self.fade_out
            ))
        }
        f.push("alimiter=limit=0.95:attack=5:release=50:level=0".into());
        f.join(",")
    }
    fn play(&mut self, edited: bool) {
        self.stop();
        let Some(p) = self.input.clone() else { return };
        if self.length() <= 0.0 {
            return;
        }
        let filter = if edited {
            self.filter()
        } else {
            "anull".into()
        };
        self.preview_start = self.start;
        self.preview_end = self.end;
        let start = self.start;
        let length = self.length();
        let (tx, rx) = mpsc::channel();
        self.preview_job = Some(rx);
        self.status = if edited {
            "Preparing edited preview…".into()
        } else {
            "Preparing original preview…".into()
        };
        thread::spawn(move || {
            let output = std::env::temp_dir().join(format!(
                "mp3-lab-preview-{}-{}.wav",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let result = command("ffmpeg")
                .args([
                    "-hide_banner",
                    "-nostdin",
                    "-n",
                    "-loglevel",
                    "error",
                    "-ss",
                ])
                .arg(format!("{start:.5}"))
                .arg("-i")
                .arg(p)
                .arg("-t")
                .arg(format!("{length:.5}"))
                .args(["-vn", "-af"])
                .arg(filter)
                .args(["-c:a", "pcm_s16le", "-ar", "48000"])
                .arg(&output)
                .output();
            let result = match result {
                Ok(r) if r.status.success() => Ok(output.clone()),
                Ok(r) => Err(String::from_utf8_lossy(&r.stderr).into_owned()),
                Err(e) => Err(e.to_string()),
            };
            let failed = result.is_err();
            if tx.send(result).is_err() || failed {
                let _ = std::fs::remove_file(output);
            }
        });
    }
    fn export(&mut self) {
        let Some(input) = self.input.clone() else {
            return;
        };
        if self.length() <= 0.0 {
            return;
        }
        let name = format!(
            "{}_clip.mp3",
            input.file_stem().unwrap_or_default().to_string_lossy()
        );
        let Some(output) = rfd::FileDialog::new()
            .add_filter("MP3", &["mp3"])
            .set_file_name(&name)
            .save_file()
        else {
            return;
        };
        if output.exists() {
            self.status = "Output already exists; choose a new filename".into();
            return;
        }
        if input == output
            || (output.exists()
                && std::fs::canonicalize(&input).ok() == std::fs::canonicalize(&output).ok())
        {
            self.status = "Output cannot overwrite source".into();
            return;
        }
        self.stop();
        let (tx, rx) = mpsc::channel();
        self.exporting = Some(rx);
        self.export_edit = Some((input.clone(), self.edit_state()));
        let start = self.start;
        let length = self.length();
        let filters = self.filter();
        self.status = "Exporting 256 kbps MP3".into();
        thread::spawn(move || {
            let result = command("ffmpeg")
                .args([
                    "-hide_banner",
                    "-nostdin",
                    "-n",
                    "-loglevel",
                    "error",
                    "-ss",
                ])
                .arg(format!("{start:.5}"))
                .arg("-i")
                .arg(&input)
                .arg("-t")
                .arg(format!("{length:.5}"))
                .args(["-vn", "-af"])
                .arg(filters)
                .args([
                    "-codec:a",
                    "libmp3lame",
                    "-b:a",
                    "256k",
                    "-id3v2_version",
                    "3",
                ])
                .arg(&output)
                .output();
            let outcome = match result {
                Ok(r) if r.status.success() => Ok(output),
                Ok(r) => Err(String::from_utf8_lossy(&r.stderr).to_string()),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(outcome);
        });
    }
}
fn command(name: &str) -> Command {
    crate::lab_youtube::audio_command(name)
}
pub(super) fn duration(p: &Path) -> Result<f64, String> {
    let o = command("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(p)
        .output()
        .map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).to_string());
    }
    String::from_utf8_lossy(&o.stdout)
        .trim()
        .parse()
        .map_err(|_| "Duration unavailable".into())
}
fn clock(sec: f64) -> String {
    let m = (sec / 60.0).floor() as u64;
    format!("{}:{:05.2}", m, sec - (m * 60) as f64)
}
pub(super) fn read_waveform(path: &Path, duration: f64) -> Result<Vec<[f32; 2]>, String> {
    use std::io::Read;
    let info = command("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=channels",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    let mono = String::from_utf8_lossy(&info.stdout).trim() == "1";
    let mut child = command("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args([
            "-af",
            if mono {
                "pan=stereo|c0=c0|c1=c0"
            } else {
                "anull"
            },
            "-vn",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-f",
            "f32le",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stream = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut frame = [0u8; 8];
    let mut bins = vec![[0.0f32; 2]; 1600];
    let mut index = 0usize;
    while stream.read_exact(&mut frame).is_ok() {
        let bin = ((index as f64 / (duration * 48000.0)) * 1600.0) as usize;
        index += 1;
        if bin < 1600 {
            for channel in 0..2 {
                let value =
                    f32::from_le_bytes(frame[channel * 4..channel * 4 + 4].try_into().unwrap())
                        .abs();
                if value.is_finite() {
                    bins[bin][channel] = bins[bin][channel].max(value);
                }
            }
        }
    }
    if !child.wait().map_err(|e| e.to_string())?.success() {
        return Err("Waveform decoding failed".into());
    }
    Ok(bins)
}

impl Editor {
    pub(super) fn poll(&mut self, ctx: &egui::Context) {
        self.sample.poll(ctx);
        if let Some(rx) = &self.exporting {
            match rx.try_recv() {
                Ok(result) => {
                    self.status = match result {
                        Ok(p) => {
                            if let Some((source, state)) = self.export_edit.take()
                                && self.input.as_ref() == Some(&source)
                            {
                                self.saved_edit = Some(state);
                            }
                            format!("Saved {}", p.display())
                        }
                        Err(e) => format!("Export failed: {e}"),
                    };
                    self.exporting = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.status = "Export interrupted".into();
                    self.exporting = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100))
                }
            }
        }
        if let Some(rx) = &self.wave_job {
            match rx.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(w) => {
                            let peak = w
                                .iter()
                                .flat_map(|p| p.iter())
                                .copied()
                                .fold(0.0f32, f32::max);
                            self.peak_db = Some(if peak > 0.0 {
                                20.0 * (peak as f64).log10()
                            } else {
                                -120.0
                            });
                            self.waveform = w;
                            self.status = "Ready.".into();
                        }
                        Err(e) => self.status = e,
                    }
                    self.wave_job = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.wave_job = None;
                    self.status = "Waveform interrupted".into();
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100))
                }
            }
        }
        if let Some(rx) = &self.preview_job {
            match rx.try_recv() {
                Ok(result) => {
                    self.preview_job = None;
                    match result {
                        Ok(p) => {
                            self.preview_path = Some(p);
                            self.offset = 0.0;
                            self.resume();
                        }
                        Err(e) => self.status = format!("Preview failed: {e}"),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.preview_job = None;
                    self.status = "Preview interrupted".into();
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100))
                }
            }
        }
        if self.player.is_some() {
            ctx.request_repaint_after(Duration::from_millis(33));
            let done = self
                .player
                .as_mut()
                .is_some_and(|p| matches!(p.try_wait(), Ok(Some(_))));
            if done {
                self.kill_player();
                self.started = None;
                self.offset = 0.0;
                if self.repeat {
                    self.resume();
                } else {
                    self.status = "Preview finished".into();
                }
            }
        }
    }
}

impl Editor {
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        root: &Path,
        library: Option<&amp_core::Library>,
    ) {
        if !self.initialized {
            self.volume = 80.;
            self.initialized = true;
        }
        self.poll(ui.ctx());
        ui.horizontal_wrapped(|ui| {
            if ui.button("Open audio...").clicked() {
                self.open();
            }
            let mut load = None;
            ui.menu_button("From Library", |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.library_search).hint_text("Search songs"),
                );
                egui::ScrollArea::vertical()
                    .max_height(220.)
                    .show(ui, |ui| {
                        if let Some(library) = library {
                            let query = self.library_search.to_lowercase();
                            for track in library
                                .tracks
                                .iter()
                                .filter(|t| t.title.to_lowercase().contains(&query))
                                .take(200)
                            {
                                if ui.button(&track.title).clicked() {
                                    load = Some(track.clone());
                                    ui.close();
                                }
                            }
                        } else {
                            ui.weak("Add music sources in Library first.");
                        }
                    });
            });
            if let Some(track) = load {
                match amp_core::resolve(root, &track.path) {
                    Ok(path) => {
                        if self.load_path(path) {
                            self.start = track.preparation.cue_in;
                            self.end = track.preparation.cue_out.unwrap_or(self.duration);
                            self.clamp();
                            self.saved_edit = Some(self.edit_state());
                        }
                    }
                    Err(error) => self.status = error.to_string(),
                }
            }
            ui.add(
                egui::Label::new(
                    self.input
                        .as_ref()
                        .map(|p| {
                            p.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        })
                        .unwrap_or_else(|| "No audio loaded".into()),
                )
                .truncate(),
            );
        });
        crate::skin::panel(ui, false, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.strong("Preview");
                ui.add_enabled_ui(
                    self.input.is_some() && self.length() > 0. && self.preview_job.is_none(),
                    |ui| {
                        if ui.button("Original").clicked() {
                            self.play(false);
                        }
                        if ui.button("Processed").clicked() {
                            self.play(true);
                        }
                    },
                );
                if ui
                    .add_enabled(
                        self.player.is_some() || self.paused,
                        egui::Button::new(if self.paused { "Resume" } else { "Pause" }),
                    )
                    .clicked()
                {
                    self.pause();
                }
                if ui.button("Stop").clicked() {
                    self.stop();
                }
                ui.checkbox(&mut self.repeat, "Repeat");
                ui.spacing_mut().slider_width = 100.;
                ui.add(egui::Slider::new(&mut self.volume, 0. ..=100.).text("Volume"));
                ui.weak("Volume applies on next preview/resume");
            });
        });
        ui.add_space(6.);
        self.waveform_ui(ui);
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Duration {}", clock(self.duration)));
            ui.label(format!("Selection {}", clock(self.length())));
            if let Some(peak) = self.peak_db {
                ui.weak(format!("Peak {peak:.1} dBFS"));
            }
        });
        ui.add_space(8.);
        let busy = self.exporting.is_some();
        ui.add_enabled_ui(self.input.is_some() && !busy, |ui| {
            ui.columns(3, |columns| {
                crate::skin::panel(&mut columns[0], false, |ui| {
                    ui.set_min_size(egui::vec2(ui.available_width(), 165.));
                    ui.strong("Trim / selection");
                    ui.label("IN (seconds)");
                    ui.add(
                        egui::DragValue::new(&mut self.start)
                            .speed(0.01)
                            .range(0. ..=self.duration),
                    );
                    ui.label("OUT (seconds)");
                    ui.add(
                        egui::DragValue::new(&mut self.end)
                            .speed(0.01)
                            .range(0. ..=self.duration),
                    );
                    if ui.button("Select whole file").clicked() {
                        self.start = 0.;
                        self.end = self.duration;
                    }
                });
                crate::skin::panel(&mut columns[1], false, |ui| {
                    ui.set_min_size(egui::vec2(ui.available_width(), 165.));
                    ui.strong("Processing");
                    ui.add(
                        egui::DragValue::new(&mut self.fade_in)
                            .speed(0.05)
                            .range(0. ..=self.duration)
                            .prefix("Fade in ")
                            .suffix(" s"),
                    );
                    ui.add(
                        egui::DragValue::new(&mut self.fade_out)
                            .speed(0.05)
                            .range(0. ..=self.duration)
                            .prefix("Fade out ")
                            .suffix(" s"),
                    );
                    ui.add(
                        egui::DragValue::new(&mut self.gain)
                            .speed(0.1)
                            .range(-60. ..=12.)
                            .prefix("Gain ")
                            .suffix(" dB"),
                    );
                    ui.checkbox(&mut self.normalize, "Normalize (-16 LUFS)");
                    ui.weak("Limiter included");
                });
                crate::skin::panel(&mut columns[2], false, |ui| {
                    ui.set_min_size(egui::vec2(ui.available_width(), 165.));
                    ui.strong("Export");
                    ui.label("MP3 · 256 kbps");
                    if ui
                        .add_enabled(self.length() > 0., egui::Button::new("Export selection..."))
                        .clicked()
                    {
                        self.clamp();
                        self.export();
                    }
                    ui.weak("New output file; source is preserved.");
                });
            });
        });
        self.clamp();
        ui.separator();
        self.sample.show(
            ui,
            root,
            self.input.as_deref(),
            self.start,
            self.end,
            self.filter(),
        );
        ui.separator();
        ui.label(&self.status);
        if self.wave_job.is_some() || self.preview_job.is_some() || busy {
            ui.spinner();
        }
    }
    fn waveform_ui(&mut self, ui: &mut egui::Ui) {
        ui.strong("Waveform editor");
        let palette = crate::theme::current(ui.ctx()).palette();
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 140.), egui::Sense::click());
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("lab-waveform-rect"), rect));
        ui.painter().rect_filled(rect, 6., palette.background);
        if self.duration <= 0. || self.waveform.is_empty() {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if self.wave_job.is_some() {
                    "Decoding waveform..."
                } else {
                    "Open audio or choose a song from Library"
                },
                egui::FontId::proportional(16.),
                palette.muted,
            );
            return;
        }
        let x = |seconds: f64| {
            rect.left() + rect.width() * (seconds / self.duration).clamp(0., 1.) as f32
        };
        let selection = egui::Rect::from_min_max(
            egui::pos2(x(self.start), rect.top()),
            egui::pos2(x(self.end), rect.bottom()),
        );
        ui.painter()
            .rect_filled(selection, 0., palette.accent.gamma_multiply(0.18));
        for (i, peak) in self.waveform.iter().enumerate() {
            let x = rect.left() + rect.width() * i as f32 / self.waveform.len() as f32;
            let amplitude = peak[0].max(peak[1]).clamp(0., 1.) * rect.height() * 0.44;
            ui.painter().line_segment(
                [
                    egui::pos2(x, rect.center().y - amplitude),
                    egui::pos2(x, rect.center().y + amplitude),
                ],
                egui::Stroke::new(1., palette.signal),
            );
        }
        for i in 0..=4 {
            let seconds = self.duration * i as f64 / 4.;
            let at = x(seconds);
            ui.painter().text(
                egui::pos2(
                    at + if i == 0 {
                        4.
                    } else if i == 4 {
                        -4.
                    } else {
                        0.
                    },
                    rect.bottom() - 3.,
                ),
                if i == 0 {
                    egui::Align2::LEFT_BOTTOM
                } else if i == 4 {
                    egui::Align2::RIGHT_BOTTOM
                } else {
                    egui::Align2::CENTER_BOTTOM
                },
                clock(seconds),
                egui::FontId::monospace(11.),
                palette.text,
            );
        }
        for end in [false, true] {
            let seconds = if end { self.end } else { self.start };
            let at = x(seconds);
            let handle = egui::Rect::from_center_size(
                egui::pos2(at, rect.center().y),
                egui::vec2(14., rect.height()),
            );
            let drag = ui.interact(
                handle.intersect(rect),
                ui.scope_id().with(("lab-trim", end)),
                egui::Sense::drag(),
            );
            if drag.dragged()
                && let Some(pointer) = drag.interact_pointer_pos()
            {
                let value =
                    ((pointer.x - rect.left()) / rect.width()).clamp(0., 1.) as f64 * self.duration;
                if end {
                    self.end = value.max(self.start);
                } else {
                    self.start = value.min(self.end);
                }
            }
            ui.painter().line_segment(
                [egui::pos2(at, rect.top()), egui::pos2(at, rect.bottom())],
                egui::Stroke::new(2., palette.accent),
            );
            ui.painter().text(
                egui::pos2(at + if end { -4. } else { 4. }, rect.top() + 3.),
                if end {
                    egui::Align2::RIGHT_TOP
                } else {
                    egui::Align2::LEFT_TOP
                },
                if end { "OUT" } else { "IN" },
                egui::FontId::proportional(12.),
                palette.accent,
            );
        }
        if self.preview_path.is_some() {
            let at = x(self.preview_start + self.elapsed());
            ui.painter().line_segment(
                [egui::pos2(at, rect.top()), egui::pos2(at, rect.bottom())],
                egui::Stroke::new(1.5, egui::Color32::LIGHT_RED),
            );
            if response.clicked()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let target = ((pointer.x - rect.left()) / rect.width()) as f64 * self.duration;
                self.seek_preview(target - self.preview_start - self.elapsed());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declined_replacement_and_active_export_keep_unsaved_work() {
        let mut editor = Editor::default();
        editor.input = Some("original.mp3".into());
        editor.duration = 10.;
        editor.end = 10.;
        editor.saved_edit = Some(editor.edit_state());
        assert!(!editor.unsaved());
        editor.gain = 3.;
        assert!(editor.unsaved());
        assert!(!editor.load_path_with_permission("downloaded.mp3".into(), false));
        assert_eq!(editor.input.as_deref(), Some(Path::new("original.mp3")));
        assert_eq!(editor.gain, 3.);
        let (_tx, rx) = mpsc::channel();
        editor.exporting = Some(rx);
        assert!(!editor.load_path_with_permission("downloaded.mp3".into(), true));
        assert!(editor.unsaved());
    }
    #[test]
    #[ignore = "requires AMP_LAB_LOADED_MP3 and FFmpeg; real Lab open pipeline"]
    fn downloaded_mp3_loads_waveform_and_keeps_source() {
        let path = PathBuf::from(std::env::var_os("AMP_LAB_LOADED_MP3").unwrap());
        let bytes = std::fs::read(&path).unwrap();
        let mut editor = Editor::default();
        assert!(editor.load_path_with_permission(path.clone(), true));
        let ctx = egui::Context::default();
        let start = std::time::Instant::now();
        while editor.wave_job.is_some() && start.elapsed() < Duration::from_secs(15) {
            editor.poll(&ctx);
            thread::sleep(Duration::from_millis(30));
        }
        assert!(!editor.waveform.is_empty());
        assert!(editor.duration > 0.);
        assert!(!editor.unsaved());
        editor.fade_in = 0.1;
        editor.normalize = true;
        assert!(editor.unsaved());
        assert!(editor.filter().contains("loudnorm"));
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    #[test]
    fn waveform_trim_handle_drags_update_selection() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        editor.duration = 10.;
        editor.start = 1.;
        editor.end = 9.;
        editor.waveform = vec![[0.3, 0.3]; 1600];
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 300.));
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| editor.waveform_ui(ui),
        );
        out.textures_delta.clear();
        let rect = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("lab-waveform-rect")))
            .unwrap();
        let start = egui::pos2(rect.left() + rect.width() * 0.1, rect.center().y);
        let target = egui::pos2(rect.left() + rect.width() * 0.3, rect.center().y);
        for events in [
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            vec![egui::Event::PointerMoved(target)],
            vec![egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| editor.waveform_ui(ui),
            );
            out.textures_delta.clear();
        }
        assert!(
            (editor.start - 3.).abs() < 0.02,
            "IN handle was not draggable: {}",
            editor.start
        );
        assert_eq!(editor.end, 9.);
    }
    #[test]
    fn selection_keeps_fades_inside_clip() {
        let mut e = Editor::default();
        e.duration = 10.0;
        e.start = 8.0;
        e.end = 20.0;
        e.fade_in = 4.0;
        e.fade_out = 4.0;
        e.clamp();
        assert_eq!(
            (e.start, e.end, e.fade_in, e.fade_out),
            (8.0, 10.0, 2.0, 0.0)
        );
        e.start = 11.0;
        e.clamp();
        assert_eq!(e.length(), 0.0);
    }
    #[test]
    fn processing_order_and_relative_fade_position() {
        let mut e = Editor::default();
        e.start = 30.0;
        e.end = 35.0;
        e.normalize = true;
        e.gain = -6.0;
        e.fade_in = 1.0;
        e.fade_out = 2.0;
        assert_eq!(
            e.filter(),
            "asetpts=PTS-STARTPTS,loudnorm=I=-16:TP=-3:LRA=11,volume=-6.00dB,afade=t=in:st=0:d=1.0000,afade=t=out:st=3.0000:d=2.0000,alimiter=limit=0.95:attack=5:release=50:level=0"
        );
    }
    #[test]
    #[ignore = "requires FFmpeg; enabled explicitly by Linux CI"]
    fn ffmpeg_export_smoke() {
        let dir = std::env::temp_dir().join(format!("mp3-lab-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("source.wav");
        let output = dir.join("clip.mp3");
        assert!(
            command("ffmpeg")
                .args(["-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=8"])
                .arg(&input)
                .output()
                .unwrap()
                .status
                .success()
        );
        let mut e = Editor::default();
        e.start = 2.0;
        e.end = 6.0;
        e.normalize = true;
        e.gain = -3.0;
        e.fade_in = 0.5;
        e.fade_out = 0.5;
        let result = command("ffmpeg")
            .args(["-y", "-ss", "2", "-i"])
            .arg(&input)
            .args(["-t", "4", "-vn", "-af"])
            .arg(e.filter())
            .args(["-codec:a", "libmp3lame", "-b:a", "256k"])
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!((duration(&output).unwrap() - 4.0).abs() < 0.15);
        let wave = read_waveform(&input, 8.0).unwrap();
        assert_eq!(wave.len(), 1600);
        assert!(wave.iter().any(|p| p[0] > 0.1 && p[1] > 0.1));
        let probe = command("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=codec_name,bit_rate",
                "-of",
                "csv=p=0",
            ])
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&probe.stdout).trim(), "mp3,256000");
        let source_before = std::fs::read(&input).unwrap();
        e.input = Some(input.clone());
        e.play(true);
        let preview = e
            .preview_job
            .take()
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert!((duration(&preview).unwrap() - 4.).abs() < 0.15);
        e.preview_path = Some(preview.clone());
        e.stop();
        assert!(
            !preview.exists(),
            "Stopping must remove the owned preview file"
        );
        assert_eq!(
            std::fs::read(&input).unwrap(),
            source_before,
            "Preview must not modify source audio"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
