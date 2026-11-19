//! Authenticated browser studio: bounded library pages, portable edits and isolated renders.
use super::*;
use axum::extract::{Path, Query};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Mutex as StdMutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

#[derive(Default)]
struct Store {
    tickets: HashMap<String, (String, Instant)>,
    jobs: HashMap<String, Value>,
    waves: HashMap<String, Value>,
    edits: HashMap<String, amp_core::Preparation>,
}
fn store() -> &'static StdMutex<Store> {
    static S: OnceLock<StdMutex<Store>> = OnceLock::new();
    S.get_or_init(Default::default)
}
fn err(status: StatusCode, message: impl ToString) -> Response {
    (status, Json(json!({"error":message.to_string()}))).into_response()
}
fn revision(bytes: Option<&[u8]>) -> String {
    bytes.map_or_else(|| "none".into(), |b| format!("{:x}", Sha256::digest(b)))
}
fn path(cast: &Cast, id: usize) -> Result<PathBuf, String> {
    let t = cast
        .tracks
        .get(id)
        .ok_or_else(|| "Track unavailable".to_string())?;
    amp_core::resolve(&cast.root, &t.path).map_err(|e| e.to_string())
}
#[derive(Default, Deserialize)]
struct Params {
    q: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    ticket: Option<String>,
}
fn media_allowed(cast: &Cast, headers: &HeaderMap, query: &Params) -> bool {
    // Apply the same browser-origin checks even when a media ticket replaces the password.
    let mut h = headers.clone();
    if let Some(p) = &cast.password
        && let Ok(v) = p.parse()
    {
        h.insert("x-amp-password", v);
    }
    if !cast.authorized(&h) {
        return false;
    }
    cast.authorized(headers)
        || query.ticket.as_ref().is_some_and(|t| {
            store()
                .lock()
                .unwrap()
                .tickets
                .get(t)
                .is_some_and(|(server, until)| server == &cast.server_id && *until > Instant::now())
        })
}
pub fn routes() -> Router<Arc<Cast>> {
    Router::new()
        .route("/api/studio/bootstrap", get(bootstrap))
        .route("/api/studio/session", post(session))
        .route("/api/studio/tracks", get(tracks))
        .route("/api/studio/tracks/{id}/audio", get(audio))
        .route(
            "/api/studio/tracks/{id}/preparation",
            get(preparation).post(save_preparation),
        )
        .route("/api/studio/tracks/{id}/waveform", get(waveform))
        .route("/api/studio/tracks/{id}/export", post(export))
        .route("/api/studio/exports/{id}", get(export_status))
        .route("/api/studio/exports/{id}/audio", get(export_audio))
        .route("/api/studio/playlists", get(playlists).post(save_playlist))
}
async fn session(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let ticket = amp_core::coordination::token();
    let mut s = store().lock().unwrap();
    s.tickets.retain(|_, (_, e)| *e > Instant::now());
    if s.tickets.len() >= 128 {
        return err(StatusCode::TOO_MANY_REQUESTS, "Too many media sessions");
    }
    s.tickets.insert(
        ticket.clone(),
        (
            cast.server_id.clone(),
            Instant::now() + Duration::from_secs(300),
        ),
    );
    Json(json!({"ticket":ticket,"expires_in":300})).into_response()
}
async fn bootstrap(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({"tracks":cast.tracks.len(),"capabilities":{"library":true,"preparation":true,"waveform":true,"lab_export":true,"playlists":true,"smart_playlists":true,"dj":false,"stems":false,"arrangement":false},"modules":[{"id":"library","available":true},{"id":"preparation","available":true},{"id":"lab","available":true},{"id":"dj","available":false},{"id":"stems","available":false},{"id":"arrangement","available":false}]})).into_response()
}
async fn tracks(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Query(p): Query<Params>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let q = p.q.unwrap_or_default().to_lowercase();
    let offset = p.offset.unwrap_or(0);
    let limit = p.limit.unwrap_or(100).clamp(1, 200);
    let matches: Vec<_> = cast
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            q.is_empty()
                || t.title.to_lowercase().contains(&q)
                || t.path.to_string_lossy().to_lowercase().contains(&q)
                || ["artist", "album"].iter().any(|field| {
                    t.preparation
                        .extensions
                        .get(*field)
                        .and_then(Value::as_str)
                        .is_some_and(|value| value.to_lowercase().contains(&q))
                })
        })
        .collect();
    let saved = store().lock().unwrap();
    let rows:Vec<_>=matches.iter().skip(offset).take(limit).map(|(id,t)|{let p=saved.edits.get(&format!("{}:{id}",cast.server_id)).unwrap_or(&t.preparation);json!({"id":id,"title":t.title,"path":t.path,"bpm":p.bpm,"key":p.key,"duration":p.analysis.as_ref().map(|a|a.duration),"cue_in":p.cue_in,"cue_out":p.cue_out,"artist":p.extensions.get("artist"),"album":p.extensions.get("album")})}).collect();
    Json(json!({"tracks":rows,"total":matches.len(),"offset":offset,"limit":limit})).into_response()
}
async fn preparation(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Path(id): Path<usize>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let file = match path(&cast, id) {
        Ok(p) => p,
        Err(e) => return err(StatusCode::NOT_FOUND, e),
    };
    match tokio::task::spawn_blocking(move || amp_core::preparation::load_with_revision(&file))
        .await
    {
        Ok(Ok((p, r))) => {
            Json(json!({"preparation":p,"revision":revision(r.as_deref())})).into_response()
        }
        _ => err(StatusCode::SERVICE_UNAVAILABLE, "Cannot read preparation"),
    }
}
#[derive(Deserialize)]
struct Edit {
    preparation: amp_core::Preparation,
    revision: String,
}
async fn save_preparation(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Path(id): Path<usize>,
    Json(mut edit): Json<Edit>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let file = match path(&cast, id) {
        Ok(p) => p,
        Err(e) => return err(StatusCode::NOT_FOUND, e),
    };
    let result = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let (_, current) = amp_core::preparation::load_with_revision(&file)?;
        if revision(current.as_deref()) != edit.revision {
            return Err("Preparation changed; reload before saving".into());
        }
        edit.preparation.manual_override = true;
        edit.preparation.validate()?;
        amp_core::preparation::save_if_unchanged(
            &cast.root,
            &cast.tracks[id].path,
            &edit.preparation,
            current.as_deref(),
        )?;
        let (p, r) = amp_core::preparation::load_with_revision(&file)?;
        store()
            .lock()
            .unwrap()
            .edits
            .insert(format!("{}:{id}", cast.server_id), p.clone());
        Ok(json!({"preparation":p,"revision":revision(r.as_deref())}))
    })
    .await;
    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err(StatusCode::CONFLICT, e),
        Err(e) => err(StatusCode::SERVICE_UNAVAILABLE, e),
    }
}
fn byte_range(value: Option<&str>, len: u64) -> Result<(u64, u64, bool), ()> {
    if len == 0 {
        return Ok((0, 0, false));
    }
    let Some(v) = value else {
        return Ok((0, len - 1, false));
    };
    let v = v.strip_prefix("bytes=").ok_or(())?;
    if v.contains(',') {
        return Err(());
    }
    let (a, b) = v.split_once('-').ok_or(())?;
    let (start, end) = if a.is_empty() {
        let n = b.parse::<u64>().map_err(|_| ())?;
        if n == 0 {
            return Err(());
        }
        (len.saturating_sub(n), len - 1)
    } else {
        (
            a.parse::<u64>().map_err(|_| ())?,
            if b.is_empty() {
                len - 1
            } else {
                b.parse::<u64>().map_err(|_| ())?.min(len - 1)
            },
        )
    };
    if start >= len || end < start {
        return Err(());
    }
    Ok((start, end, true))
}
async fn file_response(file: PathBuf, headers: HeaderMap, download: bool) -> Response {
    let mime = match file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "m4a" | "mp4" => "audio/mp4",
        "ogg" | "opus" => "audio/ogg",
        _ => "application/octet-stream",
    };
    let mut file = match tokio::fs::File::open(&file).await {
        Ok(f) => f,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let len = match file.metadata().await {
        Ok(m) => m.len(),
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let (start, end, partial) =
        match byte_range(headers.get("range").and_then(|h| h.to_str().ok()), len) {
            Ok(r) => r,
            Err(_) => {
                return (
                    StatusCode::RANGE_NOT_SATISFIABLE,
                    [("content-range", format!("bytes */{len}"))],
                )
                    .into_response();
            }
        };
    if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let count = if len == 0 { 0 } else { end - start + 1 };
    let stream = async_stream::stream! {let mut left=count;let mut buf=vec![0u8;65536];while left>0{let n=match file.read(&mut buf[..left.min(65536) as usize]).await {Ok(n)=>n,Err(e)=>{yield Err::<Bytes,std::io::Error>(e);break}};if n==0{break}left-=n as u64;yield Ok::<Bytes,std::io::Error>(Bytes::copy_from_slice(&buf[..n]));}};
    let body = Body::from_stream(stream);
    let mut response = Response::new(body);
    *response.status_mut() = if partial {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    let h = response.headers_mut();
    h.insert("content-type", mime.parse().unwrap());
    h.insert("accept-ranges", "bytes".parse().unwrap());
    h.insert("content-length", count.to_string().parse().unwrap());
    h.insert("cache-control", "private, no-store".parse().unwrap());
    if partial {
        h.insert(
            "content-range",
            format!("bytes {start}-{end}/{len}").parse().unwrap(),
        );
    }
    if download {
        h.insert(
            "content-disposition",
            "attachment; filename=amp-verse-export.mp3".parse().unwrap(),
        );
        h.insert("content-type", "audio/mpeg".parse().unwrap());
    }
    response
}
async fn audio(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Path(id): Path<usize>,
    Query(p): Query<Params>,
) -> Response {
    if !media_allowed(&cast, &headers, &p) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match path(&cast, id) {
        Ok(file) => file_response(file, headers, false).await,
        Err(e) => err(StatusCode::NOT_FOUND, e),
    }
}
async fn waveform(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Path(id): Path<usize>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let file = match path(&cast, id) {
        Ok(p) => p,
        Err(e) => return err(StatusCode::NOT_FOUND, e),
    };
    let key = format!("{}:{id}", cast.server_id);
    {
        let mut s = store().lock().unwrap();
        if let Some(v) = s.waves.get(&key) {
            return (
                if v["state"] == "pending" {
                    StatusCode::ACCEPTED
                } else {
                    StatusCode::OK
                },
                Json(v.clone()),
            )
                .into_response();
        }
        if s.waves.values().filter(|v| v["state"] == "pending").count() >= 2 {
            return err(StatusCode::TOO_MANY_REQUESTS, "Waveform workers busy");
        }
        if s.waves.len() >= 64
            && let Some(k) = s
                .waves
                .iter()
                .find(|(_, v)| v["state"] != "pending")
                .map(|(k, _)| k.clone())
        {
            s.waves.remove(&k);
        }
        s.waves.insert(key.clone(), json!({"state":"pending"}));
    }
    tokio::task::spawn_blocking(move || {
        let result = (|| -> Result<Value, String> {
            let pcm = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
            // Rectify at the source rate before decimation: sampling raw PCM at 100 Hz
            // would low-pass away ordinary musical tones instead of measuring their envelope.
            let output = run_ffmpeg(
                amp_core::command("ffmpeg")
                    .args([
                        "-nostdin",
                        "-v",
                        "error",
                        "-y",
                        "-protocol_whitelist",
                        "file,pipe",
                        "-i",
                    ])
                    .arg(file)
                    .args([
                        "-t",
                        "14400",
                        "-vn",
                        "-af",
                        "aformat=channel_layouts=mono,aeval=abs(val(0))",
                        "-ac",
                        "1",
                        "-ar",
                        "100",
                        "-f",
                        "f32le",
                    ])
                    .arg(pcm.path()),
            )?;
            if !output.success() {
                return Err("FFmpeg could not decode audio".into());
            }
            let bytes = std::fs::read(pcm.path()).map_err(|e| e.to_string())?;
            let samples: Vec<_> = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b).abs())
                .collect();
            let step = samples.len().div_ceil(1200).max(1);
            let peaks: Vec<_> = samples
                .chunks(step)
                .map(|s| {
                    s.iter()
                        .copied()
                        .filter(|v| v.is_finite())
                        .fold(0f32, f32::max)
                        .min(1.)
                })
                .collect();
            Ok(
                json!({"state":"ready","peaks":peaks,"duration":samples.len() as f64/100.,"truncated":samples.len()>=1440000}),
            )
        })();
        store().lock().unwrap().waves.insert(
            key,
            result.unwrap_or_else(|e| json!({"state":"error","error":e})),
        );
    });
    (StatusCode::ACCEPTED, Json(json!({"state":"pending"}))).into_response()
}
#[derive(Deserialize)]
struct Export {
    start: f64,
    end: f64,
    #[serde(default)]
    fade_in: f64,
    #[serde(default)]
    fade_out: f64,
    #[serde(default)]
    gain_db: f64,
    #[serde(default)]
    normalize: bool,
}
impl Export {
    fn valid(&self) -> bool {
        [
            self.start,
            self.end,
            self.fade_in,
            self.fade_out,
            self.gain_db,
        ]
        .iter()
        .all(|n| n.is_finite())
            && self.start >= 0.
            && self.end > self.start
            && self.end - self.start <= 14400.
            && self.fade_in >= 0.
            && self.fade_out >= 0.
            && self.fade_in + self.fade_out <= self.end - self.start
            && (-60. ..=24.).contains(&self.gain_db)
    }
}
async fn export(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Path(track): Path<usize>,
    Json(e): Json<Export>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !e.valid() {
        return err(
            StatusCode::BAD_REQUEST,
            "Choose a valid trim, fades and gain",
        );
    }
    let source = match path(&cast, track) {
        Ok(p) => p,
        Err(e) => return err(StatusCode::NOT_FOUND, e),
    };
    let Some(config) = cast.config_dir.lock().unwrap().clone() else {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            "Persistent export folder unavailable",
        );
    };
    let id = amp_core::coordination::token();
    let key = format!("{}:{id}", cast.server_id);
    {
        let mut s = store().lock().unwrap();
        if s.jobs.values().filter(|v| v["state"] == "pending").count() >= 2 || s.jobs.len() >= 100 {
            return err(StatusCode::TOO_MANY_REQUESTS, "Export queue full");
        }
        s.jobs
            .insert(key.clone(), json!({"id":id,"state":"pending"}));
    }
    let output = config.join("downloads/exports").join(format!("{id}.mp3"));
    let job_id = id.clone();
    tokio::task::spawn_blocking(move || {
        let result = (|| -> Result<(), String> {
            std::fs::create_dir_all(output.parent().unwrap()).map_err(|e| e.to_string())?;
            let length = e.end - e.start;
            let mut filters = vec![format!("volume={}dB", e.gain_db)];
            if e.fade_in > 0. {
                filters.push(format!("afade=t=in:d={}", e.fade_in));
            }
            if e.fade_out > 0. {
                filters.push(format!(
                    "afade=t=out:st={}:d={}",
                    length - e.fade_out,
                    e.fade_out
                ));
            }
            if e.normalize {
                filters.push("loudnorm=I=-16:TP=-1.5:LRA=11".into());
            }
            let result = run_ffmpeg(
                amp_core::command("ffmpeg")
                    .args([
                        "-nostdin",
                        "-v",
                        "error",
                        "-n",
                        "-ss",
                        &e.start.to_string(),
                        "-protocol_whitelist",
                        "file,pipe",
                        "-i",
                    ])
                    .arg(source)
                    .args([
                        "-t",
                        &length.to_string(),
                        "-vn",
                        "-af",
                        &filters.join(","),
                        "-codec:a",
                        "libmp3lame",
                        "-b:a",
                        "256k",
                    ])
                    .arg(&output),
            )?;
            if !result.success() {
                let _ = std::fs::remove_file(&output);
                return Err("FFmpeg export failed".into());
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&output);
        }
        store().lock().unwrap().jobs.insert(key,match result{Ok(())=>json!({"id":job_id,"state":"ready","url":format!("/api/studio/exports/{job_id}/audio")}),Err(e)=>json!({"id":job_id,"state":"error","error":e})});
    });
    (
        StatusCode::ACCEPTED,
        Json(json!({"id":id,"state":"pending"})),
    )
        .into_response()
}
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
async fn export_status(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match store()
        .lock()
        .unwrap()
        .jobs
        .get(&format!("{}:{id}", cast.server_id))
    {
        Some(v) => Json(v.clone()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn export_audio(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(p): Query<Params>,
) -> Response {
    if !media_allowed(&cast, &headers, &p) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !safe_id(&id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let config = cast.config_dir.lock().unwrap().clone();
    match config {
        Some(c) => {
            file_response(
                c.join("downloads/exports").join(format!("{id}.mp3")),
                headers,
                true,
            )
            .await
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn playlists(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let mut lists = sharing::lists(&cast)?;
        for list in &mut lists {
            if let Some(id) = list["id"]
                .as_str()
                .and_then(|id| id.strip_prefix("normal_"))
                && safe_id(id)
            {
                let p = cast.root.join("ampdj/playlists").join(format!("{id}.json"));
                if let Ok(p) = amp_core::Playlist::load(&p, &cast.root) {
                    list["revision"] = serde_json::to_value(p.sync).map_err(|e| e.to_string())?;
                }
            }
        }
        let settings = amp_core::show::Settings::load(&cast.root)?;
        Ok(json!({"playlists":lists,"smart_revision":settings}))
    })
    .await;
    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        _ => err(StatusCode::SERVICE_UNAVAILABLE, "Cannot read playlists"),
    }
}
#[derive(Deserialize)]
struct PlaylistEdit {
    id: Option<String>,
    name: String,
    #[serde(default)]
    tracks: Vec<usize>,
    rules: Option<amp_core::show::SmartPlaylist>,
    revision: Option<Value>,
}
async fn save_playlist(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Json(e): Json<PlaylistEdit>,
) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if e.name.trim().is_empty() || e.name.len() > 200 || e.tracks.len() > 20000 {
        return err(StatusCode::BAD_REQUEST, "Invalid playlist");
    }
    let result = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        if let Some(mut rules) = e.rules {
            let mut settings = amp_core::show::Settings::load(&cast.root)?;
            let current = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
            if e.revision.as_ref() != Some(&current) {
                return Err("Smart playlists changed; reload settings revision".into());
            }
            rules.name = e.name;
            let id = e.id.unwrap_or_else(amp_core::coordination::token);
            let id = id.strip_prefix("smart_").unwrap_or(&id);
            if !safe_id(id) {
                return Err("Invalid playlist ID".into());
            }
            rules.id = id.into();
            if let Some(p) = settings.playlists.iter_mut().find(|p| p.id == id) {
                *p = rules
            } else {
                settings.playlists.push(rules)
            }
            settings.validate()?;
            settings.save(&cast.root)?;
            Ok(json!({"id":format!("smart_{id}")}))
        } else {
            let id =
                e.id.unwrap_or_else(|| format!("normal_{}", amp_core::coordination::token()));
            let stem = id.strip_prefix("normal_").ok_or("Invalid playlist ID")?;
            if !safe_id(stem) {
                return Err("Invalid playlist ID".into());
            }
            let path = cast
                .root
                .join("ampdj/playlists")
                .join(format!("{stem}.json"));
            std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
            if !path
                .parent()
                .unwrap()
                .canonicalize()
                .map_err(|e| e.to_string())?
                .starts_with(&cast.root)
            {
                return Err("Playlist folder leaves library".into());
            }
            if path.exists()
                && !path
                    .canonicalize()
                    .map_err(|e| e.to_string())?
                    .starts_with(&cast.root)
            {
                return Err("Playlist leaves library".into());
            }
            let existing = amp_core::Playlist::load(&path, &cast.root).ok();
            let current = existing
                .as_ref()
                .and_then(|p| p.sync.as_ref())
                .map(|r| serde_json::to_value(r).unwrap());
            if current != e.revision {
                return Err("Playlist changed; reload before saving".into());
            }
            let list = amp_core::Playlist {
                version: 1,
                name: e.name,
                tracks: e
                    .tracks
                    .iter()
                    .map(|i| {
                        cast.tracks
                            .get(*i)
                            .map(|t| t.path.clone())
                            .ok_or("Track unavailable")
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                sync: existing.and_then(|p| p.sync),
                companion_available: false,
            };
            list.save_in_library(&path, &cast.root)
                .map_err(|e| e.to_string())?;
            Ok(json!({"id":id}))
        }
    })
    .await;
    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err(StatusCode::CONFLICT, e),
        Err(e) => err(StatusCode::SERVICE_UNAVAILABLE, e),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges() {
        assert_eq!(byte_range(Some("bytes=2-5"), 10), Ok((2, 5, true)));
        assert_eq!(byte_range(Some("bytes=-3"), 10), Ok((7, 9, true)));
        assert!(byte_range(Some("bytes=10-"), 10).is_err());
        assert!(byte_range(Some("bytes=0-1,3-4"), 10).is_err());
    }
    #[test]
    fn identifiers() {
        assert!(!safe_id("../original"));
        assert!(!safe_id("C:\\music"));
        assert!(safe_id("normal_abc-123"));
    }
    #[test]
    fn exports_validate() {
        let e = Export {
            start: 1.,
            end: 2.,
            fade_in: 2.,
            fade_out: 0.,
            gain_db: 0.,
            normalize: false,
        };
        assert!(!e.valid());
    }
}

#[cfg(test)]
mod api_tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;
    fn cast() -> Arc<Cast> {
        Cast::new(
            std::env::temp_dir(),
            amp_core::Library {
                tracks: vec![],
                tree: amp_core::Folder {
                    name: "test".into(),
                    folders: vec![],
                    tracks: vec![],
                },
            },
            Some("secret".into()),
        )
    }
    fn fixture() -> (tempfile::TempDir, Arc<Cast>, Router) {
        let root = tempfile::tempdir().unwrap();
        let tracks = (0..2)
            .map(|i| {
                let path = PathBuf::from(format!("track-{i}.mp3"));
                std::fs::write(root.path().join(&path), b"0123456789abcdef").unwrap();
                amp_core::Track {
                    path,
                    title: format!("Track {i}"),
                    preparation: Default::default(),
                }
            })
            .collect();
        let cast = Cast::new(
            root.path().into(),
            amp_core::Library {
                tracks,
                tree: amp_core::Folder {
                    name: "test".into(),
                    folders: vec![],
                    tracks: vec![0, 1],
                },
            },
            Some("secret".into()),
        );
        let app = routes().with_state(cast.clone());
        (root, cast, app)
    }
    async fn value(response: Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
    async fn read(app: &Router, url: &str) -> Response {
        app.clone()
            .oneshot(
                Request::get(url)
                    .header("x-amp-password", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }
    async fn write(app: &Router, url: &str, body: Value) -> Response {
        app.clone()
            .oneshot(
                Request::post(url)
                    .header("x-amp-password", "secret")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }
    async fn ticket(app: &Router) -> String {
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/studio/session")
                    .header("x-amp-password", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        value(response).await["ticket"].as_str().unwrap().to_owned()
    }
    #[tokio::test]
    async fn waveform_preserves_440_hz_audio_energy() {
        let (root, _cast, app) = fixture();
        let status = run_ffmpeg(
            amp_core::command("ffmpeg")
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=1",
                    "-codec:a",
                    "libmp3lame",
                ])
                .arg(root.path().join("track-0.mp3")),
        )
        .expect("FFmpeg required for waveform regression test");
        assert!(status.success());
        let url = "/api/studio/tracks/0/waveform";
        let response = read(&app, url).await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let response = read(&app, url).await;
            let waveform = value(response).await;
            if waveform["state"] == "pending" {
                continue;
            }
            assert_eq!(waveform["state"], "ready", "{waveform}");
            let peaks = waveform["peaks"].as_array().unwrap();
            assert!(!peaks.is_empty());
            assert!(
                peaks.iter().any(|p| p.as_f64().unwrap() > 0.05),
                "440 Hz energy was removed by waveform decimation: {waveform}"
            );
            assert!(waveform["duration"].as_f64().unwrap() > 0.9);
            return;
        }
        panic!("Waveform worker did not finish within ten seconds");
    }
    #[tokio::test]
    async fn media_ticket_ranges_and_origin_are_enforced() {
        let (_root, _cast, app) = fixture();
        let response = app
            .clone()
            .oneshot(
                Request::get("/api/studio/tracks/0/audio")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let ticket = ticket(&app).await;
        let url = format!("/api/studio/tracks/0/audio?ticket={ticket}");
        let response = app
            .clone()
            .oneshot(
                Request::get(&url)
                    .header("range", "bytes=2-5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()["content-range"], "bytes 2-5/16");
        assert_eq!(response.headers()["content-length"], "4");
        assert_eq!(
            &axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap()[..],
            b"2345"
        );
        let response = app
            .clone()
            .oneshot(
                Request::get(&url)
                    .header("sec-fetch-site", "cross-site")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .clone()
            .oneshot(
                Request::get(&url)
                    .header("host", "studio.local")
                    .header("origin", "http://attacker.local")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let missing = format!("/api/studio/tracks/99/audio?ticket={ticket}");
        let response = app
            .clone()
            .oneshot(Request::get(missing).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let response = app
            .oneshot(
                Request::get(url)
                    .header("range", "bytes=99-")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    }
    #[tokio::test]
    async fn stale_preparation_cannot_replace_saved_manual_fields() {
        let (root, _cast, app) = fixture();
        let original = value(read(&app, "/api/studio/tracks/0/preparation").await).await;
        let mut edited = original["preparation"].clone();
        edited["cue_in"] = json!(1.25);
        edited["bpm"] = json!(123.);
        edited["manual_cues"] =
            json!([{"id":"manual-one","label":"Intro","seconds":2.5,"kind":"hotcue","number":1}]);
        let response = write(
            &app,
            "/api/studio/tracks/0/preparation",
            json!({"revision":original["revision"],"preparation":edited}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let saved = value(response).await;
        assert_ne!(saved["revision"], original["revision"]);
        let mut stale = original["preparation"].clone();
        stale["cue_in"] = json!(9.);
        let response = write(
            &app,
            "/api/studio/tracks/0/preparation",
            json!({"revision":original["revision"],"preparation":stale}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let current = value(read(&app, "/api/studio/tracks/0/preparation").await).await;
        assert_eq!(current, saved);
        let persisted = amp_core::preparation::load(&root.path().join("track-0.mp3")).unwrap();
        assert!(persisted.manual_override);
        assert_eq!(persisted.cue_in, 1.25);
        assert_eq!(persisted.manual_cues[0].label, "Intro");
        let summary = value(read(&app, "/api/studio/tracks?limit=1").await).await;
        assert_eq!(summary["tracks"][0]["bpm"], 123.);
        assert_eq!(summary["tracks"][0]["cue_in"], 1.25);
    }
    #[tokio::test]
    async fn playlist_conflicts_and_traversal_preserve_existing_data() {
        let (root, _cast, app) = fixture();
        let url = "/api/studio/playlists";
        let response = write(
            &app,
            url,
            json!({"id":"normal_test","name":"Original","tracks":[0]}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let file = root.path().join("ampdj/playlists/test.json");
        let original = amp_core::Playlist::load(&file, root.path()).unwrap();
        let revision = serde_json::to_value(&original.sync).unwrap();
        let response = write(
            &app,
            url,
            json!({"id":"normal_test","name":"Updated","tracks":[1],"revision":revision}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let response = write(
            &app,
            url,
            json!({"id":"normal_test","name":"Stale overwrite","tracks":[0],"revision":revision}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let current = amp_core::Playlist::load(&file, root.path()).unwrap();
        assert_eq!(current.name, "Updated");
        assert_eq!(current.tracks, vec![PathBuf::from("track-1.mp3")]);
        let response = write(
            &app,
            url,
            json!({"id":"normal_../escape","name":"Escape","tracks":[0]}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert!(!root.path().join("ampdj/escape.json").exists());
        let response = write(
            &app,
            url,
            json!({"id":"normal_missing","name":"Missing track","tracks":[99]}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert!(!root.path().join("ampdj/playlists/missing.json").exists());
        assert_eq!(
            std::fs::read(root.path().join("track-0.mp3")).unwrap(),
            b"0123456789abcdef"
        );
    }
    #[tokio::test]
    async fn management_requires_password() {
        let response = routes()
            .with_state(cast())
            .oneshot(
                Request::get("/api/studio/tracks")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    #[tokio::test]
    async fn empty_page_and_session() {
        let app = routes().with_state(cast());
        let response = app
            .clone()
            .oneshot(
                Request::get("/api/studio/tracks?limit=99999")
                    .header("x-amp-password", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["limit"], 200);
        assert_eq!(value["tracks"], json!([]));
        let response = app
            .oneshot(
                Request::post("/api/studio/session")
                    .header("x-amp-password", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    #[test]
    fn ticket_is_server_bound_and_expires() {
        let cast = cast();
        let ticket = amp_core::coordination::token();
        store().lock().unwrap().tickets.insert(
            ticket.clone(),
            (
                "different-server".into(),
                Instant::now() + Duration::from_secs(60),
            ),
        );
        assert!(!media_allowed(
            &cast,
            &HeaderMap::new(),
            &Params {
                ticket: Some(ticket),
                ..Default::default()
            }
        ));
    }
}

// Workers have a wall-clock deadline as well as a source-duration cap. Never pipe
// unbounded decoder diagnostics into memory or leave a failed child running.
fn run_ffmpeg(command: &mut std::process::Command) -> Result<std::process::ExitStatus, String> {
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Audio task exceeded five-minute limit".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
