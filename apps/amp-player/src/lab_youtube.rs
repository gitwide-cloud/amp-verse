//! AMP Lab-only URL import. No shell, cookies, user yt-dlp config, or hidden playlist expansion.
use eframe::egui;
use process_wrap::std::*;
use std::{
    fs,
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

pub fn video_url(input: &str) -> Result<String, String> {
    let url = url::Url::parse(input.trim())
        .map_err(|_| "Paste a complete YouTube video URL.".to_owned())?;
    if !matches!(url.scheme(), "https" | "http")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err("Use an ordinary https:// YouTube video URL.".into());
    }
    let host = url.host_str().unwrap_or("");
    let id = match host {
        "youtu.be" => url
            .path()
            .strip_prefix('/')
            .filter(|s| !s.contains('/'))
            .map(str::to_owned),
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com" => {
            if url.path() == "/watch" {
                url.query_pairs()
                    .find(|(k, _)| k == "v")
                    .map(|(_, v)| v.into_owned())
            } else {
                url.path()
                    .strip_prefix("/shorts/")
                    .or_else(|| url.path().strip_prefix("/live/"))
                    .or_else(|| url.path().strip_prefix("/embed/"))
                    .map(str::to_owned)
            }
        }
        _ => return Err("Only YouTube video URLs are supported.".into()),
    }
    .ok_or("Choose one video, not a playlist or channel URL.")?;
    if id.len() != 11
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("That URL does not contain a valid YouTube video ID.".into());
    }
    Ok(format!("https://www.youtube.com/watch?v={id}"))
}

fn playlist_url(input: &str) -> Result<String, String> {
    let u = url::Url::parse(input.trim()).map_err(|_| "Paste a complete YouTube playlist URL.")?;
    if !matches!(u.scheme(), "http" | "https")
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
        || !matches!(
            u.host_str(),
            Some("youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com")
        )
        || !matches!(u.path(), "/playlist" | "/watch")
    {
        return Err("Use a YouTube playlist or video-with-playlist URL.".into());
    }
    let id = u
        .query_pairs()
        .find(|(k, _)| k == "list")
        .map(|(_, v)| v.into_owned())
        .ok_or("This URL has no playlist ID.")?;
    if !(2..=150).contains(&id.len())
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("Invalid playlist ID.".into());
    }
    Ok(format!("https://www.youtube.com/playlist?list={id}"))
}
fn playlist_entries(info: &serde_json::Value) -> Result<Vec<String>, String> {
    let entries = info["entries"]
        .as_array()
        .ok_or("No playlist entries returned.")?;
    if entries.is_empty() || entries.len() > 500 {
        return Err("Choose a playlist containing 1–500 videos.".into());
    }
    let mut urls = Vec::new();
    for entry in entries {
        if let Some(id) = entry["id"].as_str()
            && let Ok(url) = video_url(&format!("https://www.youtube.com/watch?v={id}"))
            && !urls.contains(&url)
        {
            urls.push(url);
        }
    }
    if urls.is_empty() {
        return Err("No available video IDs in this playlist.".into());
    }
    Ok(urls)
}
fn save_download_playlist(paths: &[PathBuf], dir: &Path, name: &str) -> Result<PathBuf, String> {
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    writeln!(file, "#EXTM3U").map_err(|e| e.to_string())?;
    for path in paths {
        let relative = path
            .strip_prefix(dir)
            .map_err(|_| "Unexpected playlist file location")?;
        writeln!(file, "{}", relative.to_string_lossy()).map_err(|e| e.to_string())?;
    }
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    let name = safe_name(name);
    for i in 0..10000 {
        let path = dir.join(if i == 0 {
            format!("{name}.m3u8")
        } else {
            format!("{name} ({i}).m3u8")
        });
        match file.persist_noclobber(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => file = e.file,
            Err(e) => return Err(e.error.to_string()),
        }
    }
    Err("Too many playlists with this name".into())
}

fn playlist_download(
    url: &str,
    dir: &Path,
    tools: &Tools,
    cancel: &AtomicBool,
    tx: &mpsc::SyncSender<Event>,
) -> Result<(Vec<PathBuf>, String), String> {
    let mut cmd = tools.yt_command();
    cmd.args([
        "--yes-playlist",
        "--flat-playlist",
        "--playlist-end",
        "501",
        "--dump-single-json",
        "--skip-download",
        "--",
    ])
    .arg(url);
    progress(tx, "Reading playlist…", None);
    let raw = run(cmd, cancel, tx, Duration::from_secs(180))?;
    let info: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| "Invalid playlist information")?;
    let urls = playlist_entries(&info)?;
    let mut saved = Vec::new();
    let mut errors = Vec::new();
    for (i, url) in urls.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        progress(
            tx,
            &format!("Playlist item {} / {}", i + 1, urls.len()),
            None,
        );
        match download(url, dir, tools, cancel, tx) {
            Ok(path) => {
                let _ = tx.send(Event::Saved(path.clone()));
                saved.push(path);
            }
            Err(e) => {
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                errors.push(format!("Item {}: {e}", i + 1));
            }
        }
    }
    let state = if cancel.load(Ordering::SeqCst) {
        "Cancelled"
    } else {
        "Finished"
    };
    let message = format!(
        "{state}: {} MP3s saved, {} failed. Completed files are kept.{}",
        saved.len(),
        errors.len(),
        errors.first().map(|e| format!("\n{e}")).unwrap_or_default()
    );
    let playlist = if saved.is_empty() {
        String::new()
    } else {
        match save_download_playlist(
            &saved,
            dir,
            info["title"].as_str().unwrap_or("YouTube playlist"),
        ) {
            Ok(path) => format!("\nPlaylist saved: {}", path.display()),
            Err(e) => format!("\nMP3s kept, but could not save the playlist: {e}"),
        }
    };
    Ok((saved, format!("{message}{playlist}")))
}

fn downloads() -> Result<PathBuf, String> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
    };
    base.map(|p| {
        p.join(if cfg!(windows) {
            "AMPVerse/AMP Lab/downloads"
        } else {
            "amp-verse/lab/downloads"
        })
    })
    .ok_or("Your local downloads directory is unavailable.".into())
}

#[derive(Clone)]
struct Tools {
    yt: PathBuf,
    deno: PathBuf,
    ffmpeg: PathBuf,
    probe: PathBuf,
}
pub(super) fn audio_command(name: &str) -> Command {
    match locate(name) {
        Ok(path) => {
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                let mut command = Command::new(path);
                command.creation_flags(0x08000000);
                command
            }
            #[cfg(not(windows))]
            {
                Command::new(path)
            }
        }
        Err(_) => amp_core::command(name),
    }
}
fn locate(name: &str) -> Result<PathBuf, String> {
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    };
    let mut dirs = Vec::new();
    if let Some(dir) = crate::theme::settings_path()
        .and_then(|p| fs::read_to_string(p.with_file_name("lab-tools-directory.txt")).ok())
    {
        dirs.push(PathBuf::from(dir.trim()));
    }
    if let Some(dir) = std::env::var_os("AMP_LAB_TOOLS") {
        dirs.push(dir.into());
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        dirs.push(dir.join("tools/lab-youtube"));
        dirs.push(dir);
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.into_iter()
        .map(|p| p.join(&file))
        .find(|p| p.is_file())
        .ok_or_else(|| format!("Missing {name}. Use Locate tools or Setup instructions below."))
}
impl Tools {
    fn discover() -> Result<Self, String> {
        Ok(Self {
            yt: locate("yt-dlp")?,
            deno: locate("deno")?,
            ffmpeg: locate("ffmpeg")?,
            probe: locate("ffprobe")?,
        })
    }
    fn yt_command(&self) -> Command {
        let mut cmd = Command::new(&self.yt);
        cmd.args([
            "--ignore-config",
            "--no-plugin-dirs",
            "--no-playlist",
            "--no-update",
            "--socket-timeout",
            "20",
            "--retries",
            "2",
            "--fragment-retries",
            "2",
            "--no-js-runtimes",
            "--js-runtimes",
        ])
        .arg(format!("deno:{}", self.deno.display()))
        .args(["--remote-components", "ejs:github"]);
        cmd
    }
}

enum Event {
    Progress(String, Option<f32>),
    Finished(Result<PathBuf, String>),
    Saved(PathBuf),
    Batch(Result<(Vec<PathBuf>, String), String>),
}
struct Job {
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Event>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}
#[derive(Default)]
pub struct Panel {
    url: String,
    playlist: bool,
    completed: Vec<PathBuf>,
    status: String,
    progress: Option<f32>,
    job: Option<Job>,
    ready: Option<PathBuf>,
    open_pending: bool,
    failed: bool,
}
impl Panel {
    fn start(&mut self) {
        let playlist = self.playlist;
        let url = match if playlist {
            playlist_url(&self.url)
        } else {
            video_url(&self.url)
        } {
            Ok(u) => u,
            Err(e) => {
                self.status = e;
                self.failed = true;
                return;
            }
        };
        let (tx, rx) = mpsc::sync_channel(64);
        let cancel = Arc::new(AtomicBool::new(false));
        self.job = Some(Job {
            cancel: cancel.clone(),
            rx,
        });
        self.progress = None;
        self.failed = false;
        self.status = "Checking download tools…".into();
        thread::spawn(move || {
            if playlist {
                let result = Tools::discover().and_then(|tools| {
                    downloads().and_then(|dir| playlist_download(&url, &dir, &tools, &cancel, &tx))
                });
                let _ = tx.send(Event::Batch(result));
                return;
            }
            let result = Tools::discover().and_then(|tools| {
                downloads().and_then(|dir| download(&url, &dir, &tools, &cancel, &tx))
            });
            let _ = tx.send(Event::Finished(result));
        });
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        let mut finished = None;
        if let Some(job) = &self.job {
            loop {
                match job.rx.try_recv() {
                    Ok(Event::Progress(s, p)) => {
                        if !job.cancel.load(Ordering::SeqCst) {
                            self.status = s;
                            self.progress = p;
                        }
                    }
                    Ok(Event::Saved(path)) => {
                        self.completed.push(path);
                    }
                    Ok(Event::Batch(result)) => {
                        match result {
                            Ok((_, message)) => {
                                self.status = message;
                                self.failed = false;
                            }
                            Err(e) => {
                                self.status = e;
                                self.failed = true;
                            }
                        }
                        finished = Some(Err(String::new()));
                        break;
                    }
                    Ok(Event::Finished(result)) => {
                        finished = Some(result);
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = Some(Err("Download worker interrupted. Completed MP3s were kept; Retry to start again.".into()));
                        break;
                    }
                }
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if let Some(result) = finished {
            self.job = None;
            self.progress = None;
            if !result.as_ref().is_err_and(|e| e.is_empty()) {
                self.failed = result.is_err();
            }
            match result {
                Ok(path) => {
                    self.status = "MP3 saved. Opening in AMP Lab…".into();
                    self.ready = Some(path);
                    self.open_pending = true;
                }
                Err(e) => {
                    if !e.is_empty() {
                        self.status = e;
                    }
                }
            }
        }
        if self.open_pending {
            self.open_pending = false;
            return self.ready.clone();
        }
        None
    }
    pub fn opened(&mut self, loaded: bool) {
        self.status = if loaded {
            "MP3 saved and opened in AMP Lab."
        } else {
            "MP3 saved. Current edits were kept; use Open downloaded MP3 after exporting."
        }
        .into();
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        crate::skin::panel(ui, false, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("YouTube URL");
                ui.add_enabled(
                    self.job.is_none(),
                    egui::Checkbox::new(&mut self.playlist, "Playlist"),
                );
                ui.add_enabled(
                    self.job.is_none(),
                    egui::TextEdit::singleline(&mut self.url)
                        .id(egui::Id::unique("lab-youtube-url"))
                        .desired_width((ui.available_width() - 230.).max(130.))
                        .hint_text("Paste a YouTube video URL…"),
                );
                if ui
                    .add_enabled(self.job.is_none(), egui::Button::new("Download MP3"))
                    .clicked()
                {
                    self.start();
                }
                if let Some(job) = &self.job
                    && ui.button("Cancel").clicked()
                {
                    job.cancel.store(true, Ordering::SeqCst);
                    self.status = "Cancelling and removing incomplete files…".into();
                }
                if self.job.is_none()
                    && !self.status.is_empty()
                    && ui.small_button("Retry").clicked()
                {
                    self.start();
                }
                if self.job.is_none()
                    && self.ready.is_some()
                    && ui.small_button("Open downloaded MP3").clicked()
                {
                    self.open_pending = true;
                }
            });
            if !self.completed.is_empty() {
                egui::CollapsingHeader::new(format!("Downloaded MP3s ({})", self.completed.len()))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(140.)
                            .show(ui, |ui| {
                                for path in &self.completed {
                                    if ui
                                        .button(
                                            path.file_name().unwrap_or_default().to_string_lossy(),
                                        )
                                        .clicked()
                                    {
                                        self.ready = Some(path.clone());
                                        self.open_pending = true;
                                    }
                                }
                            });
                    });
            }
            if !self.status.is_empty() {
                ui.label(&self.status);
            }
            if self.job.is_some() {
                if let Some(p) = self.progress {
                    ui.add(egui::ProgressBar::new(p).show_percentage());
                } else {
                    ui.spinner();
                }
            }
            egui::CollapsingHeader::new("Downloads / tools").id_salt("lab-youtube-tools").show(ui, |ui| {
                ui.weak("One video · MP3 256 kbps · conversion does not improve source quality.");
                if let Ok(dir) = downloads() { ui.label(format!("Original MP3s: {}", dir.display())); }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Locate tools…").clicked() && let Some(dir) = rfd::FileDialog::new().set_title("Folder containing yt-dlp, Deno or FFmpeg tools").pick_folder() {
                        self.status = crate::theme::settings_path().ok_or_else(|| "Settings directory unavailable".to_owned()).and_then(|p| {
                            let p = p.with_file_name("lab-tools-directory.txt"); fs::create_dir_all(p.parent().unwrap()).and_then(|_| fs::write(p, dir.to_string_lossy().as_bytes())).map_err(|e| e.to_string())
                        }).map(|_| "Tools folder saved. Retry the download.".into()).unwrap_or_else(|e| e);
                    }
                    ui.hyperlink_to("Setup instructions", "https://github.com/gitwide-cloud/amp-verse/blob/feature/six-deck-controls/docs/LAB_YOUTUBE.md");
                });
            });
        });
    }
}

fn progress(tx: &mpsc::SyncSender<Event>, status: &str, value: Option<f32>) {
    let _ = tx.try_send(Event::Progress(status.into(), value));
}
fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::SeqCst) {
        Err("Cancelled. Completed downloads were kept; incomplete files were removed.".into())
    } else {
        Ok(())
    }
}

// Each stage owns a process tree: a Windows Job Object or a Unix process group.
// Job Object setup suspends the child until it is assigned, avoiding fork races.
fn run(
    mut cmd: Command,
    cancel: &AtomicBool,
    tx: &mpsc::SyncSender<Event>,
    limit: Duration,
) -> Result<String, String> {
    cancelled(cancel)?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut command = CommandWrap::from(cmd);
    #[cfg(windows)]
    {
        command
            .wrap(CreationFlags(
                windows::Win32::System::Threading::CREATE_NO_WINDOW
                    | windows::Win32::System::Threading::BELOW_NORMAL_PRIORITY_CLASS,
            ))
            .wrap(JobObject);
    }
    #[cfg(unix)]
    {
        command.wrap(ProcessGroup::leader());
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not start download tool: {e}. Check Downloads / tools."))?;
    let (lines_tx, lines_rx) = mpsc::sync_channel(64);
    let pipes: Vec<Box<dyn Read + Send>> = vec![
        Box::new(child.stdout().take().unwrap()),
        Box::new(child.stderr().take().unwrap()),
    ];
    let readers: Vec<_> = pipes
        .into_iter()
        .enumerate()
        .map(|(index, pipe)| {
            let tx = lines_tx.clone();
            thread::spawn(move || {
                let mut reader = io::BufReader::new(pipe);
                loop {
                    let mut line = Vec::new();
                    match reader
                        .by_ref()
                        .take(2 * 1024 * 1024)
                        .read_until(b'\n', &mut line)
                    {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            if tx
                                .send((index, String::from_utf8_lossy(&line).into_owned()))
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            })
        })
        .collect();
    drop(lines_tx);
    let started = Instant::now();
    let mut stdout = String::new();
    let mut errors = String::new();
    let mut status = None;
    let mut failure = None;
    loop {
        if cancel.load(Ordering::SeqCst) {
            failure = cancelled(cancel).err();
            break;
        }
        if started.elapsed() > limit {
            failure = Some("The download tool timed out. Check your connection and Retry.".into());
            break;
        }
        match lines_rx.recv_timeout(Duration::from_millis(30)) {
            Ok((index, line)) => {
                if let Some(p) = parse_progress(&line) {
                    progress(tx, "Downloading audio…", Some(p));
                }
                if let Some(value) = line
                    .strip_prefix("out_time_us=")
                    .and_then(|s| s.trim().parse::<f64>().ok())
                {
                    progress(
                        tx,
                        &format!(
                            "Converting to MP3 · {:.0} seconds processed",
                            value / 1_000_000.
                        ),
                        None,
                    );
                }
                if index == 0 {
                    if stdout.len() + line.len() > 8 * 1024 * 1024 {
                        failure = Some("Download tool returned too much output.".into());
                        break;
                    }
                    stdout.push_str(&line);
                } else {
                    errors.push_str(&line);
                    if errors.len() > 8192 {
                        let keep = errors.char_indices().rev().nth(4095).map_or(0, |(i, _)| i);
                        errors = errors[keep..].into();
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => match child.try_wait() {
                Ok(Some(s)) => {
                    status = Some(s);
                    break;
                }
                Err(e) => {
                    failure = Some(e.to_string());
                    break;
                }
                _ => {}
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        // Do not finish until both streams have been drained.
    }
    let _ = child.kill();
    let _ = child.wait();
    drop(lines_rx);
    for reader in readers {
        let _ = reader.join();
    }
    if let Some(e) = failure {
        return Err(e);
    }
    if !status.is_some_and(|s| s.success()) {
        return Err(readable_error(&errors));
    }
    Ok(stdout)
}
fn parse_progress(line: &str) -> Option<f32> {
    line.strip_prefix("AMP_PROGRESS:")?
        .trim()
        .trim_end_matches('%')
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|p| p.is_finite())
        .map(|p| (p / 100.).clamp(0., 1.))
}
fn readable_error(detail: &str) -> String {
    let lower = detail.to_lowercase();
    let reason = if lower.contains("private")
        || lower.contains("sign in")
        || lower.contains("unavailable")
        || lower.contains("removed")
        || lower.contains("age-restricted")
    {
        "Video unavailable or restricted. Choose a public, accessible video."
    } else if lower.contains("no space")
        || lower.contains("permission denied")
        || lower.contains("disk")
    {
        "Cannot save audio. Check free disk space and folder permissions."
    } else if lower.contains("timed out")
        || lower.contains("network")
        || lower.contains("resolve")
        || lower.contains("http error")
    {
        "Network/download failed. Check the connection, then Retry."
    } else if lower.contains("javascript") || lower.contains("ejs") || lower.contains("challenge") {
        "YouTube extraction failed. Update yt-dlp, EJS and Deno using Setup instructions."
    } else {
        "Audio download or conversion failed. Retry or check the download tools."
    };
    let detail: String = detail
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(700)
        .collect();
    format!("{reason}\n{}", detail.trim())
}
fn download(
    url: &str,
    dir: &Path,
    tools: &Tools,
    cancel: &AtomicBool,
    tx: &mpsc::SyncSender<Event>,
) -> Result<PathBuf, String> {
    let short = Duration::from_secs(45);
    let mut deno = Command::new(&tools.deno);
    deno.arg("--version");
    let version = run(deno, cancel, tx, short)?;
    let version = version.split_whitespace().nth(1).unwrap_or("");
    let parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    if parts.len() < 2 || (parts[0], parts[1]) < (2, 3) {
        return Err(
            "Deno 2.3.0 or newer is required. Use Setup instructions to update tools.".into(),
        );
    }
    for path in [&tools.ffmpeg, &tools.probe] {
        let mut cmd = Command::new(path);
        cmd.arg("-version");
        run(cmd, cancel, tx, short)?;
    }
    fs::create_dir_all(dir)
        .map_err(|e| format!("Cannot create AMP Lab downloads directory: {e}"))?;
    let work = tempfile::Builder::new()
        .prefix(".amp-lab-download-")
        .tempdir_in(dir)
        .map_err(|e| format!("Cannot write downloads directory: {e}"))?;
    progress(tx, "Reading video information…", None);
    let mut info = tools.yt_command();
    info.args([
        "--dump-single-json",
        "--skip-download",
        "--no-progress",
        "--",
    ])
    .arg(url);
    let text = run(info, cancel, tx, Duration::from_secs(120))?;
    let info: serde_json::Value = serde_json::from_str(&text)
        .map_err(|_| "yt-dlp returned invalid video information".to_owned())?;
    if info.get("entries").is_some() || info["is_live"] == true {
        return Err("Choose a single, completed video, not a playlist or live broadcast.".into());
    }
    let id = info["id"].as_str().ok_or("Video information has no ID")?;
    if !url.ends_with(&format!("v={id}")) {
        return Err("Video identity changed; no audio was imported.".into());
    }
    let name = safe_name(info["title"].as_str().unwrap_or(id));
    progress(tx, "Downloading audio…", Some(0.));
    let mut cmd = tools.yt_command();
    cmd.args([
        "--no-simulate",
        "-f",
        "bestaudio/best",
        "--newline",
        "--progress",
        "--progress-delta",
        "0.25",
        "--progress-template",
        "download:AMP_PROGRESS:%(progress._percent_str)s",
        "--print",
        "after_move:AMP_FILE:%(filepath)j",
        "-o",
    ])
    .arg(work.path().join("source.%(ext)s"))
    .arg("--")
    .arg(url);
    let output = run(cmd, cancel, tx, Duration::from_secs(6 * 60 * 60))?;
    let source = output
        .lines()
        .find_map(|line| {
            line.strip_prefix("AMP_FILE:")
                .and_then(|s| serde_json::from_str::<String>(s).ok())
        })
        .map(PathBuf::from)
        .ok_or("Download finished without an audio file. Update yt-dlp and Retry.")?;
    let source = source
        .canonicalize()
        .map_err(|e| format!("Downloaded audio missing: {e}"))?;
    if !source.starts_with(work.path().canonicalize().map_err(|e| e.to_string())?)
        || !source.is_file()
    {
        return Err("Downloader returned an unexpected file location.".into());
    }
    cancelled(cancel)?;
    progress(tx, "Converting to MP3 · 256 kbps…", None);
    let converted = work.path().join("converted.mp3");
    let mut cmd = Command::new(&tools.ffmpeg);
    cmd.args([
        "-hide_banner",
        "-nostdin",
        "-n",
        "-loglevel",
        "error",
        "-progress",
        "pipe:1",
        "-nostats",
        "-i",
    ])
    .arg(&source)
    .args([
        "-map",
        "0:a:0",
        "-vn",
        "-c:a",
        "libmp3lame",
        "-threads",
        "2",
        "-b:a",
        "256k",
        "-id3v2_version",
        "3",
    ])
    .arg(&converted);
    run(cmd, cancel, tx, Duration::from_secs(6 * 60 * 60))?;
    let mut cmd = Command::new(&tools.probe);
    cmd.args([
        "-v",
        "error",
        "-show_entries",
        "format=duration",
        "-of",
        "default=noprint_wrappers=1:nokey=1",
    ])
    .arg(&converted);
    let duration = run(cmd, cancel, tx, short)?
        .trim()
        .parse::<f64>()
        .unwrap_or(0.);
    if !duration.is_finite() || duration <= 0. {
        return Err("Converted MP3 is empty or invalid; original editor audio was kept.".into());
    }
    publish(&converted, dir, &format!("{name} [{id}]"), cancel)
}
fn safe_name(title: &str) -> String {
    let mut name = String::new();
    for c in title.chars() {
        let c = if c.is_control() || "<>:\"/\\|?*".contains(c) {
            '_'
        } else {
            c
        };
        if name.len() + c.len_utf8() > 120 {
            break;
        }
        name.push(c);
    }
    let name = name.trim_matches([' ', '.']);
    if name.is_empty() {
        "YouTube audio".into()
    } else {
        let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
        let reserved = matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
        if reserved {
            format!("_{name}")
        } else {
            name.into()
        }
    }
}

fn publish(source: &Path, dir: &Path, name: &str, cancel: &AtomicBool) -> Result<PathBuf, String> {
    let mut tmp =
        tempfile::NamedTempFile::new_in(dir).map_err(|e| format!("Cannot save MP3: {e}"))?;
    let mut input = fs::File::open(source).map_err(|e| e.to_string())?;
    let mut buf = [0; 65536];
    loop {
        cancelled(cancel)?;
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        tmp.write_all(&buf[..n])
            .map_err(|e| format!("Cannot save MP3: {e}"))?;
    }
    tmp.as_file().sync_all().map_err(|e| e.to_string())?;
    for index in 0..10000 {
        cancelled(cancel)?;
        let target = dir.join(if index == 0 {
            format!("{name}.mp3")
        } else {
            format!("{name} ({index}).mp3")
        });
        match tmp.persist_noclobber(&target) {
            Ok(_) => return Ok(target),
            Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => tmp = e.file,
            Err(e) => return Err(format!("Cannot publish MP3: {}", e.error)),
        }
    }
    Err("Too many downloads with this filename; choose a different downloads folder.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn url_field_accepts_paste_and_worker_disconnect_is_recoverable() {
        let ctx = egui::Context::default();
        let mut panel = Panel::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900., 400.),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw.clone(), |ui| panel.show(ui));
        output.textures_delta.clear();
        ctx.memory_mut(|m| m.request_focus(egui::Id::unique("lab-youtube-url")));
        let mut input = raw;
        input
            .events
            .push(egui::Event::Paste("https://youtu.be/BaW_jenozKc".into()));
        let mut output = ctx.run_ui(input, |ui| panel.show(ui));
        output.textures_delta.clear();
        assert_eq!(panel.url, "https://youtu.be/BaW_jenozKc");
        let (tx, rx) = mpsc::sync_channel(1);
        drop(tx);
        panel.job = Some(Job {
            cancel: Arc::new(AtomicBool::new(false)),
            rx,
        });
        assert!(panel.poll(&ctx).is_none());
        assert!(panel.failed);
        assert!(panel.job.is_none());
        assert!(panel.status.contains("interrupted"));
    }
    #[test]
    fn batch_results_do_not_replace_unsaved_editor_automatically() {
        let (tx, rx) = mpsc::sync_channel(4);
        let mut panel = Panel {
            job: Some(Job {
                cancel: Arc::new(AtomicBool::new(false)),
                rx,
            }),
            ..Default::default()
        };
        tx.send(Event::Saved(PathBuf::from("saved.mp3"))).unwrap();
        tx.send(Event::Batch(Ok((
            vec![PathBuf::from("saved.mp3")],
            "Finished: 1 MP3 saved, 1 failed".into(),
        ))))
        .unwrap();
        assert!(panel.poll(&egui::Context::default()).is_none());
        assert_eq!(panel.completed.len(), 1);
        assert!(panel.status.contains("1 failed"));
        assert!(panel.job.is_none());
    }
    #[test]
    #[ignore = "requires AMP_LAB_YOUTUBE_FIXTURE executable, Deno and real FFmpeg"]
    fn local_playlist_keeps_success_when_an_item_is_private() {
        let mut tools = Tools::discover().unwrap();
        tools.yt = PathBuf::from(std::env::var_os("AMP_LAB_YOUTUBE_FIXTURE").unwrap());
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::sync_channel(64);
        let (saved, message) = playlist_download(
            "https://www.youtube.com/playlist?list=PLfixture",
            dir.path(),
            &tools,
            &AtomicBool::new(false),
            &tx,
        )
        .unwrap();
        assert_eq!(saved.len(), 1);
        assert!(saved[0].is_file());
        assert!(message.contains("1 failed"));
        assert!(crate::lab_editor::duration(&saved[0]).unwrap() > 0.);
        assert!(rx.try_iter().any(|e| matches!(e, Event::Saved(_))));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        let list = fs::read_to_string(dir.path().join("YouTube playlist.m3u8")).unwrap();
        assert!(list.starts_with("#EXTM3U\n"));
        assert!(list.contains("[BaW_jenozKc].mp3"));
        assert!(!list.contains("abcdefghijk"));
    }
    #[test]
    #[ignore = "requires AMP_LAB_YOUTUBE_FIXTURE executable, Deno and real FFmpeg"]
    fn local_pipeline_converts_reports_progress_cancels_tree_and_cleans_failures() {
        let fixture = PathBuf::from(std::env::var_os("AMP_LAB_YOUTUBE_FIXTURE").unwrap());
        let mut tools = Tools::discover().unwrap();
        tools.yt = fixture.clone();
        let dir = tempfile::tempdir().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel(64);
        let url = video_url("https://youtu.be/BaW_jenozKc?list=PLtest").unwrap();
        let first = download(&url, dir.path(), &tools, &cancel, &tx).unwrap();
        assert!(first.is_file());
        assert_eq!(first.extension().unwrap(), "mp3");
        assert!(crate::lab_editor::duration(&first).unwrap() > 0.);
        let (done_tx, done_rx) = mpsc::sync_channel(4);
        let mut panel = Panel {
            job: Some(Job {
                cancel: Arc::new(AtomicBool::new(false)),
                rx: done_rx,
            }),
            ..Default::default()
        };
        done_tx.send(Event::Finished(Ok(first.clone()))).unwrap();
        let delivered = panel.poll(&egui::Context::default()).unwrap();
        let mut editor = crate::lab_editor::Editor::default();
        assert!(editor.load_path(delivered));
        panel.opened(true);
        assert!(panel.status.contains("opened"));
        assert!(
            panel.poll(&egui::Context::default()).is_none(),
            "File opened twice"
        );
        let events: Vec<_> = rx.try_iter().collect();
        assert!(
            events
                .iter()
                .any(|e| matches!(e,Event::Progress(_,Some(p)) if *p>=0.5))
        );
        let blocked = tempfile::tempdir().unwrap();
        let file = blocked.path().join("not-a-directory");
        fs::write(&file, b"keep").unwrap();
        assert!(
            download(&url, &file, &tools, &cancel, &tx)
                .unwrap_err()
                .contains("Cannot create")
        );
        assert_eq!(fs::read(&file).unwrap(), b"keep");
        let mut missing = tools.clone();
        missing.deno = blocked.path().join("missing-deno");
        assert!(
            download(&url, dir.path(), &missing, &cancel, &tx)
                .unwrap_err()
                .contains("Could not start")
        );
        let original = fs::read(&first).unwrap();
        let second = download(&url, dir.path(), &tools, &cancel, &tx).unwrap();
        assert_ne!(first, second);
        assert_eq!(original, fs::read(&first).unwrap());
        for mode in ["private", "invalid", "network"] {
            tools.yt = fixture.with_file_name(if cfg!(windows) {
                format!("yt-dlp-{mode}.exe")
            } else {
                format!("yt-dlp-{mode}")
            });
            fs::copy(&fixture, &tools.yt).unwrap();
            let result = download(&url, dir.path(), &tools, &cancel, &tx);
            assert!(result.is_err(), "{mode}");
            assert_eq!(
                fs::read_dir(dir.path()).unwrap().count(),
                2,
                "Incomplete files left after {mode}"
            );
        }
        tools.yt = fixture.with_file_name(if cfg!(windows) {
            "yt-dlp-cancel.exe"
        } else {
            "yt-dlp-cancel"
        });
        fs::copy(&fixture, &tools.yt).unwrap();
        let heartbeat = dir.path().join("heartbeat");
        let observer = heartbeat.clone();
        let trigger = cancel.clone();
        let canceller = thread::spawn(move || {
            let start = Instant::now();
            while !observer.exists() && start.elapsed() < Duration::from_secs(15) {
                thread::sleep(Duration::from_millis(20));
            }
            trigger.store(true, Ordering::SeqCst);
        });
        let result = download(&url, dir.path(), &tools, &cancel, &tx);
        canceller.join().unwrap();
        assert!(result.unwrap_err().contains("Cancelled"));
        assert!(heartbeat.exists(), "Child process did not start");
        let size = fs::metadata(&heartbeat).unwrap().len();
        thread::sleep(Duration::from_millis(200));
        assert_eq!(
            size,
            fs::metadata(&heartbeat).unwrap().len(),
            "Grandchild survived Cancel"
        );
        assert!(fs::read_dir(dir.path()).unwrap().all(|e| {
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".amp-lab")
        }));
        assert_eq!(fs::read(first).unwrap(), original);
        let (tx, _rx) = mpsc::sync_channel(64);
        let token = Arc::new(AtomicBool::new(false));
        let stop = token.clone();
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            stop.store(true, Ordering::SeqCst);
        });
        let mut cmd = Command::new(&tools.ffmpeg);
        cmd.args([
            "-nostdin",
            "-n",
            "-loglevel",
            "error",
            "-progress",
            "pipe:1",
            "-re",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=30",
            "-c:a",
            "libmp3lame",
            "-b:a",
            "256k",
        ])
        .arg(dir.path().join("partial-conversion.mp3"));
        assert!(
            run(cmd, &token, &tx, Duration::from_secs(40))
                .unwrap_err()
                .contains("Cancelled")
        );
        canceller.join().unwrap();
    }
    #[test]
    #[ignore = "live YouTube access; requires yt-dlp, Deno and FFmpeg"]
    fn live_public_test_video() {
        let tools = Tools::discover().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = mpsc::sync_channel(64);
        let url = video_url(
            &std::env::var("AMP_LAB_LIVE_URL")
                .unwrap_or_else(|_| "https://www.youtube.com/watch?v=aqz-KE-bpKQ".into()),
        )
        .unwrap();
        let result = download(&url, dir.path(), &tools, &AtomicBool::new(false), &tx);
        assert!(result.is_ok(), "Live YouTube test: {result:?}");
    }
    #[test]
    fn playlist_validation_bounds_and_deduplication() {
        assert_eq!(
            playlist_url("https://youtube.com/watch?v=BaW_jenozKc&list=PL123").unwrap(),
            "https://www.youtube.com/playlist?list=PL123"
        );
        for u in [
            "https://youtube.com.evil/playlist?list=PL123",
            "https://user@youtube.com/playlist?list=PL123",
            "https://youtube.com/channel/abc?list=PL123",
            "https://youtu.be/BaW_jenozKc",
        ] {
            assert!(playlist_url(u).is_err());
        }
        let v = serde_json::json!({"entries":[{"id":"BaW_jenozKc"},null,{"id":"bad"},{"id":"BaW_jenozKc"}]});
        assert_eq!(playlist_entries(&v).unwrap().len(), 1);
        assert!(playlist_entries(&serde_json::json!({"entries":[]})).is_err());
        assert!(
            playlist_entries(&serde_json::json!({"entries":vec![serde_json::Value::Null;501]}))
                .is_err()
        );
    }
    #[test]
    fn validates_video_and_strips_playlist_and_tracking() {
        for url in [
            "https://youtu.be/BaW_jenozKc?list=PL123",
            "https://www.youtube.com/watch?v=BaW_jenozKc&list=PL123",
            "https://m.youtube.com/shorts/BaW_jenozKc",
        ] {
            assert_eq!(
                video_url(url).unwrap(),
                "https://www.youtube.com/watch?v=BaW_jenozKc"
            );
        }
        for url in [
            "https://youtube.com/playlist?list=PL123",
            "https://youtube.com.evil/watch?v=BaW_jenozKc",
            "file:///a",
            "https://user@youtube.com/watch?v=BaW_jenozKc",
            "--exec bad",
            "https://youtu.be/../abc",
            "https://youtube.com/watch?v=bad",
        ] {
            assert!(video_url(url).is_err(), "{url}");
        }
    }
    #[test]
    fn dependency_start_failure_and_invalid_url_are_recoverable() {
        let (tx, _rx) = mpsc::sync_channel(4);
        let error = run(
            Command::new("amp-lab-intentionally-missing-tool"),
            &AtomicBool::new(false),
            &tx,
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(error.contains("Check Downloads / tools"));
        let mut panel = Panel {
            url: "https://youtube.com/playlist?list=PLtest".into(),
            ..Default::default()
        };
        panel.start();
        assert!(panel.failed);
        assert!(panel.job.is_none());
        assert!(panel.status.contains("playlist"));
        assert!(safe_name(&"🎵".repeat(200)).len() <= 120);
        assert_eq!(safe_name("CON.track"), "_CON.track");
        assert_eq!(safe_name("lpt1"), "_lpt1");
    }
    #[test]
    fn progress_names_and_collision_safe_publication() {
        assert_eq!(parse_progress("AMP_PROGRESS: 12.5%"), Some(0.125));
        assert_eq!(parse_progress("AMP_PROGRESS: NA"), None);
        assert_eq!(safe_name("../a:b?*"), "_a_b__");
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("in");
        fs::write(&source, b"completed audio").unwrap();
        let cancel = AtomicBool::new(false);
        let a = publish(&source, dir.path(), "name", &cancel).unwrap();
        let b = publish(&source, dir.path(), "name", &cancel).unwrap();
        assert_ne!(a, b);
        assert_eq!(fs::read(a).unwrap(), b"completed audio");
        cancel.store(true, Ordering::SeqCst);
        assert!(publish(&source, dir.path(), "cancelled", &cancel).is_err());
        assert!(!dir.path().join("cancelled.mp3").exists());
    }
}
