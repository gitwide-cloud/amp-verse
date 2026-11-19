//! Native Cast V2 output; no analysis engine or Python is required.
use eframe::egui;
use std::{
    net::{IpAddr, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
        mpsc,
    },
    time::{Duration, Instant},
};
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Device {
    name: String,
    host: String,
    port: u16,
}
fn discover() -> Result<Vec<Device>, String> {
    let daemon = mdns_sd::ServiceDaemon::new().map_err(|e| e.to_string())?;
    let events = daemon
        .browse("_googlecast._tcp.local.")
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    let mut devices = Vec::new();
    while started.elapsed() < Duration::from_secs(5) {
        if let Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) =
            events.recv_timeout(Duration::from_millis(200))
            && let Some(host) = info
                .get_addresses_v4()
                .into_iter()
                .find(|ip| !ip.is_loopback() && !ip.is_unspecified())
        {
            let name = info
                .get_property_val_str("fn")
                .unwrap_or(info.get_fullname())
                .to_string();
            if !devices
                .iter()
                .any(|d: &Device| d.host == host.to_string() && d.port == info.get_port())
            {
                devices.push(Device {
                    name,
                    host: host.to_string(),
                    port: info.get_port(),
                });
            }
        }
    }
    let _ = daemon.shutdown();
    devices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(devices)
}
fn stream_url(address: &str, device: &str) -> Result<String, String> {
    let (host, port) = address
        .rsplit_once(':')
        .ok_or("Connect to an AMP Cast server first")?;
    let host = if host == "localhost" || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback()) {
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        socket.connect((device, 8009)).map_err(|e| e.to_string())?;
        socket
            .local_addr()
            .map_err(|e| e.to_string())?
            .ip()
            .to_string()
    } else {
        host.to_string()
    };
    Ok(format!("http://{host}:{port}/listen.mp3"))
}
struct Session {
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<String>>,
    done: Arc<AtomicBool>,
    accepted: Arc<AtomicBool>,
    stop_confirmed: Arc<AtomicBool>,
    activity: Arc<AtomicU64>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
    }
}
#[path = "cast_worker.rs"]
mod worker;
pub fn worker_main() -> bool {
    worker::worker_main()
}
fn cast(device: Device, url: String) -> Session {
    worker::start(device, url)
}
fn cast_in_process(device: Device, url: String) -> Session {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let stop = Arc::new(AtomicBool::new(false));
    let cancel = stop.clone();
    let status = Arc::new(Mutex::new("Connecting to Google Cast…".into()));
    let message = status.clone();
    let done = Arc::new(AtomicBool::new(false));
    let ended = done.clone();
    let accepted = Arc::new(AtomicBool::new(false));
    let connected = accepted.clone();
    let stop_confirmed = Arc::new(AtomicBool::new(false));
    let confirmed = stop_confirmed.clone();
    let activity = Arc::new(AtomicU64::new(0));
    let packets = activity.clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let ip = device
                .host
                .parse::<std::net::IpAddr>()
                .map_err(|_| "Invalid speaker address")?;
            std::net::TcpStream::connect_timeout(&std::net::SocketAddr::new(ip,device.port),Duration::from_secs(5))
                .map_err(|_|format!("{} is visible but unreachable. Check that the speaker and computer can reach each other on the LAN.",device.name))?;
            let cast =
                rust_cast::CastDevice::connect_without_host_verification(device.host, device.port)
                    .map_err(|e| e.to_string())?;
            *message.lock().unwrap() = "Opening Google Cast receiver connection…".into();
            cast.connection
                .connect("receiver-0")
                .map_err(|e| e.to_string())?;
            cast.heartbeat.ping().map_err(|e| e.to_string())?;
            *message.lock().unwrap() = "Launching Google Cast audio receiver…".into();
            let app = cast
                .receiver
                .launch_app(&rust_cast::channels::receiver::CastDeviceApp::DefaultMediaReceiver)
                .map_err(|e| e.to_string())?;
            cast.connection
                .connect(&app.transport_id)
                .map_err(|e| e.to_string())?;
            cast.media
                .load(
                    &app.transport_id,
                    &app.session_id,
                    &rust_cast::channels::media::Media {
                        content_id: url,
                        content_type: "audio/mpeg".into(),
                        stream_type: rust_cast::channels::media::StreamType::Live,
                        duration: None,
                        metadata: None,
                    },
                )
                .map_err(|e| e.to_string())?;
            connected.store(true, Relaxed);
            *message.lock().unwrap() = format!(
                "{} accepted the stream; buffering may take a few seconds",
                device.name
            );
            while !cancel.load(Relaxed) {
                let response = cast.receive().map_err(|e| e.to_string())?;
                packets.fetch_add(1, Relaxed);
                match response {
                    rust_cast::ChannelMessage::Connection(
                        rust_cast::channels::connection::ConnectionResponse::Close,
                    ) => return Err("Speaker closed the Cast session".into()),
                    rust_cast::ChannelMessage::Heartbeat(
                        rust_cast::channels::heartbeat::HeartbeatResponse::Ping,
                    ) => cast.heartbeat.pong().map_err(|e| e.to_string())?,
                    rust_cast::ChannelMessage::Media(
                        rust_cast::channels::media::MediaResponse::Status(status),
                    ) => {
                        if let Some(entry) = status.entries.first() {
                            let state = match entry.player_state.to_string().as_str() {
                                "PLAYING" => "Receiver reports playing (audibility not measured)",
                                "BUFFERING" => "Receiver buffering…",
                                "PAUSED" => "Receiver paused",
                                _ => "Receiver waiting for audio",
                            };
                            *message.lock().unwrap() = format!("{} · {state}", device.name);
                            if entry.idle_reason.is_some() {
                                return Err("Receiver stopped playback".into());
                            }
                        }
                    }
                    rust_cast::ChannelMessage::Media(
                        rust_cast::channels::media::MediaResponse::LoadFailed(_)
                        | rust_cast::channels::media::MediaResponse::Error(_),
                    ) => return Err(
                        "Speaker could not play this stream. Check LAN access to the Cast server."
                            .into(),
                    ),
                    _ => {}
                }
            }
            cast.receiver
                .stop_app(&app.session_id)
                .map_err(|e| e.to_string())?;
            confirmed.store(true, Relaxed);
            Ok(())
        })();
        *message.lock().unwrap() = result.map_or_else(
            |e| {
                format!(
                    "Google Cast: {}. Local playback continues.",
                    e.trim_end_matches('.')
                )
            },
            |()| "Casting stopped. Local playback continues.".into(),
        );
        ended.store(true, Relaxed);
    });
    Session {
        stop,
        status,
        done,
        accepted,
        stop_confirmed,
        activity,
    }
}
#[derive(Default)]
pub struct Output {
    devices: Vec<Device>,
    scan: Option<mpsc::Receiver<Result<Vec<Device>, String>>>,
    session: Option<Session>,
    manual: String,
    message: String,
    pending: Option<(Device, String, Instant)>,
    stop_requested: bool,
    terminal_error: Option<String>,
}
impl Output {
    pub fn active(&self) -> bool {
        self.pending.is_some() || self.session.as_ref().is_some_and(|s| !s.done.load(Relaxed))
    }
    pub fn status_text(&self) -> String {
        if self.stopping() {
            return "Stopping cast… Local playback continues.".into();
        }
        if let Some(session) = &self.session {
            return session.status.lock().unwrap().clone();
        }
        if self.message.is_empty() {
            "Open Cast to choose a receiver".into()
        } else {
            self.message.clone()
        }
    }
    pub fn fail(&mut self, reason: &str) {
        self.message = format!(
            "{}. Local playback continues.",
            reason.trim_end_matches('.')
        );
        self.terminal_error = Some(self.message.clone());
        self.cancel();
    }
    pub fn stopping(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|s| s.stop.load(Relaxed) && !s.done.load(Relaxed))
    }
    pub fn cancel(&mut self) {
        if self.pending.take().is_some() && self.terminal_error.is_none() {
            self.message = "Casting stopped. Local playback continues.".into();
        }
        if let Some(session) = &self.session {
            session.stop.store(true, Relaxed);
        }
    }
    /// Runs even when the Cast window is closed. Never uses station running state.
    pub fn poll_current(&mut self, ready: bool, running: bool) -> bool {
        if self.stop_requested {
            self.stop_requested = false;
            self.terminal_error = None;
            self.message = if self.session.is_some() {
                "Stopping cast… Local playback continues."
            } else {
                "Casting stopped. Local playback continues."
            }
            .into();
            self.cancel();
            return true;
        }
        if !running && self.session.as_ref().is_some_and(|s| !s.stop.load(Relaxed)) {
            self.cancel();
            return true;
        }
        if self.pending.is_some() {
            if !running || self.pending.as_ref().unwrap().2.elapsed() > Duration::from_secs(15) {
                self.cancel();
                self.message =
                    "Cast could not start the current stream. Local playback continues.".into();
                return true;
            }
            if ready {
                let (device, url, _) = self.pending.take().unwrap();
                self.message.clear();
                self.session = Some(cast(device, url));
            }
        }
        if self.session.as_ref().is_some_and(|s| s.done.load(Relaxed)) {
            let session = self.session.take().unwrap();
            let final_status = session.status.lock().unwrap().clone();
            self.message = match self.terminal_error.take() {
                Some(error) if final_status.contains("did not confirm stop") => {
                    format!("{error} {final_status}")
                }
                Some(error) => error,
                None => final_status,
            };
            return !session.stop.load(Relaxed);
        }
        false
    }
    pub fn show(&mut self, ui: &mut egui::Ui, address: &str, can_start: bool) -> bool {
        let mut requested = false;
        if let Some(rx) = &self.scan
            && let Ok(result) = rx.try_recv()
        {
            match result {
                Ok(devices) => {
                    self.message = format!("{} Google Cast devices found", devices.len());
                    self.devices = devices;
                }
                Err(e) => self.message = e,
            }
            self.scan = None;
        }
        ui.heading("Google Home / Chromecast");
        ui.weak("Choose a receiver for current AMP playback. Local output stays on; receivers buffer the stream and may sound later. End-to-end delay is unmeasured. AMP volume changes the Cast level too; use the PC speaker volume to lower only local sound.");
        if ui
            .add_enabled(
                self.scan.is_none(),
                egui::Button::new("Find Google speakers"),
            )
            .clicked()
        {
            let (tx, rx) = mpsc::channel();
            self.scan = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(discover());
            });
        }
        let busy = self.active();
        let mut selected = None;
        for device in &self.devices {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(can_start && !busy, egui::Button::new("Cast here"))
                    .clicked()
                {
                    selected = Some(device.clone());
                }
                ui.add(egui::Label::new(&device.name).wrap());
            });
        }
        ui.collapsing("Speaker IP address", |ui| {
            ui.text_edit_singleline(&mut self.manual);
            if ui
                .add_enabled(
                    can_start && !busy,
                    egui::Button::new("Cast to this speaker"),
                )
                .clicked()
            {
                if self.manual.parse::<std::net::Ipv4Addr>().is_ok() {
                    selected = Some(Device {
                        name: self.manual.clone(),
                        host: self.manual.clone(),
                        port: 8009,
                    });
                } else {
                    self.message = "Enter the Google speaker's IP address".into();
                }
            }
        });
        if let Some(device) = selected {
            match stream_url(address, &device.host) {
                Ok(url) => {
                    self.terminal_error = None;
                    self.pending = Some((device, url, Instant::now()));
                    self.message = "Preparing current playback stream…".into();
                    requested = true;
                }
                Err(e) => self.message = e,
            }
        }
        let status = self.status_text();
        ui.label(&status);
        if busy
            && ui
                .add_enabled(!self.stopping(), egui::Button::new("Stop casting"))
                .clicked()
        {
            self.stop_requested = true;
        }
        let waiting = self.pending.is_some()
            || self.stopping()
            || (self.session.is_some() && !status.contains("Receiver reports playing"));
        if self.scan.is_some() || waiting {
            ui.spinner();
        }
        if self.scan.is_some() || busy {
            ui.ctx().request_repaint_after(Duration::from_millis(150));
        }
        requested
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn pending_output() -> Output {
        Output {
            pending: Some((
                Device {
                    name: "Fixture".into(),
                    host: "192.0.2.1".into(),
                    port: 8009,
                },
                "http://192.0.2.2:8787/listen.mp3".into(),
                Instant::now(),
            )),
            ..Default::default()
        }
    }
    #[test]
    fn opening_output_does_not_start_a_session() {
        let mut output = Output::default();
        assert!(!output.poll_current(false, false));
        assert!(!output.active());
    }
    #[test]
    fn receiver_waits_for_current_upload_not_station_status() {
        let mut output = pending_output();
        assert!(!output.poll_current(false, true));
        assert!(output.pending.is_some());
        assert!(output.session.is_none());
    }
    #[test]
    fn startup_failure_clears_pending_receiver() {
        let mut output = pending_output();
        assert!(output.poll_current(false, false));
        assert!(!output.active());
    }
    #[test]
    fn stop_during_startup_never_launches_receiver() {
        let mut output = pending_output();
        output.stop_requested = true;
        assert!(output.poll_current(true, true));
        assert!(!output.active());
        assert!(!output.poll_current(true, true));
    }
    #[test]
    fn stop_completion_is_terminal_and_does_not_wrap_status() {
        let mut output = Output {
            session: Some(Session {
                stop: Arc::new(AtomicBool::new(true)),
                done: Arc::new(AtomicBool::new(true)),
                accepted: Arc::new(AtomicBool::new(true)),
                stop_confirmed: Arc::new(AtomicBool::new(true)),
                activity: Arc::new(AtomicU64::new(0)),
                status: Arc::new(Mutex::new(
                    "Casting stopped. Local playback continues.".into(),
                )),
            }),
            message: "Stopping cast…".into(),
            ..Default::default()
        };
        assert!(!output.poll_current(false, false));
        let status = output.message.clone();
        assert!(!output.poll_current(false, false));
        assert_eq!(output.message, status);
        assert!(!output.active());
        assert!(!status.contains("Last stream status"));
    }
    #[test]
    fn remote_station_url_is_preserved() {
        assert_eq!(
            stream_url("192.168.1.20:8787", "192.168.1.30").unwrap(),
            "http://192.168.1.20:8787/listen.mp3"
        );
    }
    #[test]
    #[ignore = "LAN discovery"]
    fn discover_local_speakers() {
        eprintln!("{:?}", discover().unwrap());
    }
    #[test]
    #[ignore = "audible hardware test; requires explicitly selected AMP_GOOGLE_TEST_HOST and running station"]
    fn selected_speaker_plays_station_and_stops() {
        let host = std::env::var("AMP_GOOGLE_TEST_HOST").expect("Select a test speaker explicitly");
        let url = stream_url("127.0.0.1:8787", &host).unwrap();
        let session = cast(
            Device {
                name: "Selected test speaker".into(),
                host,
                port: 8009,
            },
            url,
        );
        let start = Instant::now();
        let mut playing = false;
        while start.elapsed() < Duration::from_secs(30) {
            let status = session.status.lock().unwrap().clone();
            if status.contains("Receiver reports playing") {
                playing = true;
                break;
            }
            if session.done.load(Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        eprintln!("{}", session.status.lock().unwrap());
        session.stop.store(true, Relaxed);
        let stop = Instant::now();
        while !session.done.load(Relaxed) && stop.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(playing, "{}", session.status.lock().unwrap());
        assert!(session.done.load(Relaxed), "Speaker did not confirm stop");
    }
}
