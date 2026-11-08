//! Native Cast dashboard. The independent server owns broadcasting, not this viewport.
use crate::skin;
use eframe::egui::{self, RichText};
use serde::Deserialize;
use std::{sync::mpsc, time::Duration};

#[derive(Default, Deserialize)]
struct Status {
    running: bool,
    now_playing: Option<String>,
    current_index: Option<usize>,
    error: Option<String>,
    tracks: usize,
    listeners: usize,
    password_required: bool,
}
#[derive(Deserialize)]
struct Track {
    index: usize,
    title: String,
    cue_in: f64,
    cue_out: Option<f64>,
}
#[derive(Default, Deserialize)]
struct Queue {
    tracks: Vec<Track>,
}
struct Snapshot {
    status: Status,
    queue: Queue,
}
struct Client {
    agent: ureq::Agent,
    base: String,
    password: String,
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
        let queue = self
            .agent
            .get(format!("{}/api/playlist", self.base))
            .call()
            .map_err(failure)?
            .body_mut()
            .read_json::<Queue>()
            .map_err(failure)?;
        Ok(Snapshot { status, queue })
    }
    fn control(&self, action: &str) -> Result<(), String> {
        self.agent
            .post(format!("{}/api/{action}", self.base))
            .header("x-amp-password", &self.password)
            .send_empty()
            .map_err(|error| match error {
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
enum Command {
    Connect(String, String),
    Start,
    Stop,
    Refresh,
}
struct Update {
    snapshot: Result<Snapshot, String>,
    control_error: Option<String>,
}
pub struct Dashboard {
    address: String,
    password: String,
    connected_address: String,
    tx: mpsc::Sender<Command>,
    rx: mpsc::Receiver<Update>,
    snapshot: Option<Snapshot>,
    busy: bool,
    message: String,
    section: usize,
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
            address: std::env::var("AMP_CAST_ADDRESS").unwrap_or_else(|_| "127.0.0.1:8787".into()),
            password: String::new(),
            connected_address: String::new(),
            tx,
            rx,
            snapshot: None,
            busy: false,
            message: "Connect to a running AMP Cast server.".into(),
            section: 0,
        };
        if std::env::var("AMP_CAST_AUTOCONNECT").as_deref() == Ok("1") {
            dashboard.connected_address = dashboard.address.trim().into();
            dashboard.send(Command::Connect(dashboard.address.clone(), String::new()));
        }
        dashboard
    }
}
impl Dashboard {
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
        while let Ok(update) = self.rx.try_recv() {
            self.busy = false;
            match update.snapshot {
                Ok(snapshot) => {
                    if let Some(error) = update
                        .control_error
                        .or_else(|| snapshot.status.error.clone())
                    {
                        self.message = error;
                    }
                    self.snapshot = Some(snapshot);
                }
                Err(error) => {
                    self.snapshot = None;
                    self.message = error;
                }
            }
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.poll();
        ui.horizontal_wrapped(|ui| {
            ui.label("Server host:port");
            ui.add(egui::TextEdit::singleline(&mut self.address).desired_width(175.));
            ui.label("Admin password");
            ui.add(
                egui::TextEdit::singleline(&mut self.password)
                    .password(true)
                    .desired_width(140.),
            );
            if ui
                .add_enabled(!self.busy, egui::Button::new("Connect / apply"))
                .clicked()
            {
                self.connected_address = self.address.trim().into();
                self.snapshot = None;
                self.send(Command::Connect(
                    self.address.clone(),
                    self.password.clone(),
                ));
            }
        });
        ui.add_space(8.);
        ui.horizontal_top(|ui| {
            skin::panel(ui, true, |ui| {
                ui.set_width(140.);
                ui.set_min_height(565.);
                ui.label(RichText::new("AMP CAST").size(20.).strong().color(skin::accent(ui)));
                ui.add_space(15.);
                for (index, title) in ["Dashboard", "Queue", "Streaming", "Settings"].into_iter().enumerate() {
                    ui.add_space(8.);
                    ui.selectable_value(&mut self.section, index, title);
                    ui.separator();
                }
                ui.add_space(25.);
                ui.label(RichText::new(if self.snapshot.is_some() { "SERVER ONLINE" } else { "NOT CONNECTED" }).color(if self.snapshot.is_some() { skin::signal(ui) } else { skin::foreground(ui) }));
            });
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
        let running = self.snapshot.as_ref().is_some_and(|s| s.status.running);
        let connected = self.snapshot.is_some();
        if self.section == 0 || self.section == 2 {
            ui.columns(2, |columns| {
                skin::panel(&mut columns[0], true, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(160.);
                    ui.label(
                        RichText::new("NOW BROADCASTING")
                            .color(skin::accent(ui))
                            .strong(),
                    );
                    ui.add_space(12.);
                    let title = self
                        .snapshot
                        .as_ref()
                        .and_then(|s| s.status.now_playing.as_deref())
                        .unwrap_or(if connected {
                            "Station stopped"
                        } else {
                            "Connect to your Cast server"
                        });
                    ui.label(RichText::new(title).size(24.).strong());
                    ui.label("MP3 / 128 kbps / 44.1 kHz / Stereo");
                    ui.add_space(12.);
                    ui.label("The independent server owns playback and prepared cue points.");
                });
                skin::panel(&mut columns[1], true, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(160.);
                    ui.label(
                        RichText::new("STREAMING STATUS")
                            .strong()
                            .color(skin::accent(ui)),
                    );
                    ui.label(format!(
                        "Station: {}",
                        if running {
                            "ON AIR"
                        } else if connected {
                            "OFF AIR"
                        } else {
                            "UNAVAILABLE"
                        }
                    ));
                    ui.label(format!("Server: {}", self.connected_address));
                    ui.label(format!(
                        "Listeners: {}",
                        self.snapshot
                            .as_ref()
                            .map(|s| s.status.listeners.to_string())
                            .unwrap_or_else(|| "--".into())
                    ));
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                connected && !running && !self.busy,
                                egui::Button::new("Start broadcast"),
                            )
                            .clicked()
                        {
                            self.send(Command::Start);
                        }
                        if ui
                            .add_enabled(
                                connected && running && !self.busy,
                                egui::Button::new("Stop broadcast"),
                            )
                            .clicked()
                        {
                            self.send(Command::Stop);
                        }
                    });
                    if connected {
                        ui.hyperlink_to(
                            "Open browser listener",
                            format!("http://{}", self.connected_address),
                        );
                    }
                });
            });
            ui.add_space(10.);
        }
        if self.section == 0 || self.section == 1 {
            skin::panel(ui, true, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(205.);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("PLAYLIST / QUEUE")
                            .strong()
                            .color(skin::accent(ui)),
                    );
                    ui.label(format!(
                        "{} tracks",
                        self.snapshot.as_ref().map(|s| s.status.tracks).unwrap_or(0)
                    ));
                });
                egui::ScrollArea::vertical()
                    .id_salt("cast-queue")
                    .max_height(245.)
                    .show(ui, |ui| {
                        if let Some(snapshot) = &self.snapshot {
                            egui::Grid::new("cast-tracks")
                                .num_columns(3)
                                .striped(true)
                                .min_col_width(90.)
                                .show(ui, |ui| {
                                    ui.strong("#");
                                    ui.strong("Title");
                                    ui.strong("Prepared cues");
                                    ui.end_row();
                                    for track in &snapshot.queue.tracks {
                                        let active = running
                                            && snapshot.status.current_index == Some(track.index);
                                        ui.label(
                                            RichText::new(format!(
                                                "{} {}",
                                                if active { ">" } else { " " },
                                                track.index + 1
                                            ))
                                            .color(
                                                if active {
                                                    skin::signal(ui)
                                                } else {
                                                    skin::foreground(ui)
                                                },
                                            ),
                                        );
                                        ui.label(&track.title);
                                        ui.label(format!(
                                            "{:.2}s / {}",
                                            track.cue_in,
                                            track
                                                .cue_out
                                                .map(|v| format!("{v:.2}s"))
                                                .unwrap_or_else(|| "End".into())
                                        ));
                                        ui.end_row();
                                    }
                                });
                            if snapshot.queue.tracks.is_empty() {
                                ui.label("The server music folder is empty.");
                            }
                        } else {
                            ui.label("Connect to load the server queue.");
                        }
                    });
            });
            ui.add_space(10.);
        }
        skin::panel(ui, false, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                RichText::new("AUTO DJ / PREPARED SEQUENTIAL LOOP")
                    .strong()
                    .color(skin::accent(ui)),
            );
            ui.label(if running {
                "Running on server / folder order with cue IN/OUT"
            } else {
                "Starts with the station / folder order with cue IN/OUT"
            });
            ui.label("Beat sync, crossfade, tag rules and queue editing are future milestones.");
            if self.section == 3 {
                ui.separator();
                ui.label("Start amp-cast with your music folder; use this window to connect, launch and stop its broadcast.");
                ui.label("HTTP on your trusted LAN. Password is held in memory only. Server lifetime is independent of this window.");
                ui.label(
                    if self
                        .snapshot
                        .as_ref()
                        .is_some_and(|s| s.status.password_required)
                    {
                        "Server requires an admin password."
                    } else {
                        "Server password status is available after connection."
                    },
                );
            }
        });
        if self.busy {
            ui.spinner();
        }
        if !self.message.is_empty() {
            ui.label(RichText::new(&self.message).color(skin::accent(ui)));
        }
        ui.label("Native AMP Cast / 0.1 prototype / Soundcard routing and listener analytics are planned.");
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
