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
        Ok(Err(error)) => playlist_failure(&error),
        Err(_) => playlist_failure("worker"),
    }
}
fn playlist_failure(error: &str) -> Response {
    // Do not return raw filesystem/database errors: they may contain private paths.
    let detail = if error.to_ascii_lowercase().contains("lock")
        || error.to_ascii_lowercase().contains("busy")
    {
        "Saved playlists are busy. Wait a moment and retry; close another app that is updating this library if it persists."
    } else {
        "The server cannot read saved playlists. Check that its music folder is online and the server account can read the library metadata, then retry."
    };
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({"code":"playlist_store_unavailable","error":detail})),
    )
        .into_response()
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
        Ok(Ok(None)) => {
            return (
                StatusCode::NOT_FOUND,
                "Saved playlist unavailable. Refresh the list and choose it again.",
            )
                .into_response();
        }
        Ok(Err(error)) => return playlist_failure(&error),
        Err(_) => return playlist_failure("worker"),
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
    let destination = local_listener_address(address).to_string();
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
    *cast.server_output_error.lock().unwrap() = None;
    if let Some(mut output) = cast.server_output.lock().unwrap().take() {
        output.stop();
    }
}
pub(super) fn output_running(cast: &Cast) -> bool {
    let mut slot = cast.server_output.lock().unwrap();
    if slot.as_mut().is_some_and(|output| output.running()) {
        return true;
    }
    if let Some(mut output) = slot.take() {
        *cast.server_output_error.lock().unwrap() = output.error();
        output.stop();
    }
    false
}
pub(super) async fn probe_output() -> Result<(), String> {
    let mut command = amp_core::command("ffplay");
    command.args([
        "-nodisp",
        "-autoexit",
        "-loglevel",
        "error",
        "-f",
        "s16le",
        "-sample_rate",
        "44100",
        "-ch_layout",
        "stereo",
        "-i",
        "pipe:0",
    ]);
    probe_output_command(command).await
}
async fn probe_output_command(command: std::process::Command) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let message = "Server speakers are unavailable. Select This device, or enable an audio device for the server account and retry.";
    let mut child=tokio::process::Command::from(command).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).kill_on_drop(true).spawn().map_err(|_|"Server audio player is unavailable. Install FFplay on the server, or select This device.".to_string())?;
    let mut input = child.stdin.take().ok_or(message.to_string())?;
    let write = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        input.write_all(&[0; 8820]),
    )
    .await;
    drop(input);
    let status = tokio::time::timeout(std::time::Duration::from_secs(3), child.wait()).await;
    if !matches!(write, Ok(Ok(()))) || !matches!(status,Ok(Ok(exit)) if exit.success()) {
        let _ = child.kill().await;
        let _ = child.wait().await;
        return Err(message.into());
    }
    Ok(())
}
async fn output_start(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let _operation = cast.operation.lock().await;
    if let Err(message) = probe_output().await {
        *cast.server_output_error.lock().unwrap() = Some(message.clone());
        return (StatusCode::SERVICE_UNAVAILABLE, message).into_response();
    }
    output_start_unlocked(cast.clone()).await
}
pub(super) async fn output_start_unlocked(cast: Arc<Cast>) -> Response {
    if !cast.station.lock().await.running {
        return (StatusCode::CONFLICT, "Start a station first").into_response();
    }
    if output_running(&cast) {
        return StatusCode::NO_CONTENT.into_response();
    }
    let mut command = amp_core::command("ffplay");
    command.args([
        "-nodisp",
        "-autoexit",
        "-loglevel",
        "error",
        "-f",
        "mp3",
        "-i",
        "pipe:0",
    ]);
    match server_output::Output::spawn(command,cast.audio.subscribe()) {
        Ok(output)=>{*cast.server_output.lock().unwrap()=Some(output);*cast.server_output_error.lock().unwrap()=None;StatusCode::NO_CONTENT.into_response()},
        Err(_)=>(StatusCode::SERVICE_UNAVAILABLE,"Server speakers could not start. Check FFplay and the server audio device, or select This device.").into_response(),
    }
}
async fn output_stop(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    stop_output(&cast);
    StatusCode::NO_CONTENT.into_response()
}

// A specific interface binding does not also listen on loopback.
fn local_listener_address(address: std::net::SocketAddr) -> std::net::SocketAddr {
    if address.ip().is_unspecified() {
        std::net::SocketAddr::new(
            if address.is_ipv4() {
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
            } else {
                std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
            },
            address.port(),
        )
    } else {
        address
    }
}

#[cfg(test)]
mod listener_address_tests {
    #[test]
    fn callbacks_use_the_bound_interface_and_preserve_ipv6_url_brackets() {
        for (bound, expected) in [
            ("0.0.0.0:8877", "127.0.0.1:8877"),
            ("192.168.10.5:8877", "192.168.10.5:8877"),
            ("127.0.0.1:8877", "127.0.0.1:8877"),
            ("[::]:8877", "[::1]:8877"),
            ("[fd00::5]:8877", "[fd00::5]:8877"),
        ] {
            assert_eq!(
                super::local_listener_address(bound.parse().unwrap()).to_string(),
                expected
            );
        }
    }
}

#[cfg(test)]
mod playlist_error_tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;
    #[tokio::test]
    async fn unreadable_saved_playlist_store_is_actionable_503_not_missing_404() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("ampdj")).unwrap();
        std::fs::write(
            root.path().join("ampdj/smart-playlists.json"),
            b"invalid-json",
        )
        .unwrap();
        let cast = Cast::new(
            root.path().into(),
            amp_core::Library {
                tracks: vec![],
                tree: amp_core::Folder {
                    name: "test".into(),
                    folders: vec![],
                    tracks: vec![],
                },
            },
            Some("secret".into()),
        );
        let app = routes().with_state(cast);
        for (method, uri) in [
            ("GET", "/api/station/playlists"),
            ("POST", "/api/station/play/missing"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-amp-password", "secret")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE, "{uri}");
            let body = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value["code"], "playlist_store_unavailable");
            assert!(value["error"].as_str().unwrap().contains("music folder"));
            assert!(
                !String::from_utf8_lossy(&body)
                    .contains(&root.path().to_string_lossy().to_string())
            );
        }
    }
}

#[cfg(test)]
mod output_device_tests {
    #[tokio::test]
    async fn unavailable_output_child_is_reaped_and_error_offers_this_device() {
        #[cfg(windows)]
        let mut command = std::process::Command::new("cmd");
        #[cfg(windows)]
        command.args(["/c", "exit", "1"]);
        #[cfg(not(windows))]
        let mut command = std::process::Command::new("sh");
        #[cfg(not(windows))]
        command.args(["-c", "exit 1"]);
        let error = super::probe_output_command(command).await.unwrap_err();
        assert!(error.contains("This device"));
        assert!(error.contains("Server speakers"));
    }
}
