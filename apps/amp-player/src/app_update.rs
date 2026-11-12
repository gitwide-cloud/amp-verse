//! One explicit Update action: resolve release, verify, stage, hand off installation.
use crate::update_package::Manifest;
use eframe::egui;
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
pub const GITHUB: &str = "https://github.com/gitwide-cloud/amp-verse";
#[derive(Default)]
pub struct Updater {
    rx: Option<mpsc::Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    status: String,
    loaded_result: bool,
}
enum Event {
    Progress(String),
    Ready(PathBuf),
    Finished(Result<(), String>),
}
fn release_url(tag: &str, name: &str) -> Result<String, String> {
    let mut url = url::Url::parse(&format!("{GITHUB}/releases/download/")).unwrap();
    url.path_segments_mut()
        .map_err(|_| "Invalid release URL")?
        .pop_if_empty()
        .push(tag)
        .push(name);
    Ok(url.to_string())
}
fn read(
    url: &str,
    limit: u64,
    cancel: &AtomicBool,
    tx: &mpsc::Sender<Event>,
) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(300)))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .header("User-Agent", "AMPVerse-Updater")
        .call()
        .map_err(|e| format!("Update download failed: {e}"))?;
    let mut output = Vec::new();
    let mut buffer = [0; 65536];
    let mut input = response.body_mut().as_reader();
    let mut last = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Update cancelled; installed files were kept".into());
        }
        let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if output.len() as u64 + n as u64 > limit {
            return Err("Update exceeds expected size".into());
        }
        output.extend_from_slice(&buffer[..n]);
        if output.len() / 1048576 > last {
            last = output.len() / 1048576;
            let _ = tx.send(Event::Progress(format!("Downloading… {last} MB")));
        }
    }
    Ok(output)
}
fn stage(cancel: &AtomicBool, tx: &mpsc::Sender<Event>) -> Result<Option<PathBuf>, String> {
    let info = read(
        "https://api.github.com/repos/gitwide-cloud/amp-verse/releases/latest",
        2 * 1024 * 1024,
        cancel,
        tx,
    )?;
    let info: serde_json::Value = serde_json::from_slice(&info).map_err(|e| e.to_string())?;
    if info["draft"] != false || info["prerelease"] != false {
        return Err("No stable update is published".into());
    }
    let tag = info["tag_name"].as_str().ok_or("Release version missing")?;
    let current = crate::update_package::version(env!("CARGO_PKG_VERSION"))?;
    if crate::update_package::version(tag)? <= current {
        return Ok(None);
    }
    let data = read(&release_url(tag, "amp-update.json")?, 1048576, cancel, tx)?;
    let manifest: Manifest =
        serde_json::from_slice(&data).map_err(|e| format!("Invalid update manifest: {e}"))?;
    if manifest.schema != 1
        || crate::update_package::version(&manifest.version)?
            != crate::update_package::version(tag)?
    {
        return Err("Update manifest version mismatch".into());
    }
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let package = manifest
        .packages
        .get(&platform)
        .filter(|p| crate::update_package::valid_package(p) && p.name.ends_with(".zip"))
        .ok_or("No compatible update package is published for this platform")?;
    let _ = tx.send(Event::Progress(format!(
        "Downloading {}…",
        manifest.version
    )));
    let bytes = read(&release_url(tag, &package.name)?, package.size, cancel, tx)?;
    let root = crate::theme::settings_path()
        .ok_or("Settings folder unavailable")?
        .parent()
        .ok_or("Settings folder unavailable")?
        .join("updates");
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let temp = tempfile::TempDir::new_in(&root).map_err(|e| e.to_string())?;
    let archive = temp.path().join(&package.name);
    std::fs::write(&archive, bytes).map_err(|e| e.to_string())?;
    crate::update_package::verify(&archive, package)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Update cancelled".into());
    }
    let stage = temp.path().join("files");
    let files = crate::update_package::extract(&archive, &stage)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let destination = exe
        .parent()
        .ok_or("Application folder missing")?
        .to_path_buf();
    let player = if cfg!(windows) {
        "amp-player.exe"
    } else {
        "amp-player"
    };
    let helper = if cfg!(windows) {
        "amp-update.exe"
    } else {
        "amp-update"
    };
    if !files.contains(&PathBuf::from(player)) || !files.contains(&PathBuf::from(helper)) {
        return Err("Update package is incomplete; current installation retained".into());
    }
    let probe = tempfile::NamedTempFile::new_in(&destination)
        .map_err(|e| format!("Application folder is not writable: {e}"))?;
    drop(probe);
    let runner = temp.path().join(helper);
    std::fs::copy(stage.join(helper), &runner).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    let job = temp.path().join("job.json");
    let result = root.join("last-result.txt");
    let mut output = std::fs::File::create(&job).map_err(|e| e.to_string())?;
    output.write_all(&serde_json::to_vec(&serde_json::json!({"parent":std::process::id(),"destination":destination,"stage":stage,"files":files,"result":result})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    output.sync_all().map_err(|e| e.to_string())?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Update cancelled".into());
    }
    let kept = temp.keep();
    Ok(Some(kept.join("job.json")))
}
impl Updater {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        if !self.loaded_result {
            self.loaded_result = true;
            if let Some(path) = crate::theme::settings_path()
                .and_then(|p| p.parent().map(|p| p.join("updates/last-result.txt")))
                && let Ok(result) = std::fs::read_to_string(path)
            {
                self.status = result;
            }
        }
        let mut disconnected = false;
        let mut events = Vec::new();
        if let Some(rx) = &self.rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => events.push(event),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        for event in events {
            match event {
                Event::Progress(s) => self.status = s,
                Event::Finished(result) => {
                    self.status = result.err().unwrap_or_else(|| "Already up to date".into());
                }
                Event::Ready(job) => {
                    if self.cancel.load(Ordering::Relaxed) {
                        self.status = "Update cancelled; downloaded files retained".into();
                        continue;
                    }
                    let helper = job.parent().unwrap().join(if cfg!(windows) {
                        "amp-update.exe"
                    } else {
                        "amp-update"
                    });
                    let mut cmd = std::process::Command::new(helper);
                    cmd.arg(job);
                    #[cfg(windows)]
                    {
                        use std::os::windows::process::CommandExt;
                        cmd.creation_flags(0x08000000);
                    }
                    match cmd.spawn() {
                        Ok(_) => {
                            self.status = "Installing update and restarting…".into();
                            ui.ctx().send_viewport_cmd_to(
                                egui::ViewportId::ROOT,
                                egui::ViewportCommand::Close,
                            );
                        }
                        Err(e) => self.status = format!("Could not start installer: {e}"),
                    }
                }
            }
        }
        if disconnected {
            self.rx = None;
        }
        let running = self.rx.is_some();
        if ui
            .add_enabled(
                !running,
                egui::Button::new(if running { "Updating…" } else { "Update" }),
            )
            .clicked()
        {
            self.cancel = Arc::new(AtomicBool::new(false));
            let cancel = self.cancel.clone();
            let (tx, rx) = mpsc::channel();
            self.rx = Some(rx);
            self.status = "Checking for updates…".into();
            std::thread::spawn(move || match stage(&cancel, &tx) {
                Ok(Some(job)) => {
                    let _ = tx.send(Event::Ready(job));
                }
                Ok(None) => {
                    let _ = tx.send(Event::Finished(Ok(())));
                }
                Err(e) => {
                    let _ = tx.send(Event::Finished(Err(e)));
                }
            });
        }
        ui.hyperlink_to("GitHub", GITHUB);
        if running && ui.button("Cancel update").clicked() {
            self.cancel.store(true, Ordering::Relaxed);
            self.status = "Cancelling…".into();
        }
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        ui.weak(format!(
            "Installed {} · Update downloads, installs and restarts the app.",
            env!("CARGO_PKG_VERSION")
        ));
    }
}
