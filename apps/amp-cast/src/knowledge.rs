//! Bounded local knowledge indexing; no network lookup, audio upload or library mutation.
use super::*;
use amp_core::knowledge::{Observation, Store};
use axum::extract::{DefaultBodyLimit, Path, Query};
use serde::Deserialize;
static REQUESTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
pub fn routes() -> Router<Arc<Cast>> {
    Router::new()
        .route("/api/knowledge/status", get(status))
        .route("/api/knowledge/audio/{hash}", get(lookup))
        .route("/api/knowledge/import", post(import))
        .layer(DefaultBodyLimit::max(1024))
}
// A disconnected HTTP handler must not release admission while its blocking work continues.
fn background<T: Send + 'static>(
    slot: tokio::sync::SemaphorePermit<'static>,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> tokio::task::JoinHandle<Result<T, String>> {
    tokio::task::spawn_blocking(move || {
        let _slot = slot;
        work()
    })
}
fn store(cast: &Cast) -> Result<Store, String> {
    cast.config_dir
        .lock()
        .map_err(|_| "Configuration unavailable")?
        .as_ref()
        .map(|dir| Store::new(dir.join("knowledge.ampdb")))
        .ok_or_else(|| "Server configuration directory is not available".into())
}
pub(crate) fn index_preparation(
    cast: &Cast,
    prep: &amp_core::Preparation,
) -> Result<usize, String> {
    let Some(record) = Observation::from_preparation(prep)? else {
        return Ok(0);
    };
    store(cast)?.insert(&[record])
}
fn failed(error: impl std::fmt::Display) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({"error":error.to_string()})),
    )
        .into_response()
}
async fn status(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(slot) = REQUESTS.try_acquire() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let db = match store(&cast) {
        Ok(db) => db,
        Err(e) => return failed(e),
    };
    match background(slot, move || db.stats()).await {
        Ok(Ok(stats))=>Json(serde_json::json!({"schema":1,"server_id":cast.server_id,"stats":stats,"automatic_reuse":false,"storage":"device_local","source_audio_stored":false})).into_response(),
        Ok(Err(e))=>failed(e),Err(e)=>failed(e),
    }
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    after: Option<String>,
    limit: Option<usize>,
}
async fn lookup(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Path(hash): Path<String>,
    Query(page): Query<Page>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let limit = page.limit.unwrap_or(50);
    if !(1..=100).contains(&limit)
        || hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || page.after.as_ref().is_some_and(|s| {
            s.len() != 64
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })
    {
        return (StatusCode::BAD_REQUEST, "Invalid identity or page bounds").into_response();
    }
    let Ok(slot) = REQUESTS.try_acquire() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let db = match store(&cast) {
        Ok(db) => db,
        Err(e) => return failed(e),
    };
    match background(slot, move || db.lookup(&hash, page.after.as_deref(), limit)).await {
        Ok(Ok(rows)) => {
            let next = if rows.len() == limit {
                rows.last().map(|r| r.id.clone())
            } else {
                None
            };
            Json(serde_json::json!({"observations":rows,"next_after":next})).into_response()
        }
        Ok(Err(e)) => failed(e),
        Err(e) => failed(e),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    #[serde(default)]
    offset: usize,
    limit: Option<usize>,
}
async fn import(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Json(batch): Json<Batch>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let limit = batch.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) || batch.offset > cast.tracks.len() {
        return (StatusCode::BAD_REQUEST, "Invalid import bounds").into_response();
    }
    let Ok(slot) = REQUESTS.try_acquire() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let db = match store(&cast) {
        Ok(db) => db,
        Err(e) => return failed(e),
    };
    // Only cached preparation summaries are read here. No scan, rehash or audio decode.
    // Background analysis also indexes newly completed preparation.
    let end = batch.offset.saturating_add(limit).min(cast.tracks.len());
    let preparations: Vec<_> = cast.tracks[batch.offset..end]
        .iter()
        .map(|t| t.preparation.clone())
        .collect();
    let total = cast.tracks.len();
    match background(slot, move || -> Result<_,String> {
        let mut records=Vec::new(); let mut unavailable=0; let mut invalid=0;
        for prep in preparations {
            match Observation::from_preparation(&prep) {
                Ok(Some(row))=>records.push(row),Ok(None)=>unavailable+=1,Err(_)=>invalid+=1,
            }
        }
        let eligible=records.len(); let inserted=db.insert(&records)?;
        Ok(serde_json::json!({"snapshot":"server_loaded_preparation","examined":end-batch.offset,"inserted":inserted,"already_present":eligible-inserted,"unavailable":unavailable,"invalid":invalid,"next_offset":if end<total {Some(end)}else{None},"automatic_reuse":false}))
    }).await {
        Ok(Ok(value))=>Json(value).into_response(),Ok(Err(e))=>failed(e),Err(e)=>failed(e),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, http::Request};
    use tower::ServiceExt;
    #[tokio::test]
    async fn disconnect_does_not_admit_more_work_until_blocking_job_finishes() {
        static LIMIT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
        let slot = LIMIT.try_acquire().unwrap();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let handler = tokio::spawn(async move {
            background(slot, move || {
                let _ = started.send(());
                wait.recv().map_err(|e| e.to_string())?;
                Ok(())
            })
            .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(2), ready)
            .await
            .unwrap()
            .unwrap();
        handler.abort();
        let _ = handler.await;
        assert!(LIMIT.try_acquire().is_err());
        release.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if LIMIT.try_acquire().is_ok() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn authenticated_bounded_import_reuses_data_without_changing_playback_or_music() {
        let root = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let track = amp_core::Track {
            path: "missing.mp3".into(),
            title: "Test".into(),
            preparation: amp_core::Preparation {
                source_sha256: Some("a".repeat(64)),
                bpm: Some(120.),
                analysis: Some(amp_core::preparation::AnalysisInfo {
                    engine: "fixture".into(),
                    duration: 30.,
                    tempo_confidence: 0.8,
                    key_confidence: 0.,
                    reviewed: false,
                }),
                ..Default::default()
            },
        };
        let mut library = amp_core::scan(root.path()).unwrap();
        library.tracks.push(track);
        let cast = Cast::new(root.path().into(), library, Some("test-password".into()));
        *cast.config_dir.lock().unwrap() = Some(config.path().into());
        let app = router(cast.clone());
        for url in [
            "/api/knowledge/status",
            "/api/knowledge/audio/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ] {
            assert_eq!(
                app.clone()
                    .oneshot(Request::get(url).body(Body::empty()).unwrap())
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
        }
        let request = || {
            Request::post("/api/knowledge/import")
                .header("content-type", "application/json")
                .header("x-amp-password", "test-password")
                .body(Body::from("{\"limit\":1}"))
                .unwrap()
        };
        for inserted in [1, 0] {
            let response = app.clone().oneshot(request()).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let value: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap())
                    .unwrap();
            assert_eq!(value["inserted"], inserted);
        }
        assert!(!cast.station.lock().await.running);
        assert!(!root.path().join("missing.mp3").exists());
        assert!(config.path().join("knowledge.ampdb").is_file());
        let req = Request::get(format!("/api/knowledge/audio/{}", "a".repeat(64)))
            .header("x-amp-password", "test-password")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let value: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
        assert_eq!(value["observations"].as_array().unwrap().len(), 1);
        let req = Request::post("/api/knowledge/import")
            .header("content-type", "application/json")
            .header("x-amp-password", "test-password")
            .body(Body::from("{\"limit\":101}"))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
        let req = Request::post("/api/knowledge/import")
            .header("content-type", "application/json")
            .header("x-amp-password", "test-password")
            .header("origin", "https://other.invalid")
            .header("host", "localhost")
            .body(Body::from("{}"))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }
}
