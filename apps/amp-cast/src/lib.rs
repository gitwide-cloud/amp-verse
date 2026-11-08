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
    task::AbortHandle,
};

#[derive(Default, Serialize)]
struct Station {
    running: bool,
    now_playing: Option<String>,
    error: Option<String>,
    #[serde(skip)]
    worker: Option<AbortHandle>,
}
pub struct Cast {
    root: PathBuf,
    tracks: Vec<Track>,
    password: Option<String>,
    station: Mutex<Station>,
    audio: broadcast::Sender<Bytes>,
}
impl Cast {
    pub fn new(root: PathBuf, library: Library, password: Option<String>) -> Arc<Self> {
        let (audio, _) = broadcast::channel(256);
        Arc::new(Self {
            root,
            tracks: library.tracks,
            password,
            station: Mutex::new(Station::default()),
            audio,
        })
    }
    fn authorized(&self, headers: &HeaderMap) -> bool {
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
        .route("/api/status", get(status))
        .route("/api/start", post(start))
        .route("/api/stop", post(stop))
        .route("/listen.mp3", get(listen))
        .with_state(cast)
}
async fn status(State(cast): State<Arc<Cast>>) -> Json<serde_json::Value> {
    let station = cast.station.lock().await;
    Json(
        serde_json::json!({"running":station.running,"now_playing":station.now_playing,"error":station.error,"tracks":cast.tracks.len(),"listeners":cast.audio.receiver_count(),"password_required":cast.password.is_some()}),
    )
}
async fn start(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
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
    station.error = None;
    let worker_cast = cast.clone();
    let task = tokio::spawn(async move {
        let outcome = broadcast_tracks(worker_cast.clone()).await;
        let mut station = worker_cast.station.lock().await;
        station.running = false;
        station.now_playing = None;
        station.error = outcome.err();
        station.worker = None;
    });
    station.worker = Some(task.abort_handle());
    StatusCode::NO_CONTENT.into_response()
}
async fn stop(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut station = cast.station.lock().await;
    if let Some(worker) = station.worker.take() {
        worker.abort();
    }
    station.running = false;
    station.now_playing = None;
    StatusCode::NO_CONTENT.into_response()
}
async fn broadcast_tracks(cast: Arc<Cast>) -> Result<(), String> {
    loop {
        for track in &cast.tracks {
            let path = amp_core::resolve(&cast.root, &track.path).map_err(|e| e.to_string())?;
            cast.station.lock().await.now_playing = Some(track.title.clone());
            let mut command = tokio::process::Command::new("ffmpeg");
            #[cfg(windows)]
            command.creation_flags(0x08000000);
            let mut child = command
                .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-re"])
                .arg("-ss")
                .arg(track.preparation.cue_in.to_string())
                .arg("-i")
                .arg(path)
                .args(
                    track
                        .preparation
                        .cue_out
                        .map(|out| {
                            vec![
                                "-t".to_string(),
                                (out - track.preparation.cue_in).to_string(),
                            ]
                        })
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
}
