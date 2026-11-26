//! One encoded timeline, bounded replay, and identical chunks on every server interface.
use super::*;
use axum::extract::{Path, Query};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex as StdMutex,
    time::{Duration, Instant},
};
const CHUNK: usize = 8192;
const LIMIT: usize = 2 * 1024 * 1024;
struct Chunk {
    sequence: u64,
    bytes: Bytes,
    hash: String,
    at: Instant,
}
struct Buffer {
    epoch: String,
    next: u64,
    chunks: VecDeque<Chunk>,
    pending: Vec<u8>,
    bytes: usize,
    listeners: HashMap<String, Instant>,
}
pub(super) struct Recovery {
    buffer: StdMutex<Buffer>,
    changed: tokio::sync::Notify,
}
impl Default for Recovery {
    fn default() -> Self {
        Self {
            buffer: StdMutex::new(Buffer {
                epoch: amp_core::coordination::token(),
                next: 0,
                chunks: VecDeque::new(),
                pending: Vec::new(),
                bytes: 0,
                listeners: HashMap::new(),
            }),
            changed: Default::default(),
        }
    }
}
impl Recovery {
    pub fn reset(&self) {
        let mut b = self.buffer.lock().unwrap();
        b.epoch = amp_core::coordination::token();
        b.next = 0;
        b.chunks.clear();
        b.pending.clear();
        b.bytes = 0;
        b.listeners.clear();
        drop(b);
        self.changed.notify_waiters();
    }
    pub fn push(&self, bytes: &[u8]) {
        let mut b = self.buffer.lock().unwrap();
        b.pending.extend_from_slice(bytes);
        while b.pending.len() >= CHUNK {
            let tail = b.pending.split_off(CHUNK);
            let data = Bytes::from(std::mem::replace(&mut b.pending, tail));
            let sequence = b.next;
            b.next += 1;
            b.bytes += data.len();
            b.chunks.push_back(Chunk {
                sequence,
                hash: format!("{:x}", Sha256::digest(&data)),
                bytes: data,
                at: Instant::now(),
            });
            while b.bytes > LIMIT || b.chunks.len() > 2048 {
                if let Some(old) = b.chunks.pop_front() {
                    b.bytes -= old.bytes.len();
                }
            }
        }
        drop(b);
        self.changed.notify_waiters();
    }
    pub fn listeners(&self) -> usize {
        let mut b = self.buffer.lock().unwrap();
        b.listeners
            .retain(|_, seen| seen.elapsed() < Duration::from_secs(10));
        b.listeners.len()
    }
}
pub fn routes() -> Router<Arc<Cast>> {
    Router::new()
        .route("/api/audio/manifest", get(manifest))
        .route("/api/audio/chunks/{sequence}", get(chunk))
}
fn matches(cast: &Cast, headers: &HeaderMap) -> bool {
    [
        ("x-amp-server", &cast.server_id),
        ("x-amp-station", &cast.station_id),
    ]
    .iter()
    .all(|(key, value)| {
        headers
            .get(*key)
            .is_none_or(|v| v.to_str().ok() == Some(value.as_str()))
    })
}
async fn manifest(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !matches(&cast, &headers) {
        return StatusCode::CONFLICT.into_response();
    }
    if !cast.station.lock().await.running {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let port = cast
        .listen_address
        .lock()
        .unwrap()
        .map_or(8787, |address| address.port());
    let mut interfaces = std::collections::BTreeMap::new();
    if let Ok(addresses) = if_addrs::get_if_addrs() {
        for address in addresses {
            if let std::net::IpAddr::V4(ip) = address.ip()
                && ip.is_private()
                && !address.is_loopback()
            {
                interfaces
                    .entry(address.name.clone())
                    .or_insert(format!("http://{ip}:{port}"));
            }
        }
    }
    let b = cast.recovery.buffer.lock().unwrap();
    let Some(first) = b.chunks.front() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let start = b
        .chunks
        .iter()
        .find(|c| c.at.elapsed() <= Duration::from_secs(6))
        .map_or(first.sequence, |c| c.sequence);
    Json(serde_json::json!({"protocol":1,"server_id":cast.server_id,"station_id":cast.station_id,"epoch":b.epoch,"first_sequence":first.sequence,"start_sequence":start,"next_sequence":b.next,"buffer_ms":6000,"max_chunk_bytes":CHUNK,"paths":interfaces.into_values().take(8).collect::<Vec<_>>()})).into_response()
}
#[derive(Deserialize)]
struct Request {
    epoch: String,
}
async fn chunk(
    State(cast): State<Arc<Cast>>,
    Path(sequence): Path<u64>,
    Query(request): Query<Request>,
    headers: HeaderMap,
) -> Response {
    if !matches(&cast, &headers) {
        return StatusCode::CONFLICT.into_response();
    }
    let deadline = Instant::now() + Duration::from_millis(750);
    loop {
        let changed = cast.recovery.changed.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        {
            let mut b = cast.recovery.buffer.lock().unwrap();
            if request.epoch != b.epoch {
                return (StatusCode::CONFLICT, "Audio session changed").into_response();
            }
            if b.chunks
                .front()
                .is_some_and(|first| sequence < first.sequence)
            {
                return (StatusCode::GONE, "Recovery window expired").into_response();
            }
            if sequence > b.next {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if let Some(id) = headers
                .get("x-amp-listener")
                .and_then(|v| v.to_str().ok())
                .filter(|s| {
                    s.len() <= 64 && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
                })
                && (b.listeners.len() < 256 || b.listeners.contains_key(id))
            {
                b.listeners.insert(id.to_string(), Instant::now());
            }
            if let Some(chunk) = b.chunks.iter().find(|c| c.sequence == sequence) {
                return (
                    [
                        ("content-type", "audio/mpeg".to_string()),
                        ("cache-control", "no-store".into()),
                        ("x-amp-epoch", b.epoch.clone()),
                        ("x-amp-sequence", sequence.to_string()),
                        ("x-amp-sha256", chunk.hash.clone()),
                    ],
                    chunk.bytes.clone(),
                )
                    .into_response();
            }
        }
        if Instant::now() >= deadline {
            return StatusCode::NO_CONTENT.into_response();
        }
        let _ =
            tokio::time::timeout(deadline.saturating_duration_since(Instant::now()), changed).await;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request as HttpRequest;
    use tower::ServiceExt;
    #[tokio::test]
    async fn replay_is_identical_across_connections_and_never_crosses_a_station_epoch() {
        let root = tempfile::tempdir().unwrap();
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        cast.station.lock().await.running = true;
        cast.recovery.push(&vec![7; CHUNK * 2]);
        let app = router(cast.clone());
        let bytes = axum::body::to_bytes(
            app.clone()
                .oneshot(
                    HttpRequest::get("/api/audio/manifest")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .into_body(),
            8192,
        )
        .await
        .unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let epoch = manifest["epoch"].as_str().unwrap();
        for _ in 0..2 {
            let reply = app
                .clone()
                .oneshot(
                    HttpRequest::get(format!("/api/audio/chunks/0?epoch={epoch}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(reply.status(), 200);
            assert_eq!(reply.headers()["x-amp-sequence"], "0");
            assert_eq!(
                axum::body::to_bytes(reply.into_body(), CHUNK)
                    .await
                    .unwrap()
                    .len(),
                CHUNK
            );
        }
        cast.recovery.reset();
        assert_eq!(
            app.oneshot(
                HttpRequest::get(format!("/api/audio/chunks/0?epoch={epoch}"))
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
            409
        );
    }
    #[test]
    fn replay_memory_is_bounded_and_old_chunks_expire() {
        let recovery = Recovery::default();
        recovery.push(&vec![1; LIMIT * 3]);
        let b = recovery.buffer.lock().unwrap();
        assert!(b.bytes <= LIMIT);
        assert!(b.chunks.front().unwrap().sequence > 0);
        assert_eq!(b.pending.len(), 0);
    }
}
