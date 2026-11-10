pub mod analysis_worker;
pub mod config;
mod live;
pub mod sharing;
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
    source: String,
    #[serde(skip)]
    live_token: Option<String>,
    #[serde(skip)]
    live_seen: Option<std::time::Instant>,
    running: bool,
    now_playing: Option<String>,
    current_index: Option<usize>,
    error: Option<String>,
    #[serde(skip)]
    worker: Option<JoinHandle<()>>,
}
pub struct Cast {
    operation: Mutex<()>,
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
    selection: std::sync::Mutex<Option<serde_json::Value>>,
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
            selection: std::sync::Mutex::new(None),
        })
    }
    pub async fn shutdown(&self) {
        let worker = {
            let mut station = self.station.lock().await;
            station.running = false;
            station.live_token = None;
            station.now_playing = None;
            station.current_index = None;
            station.worker.take()
        };
        if let Some(worker) = worker {
            worker.abort();
            let _ = worker.await;
        }
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
        serde_json::json!({"library_id":cast.library_id,"source":station.source,"capabilities":{"coordination_version":1,"stations_per_server":1,"live_dj":true,"playlist_playback":true,"google_cast":false,"hls":false,"smart_sharing":cast.sharing.load(std::sync::atomic::Ordering::Relaxed)},"server_id":cast.server_id,"station_id":cast.station_id,"owner_server_id":cast.server_id,"station_name":cast.station_name,"analysis":analysis,"running":station.running,"now_playing":station.now_playing,"current_index":station.current_index,"error":station.error,"tracks":cast.tracks.len(),"listeners":cast.audio.receiver_count(),"password_required":cast.password.is_some()}),
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
    Json(
        serde_json::json!({"tracks":cast.tracks.iter().enumerate().map(|(index,track)|serde_json::json!({"index":index,"title":track.title,"cue_in":track.preparation.cue_in,"cue_out":track.preparation.cue_out,"bpm":track.preparation.bpm})).collect::<Vec<_>>()}),
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
    station.source = "playlist".into();
    station.error = None;
    let worker_cast = cast.clone();
    let task = tokio::spawn(async move {
        let outcome = broadcast_tracks(worker_cast.clone()).await;
        let mut station = worker_cast.station.lock().await;
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
async fn broadcast_tracks(cast: Arc<Cast>) -> Result<(), String> {
    let selection = cast.selection.lock().unwrap().clone();
    let indices: Vec<usize> = selection
        .as_ref()
        .and_then(|p| p["tracks"].as_array())
        .map(|a| {
            a.iter()
                .filter_map(|i| i.as_u64().map(|i| i as usize))
                .collect()
        })
        .unwrap_or_else(|| (0..cast.tracks.len()).collect());
    let profile: Option<amp_core::show::SmartPlaylist> = selection
        .as_ref()
        .and_then(|p| serde_json::from_value(p["rules"].clone()).ok());
    let mut recent = Vec::new();
    let mut current = None;
    let mut cursor = 0;
    loop {
        let index = if let Some(profile) = &profile {
            amp_core::show::choose(
                profile,
                &cast.tracks,
                current,
                &recent,
                profile.target_energy,
                0,
            )
            .or_else(|| {
                current.filter(|i| {
                    profile.fallback != amp_core::show::Fallback::Stop
                        && amp_core::show::eligible(profile, &cast.tracks[*i])
                })
            })
            .ok_or("No eligible songs remain in Smart Playlist")?
        } else {
            let i = *indices
                .get(cursor % indices.len().max(1))
                .ok_or("Playlist is empty")?;
            cursor += 1;
            i
        };
        let track = cast.tracks.get(index).ok_or("Invalid playlist track")?;
        let path = amp_core::resolve(&cast.root, &track.path).map_err(|e| e.to_string())?;
        // Read once before loading; never change the running track's cues.
        let preparation = amp_core::preparation::load(&path)?;
        {
            let mut station = cast.station.lock().await;
            station.now_playing = Some(track.title.clone());
            station.current_index = Some(index);
        }
        let mut command = tokio::process::Command::new("ffmpeg");
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command
            .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-re"])
            .arg("-ss")
            .arg(preparation.cue_in.to_string())
            .arg("-i")
            .arg(path)
            .args(
                preparation
                    .cue_out
                    .map(|out| vec!["-t".to_string(), (out - preparation.cue_in).to_string()])
                    .unwrap_or_default(),
            )
            .args([
                "-vn",
                "-ac",
                "2",
                "-ar",
                "44100",
                "-c:a",
                "libmp3lame",
                "-b:a",
                "128k",
                "-write_xing",
                "0",
                "-id3v2_version",
                "0",
                "-f",
                "mp3",
                "pipe:1",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("FFmpeg unavailable: {e}"))?;
        let mut stdout = child.stdout.take().ok_or("Missing encoder pipe")?;
        let mut buffer = [0u8; 4096];
        let mut total = 0;
        loop {
            let size = stdout.read(&mut buffer).await.map_err(|e| e.to_string())?;
            if size == 0 {
                break;
            }
            total += size;
            // One encoder feeds all listeners. A listener never starts its own station.
            let _ = cast.audio.send(Bytes::copy_from_slice(&buffer[..size]));
        }
        let result = child.wait().await.map_err(|e| e.to_string())?;
        if !result.success() || total == 0 {
            return Err(format!("Audio encoding failed: {}", track.title));
        }
        current = Some(index);
        recent.push(amp_core::show::song_id(track));
        if recent.len() > 100 {
            recent.remove(0);
        }
    }
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
