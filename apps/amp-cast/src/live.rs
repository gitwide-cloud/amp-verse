//! Explicit live-DJ ownership: stale senders cannot overwrite a newer session.
use super::*;
use axum::extract::DefaultBodyLimit;
use sha2::{Digest, Sha256};
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
    if let Some(previous) = headers.get("x-amp-previous-session") {
        let mut station = cast.station.lock().await;
        if let Some(previous) = previous.to_str().ok()
            && station.source == "live_dj"
            && station.running
            && station.live_resume_from.as_deref() == Some(previous)
            && station.live_token.is_some()
            && station
                .live_seen
                .is_some_and(|seen| seen.elapsed() <= std::time::Duration::from_secs(60))
        {
            station.live_seen = Some(std::time::Instant::now());
            return Json(serde_json::json!({"session":station.live_token,"upload_sequence":true,"guarded_resume":true,"next_sequence":station.live_next_sequence})).into_response();
        }
        let allowed = previous.to_str().ok().is_some_and(|previous| {
            station.source == "live_dj"
                && (station.live_token.as_deref() == Some(previous)
                    || (!station.running
                        && station.live_token.is_some()
                        && station.live_resume_from.as_deref() == Some(previous)))
                && station
                    .live_seen
                    .is_some_and(|seen| seen.elapsed() <= std::time::Duration::from_secs(60))
        });
        if !allowed {
            return (
                StatusCode::CONFLICT,
                "Live owner changed; automatic resume denied",
            )
                .into_response();
        }
    }
    cast.shutdown().await;
    let token = amp_core::coordination::token();
    let mut station = cast.station.lock().await;
    station.running = true;
    station.audio_ready = false;
    station.source = "live_dj".into();
    station.now_playing = Some("Live DJ mix".into());
    station.error = None;
    station.live_token = Some(token.clone());
    station.live_next_sequence = 0;
    station.live_last_hash = None;
    station.live_resume_from = headers
        .get("x-amp-previous-session")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
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
                // Retain this owner token briefly for guarded reconnection. Explicit stop,
                // another owner, and radio playback still invalidate automatic resume.
                station.audio_ready = false;
                station.error = Some("Live DJ sender disconnected".into());
                break;
            }
        }
    }));
    Json(serde_json::json!({"session":token,"upload_sequence":true,"guarded_resume":true,"next_sequence":0})).into_response()
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
    let sequence = match headers.get("x-amp-live-sequence") {
        Some(value) => match value.to_str().ok().and_then(|v| v.parse::<u64>().ok()) {
            Some(sequence) if sequence < u64::MAX => Some(sequence),
            _ => return StatusCode::BAD_REQUEST.into_response(),
        },
        None => None,
    };
    let hash = sequence.map(|_| format!("{:x}", Sha256::digest(&body)));
    if let Some(sequence) = sequence {
        if sequence != station.live_next_sequence {
            if sequence.checked_add(1) == Some(station.live_next_sequence)
                && hash == station.live_last_hash
            {
                station.live_seen = Some(std::time::Instant::now());
                return (
                    StatusCode::NO_CONTENT,
                    [(
                        "x-amp-live-next-sequence",
                        station.live_next_sequence.to_string(),
                    )],
                )
                    .into_response();
            }
            return (StatusCode::CONFLICT, "Live upload sequence mismatch").into_response();
        }
        station.live_next_sequence += 1;
        station.live_last_hash = hash;
    }
    station.live_seen = Some(std::time::Instant::now());
    station.audio_ready = true;
    cast.recovery.push(&body);
    let _ = cast.audio.send(body);
    (
        StatusCode::NO_CONTENT,
        [(
            "x-amp-live-next-sequence",
            station.live_next_sequence.to_string(),
        )],
    )
        .into_response()
}
async fn stop_live(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let _operation = cast.operation.lock().await;
    {
        let station = cast.station.lock().await;
        if station.source != "live_dj"
            || !station.live_token.as_deref().is_some_and(|token| {
                headers
                    .get("x-amp-live-session")
                    .and_then(|v| v.to_str().ok())
                    == Some(token)
            })
        {
            return StatusCode::CONFLICT.into_response();
        }
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
    async fn lost_resume_ack_followed_by_watchdog_expiry_creates_an_active_guarded_owner() {
        let root = tempfile::tempdir().unwrap();
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        let app = router(cast.clone());
        async fn start(app: Router, previous: Option<&str>) -> String {
            let mut request = Request::post("/api/live/start");
            if let Some(previous) = previous {
                request = request.header("x-amp-previous-session", previous);
            }
            let response = app
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            let bytes = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["session"]
                .as_str()
                .unwrap()
                .to_owned()
        }
        let first = start(app.clone(), None).await;
        let lost_ack = start(app.clone(), Some(&first)).await;
        assert_ne!(first, lost_ack);
        // The producer never learns B's token, and its network remains unavailable
        // long enough for the real B watchdog to stop that unacknowledged session.
        cast.station.lock().await.live_seen =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(9));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while cast.station.lock().await.running {
            assert!(
                std::time::Instant::now() < deadline,
                "live watchdog did not expire"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let recovered = start(app.clone(), Some(&first)).await;
        assert_ne!(recovered, lost_ack, "never acknowledge an inactive session");
        assert!(cast.station.lock().await.running);
        // A second lost response must now acknowledge active C without restarting it.
        assert_eq!(start(app.clone(), Some(&first)).await, recovered);
        let mut listener = cast.audio.subscribe();
        let upload = |token: &str| {
            Request::post("/api/live/audio")
                .header("x-amp-live-session", token)
                .header("x-amp-live-sequence", "0")
                .body(Body::from("current audio"))
                .unwrap()
        };
        assert_eq!(
            app.clone()
                .oneshot(upload(&lost_ack))
                .await
                .unwrap()
                .status(),
            409
        );
        assert_eq!(
            app.clone()
                .oneshot(upload(&recovered))
                .await
                .unwrap()
                .status(),
            204
        );
        assert_eq!(
            listener.recv().await.unwrap(),
            Bytes::from_static(b"current audio")
        );
        let newer = start(app.clone(), None).await;
        let old_retry = Request::post("/api/live/start")
            .header("x-amp-previous-session", &first)
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.oneshot(old_retry).await.unwrap().status(), 409);
        assert_eq!(
            cast.station.lock().await.live_token.as_deref(),
            Some(newer.as_str())
        );
        cast.shutdown().await;
    }
    #[tokio::test]
    async fn guarded_resume_is_idempotent_and_cannot_steal_after_stop_new_owner_or_expiry() {
        let root = tempfile::tempdir().unwrap();
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        let app = router(cast.clone());
        async fn start(app: Router, previous: Option<&str>) -> Response {
            let mut request = Request::post("/api/live/start");
            if let Some(previous) = previous {
                request = request.header("x-amp-previous-session", previous);
            }
            app.oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap()
        }
        async fn token(response: Response) -> String {
            assert_eq!(response.status(), 200);
            let bytes = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["session"]
                .as_str()
                .unwrap()
                .to_owned()
        }
        let first = token(start(app.clone(), None).await).await;
        {
            let mut station = cast.station.lock().await;
            station.running = false;
            station.live_seen = Some(std::time::Instant::now() - std::time::Duration::from_secs(9));
        }
        let resumed = token(start(app.clone(), Some(&first)).await).await;
        assert_ne!(first, resumed);
        assert_eq!(
            token(start(app.clone(), Some(&first)).await).await,
            resumed,
            "lost ACK must not restart twice"
        );
        let newer = token(start(app.clone(), None).await).await;
        assert_eq!(start(app.clone(), Some(&resumed)).await.status(), 409);
        assert_eq!(
            cast.station.lock().await.live_token.as_deref(),
            Some(newer.as_str())
        );
        let stop = Request::post("/api/live/stop")
            .header("x-amp-live-session", &newer)
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.clone().oneshot(stop).await.unwrap().status(), 204);
        assert_eq!(start(app.clone(), Some(&newer)).await.status(), 409);
        let expired = token(start(app.clone(), None).await).await;
        cast.station.lock().await.live_seen =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(61));
        assert_eq!(start(app.clone(), Some(&expired)).await.status(), 409);
        cast.station.lock().await.source = "smart_radio".into();
        assert_eq!(start(app, Some(&expired)).await.status(), 409);
        cast.shutdown().await;
    }
    #[tokio::test]
    async fn sequenced_upload_retries_are_idempotent_and_conflicts_do_not_replace_audio() {
        let root = tempfile::tempdir().unwrap();
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        let app = router(cast.clone());
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/live/start")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let session: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let token = session["session"].as_str().unwrap();
        assert_eq!(session["upload_sequence"], true);
        let mut receiver = cast.audio.subscribe();
        let request = |sequence: &str, body: &'static str| {
            Request::post("/api/live/audio")
                .header("x-amp-live-session", token)
                .header("x-amp-live-sequence", sequence)
                .body(Body::from(body))
                .unwrap()
        };
        assert_eq!(
            app.clone()
                .oneshot(request("0", "audio-zero"))
                .await
                .unwrap()
                .status(),
            204
        );
        assert_eq!(
            receiver.recv().await.unwrap(),
            Bytes::from_static(b"audio-zero")
        );
        let retry = app
            .clone()
            .oneshot(request("0", "audio-zero"))
            .await
            .unwrap();
        assert_eq!(retry.status(), 204);
        assert_eq!(retry.headers()["x-amp-live-next-sequence"], "1");
        assert!(matches!(
            receiver.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        assert_eq!(
            app.clone()
                .oneshot(request("0", "changed audio"))
                .await
                .unwrap()
                .status(),
            409
        );
        assert_eq!(
            app.clone()
                .oneshot(request("2", "gap"))
                .await
                .unwrap()
                .status(),
            409
        );
        assert_eq!(
            app.clone()
                .oneshot(request("oops", "invalid"))
                .await
                .unwrap()
                .status(),
            400
        );
        assert_eq!(
            app.clone()
                .oneshot(request("1", "audio-one"))
                .await
                .unwrap()
                .status(),
            204
        );
        assert_eq!(
            receiver.recv().await.unwrap(),
            Bytes::from_static(b"audio-one")
        );
        cast.shutdown().await;
    }
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
