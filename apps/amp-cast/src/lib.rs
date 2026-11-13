pub mod analysis_worker;
pub mod config;
mod live;
mod radio;
mod recovery;
pub mod sharing;
mod station_control;
use amp_core::{Library, Track};
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use bytes::Bytes;
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tokio::{
    io::AsyncReadExt,
    sync::{Mutex, broadcast},
    task::JoinHandle,
};

#[derive(Default, Serialize)]
struct Station {
    name: Option<String>,
    audio_ready: bool,
    #[serde(skip)]
    generation: u64,
    source: String,
    #[serde(skip)]
    live_token: Option<String>,
    #[serde(skip)]
    live_seen: Option<std::time::Instant>,
    running: bool,
    now_playing: Option<String>,
    current_index: Option<usize>,
    mix_revision: u64,
    error: Option<String>,
    #[serde(skip)]
    worker: Option<JoinHandle<()>>,
}
pub struct Cast {
    operation: Mutex<()>,
    pub listen_address: std::sync::Mutex<Option<std::net::SocketAddr>>,
    server_output: std::sync::Mutex<Option<std::process::Child>>,
    server_id: String,
    library_id: Option<String>,
    pub config_dir: std::sync::Mutex<Option<PathBuf>>,
    station_id: String,
    station_name: String,
    pub analysis: Arc<analysis_worker::Worker>,
    pub sharing: std::sync::atomic::AtomicBool,
    root: PathBuf,
    tracks: Vec<Track>,
    password: Option<String>,
    station: Mutex<Station>,
    audio: broadcast::Sender<Bytes>,
    recovery: recovery::Recovery,
    selection: std::sync::Mutex<Option<serde_json::Value>>,
    mix: std::sync::Mutex<station_control::MixSettings>,
}
impl Cast {
    pub fn new(root: PathBuf, library: Library, password: Option<String>) -> Arc<Self> {
        Self::with_identity(
            root,
            library,
            password,
            amp_core::coordination::token(),
            amp_core::coordination::token(),
            "AMP Radio".into(),
        )
    }
    pub fn with_identity(
        root: PathBuf,
        library: Library,
        password: Option<String>,
        server_id: String,
        station_id: String,
        station_name: String,
    ) -> Arc<Self> {
        let root = root.canonicalize().unwrap_or(root);
        let (audio, _) = broadcast::channel(256);
        Arc::new(Self {
            operation: Mutex::new(()),
            listen_address: std::sync::Mutex::new(None),
            server_output: std::sync::Mutex::new(None),
            library_id: amp_core::coordination::library_id(&root).ok(),
            config_dir: std::sync::Mutex::new(None),
            server_id,
            station_id,
            station_name,
            analysis: analysis_worker::Worker::new(),
            sharing: std::sync::atomic::AtomicBool::new(false),
            root,
            tracks: library.tracks,
            password,
            station: Mutex::new(Station::default()),
            audio,
            recovery: Default::default(),
            selection: std::sync::Mutex::new(None),
            mix: std::sync::Mutex::new(Default::default()),
        })
    }
    pub async fn shutdown(&self) {
        station_control::stop_output(self);
        let worker = {
            let mut station = self.station.lock().await;
            station.running = false;
            station.generation = station.generation.wrapping_add(1);
            station.live_token = None;
            station.now_playing = None;
            station.current_index = None;
            station.worker.take()
        };
        if let Some(worker) = worker {
            worker.abort();
            let _ = worker.await;
        }
        self.recovery.reset();
    }
    fn authorized(&self, headers: &HeaderMap) -> bool {
        if headers
            .get("x-amp-server")
            .is_some_and(|v| v.to_str().ok() != Some(self.server_id.as_str()))
            || headers
                .get("x-amp-station")
                .is_some_and(|v| v.to_str().ok() != Some(self.station_id.as_str()))
        {
            return false;
        }
        // Reject browser cross-site requests even for password-free trusted-LAN operation.
        if headers
            .get("sec-fetch-site")
            .is_some_and(|v| v != "same-origin" && v != "none")
        {
            return false;
        }
        if let Some(origin) = headers.get("origin") {
            let host = headers
                .get("host")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if origin.to_str().ok() != Some(&format!("http://{host}")) {
                return false;
            }
        }
        self.password.as_ref().is_none_or(|p| {
            headers.get("x-amp-password").and_then(|v| v.to_str().ok()) == Some(p.as_str())
        })
    }
}
pub fn router(cast: Arc<Cast>) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../web/index.html")) }),
        )
        .merge(sharing::routes())
        .merge(live::routes())
        .merge(recovery::routes())
        .merge(station_control::routes())
        .route("/api/status", get(status))
        .route(
            "/app-icon.png",
            get(|| async {
                (
                    [
                        ("content-type", "image/png"),
                        ("cache-control", "public, max-age=86400"),
                    ],
                    include_bytes!("../../../assets/amp-verse-icon.png").as_slice(),
                )
            }),
        )
        .route("/api/analysis/pause", post(pause_analysis))
        .route("/api/analysis/resume", post(resume_analysis))
        .route("/health", get(|| async { "ok" }))
        .route("/api/playlist", get(playlist))
        .route("/api/start", post(start))
        .route("/api/stop", post(stop))
        .route("/listen.mp3", get(listen))
        .with_state(cast)
}
async fn status(State(cast): State<Arc<Cast>>) -> Json<serde_json::Value> {
    let station = cast.station.lock().await;
    let mut analysis = cast.analysis.snapshot();
    analysis.paused_for_radio = analysis.enabled
        && station.running
        && cast
            .analysis
            .pause_for_radio
            .load(std::sync::atomic::Ordering::Relaxed);
    Json(
        serde_json::json!({"mix_requested":cast.mix.lock().unwrap().clone(),"mix_applied_revision":station.mix_revision,"library_id":cast.library_id,"source":station.source,"current_source":if station.source=="live_dj" {serde_json::json!("Player/DJ master")} else {cast.selection.lock().unwrap().as_ref().map_or(serde_json::Value::Null,|v|v["name"].clone())},"capabilities":{"coordination_version":1,"stations_per_server":1,"live_dj":true,"playlist_playback":true,"google_cast":false,"hls":false,"reliable_audio":1,"dual_path":true,"smart_sharing":cast.sharing.load(std::sync::atomic::Ordering::Relaxed)},"server_id":cast.server_id,"station_id":cast.station_id,"owner_server_id":cast.server_id,"station_name":station.name.as_ref().unwrap_or(&cast.station_name),"analysis":analysis,"running":station.running && (station.source!="live_dj" || station.audio_ready),"on_air":station.running && station.audio_ready,"preparing":station.running && !station.audio_ready,"server_output":station_control::output_running(&cast),"now_playing":station.now_playing,"current_index":station.current_index,"error":station.error,"tracks":cast.tracks.len(),"listeners":cast.audio.receiver_count()+cast.recovery.listeners(),"password_required":cast.password.is_some()}),
    )
}
async fn pause_analysis(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if let Some(dir) = cast.config_dir.lock().unwrap().as_ref()
        && let Err(e) = config::Config::set_analysis(dir, false)
    {
        return (StatusCode::SERVICE_UNAVAILABLE, e).into_response();
    }
    cast.analysis
        .enabled
        .store(false, std::sync::atomic::Ordering::Relaxed);
    StatusCode::NO_CONTENT.into_response()
}
async fn resume_analysis(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if let Some(dir) = cast.config_dir.lock().unwrap().as_ref()
        && let Err(e) = config::Config::set_analysis(dir, true)
    {
        return (StatusCode::SERVICE_UNAVAILABLE, e).into_response();
    }
    cast.analysis
        .enabled
        .store(true, std::sync::atomic::Ordering::Relaxed);
    StatusCode::NO_CONTENT.into_response()
}
async fn playlist(State(cast): State<Arc<Cast>>) -> Json<serde_json::Value> {
    let live = cast.station.lock().await.source == "live_dj";
    let selection = cast.selection.lock().unwrap().clone();
    let selected = selection.as_ref().and_then(|v| v["tracks"].as_array());
    Json(
        serde_json::json!({"tracks":cast.tracks.iter().enumerate().filter(|(i,_)|!live&&selected.is_none_or(|list|list.iter().any(|v|v.as_u64()==Some(*i as u64)))).map(|(index,track)|serde_json::json!({"index":index,"title":track.title,"cue_in":track.preparation.cue_in,"cue_out":track.preparation.cue_out,"bpm":track.preparation.bpm})).collect::<Vec<_>>()}),
    )
}
async fn start(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    let _operation = cast.operation.lock().await;
    start_unlocked(State(cast.clone()), headers).await
}
async fn start_unlocked(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            "Password required or cross-site control denied",
        )
            .into_response();
    }
    let mut station = cast.station.lock().await;
    if station.running {
        return StatusCode::NO_CONTENT.into_response();
    }
    if cast.tracks.is_empty() {
        return (StatusCode::CONFLICT, "Music folder is empty").into_response();
    }
    station.running = true;
    station.audio_ready = false;
    station.generation = station.generation.wrapping_add(1);
    let owner = station.generation;
    station.source = "smart_radio".into();
    station.error = None;
    let worker_cast = cast.clone();
    let task = tokio::spawn(async move {
        let outcome = broadcast_tracks(worker_cast.clone(), owner).await;
        let mut station = worker_cast.station.lock().await;
        if station.generation != owner {
            return;
        }
        station.running = false;
        station.now_playing = None;
        station.current_index = None;
        station.error = outcome.err();
        station.worker = None;
    });
    station.worker = Some(task);
    StatusCode::NO_CONTENT.into_response()
}
async fn stop(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let _operation = cast.operation.lock().await;
    cast.shutdown().await;
    StatusCode::NO_CONTENT.into_response()
}
async fn broadcast_tracks(cast: Arc<Cast>, owner: u64) -> Result<(), String> {
    radio::broadcast(cast, owner).await
}
async fn listen(State(cast): State<Arc<Cast>>) -> Response {
    let mut rx = cast.audio.subscribe();
    if !cast.station.lock().await.running {
        return (StatusCode::SERVICE_UNAVAILABLE, "Station stopped").into_response();
    }
    let stream = async_stream::stream! {
        loop {
            tokio::select! {
                message = rx.recv() => match message {
                    Ok(bytes) => yield Ok::<Bytes,std::io::Error>(bytes),
                    Err(_) => break, // disconnect slow listeners instead of accumulating memory
                },
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                    if !cast.station.lock().await.running { break; }
                }
            }
        }
    };
    (
        [
            ("content-type", "audio/mpeg"),
            ("cache-control", "no-store"),
            ("access-control-allow-origin", "*"),
        ],
        Body::from_stream(stream),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;
    fn app(password: Option<String>) -> Router {
        let root = tempfile::tempdir().unwrap();
        let library = amp_core::scan(root.path()).unwrap();
        router(Cast::new(root.path().into(), library, password))
    }
    #[tokio::test]
    async fn station_mix_is_revision_checked_independent_and_does_not_restart_audio() {
        let root = tempfile::tempdir().unwrap();
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        let other = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        {
            let mut station = cast.station.lock().await;
            station.running = true;
            station.generation = 42;
            station.current_index = Some(7);
        }
        let app = router(cast.clone());
        let body =
            serde_json::json!({"revision":0,"target":8,"style":"hype","transitions":4,"hold":true});
        let update = || {
            Request::post("/api/station/mix")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap()
        };
        assert_eq!(app.clone().oneshot(update()).await.unwrap().status(), 200);
        assert_eq!(app.oneshot(update()).await.unwrap().status(), 409);
        assert_eq!(other.mix.lock().unwrap().revision, 0);
        assert_eq!(cast.mix.lock().unwrap().revision, 1);
        let station = cast.station.lock().await;
        assert!(station.running);
        assert_eq!(station.generation, 42);
        assert_eq!(station.current_index, Some(7));
    }
    #[tokio::test]
    async fn password_control_and_public_listener() {
        let app = app(Some("secret".into()));
        let response = app
            .clone()
            .oneshot(Request::post("/api/stop").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/stop")
                    .header("x-amp-password", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 204);
        let response = app
            .clone()
            .oneshot(Request::get("/api/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let response = app
            .oneshot(Request::get("/listen.mp3").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), 503);
    }
    #[tokio::test]
    async fn empty_station_and_cross_site_denied() {
        let app = app(None);
        let response = app
            .clone()
            .oneshot(Request::post("/api/start").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), 409);
        let response = app
            .oneshot(
                Request::post("/api/stop")
                    .header("sec-fetch-site", "cross-site")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn origin_is_checked_with_optional_password_disabled() {
        let app = app(None);
        for (origin, expected) in [
            ("http://radio.local:8787", 204),
            ("http://other.local", 401),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::post("/api/stop")
                        .header("host", "radio.local:8787")
                        .header("origin", origin)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
    }
}
