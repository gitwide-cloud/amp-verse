//! Explicit live-DJ ownership: stale senders cannot overwrite a newer session.
use super::*;
use axum::extract::DefaultBodyLimit;
pub fn routes() -> Router<Arc<Cast>> {
    Router::new()
        .route("/api/live/start", post(start_live))
        .route("/api/live/audio", post(audio))
        .route("/api/live/stop", post(stop_live))
        .layer(DefaultBodyLimit::max(65536))
}
async fn start_live(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let _operation = cast.operation.lock().await;
    cast.shutdown().await;
    let token = amp_core::coordination::token();
    let mut station = cast.station.lock().await;
    station.running = true;
    station.source = "live_dj".into();
    station.now_playing = Some("Live DJ mix".into());
    station.error = None;
    station.live_token = Some(token.clone());
    station.live_seen = Some(std::time::Instant::now());
    let owner = token.clone();
    let worker = cast.clone();
    station.worker = Some(tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let mut station = worker.station.lock().await;
            if station.live_token.as_deref() != Some(&owner) {
                break;
            }
            if station
                .live_seen
                .is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(8))
            {
                station.running = false;
                station.live_token = None;
                station.error = Some("Live DJ sender disconnected".into());
                break;
            }
        }
    }));
    Json(serde_json::json!({"session":token})).into_response()
}
fn owns(station: &Station, headers: &HeaderMap) -> bool {
    station.running
        && station.live_token.as_deref().is_some_and(|token| {
            headers
                .get("x-amp-live-session")
                .and_then(|v| v.to_str().ok())
                == Some(token)
        })
}
async fn audio(State(cast): State<Arc<Cast>>, headers: HeaderMap, body: Bytes) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut station = cast.station.lock().await;
    if !owns(&station, &headers) {
        return StatusCode::CONFLICT.into_response();
    }
    if body.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    station.live_seen = Some(std::time::Instant::now());
    let _ = cast.audio.send(body);
    StatusCode::NO_CONTENT.into_response()
}
async fn stop_live(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let _operation = cast.operation.lock().await;
    if !owns(&*cast.station.lock().await, &headers) {
        return StatusCode::CONFLICT.into_response();
    }
    cast.shutdown().await;
    StatusCode::NO_CONTENT.into_response()
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;
    #[tokio::test]
    async fn live_session_rejects_stale_senders_and_expires_disconnected_owner() {
        let root = tempfile::tempdir().unwrap();
        let cast = Cast::new(
            root.path().into(),
            Library {
                tracks: vec![],
                tree: amp_core::Folder {
                    name: String::new(),
                    folders: vec![],
                    tracks: vec![],
                },
            },
            Some("secret".into()),
        );
        let app = router(cast.clone());
        let denied = app
            .clone()
            .oneshot(
                Request::post("/api/live/start")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), 401);
        let mut sessions = Vec::new();
        for _ in 0..2 {
            let response = app
                .clone()
                .oneshot(
                    Request::post("/api/live/start")
                        .header("x-amp-password", "secret")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            let bytes = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            sessions.push(value["session"].as_str().unwrap().to_string());
        }
        assert_ne!(sessions[0], sessions[1]);
        let mut listener = cast.audio.subscribe();
        for (i, session) in sessions.iter().enumerate() {
            let response = app
                .clone()
                .oneshot(
                    Request::post("/api/live/audio")
                        .header("x-amp-password", "secret")
                        .header("x-amp-live-session", session)
                        .body(Body::from("test-mp3"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), if i == 0 { 409 } else { 204 });
        }
        assert_eq!(
            listener.recv().await.unwrap(),
            Bytes::from_static(b"test-mp3")
        );
        let stale = app
            .clone()
            .oneshot(
                Request::post("/api/live/stop")
                    .header("x-amp-password", "secret")
                    .header("x-amp-live-session", &sessions[0])
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stale.status(), 409);
        assert!(cast.station.lock().await.running);
        cast.station.lock().await.live_seen =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(9));
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        assert!(!cast.station.lock().await.running);
        cast.shutdown().await;
    }
}
