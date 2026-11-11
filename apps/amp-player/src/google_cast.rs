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
#[derive(Clone, Debug, serde::Serialize)]
struct Device {
    id: String,
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
                    id: info
                        .get_property_val_str("id")
                        .unwrap_or(info.get_fullname())
                        .to_owned(),
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
type ReceiverVolume = Arc<Mutex<Option<(Option<f32>, Option<bool>)>>>;
struct Session {
    device: Device,
    playback: Arc<Mutex<Option<String>>>,
    volume: ReceiverVolume,
    commands: mpsc::Sender<(f32, bool)>,
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
    let receiver_device = device.clone();
    let playback = Arc::new(Mutex::new(None));
    let actual = playback.clone();
    let volume = Arc::new(Mutex::new(None));
    let actual_volume = volume.clone();
    let (tx, rx) = mpsc::channel();
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
            if let Ok(status) = cast.receiver.get_status() {
                *actual_volume.lock().unwrap() = Some((status.volume.level, status.volume.muted));
            }
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
                while let Ok((level, muted)) = rx.try_recv() {
                    let confirmed = cast
                        .receiver
                        .set_volume(rust_cast::channels::receiver::Volume {
                            level: Some(level),
                            muted: Some(muted),
                        })
                        .map_err(|e| e.to_string())?;
                    *actual_volume.lock().unwrap() = Some((confirmed.level, confirmed.muted));
                }
                match cast.receive().map_err(|e| e.to_string())? {
                    rust_cast::ChannelMessage::Receiver(
                        rust_cast::channels::receiver::ReceiverResponse::Status(status),
                    ) => {
                        *actual_volume.lock().unwrap() =
                            Some((status.volume.level, status.volume.muted));
                    }
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
                            *actual.lock().unwrap() = Some(entry.player_state.to_string());
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
        *actual.lock().unwrap() = None;
        ended.store(true, Relaxed);
    });
    Session {
        device: receiver_device,
        playback,
        volume,
        commands: tx,
        stop,
        status,
        done,
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Operation {
    Discover,
    Start {
        receiver: String,
        address: String,
    },
    Stop,
    Volume {
        level: f32,
        #[serde(default)]
        muted: bool,
    },
}
#[derive(Default)]
pub struct Output {
    devices: Vec<Device>,
    scan: Option<mpsc::Receiver<Result<Vec<Device>, String>>>,
    session: Option<Session>,
    manual: String,
    message: String,
}
impl Output {
    pub fn poll(&mut self) {
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
    }
    pub fn snapshot(&self) -> serde_json::Value {
        use serde_json::json;
        let session = self.session.as_ref();
        json!({"receivers":self.devices,"discovering":self.scan.is_some(),"receiver":session.map(|s|&s.device),"playback_state":session.and_then(|s|s.playback.lock().ok()?.clone()),"volume":session.and_then(|s|*s.volume.lock().ok()?),"status":session.and_then(|s|s.status.lock().ok().map(|s|s.clone())).unwrap_or_else(||self.message.clone()),"stopping":session.is_some_and(|s|s.stop.load(Relaxed)&&!s.done.load(Relaxed))})
    }
    pub fn remote(&mut self, operation: Operation) -> Result<(), String> {
        match operation {
            Operation::Discover => {
                if self.scan.is_none() {
                    let (tx, rx) = mpsc::channel();
                    self.scan = Some(rx);
                    std::thread::spawn(move || {
                        let _ = tx.send(discover());
                    });
                }
            }
            Operation::Start { receiver, address } => {
                if self.session.as_ref().is_some_and(|s| !s.done.load(Relaxed)) {
                    return Err("Stop the current receiver session before switching".into());
                }
                let device = self
                    .devices
                    .iter()
                    .find(|d| d.id == receiver)
                    .cloned()
                    .ok_or("Receiver unavailable; discover receivers again")?;
                let url = stream_url(&address, &device.host)?;
                self.session = Some(cast(device, url));
            }
            Operation::Stop => {
                if let Some(s) = &self.session {
                    s.stop.store(true, Relaxed);
                }
            }
            Operation::Volume { level, muted } => {
                if !level.is_finite() || !(0. ..=1.).contains(&level) {
                    return Err("Receiver level must be 0–1".into());
                }
                self.session
                    .as_ref()
                    .filter(|s| !s.done.load(Relaxed))
                    .ok_or("No active Google Cast receiver")?
                    .commands
                    .send((level, muted))
                    .map_err(|_| "Receiver session ended")?;
            }
        }
        Ok(())
    }
    pub fn show(&mut self, ui: &mut egui::Ui, address: &str, running: bool) {
        self.poll();
        ui.heading("Google Home / Chromecast");
        ui.weak("Play the connected station or live DJ broadcast on a speaker. The speaker must be able to reach this computer over Wi-Fi/LAN.");
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
        let busy = self.session.as_ref().is_some_and(|s| !s.done.load(Relaxed));
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
                        id: format!("manual:{}", self.manual),
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
                Ok(url) => self.session = Some(cast(device, url)),
                Err(e) => self.message = e,
            }
        }
        if let Some(session) = &self.session {
            ui.label(session.status.lock().unwrap().as_str());
            if busy && ui.button("Stop Google Cast").clicked() {
                session.stop.store(true, Relaxed);
            }
            if busy {
                ui.ctx().request_repaint_after(Duration::from_millis(200));
            }
        }
        if self.scan.is_some() {
            ui.spinner();
            ui.ctx().request_repaint_after(Duration::from_millis(150));
        }
        ui.label(&self.message);
        if !running {
            ui.weak("Start a live DJ broadcast or play a station playlist first.");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
                id: "test-speaker".into(),
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
