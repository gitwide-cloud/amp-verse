//! Library-owned local speech transcription. Stems are an optional input provider.
use amp_core::{Track, transcripts::Transcript};
use eframe::egui;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
        mpsc,
    },
    time::Duration,
};
enum Event {
    Status(String),
    Progress(f32),
    Ready(Transcript),
    Failed(String),
    Installed,
}
#[derive(Default)]
pub struct Panel {
    source: Option<PathBuf>,
    root: PathBuf,
    relative: PathBuf,
    expected: u64,
    document: Option<Transcript>,
    rx: Option<mpsc::Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    status: String,
    progress: f32,
    query: String,
    vocals: bool,
    options_ready: bool,
    gpu: bool,
    language: String,
    dirty: bool,
}
fn addon() -> Option<PathBuf> {
    Some(
        std::env::current_exe()
            .ok()?
            .parent()?
            .join("addons/transcribe"),
    )
}
fn runtime() -> Option<PathBuf> {
    std::env::var_os("AMP_TRANSCRIBE_PYTHON")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| {
            let root = addon()?;
            [root.join("Scripts/python.exe"), root.join("bin/python")]
                .into_iter()
                .find(|p| p.is_file())
        })
}
fn bootstrap() -> Option<PathBuf> {
    crate::stems::runtime().or_else(|| {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .flat_map(|p| [p.join("python3"), p.join("python.exe")])
            .find(|p| p.is_file())
    })
}
fn local_store() -> Option<PathBuf> {
    Some(
        crate::theme::settings_path()?
            .parent()?
            .join("transcript-drafts.ampdb"),
    )
}
fn draft_key(root: &Path, relative: &Path) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{}|{}", root.display(), relative.display()).as_bytes())
    )
}
fn wrapped(command: std::process::Command) -> process_wrap::std::CommandWrap {
    let mut c = process_wrap::std::CommandWrap::from(command);
    #[cfg(windows)]
    c.wrap(process_wrap::std::CreationFlags(
        windows::Win32::System::Threading::CREATE_NO_WINDOW
            | windows::Win32::System::Threading::BELOW_NORMAL_PRIORITY_CLASS,
    ))
    .wrap(process_wrap::std::JobObject);
    #[cfg(unix)]
    c.wrap(process_wrap::std::ProcessGroup::leader());
    c
}
fn execute(
    mut command: std::process::Command,
    cancel: &AtomicBool,
    tx: &mpsc::Sender<Event>,
    log: &Path,
) -> Result<(), String> {
    let errors = fs::File::create(log).map_err(|e| e.to_string())?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(errors);
    crate::preparation_speed::configure(&mut command);
    let mut c = wrapped(command);
    let mut child = c.spawn().map_err(|e| e.to_string())?;
    let stdout = child
        .stdout()
        .take()
        .ok_or("Transcription progress unavailable")?;
    let sender = tx.clone();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                if v["type"] == "progress" {
                    let duration = v["duration"].as_f64().unwrap_or(1.).max(0.1);
                    let _ = sender.send(Event::Progress(
                        (v["seconds"].as_f64().unwrap_or(0.) / duration).clamp(0., 1.) as f32,
                    ));
                } else if let Some(text) = v["text"].as_str() {
                    let _ = sender.send(Event::Status(text.into()));
                }
            } else if !line.trim().is_empty() {
                let _ = sender.send(Event::Status(line.chars().take(240).collect()));
            }
        }
    });
    let result = loop {
        if cancel.load(Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            break Err("Transcription cancelled; existing transcripts were kept".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    break Ok(());
                }
                let message = fs::read_to_string(log).unwrap_or_default();
                break Err(format!(
                    "Speech worker failed. {}",
                    message.lines().rev().take(3).collect::<Vec<_>>().join(" ")
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(e.to_string());
            }
        }
    };
    let _ = reader.join();
    result
}
fn sha(source: &Path, cancel: &AtomicBool) -> Result<String, String> {
    let mut file = fs::File::open(source).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        if !crate::preparation_speed::wait(Some(cancel)) {
            return Err("Transcription cancelled".into());
        }
        let n = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn tool(name: &str) -> PathBuf {
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join(format!("{name}{suffix}"))))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}
fn duration(source: &Path, cancel: &AtomicBool) -> Result<f64, String> {
    let mut command = amp_core::command(tool("ffprobe").to_string_lossy().as_ref());
    command
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(source)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut wrapper = wrapped(command);
    let mut child = wrapper.spawn().map_err(|e| e.to_string())?;
    let started = std::time::Instant::now();
    loop {
        if cancel.load(Relaxed) || started.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Recording probe cancelled or timed out".into());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err("Cannot read recording duration".into());
            }
            let mut output = String::new();
            child
                .stdout()
                .as_mut()
                .ok_or("Missing recording duration")?
                .read_to_string(&mut output)
                .map_err(|e| e.to_string())?;
            return output
                .trim()
                .parse::<f64>()
                .map_err(|_| "Cannot read recording duration".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
impl Panel {
    pub fn poll(&mut self) {
        let mut ended = false;
        if let Some(rx) = &self.rx {
            loop {
                let event = match rx.try_recv() {
                    Ok(event) => event,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        if !ended {
                            self.status =
                                "Speech worker stopped; existing transcript retained".into();
                        }
                        ended = true;
                        break;
                    }
                };
                match event {
                    Event::Status(text) => self.status = text,
                    Event::Progress(n) => {
                        self.progress = n;
                        self.status = format!("Transcribing speech · {:.0}%", n * 100.);
                    }
                    Event::Ready(doc) => {
                        self.document = Some(doc);
                        self.dirty = true;
                        self.status =
                            "Transcript ready. Save it to the Library or export it.".into();
                        ended = true;
                    }
                    Event::Failed(e) => {
                        self.status = e;
                        ended = true;
                    }
                    Event::Installed => {
                        self.status="Speech tools installed. Ready to transcribe; the first use downloads the base model.".into();
                        ended = true;
                    }
                }
            }
        }
        if ended {
            self.rx = None;
        }
    }
    pub fn select_library(&mut self, root: &Path, track: &Track) {
        if self.rx.is_none() {
            self.choose(
                root.into(),
                track.path.clone(),
                amp_core::music_sources::audio_path(root, &track.path),
            );
        }
    }
    fn choose(&mut self, root: PathBuf, relative: PathBuf, source: PathBuf) {
        self.root = root;
        self.relative = relative;
        self.source = Some(source);
        self.query.clear();
        self.status.clear();
        self.document = match amp_core::transcripts::load(&self.root, &self.relative) {
            Ok(doc) => doc,
            Err(e) => {
                self.status = e;
                None
            }
        };
        self.expected = self.document.as_ref().map_or(0, |d| d.revision);
        self.dirty = false;
        if self.document.is_none()
            && let Some(file) = local_store()
            && let Ok(Some(value)) =
                amp_core::catalog_db::local_record(&file, &draft_key(&self.root, &self.relative))
            && let Ok(doc) = serde_json::from_value::<Transcript>(value)
            && doc.validate().is_ok()
        {
            self.document = Some(doc);
            self.dirty = true;
            self.status =
                "Recovered a local transcript draft; Save to attach it to the Library.".into();
        }
    }
    fn install(&mut self) {
        let Some(python) = bootstrap() else {
            self.status =
                "Install Python 3.10+ or the stem runtime, then retry speech-tool setup.".into();
            return;
        };
        let Some(destination) = addon() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        self.status = "Installing optional local speech tools from PyPI…".into();
        std::thread::spawn(move || {
            let result = (|| {
                let work = tempfile::tempdir().map_err(|e| e.to_string())?;
                let script = work.path().join("setup.py");
                fs::write(
                    &script,
                    include_str!("../../../tools/setup_transcription.py"),
                )
                .map_err(|e| e.to_string())?;
                let mut c = amp_core::command(python.to_string_lossy().as_ref());
                c.arg(script).arg("--dest").arg(destination);
                execute(c, &cancel, &tx, &work.path().join("setup.log"))
            })();
            let _ = tx.send(match result {
                Ok(()) => Event::Installed,
                Err(e) => Event::Failed(e),
            });
        });
    }
    fn start(&mut self) {
        let (Some(source), Some(python), Some(addon)) = (self.source.clone(), runtime(), addon())
        else {
            self.status = "Choose audio and install speech tools first.".into();
            return;
        };
        let language = self.language.trim().to_lowercase();
        if !language.is_empty()
            && (!language
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'-')
                || language.len() > 12)
        {
            self.status =
                "Use a language code such as en, es or fr; leave blank for automatic detection."
                    .into();
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let vocals = self.vocals;
        let gpu = self.gpu;
        let root = self.root.clone();
        let relative = self.relative.clone();
        self.progress = 0.;
        self.status = "Preparing speech input…".into();
        std::thread::spawn(move || {
            let result = (|| {
                let before = fs::metadata(&source).map_err(|e| e.to_string())?;
                let checksum = sha(&source, &cancel)?;
                let work = tempfile::tempdir().map_err(|e| e.to_string())?;
                let mut input = source.clone();
                let mut input_kind = "original";
                if vocals
                    && (crate::stems::ready(&source).is_some()
                        || duration(&source, &cancel)? <= 1800.)
                {
                    let _ = tx.send(Event::Status("Preparing or restoring vocal stems…".into()));
                    match crate::stems::for_transcription(&source, &cancel) {
                        Ok(paths) => {
                            input = paths[0].clone();
                            input_kind = "vocals";
                        }
                        Err(e) => {
                            if cancel.load(Relaxed) {
                                return Err("Transcription cancelled".into());
                            }
                            let _ = tx.send(Event::Status(format!(
                                "Vocal isolation unavailable; using original recording. {e}"
                            )));
                        }
                    }
                }
                let script = work.path().join("transcribe.py");
                let output = work.path().join("transcript.json");
                fs::write(&script, include_str!("transcribe_worker.py"))
                    .map_err(|e| e.to_string())?;
                let mut c = amp_core::command(python.to_string_lossy().as_ref());
                c.arg(script)
                    .arg(&input)
                    .arg(&output)
                    .arg("--models")
                    .arg(addon.join("models"))
                    .arg("--language")
                    .arg(language)
                    .arg("--device")
                    .arg(if gpu { "auto" } else { "cpu" })
                    .arg("--ffmpeg")
                    .arg(tool("ffmpeg"))
                    .arg("--ffprobe")
                    .arg(tool("ffprobe"));
                if let Some(stems) = crate::stems::runtime()
                    .and_then(|p| p.parent()?.parent().map(Path::to_path_buf))
                {
                    let dll = stems.join("Lib/site-packages/torch/lib");
                    c.env("AMP_TRANSCRIBE_CUDA_LIB", &dll);
                    if let Ok(path) = std::env::join_paths(
                        std::iter::once(dll).chain(
                            std::env::var_os("PATH")
                                .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
                                .unwrap_or_default(),
                        ),
                    ) {
                        c.env("PATH", path);
                    }
                }
                execute(c, &cancel, &tx, &work.path().join("worker.log"))?;
                if cancel.load(Relaxed) {
                    return Err("Transcription cancelled".into());
                }
                let after = fs::metadata(&source).map_err(|e| e.to_string())?;
                if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
                    return Err("Source changed during transcription. Existing transcript kept; retry the current recording.".into());
                }
                if fs::metadata(&output).map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024 {
                    return Err("Transcript is too large; split the recording into chapters".into());
                }
                let mut doc: Transcript =
                    serde_json::from_slice(&fs::read(output).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                doc.source_sha256 = checksum;
                doc.input = input_kind.into();
                doc.validate()?;
                if let Some(file) = local_store() {
                    amp_core::catalog_db::save_local_record(
                        &file,
                        &draft_key(&root, &relative),
                        &serde_json::to_value(&doc).map_err(|e| e.to_string())?,
                    )?;
                }
                Ok(doc)
            })();
            let _ = tx.send(match result {
                Ok(doc) => Event::Ready(doc),
                Err(e) => Event::Failed(e),
            });
        });
    }
    pub fn show(&mut self, ui: &mut egui::Ui, root: &Path, track: Option<&Track>) {
        self.poll();
        if !self.options_ready {
            self.vocals = true;
            self.options_ready = true;
        }
        ui.heading("Transcribe speech");
        ui.weak(
            "Podcasts, speeches and audiobooks · local processing · original audio stays unchanged",
        );
        let busy = self.rx.is_some();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !busy && track.is_some(),
                    egui::Button::new("Use selected Library recording"),
                )
                .clicked()
                && let Some(track) = track
            {
                self.select_library(root, track);
            }
            if ui
                .add_enabled(!busy, egui::Button::new("Open audio…"))
                .clicked()
                && let Some(source) = rfd::FileDialog::new()
                    .add_filter(
                        "Audio",
                        &["mp3", "wav", "flac", "m4a", "m4b", "ogg", "opus"],
                    )
                    .pick_file()
                && let (Some(parent), Some(name)) = (source.parent(), source.file_name())
            {
                self.choose(parent.into(), PathBuf::from(name), source.clone());
            }
        });
        if let Some(source) = &self.source {
            ui.label(source.display().to_string());
        } else {
            ui.label("Select a Library item, or open a recording to begin.");
        }
        ui.add_enabled_ui(!busy, |ui| {
            ui.checkbox(
                &mut self.vocals,
                "Use vocal stems (prepare them if needed; fall back to original)",
            );
            ui.checkbox(&mut self.gpu, "Use compatible GPU when available");
            ui.horizontal(|ui| {
                ui.label("Language code (blank = auto)");
                ui.add(egui::TextEdit::singleline(&mut self.language).desired_width(80.));
            });
        });
        ui.horizontal_wrapped(|ui| {
            if runtime().is_none() {
                if ui
                    .add_enabled(!busy, egui::Button::new("Install speech tools"))
                    .clicked()
                {
                    self.install();
                }
            } else if ui
                .add_enabled(
                    !busy && self.source.is_some(),
                    egui::Button::new("Transcribe"),
                )
                .clicked()
            {
                self.start();
            }
            if runtime().is_some()
                && ui
                    .add_enabled(!busy, egui::Button::new("Repair speech tools"))
                    .clicked()
            {
                self.install();
            }
            if busy && ui.button("Cancel").clicked() {
                self.cancel.store(true, Relaxed);
                self.status = "Cancelling; existing transcripts are kept…".into();
            }
        });
        if busy {
            ui.add(egui::ProgressBar::new(self.progress).animate(true));
        }
        ui.label(&self.status);
        if let Some(doc) = &self.document {
            ui.separator();
            ui.label(format!(
                "{} · {} segments · input: {}",
                doc.language,
                doc.segments.len(),
                doc.input
            ));
            ui.weak("Machine-generated transcript; review names and wording before sharing.");
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!busy && self.dirty, egui::Button::new("Save transcript to Library")).clicked() {
                    match amp_core::transcripts::save(&self.root, &self.relative, doc, self.expected) {
                        Ok(saved) => {
                            self.expected = saved.revision;
                            self.status = "Saved in ampdj/transcripts.ampdb. Audio, cues and analysis unchanged.".into();
                            self.dirty = false;
                        }
                        Err(e) => self.status = format!("{e} Local draft retained."),
                    }
                }
                for (name, extension) in [("Export TXT", "txt"), ("Export SRT", "srt")] {
                    if ui.button(name).clicked()
                        && let Some(file) = rfd::FileDialog::new().set_file_name(format!("transcript.{extension}")).add_filter(name, &[extension]).save_file()
                    {
                        if self.source.as_ref().is_some_and(|source| file.exists() && source.canonicalize().ok() == file.canonicalize().ok()) {
                            self.status = "Choose a transcript filename; original audio cannot be overwritten.".into();
                        } else {
                            let text = if extension == "srt" { doc.srt() } else { doc.text() };
                            self.status = fs::write(file, text).map_or_else(|e| e.to_string(), |()| "Transcript exported".into());
                        }
                    }
                }
            });
            ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("Search transcript"));
            let query = self.query.to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("speech-transcript")
                .max_height(360.)
                .show(ui, |ui| {
                    for segment in doc
                        .segments
                        .iter()
                        .filter(|s| query.is_empty() || s.text.to_lowercase().contains(&query))
                        .take(1000)
                    {
                        ui.horizontal_wrapped(|ui| {
                            ui.weak(format!(
                                "{:02}:{:02}",
                                (segment.start / 60.) as u64,
                                segment.start as u64 % 60
                            ));
                            ui.label(&segment.text);
                        });
                    }
                });
        }
    }
}
impl Drop for Panel {
    fn drop(&mut self) {
        self.cancel.store(true, Relaxed);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_event_is_not_lost_when_sender_disconnects() {
        let (tx, rx) = mpsc::channel();
        let mut panel = Panel::default();
        panel.rx.replace(rx);
        tx.send(Event::Failed("readable failure".into())).unwrap();
        drop(tx);
        panel.poll();
        assert_eq!(panel.status, "readable failure");
        assert!(panel.rx.is_none());
    }
    #[test]
    fn cancellation_stops_worker_promptly() {
        let work = tempfile::tempdir().unwrap();
        let (tx, _rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let timer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            signal.store(true, Relaxed);
        });
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args([
            "--exact",
            "transcription::tests::sleep_fixture",
            "--ignored",
        ]);
        let start = std::time::Instant::now();
        let error = execute(command, &cancel, &tx, &work.path().join("log")).unwrap_err();
        timer.join().unwrap();
        assert!(error.contains("cancelled"));
        assert!(start.elapsed() < Duration::from_secs(5));
    }
    #[test]
    #[ignore = "subprocess fixture for cancellation test"]
    fn sleep_fixture() {
        std::thread::sleep(Duration::from_secs(60));
    }
}
