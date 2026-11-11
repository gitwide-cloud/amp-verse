//! Local show-control gateway. The GUI owns performance; disconnecting never touches audio.
use axum::{
    Json, Router,
    extract::{Query, State, WebSocketUpgrade, ws::Message},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use tokio::sync::{oneshot, watch};
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Deck {
    #[default]
    Auto,
    A,
    B,
    A1,
    A2,
    A3,
    B1,
    B2,
    B3,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Execution {
    #[default]
    LoadOnly,
    PlayNow,
    AutoFade,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    SaveMusicScene {
        name: String,
    },
    RecallMusicScene {
        scene: String,
        #[serde(default)]
        execution: Execution,
    },
    HotCue {
        deck: Deck,
        slot: u16,
    },
    DeckFilter {
        deck: Deck,
        value: f32,
    },
    StopSampler {
        side: usize,
        pad: usize,
    },
    GoogleCast {
        operation: crate::google_cast::Operation,
    },
    BroadcastLevel {
        level: f32,
        #[serde(default)]
        muted: bool,
    },
    DeckKeyLock {
        deck: Deck,
        enabled: bool,
    },
    SelectLive {
        deck: Deck,
    },
    AssignCrossfadePair {
        a: Deck,
        b: Deck,
    },
    MasterClock {
        bpm: f32,
        quantization: u32,
    },
    DeckSync {
        deck: Deck,
        enabled: bool,
    },
    DeckLoop {
        deck: Deck,
        enabled: bool,
        #[serde(default = "four_beats")]
        beats: u32,
    },
    DeckGain {
        deck: Deck,
        value: f32,
    },
    DeckStems {
        deck: Deck,
        levels: [f32; 3],
    },
    CrossfadeDuration {
        seconds: f32,
    },
    QuantizedLaunch {
        deck: Deck,
    },
    StopAllDecks,
    StopPlayer,
    EmergencyMute,
    StopSamplers {
        #[serde(default)]
        side: Option<usize>,
    },

    TransitionSettings {
        enabled: bool,
        techniques: [bool; 6],
        every: u32,
        #[serde(default)]
        loop_build: Option<bool>,
        #[serde(default)]
        stem_swap: Option<bool>,
    },
    RequestSong {
        song: String,
        within: u32,
    },
    CallSong {
        song: String,
        #[serde(default)]
        cue: Option<String>,
        #[serde(default)]
        deck: Deck,
        #[serde(default)]
        execution: Execution,
    },
    StartLiveBroadcast {
        #[serde(default)]
        player_only: bool,
        address: String,
        #[serde(default)]
        password: String,
    },
    StopLiveBroadcast,
    AdjustVolume {
        delta: f32,
    },
    SeekRelative {
        #[serde(default)]
        deck: Deck,
        seconds: f64,
    },
    Previous,
    PlayerNext,
    PlayerOptions {
        #[serde(default)]
        shuffle: Option<bool>,
        #[serde(default)]
        repeat: Option<bool>,
    },
    LoadPlaylist {
        playlist: String,
    },
    PreloadNext,
    NextSong {
        #[serde(default)]
        immediate: bool,
    },
    SmartShuffle,
    Play {
        #[serde(default)]
        deck: Deck,
    },
    Pause {
        #[serde(default)]
        deck: Deck,
    },
    Stop {
        #[serde(default)]
        deck: Deck,
    },
    RestartCue {
        #[serde(default)]
        deck: Deck,
        #[serde(default)]
        cue: Option<String>,
    },
    Fade {
        #[serde(default)]
        deck: Deck,
        #[serde(default)]
        master: bool,
        target: f32,
        seconds: f32,
        #[serde(default)]
        stop_after: bool,
    },
    AutoFade,
    AutoCrossfade {
        enabled: bool,
    },
    MixNext {
        #[serde(default)]
        immediate: bool,
    },
    Duck {
        #[serde(default = "duck_level")]
        level: f32,
        #[serde(default = "fade_seconds")]
        seconds: f32,
    },
    Restore {
        #[serde(default = "fade_seconds")]
        seconds: f32,
    },
    LoadSmartPlaylist {
        playlist: String,
        #[serde(default)]
        execution: Execution,
        #[serde(default)]
        style: Option<amp_core::show::Style>,
        #[serde(default)]
        energy: Option<f64>,
    },
    Recognition {
        #[serde(default)]
        song: Option<String>,
        #[serde(default)]
        cue: Option<String>,
        #[serde(default)]
        execution: Execution,
    },
    SetStyle {
        style: amp_core::show::Style,
    },
    SetEnergy {
        target: f64,
    },
    EnergyStep {
        delta: f64,
    },
    Progression {
        target: f64,
        transitions: u32,
        #[serde(default = "yes")]
        hold: bool,
    },
    CancelProgression,
    ReverseProgression,
    HoldEnergy,
    AutoDj {
        enabled: bool,
    },
    ManualLock {
        enabled: bool,
    },
    SetCrossfader {
        value: f32,
    },
    Sampler {
        pad: usize,
    },
    CueDeck {
        side: usize,
        operation: crate::side_deck::Operation,
    },
    SideSampler {
        side: usize,
        pad: usize,
    },
}
fn four_beats() -> u32 {
    4
}
fn yes() -> bool {
    true
}
fn duck_level() -> f32 {
    0.2
}
fn fade_seconds() -> f32 {
    1.
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    #[serde(flatten)]
    pub command: Action,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reply {
    pub id: String,
    pub ok: bool,
    pub status: String,
    pub error: Option<String>,
}
pub struct Envelope {
    pub request: Request,
    pub reply: oneshot::Sender<Reply>,
    pub deadline: Instant,
}
struct Entry {
    body: String,
    result: watch::Receiver<Option<Reply>>,
}
struct Cache {
    entries: HashMap<String, Entry>,
    order: VecDeque<String>,
}
struct Shared {
    commands: mpsc::SyncSender<Envelope>,
    snapshot: watch::Sender<Value>,
    catalog: Mutex<Value>,
    cache: Mutex<Cache>,
    password: Option<String>,
    ctx: eframe::egui::Context,
    shutdown: Arc<AtomicBool>,
    session: String,
    discovery_ready: AtomicBool,
}
pub struct Control {
    pub commands: mpsc::Receiver<Envelope>,
    shared: Arc<Shared>,
    shutdown: Arc<AtomicBool>,
    pub address: String,
}
impl Drop for Control {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}
impl Control {
    pub fn start(
        ctx: &eframe::egui::Context,
        address: &str,
        password: Option<String>,
    ) -> Result<Self, String> {
        let listener = std::net::TcpListener::bind(address).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let address = listener
            .local_addr()
            .map_err(|e| e.to_string())?
            .to_string();
        let (tx, rx) = mpsc::sync_channel(32);
        let (snapshot, _) =
            watch::channel(json!({"amp":{"connected":true,"playback_state":"stopped"}}));
        let shutdown = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            commands: tx,
            snapshot,
            catalog: Mutex::new(json!({"songs":[],"playlists":[]})),
            cache: Mutex::new(Cache {
                entries: HashMap::new(),
                order: VecDeque::new(),
            }),
            password,
            ctx: ctx.clone(),
            shutdown: shutdown.clone(),
            session: amp_core::coordination::token(),
            discovery_ready: AtomicBool::new(false),
        });
        let discovery = shared.clone();
        let local = listener.local_addr().map_err(|e| e.to_string())?;
        if !local.ip().is_loopback() && local.is_ipv4() {
            std::thread::spawn(move || {
                let mut bound = None;
                for _ in 0..20 {
                    match std::net::UdpSocket::bind("0.0.0.0:18790") {
                        Ok(socket) => {
                            bound = Some(socket);
                            break;
                        }
                        Err(_) if !discovery.shutdown.load(Ordering::Relaxed) => {
                            std::thread::sleep(Duration::from_millis(50))
                        }
                        Err(_) => return,
                    }
                }
                let Some(socket) = bound else { return };
                discovery.discovery_ready.store(true, Ordering::Relaxed);
                let _ = socket.set_read_timeout(Some(Duration::from_millis(50)));
                let mut packet = [0u8; 128];
                let mut window = Instant::now();
                let mut replies = 0;
                while !discovery.shutdown.load(Ordering::Relaxed) {
                    if window.elapsed() > Duration::from_secs(1) {
                        window = Instant::now();
                        replies = 0;
                    }
                    if let Ok((n, peer)) = socket.recv_from(&mut packet) {
                        let local_peer = match peer.ip() {
                            std::net::IpAddr::V4(ip) => {
                                ip.is_private() || ip.is_loopback() || ip.is_link_local()
                            }
                            _ => false,
                        };
                        if local_peer
                            && &packet[..n] == b"AMPVERSE_CONTROL_DISCOVER_V1"
                            && replies < 20
                        {
                            let reply = json!({"protocol":1,"service":"amp-control","name":std::env::var("COMPUTERNAME").or_else(|_|std::env::var("HOSTNAME")).unwrap_or_else(|_|"AMP Verse".into()),"port":local.port(),"session":discovery.session,"password_required":discovery.password.is_some()});
                            let _ = socket.send_to(reply.to_string().as_bytes(), peer);
                            replies += 1;
                        }
                    }
                }
            });
        }
        let stop = shutdown.clone();
        let server = shared.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Show control runtime");
            runtime.block_on(async move {
                let router = Router::new()
                    .route("/api/v1/info", get(info))
                    .route("/api/v1/state", get(state))
                    .route("/api/v1/catalog", get(catalog))
                    .route("/api/v1/resources", get(resources))
                    .route("/api/v1/resources/playlists", get(resource_playlists))
                    .route("/api/v1/resources/scenes", get(resource_scenes))
                    .route("/api/v1/command", post(command))
                    .route("/api/v1/events", get(events))
                    .with_state(server);
                if let Ok(listener) = tokio::net::TcpListener::from_std(listener) {
                    let _ = axum::serve(listener, router)
                        .with_graceful_shutdown(async move {
                            while !stop.load(Ordering::Relaxed) {
                                tokio::time::sleep(Duration::from_millis(100)).await;
                            }
                        })
                        .await;
                }
            });
        });
        Ok(Self {
            commands: rx,
            shared,
            shutdown,
            address,
        })
    }
    pub fn publish(&self, value: Value) {
        self.shared.snapshot.send_replace(value);
    }
    pub fn catalog(&self, value: Value) {
        if let Ok(mut c) = self.shared.catalog.lock() {
            *c = value;
        }
    }
}
fn protocol(s: &Shared) -> Value {
    json!({"protocol":1,"service":"amp-control","session":s.session,"password_required":s.password.is_some(),"discovery_available":s.discovery_ready.load(Ordering::Relaxed),"capabilities":["player","dj","catalog","websocket","energy","progression","song_requests","samplers","six_decks","master_clock","scoped_resources"]})
}
fn snapshot(s: &Shared) -> Value {
    let mut value = s.snapshot.borrow().clone();
    value["control"] = protocol(s);
    value
}
async fn info(State(s): State<Arc<Shared>>, h: HeaderMap) -> axum::response::Response {
    if !authorized(&s, &h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(protocol(&s)).into_response()
}
fn authorized(s: &Shared, h: &HeaderMap) -> bool {
    if h.get("x-amp-session")
        .is_some_and(|value| value.to_str().ok() != Some(s.session.as_str()))
    {
        return false;
    }

    if h.get("sec-fetch-site")
        .is_some_and(|v| v != "same-origin" && v != "none")
    {
        return false;
    }
    if let Some(origin) = h.get("origin") {
        let host = h.get("host").and_then(|v| v.to_str().ok()).unwrap_or("");
        if origin.to_str().ok() != Some(&format!("http://{host}")) {
            return false;
        }
    }
    s.password
        .as_ref()
        .is_none_or(|p| h.get("x-amp-password").and_then(|v| v.to_str().ok()) == Some(p.as_str()))
}
async fn state(State(s): State<Arc<Shared>>, h: HeaderMap) -> axum::response::Response {
    if !authorized(&s, &h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(snapshot(&s)).into_response()
}
async fn catalog(State(s): State<Arc<Shared>>, h: HeaderMap) -> axum::response::Response {
    if !authorized(&s, &h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(s.catalog.lock().map(|v| v.clone()).unwrap_or(Value::Null)).into_response()
}
async fn resources(
    State(s): State<Arc<Shared>>,
    h: HeaderMap,
    Query(query): Query<crate::resources::Query>,
) -> axum::response::Response {
    if !authorized(&s, &h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = s
        .catalog
        .lock()
        .map_err(|_| "Catalog temporarily unavailable".to_string())
        .and_then(|c| crate::resources::page(&c, &query));
    match result {
        Ok(mut value) => {
            value["session"] = json!(s.session);
            Json(value).into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({"error":error}))).into_response(),
    }
}
async fn resource_scenes(State(s): State<Arc<Shared>>, h: HeaderMap) -> axum::response::Response {
    if !authorized(&s, &h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let scenes = s
        .catalog
        .lock()
        .ok()
        .and_then(|c| c["scenes"].as_array().cloned())
        .unwrap_or_default();
    Json(json!({"scenes":scenes,"session":s.session})).into_response()
}
async fn resource_playlists(
    State(s): State<Arc<Shared>>,
    h: HeaderMap,
) -> axum::response::Response {
    if !authorized(&s, &h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let lists: Vec<Value> = s
        .catalog
        .lock()
        .ok()
        .and_then(|c| c["resource_playlists"].as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p["available"] == true)
        .map(|mut p| {
            p.as_object_mut().unwrap().remove("tracks");
            p
        })
        .collect();
    Json(json!({"playlists":lists,"session":s.session})).into_response()
}
async fn events(
    State(s): State<Arc<Shared>>,
    h: HeaderMap,
    ws: WebSocketUpgrade,
) -> axum::response::Response {
    if !authorized(&s, &h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    ws.on_upgrade(move|mut socket|async move{
        let mut states=s.snapshot.subscribe();
        loop {
            let mut value=states.borrow_and_update().clone();value["control"]=protocol(&s);let value=value.to_string();
            if socket.send(Message::Text(value.into())).await.is_err(){break;}
            tokio::select! {
                _=tokio::time::sleep(Duration::from_millis(100))=>{if s.shutdown.load(Ordering::Relaxed){break;}},
                changed=states.changed()=>{if changed.is_err(){break;}},
                message=socket.recv()=>{if message.is_none()||matches!(message,Some(Ok(Message::Close(_)))|Some(Err(_))){break;}},
            }
        }
    }).into_response()
}
async fn command(
    State(s): State<Arc<Shared>>,
    h: HeaderMap,
    Json(request): Json<Request>,
) -> axum::response::Response {
    if !authorized(&s, &h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if request.id.is_empty() || request.id.len() > 128 {
        return (
            StatusCode::BAD_REQUEST,
            "A request id of 1–128 bytes is required",
        )
            .into_response();
    }
    let body = serde_json::to_string(&request).unwrap_or_default();
    let mut receiver = {
        let mut cache = match s.cache.lock() {
            Ok(c) => c,
            Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        };
        if let Some(entry) = cache.entries.get(&request.id) {
            if entry.body != body {
                return (
                    StatusCode::CONFLICT,
                    "Request id reused with different command",
                )
                    .into_response();
            }
            entry.result.clone()
        } else {
            if cache.order.len() >= 512 {
                let old = cache.order.front().cloned().unwrap();
                if cache
                    .entries
                    .get(&old)
                    .is_some_and(|e| e.result.borrow().is_none())
                {
                    return (StatusCode::SERVICE_UNAVAILABLE, "Command history is busy")
                        .into_response();
                }
                cache.order.pop_front();
                cache.entries.remove(&old);
            }
            let (result, receiver) = watch::channel(None);
            let id = request.id.clone();
            cache.order.push_back(id.clone());
            cache.entries.insert(
                id.clone(),
                Entry {
                    body,
                    result: receiver.clone(),
                },
            );
            let (reply, answer) = oneshot::channel();
            let sent = s.commands.try_send(Envelope {
                request,
                reply,
                deadline: Instant::now() + Duration::from_secs(4),
            });
            if sent.is_err() {
                result.send_replace(Some(Reply {
                    id,
                    ok: false,
                    status: "rejected".into(),
                    error: Some("Engine command queue is busy".into()),
                }));
            } else {
                s.ctx.request_repaint();
                tokio::spawn(async move {
                    let response = match tokio::time::timeout(Duration::from_secs(5), answer).await
                    {
                        Ok(Ok(v)) => v,
                        _ => Reply {
                            id,
                            ok: false,
                            status: "expired".into(),
                            error: Some(
                                "Engine did not process the command before its deadline".into(),
                            ),
                        },
                    };
                    result.send_replace(Some(response));
                });
            }
            receiver
        }
    };
    loop {
        let response = receiver.borrow().clone();
        if let Some(response) = response {
            let status = if response.ok {
                StatusCode::OK
            } else {
                StatusCode::CONFLICT
            };
            return (status, Json(response)).into_response();
        }
        if receiver.changed().await.is_err() {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn six_deck_and_cast_commands_have_round_trip_contracts() {
        for value in [
            json!({"action":"google_cast","operation":{"operation":"discover"}}),
            json!({"action":"google_cast","operation":{"operation":"start","receiver":"speaker-id","address":"127.0.0.1:8787"}}),
            json!({"action":"google_cast","operation":{"operation":"volume","level":0.3,"muted":false}}),
            json!({"action":"deck_key_lock","deck":"b3","enabled":true}),
            json!({"action":"deck_stems","deck":"a2","levels":[0,1,1]}),
        ] {
            let action: Action = serde_json::from_value(value).unwrap();
            let encoded = serde_json::to_value(action).unwrap();
            serde_json::from_value::<Action>(encoded).unwrap();
        }
    }
    #[test]
    fn auxiliary_commands_preserve_existing_protocol() {
        let request: Request = serde_json::from_value(
            json!({"id":"cue","action":"cue_deck","side":1,"operation":{"operation":"seek","seconds":12.5}}),
        )
        .unwrap();
        assert!(
            matches!(request.command, Action::CueDeck { side:1, operation:crate::side_deck::Operation::Seek {seconds} } if seconds == 12.5)
        );
        let request: Request =
            serde_json::from_value(json!({"id":"pad","action":"side_sampler","side":1,"pad":9}))
                .unwrap();
        assert!(matches!(
            request.command,
            Action::SideSampler { side: 1, pad: 9 }
        ));
    }
    #[test]
    fn load_only_is_default_and_actions_validate() {
        let r: Request =
            serde_json::from_value(json!({"id":"one","action":"call_song","song":"a"})).unwrap();
        assert!(matches!(
            r.command,
            Action::CallSong {
                execution: Execution::LoadOnly,
                deck: Deck::Auto,
                ..
            }
        ));
        assert!(serde_json::from_value::<Request>(json!({"id":"x","action":"invented"})).is_err());
    }
    #[test]
    fn retries_return_same_reply_without_replaying() {
        let ctx = eframe::egui::Context::default();
        let control = Control::start(&ctx, "127.0.0.1:0", Some("secret".into())).unwrap();
        let address = control.address.clone();
        let worker = std::thread::spawn(move || {
            let envelope = control
                .commands
                .recv_timeout(Duration::from_secs(3))
                .unwrap();
            let id = envelope.request.id.clone();
            envelope
                .reply
                .send(Reply {
                    id,
                    ok: true,
                    status: "accepted".into(),
                    error: None,
                })
                .unwrap();
            std::thread::sleep(Duration::from_millis(200));
            assert!(control.commands.try_recv().is_err());
        });
        let request = json!({"id":"unique-press","action":"pause"});
        for _ in 0..2 {
            let v: Value = ureq::post(&format!("http://{address}/api/v1/command"))
                .header("x-amp-password", "secret")
                .send_json(&request)
                .unwrap()
                .body_mut()
                .read_json()
                .unwrap();
            assert_eq!(v["ok"], true);
        }
        assert!(
            ureq::get(&format!("http://{address}/api/v1/state"))
                .call()
                .is_err()
        );
        worker.join().unwrap();
    }
    #[test]
    fn authenticated_protocol_identifies_session_and_rejects_stale_target() {
        let control = Control::start(
            &eframe::egui::Context::default(),
            "127.0.0.1:0",
            Some("secret".into()),
        )
        .unwrap();
        let base = format!("http://{}", control.address);
        assert!(ureq::get(format!("{base}/api/v1/info")).call().is_err());
        let value: Value = ureq::get(format!("{base}/api/v1/info"))
            .header("x-amp-password", "secret")
            .call()
            .unwrap()
            .body_mut()
            .read_json()
            .unwrap();
        assert_eq!(value["protocol"], 1);
        assert_eq!(value["service"], "amp-control");
        assert!(!value["session"].as_str().unwrap().is_empty());
        assert!(
            ureq::get(format!("{base}/api/v1/state"))
                .header("x-amp-password", "secret")
                .header("x-amp-session", "stale-session")
                .call()
                .is_err()
        );
        let state: Value = ureq::get(format!("{base}/api/v1/state"))
            .header("x-amp-password", "secret")
            .header("x-amp-session", value["session"].as_str().unwrap())
            .call()
            .unwrap()
            .body_mut()
            .read_json()
            .unwrap();
        assert_eq!(state["control"]["session"], value["session"]);
    }
}
