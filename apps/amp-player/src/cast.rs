//! Native Cast dashboard. The independent server owns broadcasting, not this viewport.
use eframe::egui;
use serde::Deserialize;
use std::{sync::mpsc, time::Duration};

#[derive(Default, Deserialize)]
struct Status {
    #[serde(default)]
    server_id: String,
    #[serde(default)]
    station_id: String,
    running: bool,
    now_playing: Option<String>,
    error: Option<String>,
    tracks: usize,
    listeners: usize,
    password_required: bool,
    #[serde(default)]
    capabilities: serde_json::Value,
}
#[derive(Deserialize)]
struct Track {
    title: String,
}
#[derive(Default, Deserialize)]
struct Queue {
    tracks: Vec<Track>,
}
struct Snapshot {
    status: Status,
    queue: Queue,
    manifest: Option<serde_json::Value>,
}
struct Client {
    agent: ureq::Agent,
    base: String,
    password: String,
    identity: std::sync::Mutex<Option<(String, String)>>,
}
impl Client {
    fn new(address: &str, password: String) -> Result<Self, String> {
        let address = address.trim();
        // Only an authority is accepted: no URL paths, credentials or query parameters.
        if address.is_empty()
            || address
                .chars()
                .any(|c| c.is_whitespace() || "/\\@?#".contains(c))
            || !address.contains(':')
        {
            return Err("Enter a server host and port, for example 127.0.0.1:8787".into());
        }
        Ok(Self {
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(3)))
                .max_redirects(0)
                .proxy(None)
                .build()
                .into(),
            base: format!("http://{address}"),
            password,
            identity: std::sync::Mutex::new(None),
        })
    }
    fn read(&self) -> Result<Snapshot, String> {
        let status = self
            .agent
            .get(format!("{}/api/status", self.base))
            .call()
            .map_err(failure)?
            .body_mut()
            .read_json::<Status>()
            .map_err(failure)?;
        {
            let mut identity = self
                .identity
                .lock()
                .map_err(|_| "Station identity lock failed")?;
            if let Some((server, station)) = identity.as_ref() {
                if *server != status.server_id || *station != status.station_id {
                    return Err("Station changed. Reconnect before controlling it.".into());
                }
            } else {
                *identity = Some((status.server_id.clone(), status.station_id.clone()));
            }
        }
        let queue = self
            .agent
            .get(format!("{}/api/playlist", self.base))
            .call()
            .map_err(failure)?
            .body_mut()
            .read_json::<Queue>()
            .map_err(failure)?;
        let manifest = if status.capabilities["smart_sharing"].as_bool() == Some(true) {
            Some(
                self.agent
                    .get(format!("{}/api/smart/manifest", self.base))
                    .header("x-amp-password", &self.password)
                    .call()
                    .map_err(failure)?
                    .body_mut()
                    .read_json()
                    .map_err(failure)?,
            )
        } else {
            None
        };
        Ok(Snapshot {
            status,
            queue,
            manifest,
        })
    }
    fn control(&self, action: &str) -> Result<(), String> {
        let mut request = self
            .agent
            .post(format!("{}/api/{action}", self.base))
            .header("x-amp-password", &self.password);
        if let Some((server, station)) = self
            .identity
            .lock()
            .map_err(|_| "Station identity lock failed")?
            .as_ref()
        {
            request = request
                .header("x-amp-server", server)
                .header("x-amp-station", station);
        }
        request.send_empty().map_err(|error| match error {
            ureq::Error::StatusCode(401) => {
                "Access denied. Enter the server's admin password and reconnect.".into()
            }
            ureq::Error::StatusCode(409) => "The server music folder is empty.".into(),
            other => failure(other),
        })?;
        Ok(())
    }
}
fn failure(error: impl std::fmt::Display) -> String {
    format!("Cast request failed: {error}")
}
#[derive(Deserialize)]
struct Radio {
    name: String,
    country: String,
    url_resolved: String,
}
fn search_radio(query: &str) -> Result<Vec<Radio>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .max_redirects(0)
        .build()
        .into();
    let mut last = String::new();
    for server in ["de1.api.radio-browser.info", "nl1.api.radio-browser.info"] {
        match agent
            .get(format!("https://{server}/json/stations/search"))
            .query("name", query)
            .query("limit", "40")
            .query("hidebroken", "true")
            .header("User-Agent", "AMPVerse/0.1")
            .call()
        {
            Ok(mut response) => {
                let mut stations: Vec<Radio> = response.body_mut().read_json().map_err(failure)?;
                stations.retain(|r| {
                    r.url_resolved.starts_with("https://") || r.url_resolved.starts_with("http://")
                });
                return Ok(stations);
            }
            Err(e) => last = e.to_string(),
        }
    }
    Err(format!("Radio search unavailable: {last}"))
}
enum Command {
    Connect(String, String),
    Start,
    Stop,
    Refresh,
    Play(String),
    Share,
}
struct Update {
    snapshot: Result<Snapshot, String>,
    control_error: Option<String>,
}
pub struct Dashboard {
    pub(super) google: crate::google_cast::Output,
    listen_after_play: bool,
    pub live_action: Option<bool>,
    pub live_active: bool,
    pub live_status: String,
    address: String,
    password: String,
    connected_address: String,
    tx: mpsc::Sender<Command>,
    rx: mpsc::Receiver<Update>,
    snapshot: Option<Snapshot>,
    busy: bool,
    message: String,
    section: usize,
    search: String,
    discovery: Option<mpsc::Receiver<Result<Vec<amp_core::discovery::Found>, String>>>,
    servers: Vec<amp_core::discovery::Found>,
    host: Option<std::process::Child>,
    host_broadcast: bool,
    hosting: String,
    download: Option<mpsc::Receiver<Result<std::path::PathBuf, String>>>,
    radio_job: Option<mpsc::Receiver<Result<Vec<Radio>, String>>>,
    radio_results: Vec<Radio>,
    radio: Option<std::process::Child>,
}
impl Default for Dashboard {
    fn default() -> Self {
        let (tx, commands) = mpsc::channel();
        let (updates, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut client: Option<Client> = None;
            loop {
                let command = match commands.recv_timeout(Duration::from_secs(2)) {
                    Ok(command) => command,
                    Err(mpsc::RecvTimeoutError::Timeout) if client.is_some() => Command::Refresh,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                let mut control_error = None;
                if let Command::Connect(address, password) = command {
                    match Client::new(&address, password) {
                        Ok(value) => client = Some(value),
                        Err(error) => {
                            client = None;
                            if updates
                                .send(Update {
                                    snapshot: Err(error),
                                    control_error: None,
                                })
                                .is_err()
                            {
                                break;
                            }
                            continue;
                        }
                    }
                } else if let Some(client) = &client {
                    let outcome = match command {
                        Command::Start => client.control("start"),
                        Command::Stop => client.control("stop"),
                        Command::Play(ref id) => {
                            let id = id
                                .bytes()
                                .map(|b| {
                                    if b.is_ascii_alphanumeric() || b"-_.".contains(&b) {
                                        (b as char).to_string()
                                    } else {
                                        format!("%{b:02X}")
                                    }
                                })
                                .collect::<String>();
                            client.control(&format!("smart/playlists/{id}/play"))
                        }
                        Command::Share => client.control("smart/enable"),
                        _ => Ok(()),
                    };
                    control_error = outcome.err();
                }
                if let Some(client) = &client
                    && updates
                        .send(Update {
                            snapshot: client.read(),
                            control_error,
                        })
                        .is_err()
                {
                    break;
                }
            }
        });
        let mut dashboard = Self {
            google: Default::default(),
            listen_after_play: false,
            live_action: None,
            live_active: false,
            live_status: String::new(),
            address: std::env::var("AMP_CAST_ADDRESS").unwrap_or_else(|_| "127.0.0.1:8787".into()),
            password: String::new(),
            connected_address: String::new(),
            tx,
            rx,
            snapshot: None,
            busy: false,
            message:
                "Search LAN or broadcast stations, or host a Smart Station to share your playlists."
                    .into(),
            section: 0,
            search: String::new(),
            discovery: None,
            servers: Vec::new(),
            host: None,
            host_broadcast: false,
            hosting: String::new(),
            download: None,
            radio_job: None,
            radio_results: Vec::new(),
            radio: None,
        };
        if std::env::var("AMP_CAST_AUTOCONNECT").as_deref() == Ok("1") {
            dashboard.connected_address = dashboard.address.trim().into();
            dashboard.send(Command::Connect(dashboard.address.clone(), String::new()));
        }
        dashboard
    }
}
impl Dashboard {
    pub fn endpoint(&self) -> (String, String) {
        (
            if self.connected_address.is_empty() {
                self.address.clone()
            } else {
                self.connected_address.clone()
            },
            self.password.clone(),
        )
    }
    fn send(&mut self, command: Command) {
        match self.tx.send(command) {
            Ok(()) => {
                self.busy = true;
                self.message.clear();
            }
            Err(_) => {
                self.busy = false;
                self.message = "Cast connection worker stopped. Restart AMP Verse.".into();
            }
        }
    }
    pub fn poll(&mut self) {
        self.google.poll();
        if let Some(rx) = &self.discovery
            && let Ok(result) = rx.try_recv()
        {
            match result {
                Ok(servers) => {
                    self.message = if servers.is_empty() {
                        "No servers found. Check the same Wi-Fi/LAN or enter an address.".into()
                    } else {
                        format!("Found {} servers. Choose one to connect.", servers.len())
                    };
                    self.servers = servers;
                }
                Err(e) => self.message = e,
            }
            self.discovery = None;
        }

        while let Ok(update) = self.rx.try_recv() {
            self.busy = false;
            match update.snapshot {
                Ok(snapshot) => {
                    if let Some(error) = update
                        .control_error
                        .or_else(|| snapshot.status.error.clone())
                    {
                        self.listen_after_play = false;
                        self.message = error;
                    }
                    self.snapshot = Some(snapshot);
                }
                Err(error) => {
                    self.listen_after_play = false;
                    self.snapshot = None;
                    self.message = error;
                }
            }
        }
    }
    fn host(&mut self, root: &std::path::Path, smart: bool) {
        let result = (|| -> Result<std::process::Child, String> {
            let binary = std::env::current_exe()
                .map_err(failure)?
                .parent()
                .unwrap()
                .join(if cfg!(windows) {
                    "amp-cast.exe"
                } else {
                    "amp-cast"
                });
            if !binary.is_file() {
                return Err(
                    "AMP Cast server is missing. Install the desktop server component.".into(),
                );
            }
            let config = crate::theme::settings_path()
                .ok_or("Settings folder unavailable")?
                .with_file_name("cast-host");
            std::fs::create_dir_all(&config).map_err(failure)?;
            let port = std::net::TcpListener::bind("0.0.0.0:8787").map_err(
                |_| "A station is already using port 8787. Find and connect to it instead.",
            )?;
            drop(port);
            let log = std::fs::File::create(config.join("server.log")).map_err(failure)?;
            let mut command = std::process::Command::new(&binary);
            command
                .env(
                    "PATH",
                    format!(
                        "{};{}",
                        binary.parent().unwrap().display(),
                        std::env::var("PATH").unwrap_or_default()
                    ),
                )
                .arg(root)
                .env("AMP_CAST_BIND", "0.0.0.0:8787")
                .env("AMP_CAST_SMART_SHARING", if smart { "1" } else { "0" })
                .env("AMP_CAST_PASSWORD", &self.password)
                .env("AMP_CAST_CONFIG_DIR", &config)
                .env("AMP_ANALYSIS_ENABLED", "0")
                .stdout(log.try_clone().map_err(failure)?)
                .stderr(log);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            command.spawn().map_err(failure)
        })();
        match result {
            Ok(child) => {
                self.host = Some(child);
                self.host_broadcast = !smart;
                self.hosting = if smart {
                    "Smart Station · playlists and offline downloads"
                } else {
                    "Live broadcast"
                }
                .into();
                self.address = "127.0.0.1:8787".into();
                self.connected_address = self.address.clone();
                self.send(Command::Connect(
                    self.address.clone(),
                    self.password.clone(),
                ));
                self.message = "Starting your station. Reading the library…".into();
            }
            Err(e) => self.message = e,
        }
    }
    fn listen(&mut self, url: &str) {
        if let Some(mut previous) = self.radio.take() {
            let _ = previous.kill();
            let _ = previous.wait();
        }
        let mut command = amp_core::command("ffplay");
        command
            .args(["-nodisp", "-autoexit", "-loglevel", "error", "-i", url])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        match command.spawn() {
            Ok(child) => self.radio = Some(child),
            Err(e) => self.message = format!("Could not play station: {e}"),
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui, root: &std::path::Path, ready: bool) {
        self.poll();
        if self.listen_after_play
            && !self.busy
            && self.snapshot.as_ref().is_some_and(|s| s.status.running)
        {
            self.listen_after_play = false;
            self.listen(&format!("http://{}/listen.mp3", self.connected_address));
        }
        if let Some(child) = &mut self.host
            && let Ok(Some(code)) = child.try_wait()
        {
            self.host = None;
            self.hosting.clear();
            self.host_broadcast = false;
            self.message = format!(
                "Station could not stay running ({code}). Check cast-host/server.log or whether port 8787 is already in use."
            );
        }
        if self.host.is_some()
            && self.connected_address == "127.0.0.1:8787"
            && self.snapshot.is_some()
        {
            let expected = crate::theme::settings_path()
                .and_then(|p| std::fs::read(p.with_file_name("cast-host").join("server.json")).ok())
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .and_then(|v| v["server_id"].as_str().map(str::to_owned));
            if expected.as_deref() != self.snapshot.as_ref().map(|s| s.status.server_id.as_str()) {
                self.snapshot = None;
                self.message = "Waiting for your selected station to start…".into();
            }
        }
        if self.host_broadcast
            && self.connected_address == "127.0.0.1:8787"
            && self.snapshot.is_some()
        {
            self.host_broadcast = false;
            self.send(Command::Start);
        }
        if let Some(rx) = &self.download
            && let Ok(result) = rx.try_recv()
        {
            self.message = result.map_or_else(
                |e| e,
                |p| format!("Saved offline with cues: {}", p.display()),
            );
            self.download = None;
        }
        if let Some(rx) = &self.radio_job
            && let Ok(result) = rx.try_recv()
        {
            match result {
                Ok(stations) => {
                    self.radio_results = stations;
                    self.message = format!("{} broadcast stations found", self.radio_results.len());
                }
                Err(e) => self.message = e,
            }
            self.radio_job = None;
        }
        ui.heading("AMP Cast");
        ui.horizontal_wrapped(|ui| {
            for (index, title) in ["Find Stations", "Broadcast", "Host Smart Station"]
                .into_iter()
                .enumerate()
            {
                ui.selectable_value(&mut self.section, index, title);
            }
        });
        ui.separator();
        if self.section == 0 {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        self.discovery.is_none(),
                        egui::Button::new("Search LAN stations"),
                    )
                    .clicked()
                {
                    let (tx, rx) = mpsc::channel();
                    self.discovery = Some(rx);
                    std::thread::spawn(move || {
                        let _ = tx.send(amp_core::discovery::discover());
                    });
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Find a broadcast station by name")
                        .desired_width(280.),
                );
                if ui
                    .add_enabled(
                        self.radio_job.is_none() && !self.search.trim().is_empty(),
                        egui::Button::new("Search radio"),
                    )
                    .clicked()
                {
                    let query = self.search.clone();
                    let (tx, rx) = mpsc::channel();
                    self.radio_job = Some(rx);
                    std::thread::spawn(move || {
                        let _ = tx.send(search_radio(&query));
                    });
                }
            });
            let mut chosen = None;
            for server in &self.servers {
                if ui
                    .button(format!("{} · Connect", server.info.name))
                    .clicked()
                {
                    chosen = Some(server.address.clone());
                }
            }
            if let Some(address) = chosen {
                self.address = address;
                self.connected_address = self.address.clone();
                self.snapshot = None;
                self.send(Command::Connect(
                    self.address.clone(),
                    self.password.clone(),
                ));
            }
            let mut play = None;
            for station in &self.radio_results {
                ui.horizontal(|ui| {
                    ui.label(format!("{} · {}", station.name, station.country));
                    if ui.button("Listen").clicked() {
                        play = Some(station.url_resolved.clone());
                    }
                });
            }
            if let Some(url) = play {
                self.listen(&url);
            }
            egui::CollapsingHeader::new("Connect by address / access code").show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.text_edit_singleline(&mut self.address);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.password)
                            .password(true)
                            .hint_text("Access code, if needed"),
                    );
                    if ui.button("Connect").clicked() {
                        self.connected_address = self.address.clone();
                        self.snapshot = None;
                        self.send(Command::Connect(
                            self.address.clone(),
                            self.password.clone(),
                        ));
                    }
                });
            });
        } else if self.section == 1 {
            ui.heading("Broadcast my live DJ mix");
            ui.label("Send the actual master mix: both decks, crossfader, EQ, stems and sampler. Headphone cue stays private.");
            let supported = self
                .snapshot
                .as_ref()
                .is_some_and(|s| s.status.capabilities["live_dj"] == true);
            if ui
                .add_enabled(
                    supported && !self.live_active,
                    egui::Button::new("Start live DJ broadcast"),
                )
                .clicked()
            {
                self.live_action = Some(true);
            }
            if self.live_active && ui.button("Stop my live DJ broadcast").clicked() {
                self.live_action = Some(false);
            }
            ui.label(&self.live_status);
            if !supported {
                ui.weak("Connect to an updated AMP Cast server first, or start one below.");
                if ui.button("Connect to Cast on this computer").clicked() {
                    self.connected_address = "127.0.0.1:8787".into();
                    self.address = self.connected_address.clone();
                    self.send(Command::Connect(
                        self.address.clone(),
                        self.password.clone(),
                    ));
                }
                if self.host.is_none()
                    && ui
                        .add_enabled(
                            ready,
                            egui::Button::new("Start Cast server on this computer"),
                        )
                        .clicked()
                {
                    self.host(root, true);
                }
            }
        } else {
            ui.heading(if self.section == 1 {
                "Broadcast live audio"
            } else {
                "Share playlists from this computer"
            });
            ui.label(if self.section==1 {"Other devices listen to the same live stream."}else{"Share saved normal and Smart Playlists on your LAN. Listeners can save original songs with your cues for offline listening."});
            ui.label(format!("Music library: {}", root.display()));
            if self.snapshot.is_some() {
                if ui.button("Share playlists on connected station").clicked() {
                    self.send(Command::Share);
                }
                ui.weak("Enables shared playlists and offline downloads on the selected server.");
            }
            if self.host.is_none() {
                if ui
                    .add_enabled(
                        ready,
                        egui::Button::new(if self.section == 1 {
                            "Start broadcast"
                        } else {
                            "Host Smart Station"
                        }),
                    )
                    .clicked()
                {
                    self.host(root, self.section == 2);
                }
                if !ready {
                    ui.weak("Choose a music library and let its scan finish first.");
                }
            } else {
                ui.label(&self.hosting);
                if ui.button("Stop my station").clicked() {
                    if let Some(mut child) = self.host.take() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    self.hosting.clear();
                    self.host_broadcast = false;
                    self.snapshot = None;
                }
            }
            egui::CollapsingHeader::new("Access code (optional)").show(ui, |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.password).password(true));
            });
        }
        ui.separator();
        let mut download = None;
        let mut listen_url = None;
        let mut stop_remote = false;
        let mut play_remote = None;
        if let Some(snapshot) = &self.snapshot {
            ui.label(format!(
                "{} songs · {} listeners",
                snapshot.status.tracks, snapshot.status.listeners
            ));
            if snapshot.status.password_required && self.password.is_empty() {
                ui.weak("This station needs its access code.");
            }
            egui::CollapsingHeader::new("Songs in live station").show(ui, |ui| {
                for track in &snapshot.queue.tracks {
                    ui.label(&track.title);
                }
            });
            if self.section == 1
                && snapshot.status.running
                && self.host.is_none()
                && ui.button("Stop broadcast").clicked()
            {
                stop_remote = true;
            }
            ui.strong(
                snapshot
                    .status
                    .now_playing
                    .as_deref()
                    .unwrap_or("Station connected"),
            );
            if snapshot.status.running && ui.button("Listen to live station").clicked() {
                listen_url = Some(format!("http://{}/listen.mp3", self.connected_address));
            }
            if let Some(manifest) = &snapshot.manifest {
                ui.label("Playlists · songs and full cue metadata are included in offline copies");
                if let Some(lists) = manifest["playlists"].as_array() {
                    for playlist in lists {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!(
                                "{} · {} songs",
                                playlist["name"].as_str().unwrap_or("Playlist"),
                                playlist["tracks"].as_array().map_or(0, Vec::len)
                            ));
                            if ui.button("Play here").clicked() {
                                play_remote = playlist["id"].as_str().map(str::to_owned);
                            }
                            if ui
                                .add_enabled(
                                    self.download.is_none(),
                                    egui::Button::new("Save offline…"),
                                )
                                .clicked()
                            {
                                download = Some((manifest.clone(), playlist.clone()));
                            }
                        });
                    }
                }
            } else if snapshot.status.capabilities["smart_sharing"].as_bool() != Some(true) {
                ui.weak("Live station. Offline playlists are available from a Smart Station.");
            }
        }
        if stop_remote {
            self.send(Command::Stop);
        }
        if let Some(id) = play_remote {
            self.listen_after_play = true;
            self.send(Command::Play(id));
        }
        if let Some(url) = listen_url {
            self.listen(&url);
        }
        ui.separator();
        self.google.show(
            ui,
            &self.connected_address,
            self.snapshot.as_ref().is_some_and(|s| s.status.running),
        );
        if self.radio.is_some()
            && ui.button("Stop listening").clicked()
            && let Some(mut child) = self.radio.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some((manifest, playlist)) = download
            && let Some(destination) = rfd::FileDialog::new()
                .set_title("Save offline playlist and cues")
                .pick_folder()
        {
            let (tx, rx) = mpsc::channel();
            self.download = Some(rx);
            let base = format!("http://{}", self.connected_address);
            let password = self.password.clone();
            std::thread::spawn(move || {
                let _ = tx.send(crate::station_download::save(
                    &base,
                    &password,
                    &manifest,
                    &playlist,
                    &destination,
                ));
            });
        }
        if self.busy
            || self.discovery.is_some()
            || self.radio_job.is_some()
            || self.download.is_some()
        {
            ui.spinner();
        }
        if !self.message.is_empty() {
            ui.label(&self.message);
        }
    }
}

impl Drop for Dashboard {
    fn drop(&mut self) {
        if let Some(mut radio) = self.radio.take() {
            let _ = radio.kill();
            let _ = radio.wait();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "uses the public Radio Browser directory"]
    fn radio_directory_returns_listenable_results() {
        let results = search_radio("BBC").unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().all(|r| !r.name.is_empty()
            && (r.url_resolved.starts_with("https://") || r.url_resolved.starts_with("http://"))));
    }
    #[test]
    fn native_client_controls_real_cast_and_reports_failures() {
        let temp = tempfile::tempdir().unwrap();
        let server = amp_cast::Cast::new(
            temp.path().into(),
            amp_core::scan(temp.path()).unwrap(),
            Some("test-password".into()),
        );
        let (tx, rx) = mpsc::channel();
        let (shutdown, stop) = tokio::sync::oneshot::channel();
        let thread = std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(listener.local_addr().unwrap()).unwrap();
                axum_serve(server, listener, stop).await;
            });
        });
        let address = rx.recv().unwrap().to_string();
        let client = Client::new(&address, "wrong".into()).unwrap();
        let state = client.read().unwrap();
        assert!(!state.status.running);
        assert!(state.status.password_required);
        assert!(state.queue.tracks.is_empty());
        assert!(
            client
                .control("start")
                .unwrap_err()
                .contains("Access denied")
        );
        let client = Client::new(&address, "test-password".into()).unwrap();
        assert!(client.control("start").unwrap_err().contains("empty"));
        client.control("stop").unwrap();
        shutdown.send(()).unwrap();
        thread.join().unwrap();
        assert!(client.read().is_err());
        assert!(Client::new("localhost:8787/path", String::new()).is_err());
    }
    async fn axum_serve(
        server: std::sync::Arc<amp_cast::Cast>,
        listener: tokio::net::TcpListener,
        stop: tokio::sync::oneshot::Receiver<()>,
    ) {
        // Test through the actual TCP router, including HTTP framing and JSON schemas.
        axum::serve(listener, amp_cast::router(server))
            .with_graceful_shutdown(async {
                let _ = stop.await;
            })
            .await
            .unwrap();
    }
}
