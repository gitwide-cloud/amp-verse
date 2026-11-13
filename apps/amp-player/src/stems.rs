//! Optional background separation and device-local prepared audio cache.
//! Never writes source audio, NAS preparation, or active deck snapshots.
use eframe::egui;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering::Relaxed},
        mpsc,
    },
    time::{Duration, SystemTime},
};
fn cache_root() -> Option<PathBuf> {
    crate::theme::settings_path().and_then(|p| p.parent().map(|p| p.join("prepared-stems")))
}
fn worker_process(command: std::process::Command) -> process_wrap::std::CommandWrap {
    let mut wrapped = process_wrap::std::CommandWrap::from(command);
    #[cfg(windows)]
    wrapped
        .wrap(process_wrap::std::CreationFlags(
            windows::Win32::System::Threading::CREATE_NO_WINDOW
                | windows::Win32::System::Threading::BELOW_NORMAL_PRIORITY_CLASS,
        ))
        .wrap(process_wrap::std::JobObject);
    #[cfg(unix)]
    wrapped.wrap(process_wrap::std::ProcessGroup::leader());
    wrapped
}
pub(crate) fn runtime() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("AMP_STEM_PYTHON")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
    {
        return Some(path);
    }
    let root = std::env::current_exe().ok()?.parent()?.join("addons/stems");
    [root.join("Scripts/python.exe"), root.join("bin/python")]
        .into_iter()
        .find(|p| p.is_file())
}
fn stamp(source: &Path) -> Result<String, String> {
    let source = source.canonicalize().map_err(|e| e.to_string())?;
    let metadata = fs::metadata(&source).map_err(|e| e.to_string())?;
    let modified = metadata
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    Ok(format!(
        "{:x}",
        Sha256::digest(format!("{}|{}|{modified}", source.display(), metadata.len()).as_bytes())
    ))
}
fn digest(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let n = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn ready_in(cache: &Path, source: &Path) -> Option<[PathBuf; 3]> {
    let id = stamp(source).ok()?;
    let dir = cache.join(&id);
    let manifest: Value = if let Some(value) =
        amp_core::catalog_db::local_record(&cache.join("stems.ampdb"), &id).ok()?
    {
        value
    } else {
        let metadata = fs::metadata(dir.join("manifest.json")).ok()?;
        if metadata.len() > 16384 {
            return None;
        }
        serde_json::from_slice(&fs::read(dir.join("manifest.json")).ok()?).ok()?
    };
    if manifest["source_stamp"].as_str()? != id
        || manifest["audio"]["engine"] != "demucs-4.0.1/htdemucs-3stem"
    {
        return None;
    }
    for name in ["vocals", "instrumental", "drums"] {
        let path = dir.join(format!("{name}.wav"));
        let metadata = fs::symlink_metadata(path).ok()?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() != manifest["audio"]["files"][name]["bytes"].as_u64()?
        {
            return None;
        }
    }
    Some([
        dir.join("vocals.wav"),
        dir.join("instrumental.wav"),
        dir.join("drums.wav"),
    ])
}
pub fn ready(source: &Path) -> Option<[PathBuf; 3]> {
    ready_in(&cache_root()?, source)
}
pub(crate) fn relocate_cached(source: &Path, destination: &Path) -> Result<(), String> {
    let Some(cache) = cache_root() else {
        return Ok(());
    };
    if ready_in(&cache, source).is_none() {
        return Ok(());
    }
    // This branch only archives existing prepared audio; it never starts separation.
    prepare(
        &cache,
        source,
        Path::new(""),
        &AtomicBool::new(false),
        &Arc::new(Mutex::new(None)),
    )?;
    if !restore_cached(&cache, destination)? {
        return Err(
            "Prepared stems could not be restored at the new location; original retained.".into(),
        );
    }
    Ok(())
}
fn restore_cached(cache: &Path, source: &Path) -> Result<bool, String> {
    let archive = cache.join("stems.ampdb");
    if !archive.is_file() {
        return Ok(false);
    }
    let id = stamp(source)?;
    let _lock = amp_core::coordination::Lock::acquire(&cache.join(&id))?;
    if ready_in(cache, source).is_some() {
        return Ok(true);
    }
    let hash = digest(source)?;
    let stage = tempfile::Builder::new()
        .prefix(".restore-")
        .tempdir_in(cache)
        .map_err(|e| e.to_string())?;
    let Some(mut manifest) = amp_core::stem_archive::restore(&archive, &hash, stage.path())? else {
        return Ok(false);
    };
    for name in ["vocals", "instrumental", "drums"] {
        if digest(&stage.path().join(format!("{name}.wav")))?
            != manifest["audio"]["files"][name]["sha256"]
                .as_str()
                .unwrap_or("")
        {
            return Err("Archived stem checksum failed".into());
        }
    }
    if stamp(source)? != id {
        return Err("Song changed while restoring stems".into());
    }
    manifest["source_stamp"] = json!(id);
    let destination = cache.join(&id);
    if destination.exists() {
        fs::rename(
            &destination,
            cache.join(format!("{id}.previous-{}", amp_core::coordination::token())),
        )
        .map_err(|e| e.to_string())?;
    }
    fs::rename(stage.path(), destination).map_err(|e| e.to_string())?;
    amp_core::catalog_db::save_local_record(&archive, &id, &manifest)?;
    Ok(true)
}
type Process = Arc<Mutex<Option<Box<dyn process_wrap::std::ChildWrapper>>>>;
pub(crate) fn separate_clip(cache: &Path, source: &Path) -> Result<[PathBuf; 3], String> {
    let python = runtime().ok_or("Install the stem separator add-on first")?;
    prepare(
        cache,
        source,
        &python,
        &AtomicBool::new(false),
        &Arc::new(Mutex::new(None)),
    )?;
    ready_in(cache, source).ok_or("Separated clip is incomplete".into())
}
fn prepare(
    cache: &Path,
    source: &Path,
    python: &Path,
    cancel: &AtomicBool,
    process: &Process,
) -> Result<(), String> {
    fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    if !crate::preparation_speed::wait(Some(cancel)) {
        return Err("Stem preparation canceled".into());
    }
    let id = stamp(source)?;
    let _lock = amp_core::coordination::Lock::acquire(&cache.join(&id))?;
    if let Some(paths) = ready_in(cache, source) {
        let archive = cache.join("stems.ampdb");
        if !amp_core::stem_archive::contains(&archive, &id) {
            let mut manifest = amp_core::catalog_db::local_record(&archive, &id)?.unwrap_or(
                serde_json::from_slice(
                    &fs::read(paths[0].parent().unwrap().join("manifest.json")).unwrap_or_default(),
                )
                .unwrap_or_default(),
            );
            manifest["source_name"] =
                json!(source.file_name().unwrap_or_default().to_string_lossy());
            amp_core::stem_archive::store_with_checkpoint(
                &archive,
                &id,
                &manifest,
                paths[0].parent().unwrap(),
                cancel,
                || {
                    if crate::preparation_speed::wait(Some(cancel)) {
                        Ok(())
                    } else {
                        Err("Stem preparation canceled".into())
                    }
                },
            )?;
        }
        return Ok(());
    }
    let stage = tempfile::Builder::new()
        .prefix(".pending-")
        .tempdir_in(cache)
        .map_err(|e| e.to_string())?;
    let input = stage.path().join("input.audio");
    fs::copy(source, &input).map_err(|e| e.to_string())?;
    let source_hash = digest(&input)?;
    if stamp(source)? != id || cancel.load(Relaxed) {
        return Err("Source changed or preparation canceled".into());
    }
    let worker = stage.path().join("worker.py");
    fs::write(&worker, include_str!("stem_worker.py")).map_err(|e| e.to_string())?;
    let log = fs::File::create(stage.path().join("worker.log")).map_err(|e| e.to_string())?;
    let mut command = amp_core::command(python.to_string_lossy().as_ref());
    command
        .arg(&worker)
        .arg(&input)
        .arg(stage.path())
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log);
    if let Some(addons) = python.parent().and_then(Path::parent) {
        command.env("TORCH_HOME", addons.join("models"));
    }
    if let Some(exe) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("ffmpeg.exe")))
        .filter(|p| p.is_file())
    {
        command.env("AMP_STEM_FFMPEG", exe);
    }
    crate::preparation_speed::configure(&mut command);
    let mut wrapped = worker_process(command);
    *process.lock().map_err(|_| "Stem process unavailable")? =
        Some(wrapped.spawn().map_err(|e| e.to_string())?);
    let status = loop {
        let mut slot = process.lock().map_err(|_| "Stem process unavailable")?;
        let child = slot.as_mut().ok_or("Stem process stopped")?;
        if cancel.load(Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Stem preparation canceled".into());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            slot.take();
            break status;
        }
        drop(slot);
        std::thread::sleep(Duration::from_millis(100));
    };
    if !status.success() {
        let log = fs::read_to_string(stage.path().join("worker.log")).unwrap_or_default();
        return Err(format!(
            "Stem preparation failed: {}",
            log.lines()
                .rev()
                .take(3)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    if !crate::preparation_speed::wait(Some(cancel)) {
        return Err("Stem preparation canceled".into());
    }
    let audio: Value = serde_json::from_slice(
        &fs::read(stage.path().join("audio.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if audio["engine"] != "demucs-4.0.1/htdemucs-3stem"
        || audio["channels"] != 2
        || audio["sample_rate"] != 44100
        || audio["frames"].as_u64().unwrap_or(0) < 11025
    {
        return Err("Invalid prepared stem format".into());
    }
    for name in ["vocals", "instrumental", "drums"] {
        let file = stage.path().join(format!("{name}.wav"));
        if digest(&file)? != audio["files"][name]["sha256"].as_str().unwrap_or("") {
            return Err("Prepared stem checksum failed".into());
        }
    }
    if stamp(source)? != id || cancel.load(Relaxed) {
        return Err("Source changed or preparation canceled; result not published".into());
    }
    let manifest = json!({"version":1,"source_stamp":id,"source_name":source.file_name().unwrap_or_default().to_string_lossy(),"source_sha256":source_hash,"audio":audio});
    amp_core::stem_archive::store_with_checkpoint(
        &cache.join("stems.ampdb"),
        &id,
        &manifest,
        stage.path(),
        cancel,
        || {
            if crate::preparation_speed::wait(Some(cancel)) {
                Ok(())
            } else {
                Err("Stem preparation canceled".into())
            }
        },
    )?;
    fs::remove_file(input).map_err(|e| format!("Remove staged input: {e}"))?;
    // A previously corrupted generation is retained, never overwritten silently.
    let destination = cache.join(&id);
    if destination.exists() {
        fs::rename(
            &destination,
            cache.join(format!("{id}.previous-{}", amp_core::coordination::token())),
        )
        .map_err(|e| e.to_string())?;
    }
    drop(wrapped);
    for file in ["audio.json", "worker.py", "worker.log"] {
        let _ = fs::remove_file(stage.path().join(file));
    }
    fs::rename(stage.path(), &destination).map_err(|e| format!("Publish stem cache: {e}"))?;
    amp_core::catalog_db::save_local_record(&cache.join("stems.ampdb"), &id, &manifest)?;
    Ok(())
}
struct Job {
    cancel: Arc<AtomicBool>,
    process: Process,
    rx: mpsc::Receiver<Result<(), String>>,
    deck: usize,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Relaxed);
        if let Ok(mut slot) = self.process.lock()
            && let Some(child) = slot.as_mut()
        {
            let _ = child.kill();
        }
    }
}
pub struct Panel {
    pub manual_change: Option<(usize, [f32; 3])>,
    job: Option<Job>,
    batch: Option<Job>,
    batch_rx: Option<mpsc::Receiver<(usize, usize, String)>>,
    batch_progress: (usize, usize),
    batch_status: String,
    gpu_rx: Option<mpsc::Receiver<String>>,
    gpu_status: String,
    gpu_checked: bool,
    status: [String; 6],
    level: [[f32; 3]; 6],
    generation: [u64; 6],
}
impl Default for Panel {
    fn default() -> Self {
        Self {
            job: None,
            manual_change: None,
            batch: None,
            batch_rx: None,
            batch_progress: (0, 0),
            batch_status: String::new(),
            gpu_rx: None,
            gpu_status: String::new(),
            gpu_checked: false,
            status: Default::default(),
            level: [[0.; 3]; 6],
            generation: [0; 6],
        }
    }
}
impl Panel {
    pub fn busy(&self) -> bool {
        self.job.is_some() || self.batch.is_some()
    }
    pub fn prepare_song(&mut self, ui: &mut egui::Ui, source: &Path) -> bool {
        if let Some(job) = &self.job
            && let Ok(result) = job.rx.try_recv()
        {
            self.status[0] = result.map_or_else(|e| e, |()| "Stems ready".into());
            self.job = None;
        }
        let prepared = ready(source).is_some();
        if !prepared
            && self.job.is_none()
            && let Some(cache) = cache_root()
            && cache.join("stems.ampdb").is_file()
            && ui.button("Restore stems from portable archive").clicked()
        {
            let source = source.to_path_buf();
            let cancel = Arc::new(AtomicBool::new(false));
            let process = Arc::new(Mutex::new(None));
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(restore_cached(&cache, &source).and_then(|found| {
                    if found {
                        Ok(())
                    } else {
                        Err("No matching prepared audio in this archive".into())
                    }
                }));
            });
            self.job = Some(Job {
                cancel,
                process,
                rx,
                deck: 0,
            });
        }
        if let Some(job) = &self.job {
            ui.spinner();
            ui.label("Preparing stems…");
            if ui.button("Cancel preparation").clicked() {
                job.cancel.store(true, Relaxed);
            }
            ui.ctx().request_repaint_after(Duration::from_millis(150));
        } else if !prepared
            && ui
                .add_enabled(
                    runtime().is_some(),
                    egui::Button::new("Prepare stems for this song"),
                )
                .clicked()
            && let (Some(cache), Some(python)) = (cache_root(), runtime())
        {
            let source = source.to_path_buf();
            let cancel = Arc::new(AtomicBool::new(false));
            let process = Arc::new(Mutex::new(None));
            let (tx, rx) = mpsc::channel();
            let stop = cancel.clone();
            let child = process.clone();
            std::thread::spawn(move || {
                let _ = tx.send(prepare(&cache, &source, &python, &stop, &child));
            });
            self.job = Some(Job {
                cancel,
                process,
                rx,
                deck: 0,
            });
        }
        if prepared {
            ui.label("Prepared stems available");
        }
        if runtime().is_none() && !prepared {
            ui.weak("Install the stem separator add-on to prepare this song.");
        }
        ui.label(&self.status[0]);
        prepared
    }
    pub fn library(&mut self, ui: &mut egui::Ui, root: &Path) {
        if let Some(rx) = &self.batch_rx {
            while let Ok((done, total, title)) = rx.try_recv() {
                self.batch_progress = (done, total);
                self.batch_status = title;
            }
        }
        if let Some(job) = &self.batch
            && let Ok(result) = job.rx.try_recv()
        {
            self.batch_status = result.map_or_else(|e| e, |()| "Stem preparation complete.".into());
            self.batch = None;
            self.batch_rx = None;
        }
        if !self.gpu_checked
            && let Some(python) = runtime()
        {
            self.gpu_checked = true;
            self.gpu_status = "Checking GPU support…".into();
            let (tx, rx) = mpsc::channel();
            self.gpu_rx = Some(rx);
            std::thread::spawn(move || {
                let result = (|| -> Result<String, String> {
                    crate::preparation_speed::wait(None);
                    let stage = tempfile::tempdir().map_err(|e| e.to_string())?;
                    let worker = stage.path().join("probe.py");
                    fs::write(&worker, include_str!("stem_worker.py"))
                        .map_err(|e| e.to_string())?;
                    let mut command = amp_core::command(python.to_string_lossy().as_ref());
                    command.arg(worker).arg("--probe");
                    crate::preparation_speed::configure(&mut command);
                    let output = command.output().map_err(|e| e.to_string())?;
                    if !output.status.success() {
                        return Err(
                            "Stem runtime could not be checked; reinstall the stem add-on".into(),
                        );
                    }
                    let value: Value =
                        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
                    Ok(if value["device"] == "cuda" {
                        format!(
                            "GPU ready: {}",
                            value["name"].as_str().unwrap_or("NVIDIA GPU")
                        )
                    } else {
                        "CPU fallback — GPU runtime unavailable. Install the GPU stem add-on to accelerate compatible NVIDIA hardware.".into()
                    })
                })();
                let _ = tx.send(result.unwrap_or_else(|error| error));
            });
        }
        if let Some(rx) = &self.gpu_rx
            && let Ok(status) = rx.try_recv()
        {
            self.gpu_status = status;
            self.gpu_rx = None;
        }
        ui.heading("Prepare Stems");
        ui.label(&self.gpu_status);
        ui.label("Separate vocals, instruments and beat for the DJ stem knobs.");
        ui.weak("Uses your GPU when available, with a safe CPU fallback. Completed stems are reused. Your playback setting chooses slower work or an automatic pause.");
        if let Some(job) = &self.batch {
            let (done, total) = self.batch_progress;
            ui.add(
                egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                    .text(format!("{done} / {total} songs")),
            );
            ui.spinner();
            if ui.button("Stop preparing stems").clicked() {
                job.cancel.store(true, Relaxed);
            }
            ui.ctx().request_repaint_after(Duration::from_millis(150));
        } else if ui
            .add_enabled(
                !root.as_os_str().is_empty() && self.job.is_none() && runtime().is_some(),
                egui::Button::new("Prepare library stems"),
            )
            .clicked()
            && let (Some(cache), Some(python)) = (cache_root(), runtime())
        {
            let root = root.to_path_buf();
            self.batch_progress = (0, 0);
            self.batch_status = "Finding songs in all music sources and subfolders…".into();
            let cancel = Arc::new(AtomicBool::new(false));
            let process = Arc::new(Mutex::new(None));
            let (tx, rx) = mpsc::channel();
            let (progress, updates) = mpsc::channel();
            let stop = cancel.clone();
            let child = process.clone();
            std::thread::spawn(move || {
                let result = (|| {
                    let library = amp_core::scan_job_inputs(&root).map_err(|e| e.to_string())?;
                    let sources: Vec<_> = library
                        .tracks
                        .iter()
                        .filter_map(|track| {
                            let source = amp_core::music_sources::audio_path(&root, &track.path);
                            let mut prep = amp_core::preparation::load(&source).unwrap_or_default();
                            amp_core::media_kind::inspect_audio(&source, &mut prep);
                            amp_core::media_kind::is_music(&source, &prep)
                                .then(|| (source, track.title.clone()))
                        })
                        .collect();
                    let total = sources.len();
                    let mut failed = 0;
                    let mut last_error = String::new();
                    for (index, (source, title)) in sources.into_iter().enumerate() {
                        if stop.load(Relaxed) {
                            return Err(
                                "Stem preparation stopped. Completed songs are retained.".into()
                            );
                        }
                        let _ = progress.send((index, total, format!("Preparing {title}")));
                        match prepare(&cache, &source, &python, &stop, &child) {
                            Ok(()) => {
                                let _ =
                                    progress.send((index + 1, total, format!("Prepared {title}")));
                            }
                            Err(error) if stop.load(Relaxed) => return Err(error),
                            Err(error) => {
                                failed += 1;
                                last_error = format!("{title}: {error}");
                                let _ = progress.send((
                                    index + 1,
                                    total,
                                    format!("Skipped failed song: {last_error}"),
                                ));
                            }
                        }
                    }
                    if failed > 0 {
                        return Err(format!(
                            "Stem preparation finished: {} prepared/reused, {failed} failed. Last error: {last_error}",
                            total - failed
                        ));
                    }
                    Ok(())
                })();
                let _ = tx.send(result);
            });
            self.batch = Some(Job {
                cancel,
                process,
                rx,
                deck: 0,
            });
            self.batch_rx = Some(updates);
        }
        if runtime().is_none() {
            ui.hyperlink_to(
                "Install the stem separator add-on",
                "https://github.com/gitwide-cloud/amp-verse/blob/main/docs/STEMS.md",
            );
        }
        ui.label(&self.batch_status);
    }
    pub fn live_knobs(
        &mut self,
        ui: &mut egui::Ui,
        deck: usize,
        audio: Option<&crate::mixer::Engine>,
    ) -> Option<[f32; 3]> {
        self.live_knobs_layout(ui, deck, audio, false)
    }
    pub fn live_knobs_vertical(
        &mut self,
        ui: &mut egui::Ui,
        deck: usize,
        audio: Option<&crate::mixer::Engine>,
    ) -> Option<[f32; 3]> {
        self.live_knobs_layout(ui, deck, audio, true)
    }
    fn live_knobs_layout(
        &mut self,
        ui: &mut egui::Ui,
        deck: usize,
        audio: Option<&crate::mixer::Engine>,
        vertical: bool,
    ) -> Option<[f32; 3]> {
        let active = audio.is_some_and(|a| a.controls.stems_loaded[deck].load(Relaxed));
        if let Some(a) = audio {
            for i in 0..3 {
                self.level[deck][i] = f32::from_bits(a.controls.stem_knobs[deck][i].load(Relaxed));
            }
        }
        let previous = self.level[deck];
        ui.add_enabled_ui(active, |ui| {
            ui.with_layout(
                if vertical {
                    egui::Layout::top_down(egui::Align::Center)
                } else {
                    egui::Layout::left_to_right(egui::Align::Min)
                },
                |ui| {
                    for (stem, label) in [(0, "VOC"), (2, "BEAT"), (1, "INST")] {
                        ui.vertical(|ui| {
                            crate::dj_mixer::dial(
                                ui,
                                label,
                                &mut self.level[deck][stem],
                                -1. ..=1.,
                                if deck.is_multiple_of(2) {
                                    crate::ORANGE
                                } else {
                                    crate::CYAN
                                },
                            );
                            ui.small(label);
                        });
                    }
                },
            );
        });
        if !active && !vertical {
            ui.small("Prepare stems in Library");
        }
        (previous != self.level[deck]).then_some(self.level[deck])
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        deck: usize,
        source: Option<&Path>,
        audio: Option<&crate::mixer::Engine>,
        can_reload: bool,
    ) -> bool {
        if let Some(job) = &self.job
            && let Ok(result) = job.rx.try_recv()
        {
            self.status[job.deck] = result.map_or_else(
                |e| e,
                |()| "Ready. Reload this paused deck, or use stems on its next song load.".into(),
            );
            self.job = None;
        }
        if let Some(audio) = audio {
            let generation =
                audio.controls.deck_generation[deck].load(std::sync::atomic::Ordering::Acquire);
            if self.generation[deck] != generation {
                self.level[deck] = std::array::from_fn(|i| {
                    f32::from_bits(audio.controls.stem_knobs[deck][i].load(Relaxed))
                });
                self.generation[deck] = generation;
            }
        }
        let previous_levels = self.level[deck];
        let mut reload = false;
        let response = ui.push_id(("deck-stems", deck), |ui| {
            ui.colored_label(
                if deck.is_multiple_of(2) {
                    crate::ORANGE
                } else {
                    crate::CYAN
                },
                if deck.is_multiple_of(2) {
                    "A · STEMS"
                } else {
                    "B · STEMS"
                },
            );
            let prepared = source.is_some_and(|p| ready(p).is_some());
            let active = audio.is_some_and(|a| {
                a.load_current(deck) && a.controls.stems_loaded[deck].load(Relaxed)
            });
            if let Some(job) = &self.job {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Separating locally…");
                    if ui.button("Cancel").clicked() {
                        job.cancel.store(true, Relaxed);
                    }
                });
                ui.ctx().request_repaint_after(Duration::from_millis(150));
            } else if !prepared
                && ui
                    .add_enabled(
                        source.is_some() && runtime().is_some() && self.batch.is_none(),
                        egui::Button::new("Prepare stems"),
                    )
                    .clicked()
            {
                let source = source.unwrap().to_path_buf();
                let python = runtime().unwrap();
                if let Some(cache) = cache_root() {
                    let cancel = Arc::new(AtomicBool::new(false));
                    let process = Arc::new(Mutex::new(None));
                    let (tx, rx) = mpsc::channel();
                    let stop = cancel.clone();
                    let child = process.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(prepare(&cache, &source, &python, &stop, &child));
                    });
                    self.job = Some(Job {
                        cancel,
                        process,
                        rx,
                        deck,
                    });
                }
            }
            if runtime().is_none() {
                ui.hyperlink_to(
                    "Install the stem separator add-on",
                    "https://github.com/gitwide-cloud/amp-verse/blob/main/docs/STEMS.md",
                );
            }
            if prepared && !active {
                reload = ui
                    .add_enabled(can_reload, egui::Button::new("Load prepared stems"))
                    .clicked();
                ui.weak("Active playback is kept unchanged until the deck is reloaded.");
            }
            ui.add_enabled_ui(active, |ui| {
                for (stem, name) in [(0, "Vocals"), (1, "Instruments"), (2, "Beat")] {
                    ui.horizontal(|ui| {
                        crate::dj_mixer::dial(
                            ui,
                            name,
                            &mut self.level[deck][stem],
                            -1. ..=1.,
                            if deck.is_multiple_of(2) {
                                crate::ORANGE
                            } else {
                                crate::CYAN
                            },
                        );
                        ui.label(name);
                    });
                }
                if ui.small_button("Reset").clicked() {
                    self.level[deck] = [0.; 3];
                }
            });
            ui.label(&self.status[deck]);
        });
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::unique(("dj-stems", deck)).with("screen"),
                (response.response.rect, ui.layer_id()),
            )
        });
        if self.level[deck] != previous_levels {
            self.manual_change = Some((deck, self.level[deck]));
        }
        reload
    }
}
/// Center is original mix; left removes this part, right attenuates other parts.
pub fn remix_gains(knobs: [f32; 3]) -> [f32; 3] {
    amp_core::stem_mix::gains(knobs)
}
#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    #[ignore = "requires the installed Windows stem Python runtime"]
    fn real_worker_job_keeps_python_console_hidden() {
        let python = std::env::var("AMP_STEM_PYTHON").expect("stem Python fixture");
        let folder = tempfile::tempdir().unwrap();
        let result = folder.path().join("console.txt");
        let script = folder.path().join("probe.py");
        std::fs::write(&script, "import ctypes,sys\nfrom pathlib import Path\nk=ctypes.windll.kernel32\nk.GetCurrentProcess.restype=ctypes.c_void_p\nk.GetPriorityClass.argtypes=[ctypes.c_void_p]\nPath(sys.argv[1]).write_text(str(k.GetConsoleWindow())+','+str(k.GetPriorityClass(k.GetCurrentProcess())))\n").unwrap();
        let mut command = amp_core::command(&python);
        command
            .arg(script)
            .arg(&result)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut child = super::worker_process(command).spawn().unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(std::fs::read_to_string(result).unwrap().trim(), "0,16384");
    }
    #[test]
    fn remix_knobs_mute_isolate_and_restore_each_component() {
        assert_eq!(super::remix_gains([0.; 3]), [1.; 3]);
        for stem in 0..3 {
            let mut knobs = [0.; 3];
            knobs[stem] = -1.;
            let gains = super::remix_gains(knobs);
            assert_eq!(gains[stem], 0.);
            assert_eq!(gains[(stem + 1) % 3], 1.);
            knobs[stem] = 1.;
            let gains = super::remix_gains(knobs);
            assert_eq!(gains[stem], 1.);
            assert_eq!(gains[(stem + 1) % 3], 0.);
        }
    }

    use super::*;
    #[test]
    #[ignore = "requires AMP_STEM_PYTHON, FFmpeg and the installed model; real local inference"]
    fn real_separation_publishes_aligned_cache_without_changing_source() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = PathBuf::from(std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap());
        let source = dir.path().join("source.wav");
        fs::copy(fixture, &source).unwrap();
        let before = digest(&source).unwrap();
        let python = PathBuf::from(std::env::var_os("AMP_STEM_PYTHON").unwrap());
        let cache = dir.path().join("cache");
        let cancel = AtomicBool::new(false);
        let process = Arc::new(Mutex::new(None));
        prepare(&cache, &source, &python, &cancel, &process).unwrap();
        let paths = ready_in(&cache, &source).unwrap();
        assert!(paths.iter().all(|p| p.is_file()));
        assert_eq!(digest(&source).unwrap(), before);
        let manifest = amp_core::catalog_db::local_record(
            &cache.join("stems.ampdb"),
            &stamp(&source).unwrap(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(manifest["source_sha256"], before);
        assert!(manifest["audio"]["frames"].as_u64().unwrap() > 44100);
        let decode = |path: &Path| {
            let output = amp_core::command("ffmpeg")
                .args(["-v", "error", "-i"])
                .arg(path)
                .args(["-ac", "2", "-ar", "44100", "-f", "f32le", "pipe:1"])
                .output()
                .unwrap();
            assert!(output.status.success());
            output
                .stdout
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect::<Vec<_>>()
        };
        let transported = dir.path().join("transported");
        fs::create_dir(&transported).unwrap();
        fs::copy(cache.join("stems.ampdb"), transported.join("stems.ampdb")).unwrap();
        let renamed = dir.path().join("renamed.wav");
        fs::copy(&source, &renamed).unwrap();
        assert!(restore_cached(&transported, &renamed).unwrap());
        let restored = ready_in(&transported, &renamed).unwrap();
        for (a, b) in paths.iter().zip(restored.iter()) {
            assert_eq!(digest(a).unwrap(), digest(b).unwrap());
        }
        let original = decode(&source);
        let vocals = decode(&paths[0]);
        let instruments = decode(&paths[1]);
        let drums = decode(&paths[2]);
        assert_eq!(original.len(), drums.len());
        assert_eq!(original.len(), vocals.len());
        assert_eq!(original.len(), instruments.len());
        assert!(
            original
                .iter()
                .zip(vocals.iter().zip(instruments.iter().zip(&drums)))
                .all(|(o, (v, (i, d)))| (o - (v + i + d)).abs() < 0.00001)
        );

        // A completed local cache is reused, even without invoking another worker.
        prepare(
            &cache,
            &source,
            Path::new("not-an-interpreter"),
            &cancel,
            &process,
        )
        .unwrap();
    }
    #[test]
    fn canceled_preparation_does_not_publish_or_change_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("song.wav");
        fs::write(&source, b"source bytes").unwrap();
        let before = digest(&source).unwrap();
        let cache = dir.path().join("cache");
        let result = prepare(
            &cache,
            &source,
            Path::new("not-an-interpreter"),
            &AtomicBool::new(true),
            &Arc::new(Mutex::new(None)),
        );
        assert!(result.unwrap_err().contains("canceled"));
        assert!(ready_in(&cache, &source).is_none());
        assert_eq!(digest(&source).unwrap(), before);
        assert!(!fs::read_dir(&cache).unwrap().any(|p| {
            p.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".pending-")
        }));
    }
    #[test]
    fn cache_invalidation_and_incomplete_outputs_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("song.wav");
        fs::write(&source, b"original").unwrap();
        let cache = dir.path().join("cache");
        let id = stamp(&source).unwrap();
        let target = cache.join(&id);
        fs::create_dir_all(&target).unwrap();
        assert!(ready_in(&cache, &source).is_none());
        for name in ["vocals", "instrumental", "drums"] {
            fs::write(target.join(format!("{name}.wav")), b"stem").unwrap();
        }
        fs::write(target.join("manifest.json"),serde_json::to_vec(&json!({"source_stamp":id,"audio":{"engine":"demucs-4.0.1/htdemucs-3stem","files":{"vocals":{"bytes":4},"instrumental":{"bytes":4},"drums":{"bytes":4}}}})).unwrap()).unwrap();
        assert!(ready_in(&cache, &source).is_some());
        fs::write(&source, b"new version").unwrap();
        assert!(ready_in(&cache, &source).is_none());
        assert_eq!(fs::read(&source).unwrap(), b"new version");
    }
}
