//! Browser podcast subscriptions share the desktop feed parser. Source media stays unchanged.
use super::*;
use axum::extract::Query;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    net::{IpAddr, ToSocketAddrs},
    path::Path,
};

pub fn routes() -> Router<Arc<Cast>> {
    Router::new()
        .route("/api/studio/podcasts", get(list).post(save))
        .route("/api/studio/podcasts/preview", post(preview))
        .route("/api/studio/podcasts/search", get(search))
}
#[derive(Default, Serialize, Deserialize)]
struct Subscriptions {
    feeds: Vec<String>,
}
#[derive(Deserialize)]
struct Save {
    revision: String,
    feeds: Vec<String>,
}
#[derive(Deserialize)]
struct Preview {
    url: String,
}
#[derive(Deserialize)]
struct Search {
    q: String,
}
fn file(cast: &Cast) -> Result<PathBuf, String> {
    cast.config_dir
        .lock()
        .unwrap()
        .as_ref()
        .map(|p| p.join("podcast-subscriptions.json"))
        .ok_or_else(|| "Server settings directory is unavailable".into())
}
fn read(file: &Path) -> Result<(Subscriptions, String), String> {
    let bytes = match std::fs::read(file) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => b"{\"feeds\":[]}".to_vec(),
        Err(e) => return Err(e.to_string()),
    };
    let value: Subscriptions = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let revision = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&value).map_err(|e| e.to_string())?)
    );
    Ok((value, revision))
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => {
            !a.is_private()
                && !a.is_loopback()
                && !a.is_link_local()
                && !a.is_multicast()
                && !a.is_unspecified()
                && !a.is_broadcast()
                && !a.is_documentation()
                && a.octets()[0] != 0
                && a.octets()[0] < 224
                && !(a.octets()[0] == 100 && (64..=127).contains(&a.octets()[1]))
                && !(a.octets()[0] == 198 && (18..=19).contains(&a.octets()[1]))
        }
        // Only globally routed unicast; reject mapped IPv4, ULA, local and reserved addresses.
        IpAddr::V6(a) => {
            (a.segments()[0] & 0xe000) == 0x2000
                && !(a.segments()[0] == 0x2001 && a.segments()[1] == 0x0db8)
        }
    }
}
/// curl's explicit DNS pin avoids URL redirects or DNS rebinding reaching the LAN.
fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let mut current = amp_core::podcast_feed::http_url(url)?;
    for _ in 0..6 {
        let host = current.host_str().ok_or("Missing publisher hostname")?;
        let port = current
            .port_or_known_default()
            .ok_or("Missing publisher port")?;
        if !matches!(port, 80 | 443) {
            return Err("Podcast publishers must use standard HTTP/HTTPS ports".into());
        }
        let ips: Vec<_> = (host, port)
            .to_socket_addrs()
            .map_err(|_| "Publisher name could not be resolved")?
            .map(|a| a.ip())
            .collect();
        if ips.is_empty() || ips.iter().any(|ip| !public_ip(*ip)) {
            return Err(
                "Use a public podcast publisher URL; local/private addresses are not allowed"
                    .into(),
            );
        }
        let ip = ips[0];
        let addr = match ip {
            IpAddr::V6(a) => format!("[{a}]"),
            _ => ip.to_string(),
        };
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let headers = dir.path().join("headers");
        let body = dir.path().join("body");
        let output = amp_core::command("curl")
            .args([
                "--silent",
                "--show-error",
                "--noproxy",
                "*",
                "--proto",
                "=http,https",
                "--connect-timeout",
                "10",
                "--max-time",
                "25",
                "--max-filesize",
                "8388608",
                "--resolve",
                &format!("{host}:{port}:{addr}"),
                "--dump-header",
            ])
            .arg(&headers)
            .arg("--output")
            .arg(&body)
            .arg("--write-out")
            .arg("%{http_code}")
            .arg(current.as_str())
            .output()
            .map_err(
                |_| "Podcast fetching requires curl. Install the server dependency package.",
            )?;
        if !output.status.success() {
            return Err("Publisher download failed or exceeded the 8 MB feed limit".into());
        }
        let status = String::from_utf8_lossy(&output.stdout);
        if status.starts_with('3') {
            let h = std::fs::read_to_string(headers).map_err(|e| e.to_string())?;
            let location = h
                .lines()
                .rev()
                .find_map(|l| {
                    l.split_once(':')
                        .filter(|(k, _)| k.eq_ignore_ascii_case("location"))
                        .map(|(_, v)| v.trim())
                })
                .ok_or("Publisher redirected without a destination")?;
            current = current
                .join(location)
                .map_err(|_| "Invalid publisher redirect")?;
            current = amp_core::podcast_feed::http_url(current.as_str())?;
            continue;
        }
        if !status.starts_with('2') {
            return Err(format!("Publisher returned HTTP {status}"));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(body)
            .map_err(|e| e.to_string())?
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("Podcast feed is too large".into());
        }
        return Ok(bytes);
    }
    Err("Publisher redirected too many times".into())
}
fn feed(url: &str) -> Result<amp_core::podcast_feed::Feed, String> {
    let bytes = fetch(url)?;
    amp_core::podcast_feed::parse_feed(
        url,
        std::str::from_utf8(&bytes).map_err(|_| "Feed is not UTF-8 XML")?,
    )
}
fn error(e: String) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({"error":e}))).into_response()
}
async fn preview(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Json(p): Json<Preview>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match tokio::task::spawn_blocking(move || feed(&p.url)).await {
        Ok(Ok(f)) => Json(json!(f)).into_response(),
        Ok(Err(e)) => error(e),
        Err(_) => error("Feed task failed".into()),
    }
}
async fn list(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let file = match file(&cast) {
        Ok(f) => f,
        Err(e) => return error(e),
    };
    match tokio::task::spawn_blocking(move||read(&file)).await {
        Ok(Ok((s,rev)))=>Json(json!({"revision":rev,"feeds":s.feeds,"auto_purge":false,"keep_days":30,"downloads_supported":false})).into_response(),
        Ok(Err(e))=>error(e),Err(_)=>error("Subscriptions unavailable".into())
    }
}
async fn save(State(cast): State<Arc<Cast>>, headers: HeaderMap, Json(p): Json<Save>) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if p.feeds.len() > 200 {
        return error("Keep at most 200 subscriptions".into());
    }
    let file = match file(&cast) {
        Ok(f) => f,
        Err(e) => return error(e),
    };
    let work = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let _lock = amp_core::coordination::Lock::acquire(&file)?;
        let (_, revision) = read(&file)?;
        if revision != p.revision {
            return Err("Subscriptions changed elsewhere; refresh before saving".into());
        }
        let mut feeds = Vec::new();
        for f in p.feeds {
            let url = amp_core::podcast_feed::http_url(&f)?.to_string();
            if !feeds.contains(&url) {
                feeds.push(url)
            }
        }
        let s = Subscriptions { feeds };
        amp_core::coordination::atomic_write(&file, &s)?;
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&s).map_err(|e| e.to_string())?)
        ))
    })
    .await;
    match work {
        Ok(Ok(rev)) => Json(json!({"revision":rev})).into_response(),
        Ok(Err(e)) if e.starts_with("Subscriptions changed") => {
            (StatusCode::CONFLICT, Json(json!({"error":e}))).into_response()
        }
        Ok(Err(e)) => error(e),
        Err(_) => error("Could not save subscriptions".into()),
    }
}
async fn search(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Query(p): Query<Search>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if p.q.trim().is_empty() || p.q.len() > 200 {
        return error("Enter a podcast title or topic (up to 200 characters)".into());
    }
    let mut url = amp_core::podcast_feed::http_url("https://itunes.apple.com/search").unwrap();
    url.query_pairs_mut()
        .append_pair("media", "podcast")
        .append_pair("limit", "30")
        .append_pair("term", p.q.trim());
    match tokio::task::spawn_blocking(move || {
        fetch(url.as_str()).and_then(|b| {
            serde_json::from_slice::<serde_json::Value>(&b).map_err(|e| e.to_string())
        })
    })
    .await
    {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => error(e),
        Err(_) => error("Podcast search unavailable".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocks_local_and_reserved() {
        for s in [
            "127.0.0.1",
            "10.0.0.2",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "::ffff:127.0.0.1",
            "fc00::1",
            "2001:db8::1",
        ] {
            assert!(!public_ip(s.parse().unwrap()), "{s}")
        }
        assert!(public_ip("1.1.1.1".parse().unwrap()));
    }
}
