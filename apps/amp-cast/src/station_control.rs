//! Management controls select a station; listening never creates a playback engine.
use super::*;
use axum::extract::Path;
use serde::Deserialize;
pub fn routes() -> Router<Arc<Cast>> {
    Router::new()
        .route("/api/station/playlists", get(playlists))
        .route("/api/station/play/{id}", post(play))
        .route("/api/station/name", post(name))
        .route("/api/station/mix", post(mix))
        .route("/api/live/launch", post(launch))
        .route("/api/output/start", post(output_start))
        .route("/api/output/stop", post(output_stop))
}
#[derive(Clone, Default, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MixSettings {
    #[serde(default)]
    pub revision: u64,
    pub target: Option<f64>,
    pub style: Option<amp_core::show::Style>,
    pub transitions: Option<u32>,
    #[serde(default = "yes")]
    pub hold: bool,
}
fn yes() -> bool {
    true
}
impl MixSettings {
    fn validate(&self) -> bool {
        self.target
            .is_none_or(|e| e.is_finite() && (1. ..=10.).contains(&e))
            && self
                .transitions
                .is_none_or(|n| (1..=100).contains(&n) && self.target.is_some())
    }
}
async fn mix(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Json(mut settings): Json<MixSettings>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !settings.validate() {
        return (
            StatusCode::BAD_REQUEST,
            "Choose energy 1–10 and 1–100 transitions",
        )
            .into_response();
    }
    let mut current = cast.mix.lock().unwrap();
    if settings.revision != current.revision {
        return (
            StatusCode::CONFLICT,
            "Station mix changed; refresh before updating",
        )
            .into_response();
    }
    settings.revision = current.revision.saturating_add(1);
    *current = settings.clone();
    Json(serde_json::json!({"requested":settings,"applies":"next song selection; current audio is unchanged"})).into_response()
}
async fn playlists(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = tokio::task::spawn_blocking(move || sharing::lists(&cast)).await;
    match result {
        Ok(Ok(lists)) => Json(serde_json::json!({"playlists":lists})).into_response(),
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
async fn play(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let source = cast.clone();
    let selected = tokio::task::spawn_blocking(move || {
        sharing::lists(&source)
            .map(|lists| lists.into_iter().find(|p| p["id"].as_str() == Some(&id)))
    })
    .await;
    let value = match selected {
        Ok(Ok(Some(value))) => value,
        _ => return (StatusCode::NOT_FOUND, "Saved playlist unavailable").into_response(),
    };
    if value["tracks"].as_array().is_none_or(|a| a.is_empty()) {
        return (StatusCode::CONFLICT, "Playlist has no available songs").into_response();
    }
    let _operation = cast.operation.lock().await;
    cast.shutdown().await;
    *cast.selection.lock().unwrap() = Some(value);
    super::start_unlocked(State(cast.clone()), headers).await
}
#[derive(Deserialize)]
struct Name {
    name: String,
}
async fn name(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Json(value): Json<Name>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let name = value.name.trim();
    if name.is_empty() || name.len() > 120 || name.chars().any(char::is_control) {
        return (
            StatusCode::BAD_REQUEST,
            "Choose a station name (1–120 characters)",
        )
            .into_response();
    }
    cast.station.lock().await.name = Some(name.to_owned());
    StatusCode::NO_CONTENT.into_response()
}
#[derive(Deserialize)]
struct Live {
    #[serde(default)]
    source_password: String,
}
async fn launch(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Json(value): Json<Live>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(address) = *cast.listen_address.lock().unwrap() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Server listener address unavailable",
        )
            .into_response();
    };
    let destination = format!("127.0.0.1:{}", address.port());
    let secret = cast.password.clone().unwrap_or_default();
    let result=tokio::task::spawn_blocking(move||->Result<(),String>{
        let agent:ureq::Agent=ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(5))).max_redirects(0).proxy(None).build().into();
        let command=serde_json::json!({"id":format!("cast-live-{}",amp_core::coordination::token()),"action":"start_live_broadcast","address":destination,"password":secret});
        let control=std::env::var("AMP_CAST_SOURCE_CONTROL").unwrap_or_else(|_|"127.0.0.1:8790".into());
        let control:std::net::SocketAddr=control.parse().map_err(|_|"Invalid AMP_CAST_SOURCE_CONTROL")?;
        let mut response=agent.post(format!("http://{control}/api/v1/command")).header("x-amp-password",&value.source_password).send_json(&command).map_err(|e|format!("Cannot reach AMP Player/DJ on this server: {e}. Open the player and check its control password."))?;
        let reply:serde_json::Value=response.body_mut().read_json().map_err(|e|e.to_string())?;
        if reply["ok"]!=true{return Err(reply["error"].as_str().unwrap_or("Live sender rejected request").into());}Ok(())
    }).await;
    match result {
        Ok(Ok(())) => StatusCode::ACCEPTED.into_response(),
        Ok(Err(e)) => (StatusCode::CONFLICT, e).into_response(),
        _ => (StatusCode::SERVICE_UNAVAILABLE, "Live sender unavailable").into_response(),
    }
}
pub(super) fn stop_output(cast: &Cast) {
    if let Some(mut child) = cast.server_output.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}
pub(super) fn output_running(cast: &Cast) -> bool {
    let mut output = cast.server_output.lock().unwrap();
    if let Some(child) = output.as_mut() {
        matches!(child.try_wait(), Ok(None))
    } else {
        false
    }
}
async fn output_start(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let _operation = cast.operation.lock().await;
    if !cast.station.lock().await.running {
        return (StatusCode::CONFLICT, "Start a station first").into_response();
    }
    if output_running(&cast) {
        return StatusCode::NO_CONTENT.into_response();
    }
    let Some(address) = *cast.listen_address.lock().unwrap() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let url = format!("http://127.0.0.1:{}/listen.mp3", address.port());
    let mut command = amp_core::command("ffplay");
    command
        .args([
            "-nodisp",
            "-autoexit",
            "-loglevel",
            "error",
            "-f",
            "mp3",
            "-i",
            "pipe:0",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    match command.spawn() {
        Ok(mut child) => {
            let Some(mut input) = child.stdin.take() else {
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            };
            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let agent: ureq::Agent = ureq::Agent::config_builder()
                    .proxy(None)
                    .max_redirects(0)
                    .build()
                    .into();
                if let Ok(mut response) = agent.get(url).call() {
                    let mut source = response.body_mut().as_reader();
                    let mut buffer = [0u8; 8192];
                    while let Ok(n) = source.read(&mut buffer) {
                        if n == 0 || input.write_all(&buffer[..n]).is_err() {
                            break;
                        }
                    }
                }
            });
            *cast.server_output.lock().unwrap() = Some(child);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("Server sound output unavailable (FFplay): {e}"),
        )
            .into_response(),
    }
}
async fn output_stop(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    stop_output(&cast);
    StatusCode::NO_CONTENT.into_response()
}
