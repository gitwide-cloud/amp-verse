//! Native Cast V2 output; no analysis engine or Python is required.
use eframe::egui;
use std::{
    net::{IpAddr, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering::Relaxed},
        mpsc,
    },
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
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
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
    }
}
fn cast(device: Device, url: String) -> Session {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let stop = Arc::new(AtomicBool::new(false));
    let cancel = stop.clone();
    let status = Arc::new(Mutex::new("Connecting to Google Cast…".into()));
    let message = status.clone();
    let done = Arc::new(AtomicBool::new(false));
    let ended = done.clone();
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
            *message.lock().unwrap() =
                format!("{} accepted the stream; waiting for playback", device.name);
            while !cancel.load(Relaxed) {
                match cast.receive().map_err(|e| e.to_string())? {
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
                            *message.lock().unwrap() =
                                format!("{} · {}", device.name, entry.player_state);
                            if entry.idle_reason.is_some() {
                                return Err(format!(
                                    "Speaker stopped playback: {:?}",
                                    entry.idle_reason
                                ));
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
            Ok(())
        })();
        *message.lock().unwrap() = result.map_or_else(
            |e| format!("Google Cast: {e}"),
            |()| "Google Cast stopped".into(),
        );
        ended.store(true, Relaxed);
    });
    Session { stop, status, done }
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
}
impl Output {
    pub fn active(&self) -> bool {
        self.pending.is_some() || self.session.as_ref().is_some_and(|s| !s.done.load(Relaxed))
    }
    pub fn cancel(&mut self) {
        self.pending = None;
        if let Some(session) = &self.session {
            session.stop.store(true, Relaxed);
        }
    }
    /// Runs even when the Cast window is closed. Never uses station running state.
    pub fn poll_current(&mut self, ready: bool, running: bool) -> bool {
        if self.stop_requested {
            self.stop_requested = false;
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
                self.session = Some(cast(device, url));
            }
        }
        if self.session.as_ref().is_some_and(|s| s.done.load(Relaxed)) {
            let session = self.session.take().unwrap();
            self.message = session.status.lock().unwrap().clone();
            return true;
        }
        false
    }
    pub fn show(&mut self, ui: &mut egui::Ui, address: &str, running: bool) -> bool {
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
        ui.weak("Choose a receiver for current AMP playback. Local output stays on; receivers buffer the stream and may sound later. End-to-end delay is unmeasured.");
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
            ui.horizontal(|ui| {
                ui.label(&device.name);
                if ui
                    .add_enabled(running && !busy, egui::Button::new("Cast here"))
                    .clicked()
                {
                    selected = Some(device.clone());
                }
            });
        }
        ui.collapsing("Speaker IP address", |ui| {
            ui.text_edit_singleline(&mut self.manual);
            if ui
                .add_enabled(running && !busy, egui::Button::new("Cast to this speaker"))
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
                    self.pending = Some((device, url, Instant::now()));
                    self.message = "Preparing current playback stream…".into();
                    requested = true;
                }
                Err(e) => self.message = e,
            }
        }
        if let Some(session) = &self.session {
            ui.label(session.status.lock().unwrap().as_str());

            if busy {
                ui.ctx().request_repaint_after(Duration::from_millis(200));
            }
        }
        if busy && ui.button("Stop casting").clicked() {
            self.stop_requested = true;
        }
        if self.scan.is_some() {
            ui.spinner();
            ui.ctx().request_repaint_after(Duration::from_millis(150));
        }
        ui.label(&self.message);
        if !running {
            ui.weak(
                "Play a supported source and connect to an AMP Cast server to choose a receiver.",
            );
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
            if status.contains("PLAYING") {
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
