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
        .route("/api/studio/session/logout", post(logout))
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
    let mut credential_headers = headers.clone();
    credential_headers.remove("cookie");
    if !cast.authorized(&credential_headers) {
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
    let cookie = match browser_session::login(&cast, &headers) {
        Ok(cookie) => cookie,
        Err(status) => {
            return err(
                status,
                if status == StatusCode::TOO_MANY_REQUESTS {
                    "Too many browser sessions. Try again later."
                } else {
                    "Cannot save browser sign-in. Check the server account can write its private configuration folder, then retry."
                },
            );
        }
    };
    let mut response =
        Json(json!({"ticket":ticket,"expires_in":300,"signed_in":true})).into_response();
    response
        .headers_mut()
        .insert("set-cookie", cookie.parse().unwrap());
    response
        .headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    response
}
async fn logout(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let cookie = match browser_session::logout(&cast, &headers) {
        Ok(cookie) => cookie,
        Err(status) => {
            return err(
                status,
                "Could not revoke this browser session. Check the server configuration folder and retry signing out.",
            );
        }
    };
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert("set-cookie", cookie.parse().unwrap());
    response
}
async fn bootstrap(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({"password_required":cast.password.is_some(),"browser_session_persistent":cast.config_dir.lock().unwrap().is_some(),"library_id":cast.library_id,"server_name":cast.station_name,"smart_playlist_template":amp_core::show::Settings::default().playlists.into_iter().next(),"tracks":cast.tracks.len(),"capabilities":{"library":true,"preparation":true,"waveform":true,"lab_export":true,"playlists":true,"smart_playlists":true,"google_cast":true,"google_cast_server_owned":true,"dj":false,"stems":false,"arrangement":false},"modules":[{"id":"library","available":true},{"id":"preparation","available":true},{"id":"lab","available":true},{"id":"dj","available":false},{"id":"stems","available":false},{"id":"arrangement","available":false}]})).into_response()
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
    let rows:Vec<_>=matches.iter().skip(offset).take(limit).map(|(id,t)|{let p=saved.edits.get(&format!("{}:{id}",cast.server_id)).unwrap_or(&t.preparation);json!({"id":id,"title":t.title,"path":player_control::portable_path(&t.path),"bpm":p.bpm,"key":p.key,"duration":p.analysis.as_ref().map(|a|a.duration),"cue_in":p.cue_in,"cue_out":p.cue_out,"artist":p.extensions.get("artist"),"album":p.extensions.get("album")})}).collect();
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
pub(super) enum Attachment {
    Inline,
    Original,
    Export,
}
pub(super) async fn file_response(
    file: PathBuf,
    headers: HeaderMap,
    attachment: Attachment,
) -> Response {
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
    let extension = file
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| !e.is_empty() && e.len() <= 10 && e.bytes().all(|b| b.is_ascii_alphanumeric()))
        .unwrap_or("bin")
        .to_ascii_lowercase();
    let mut file = match tokio::fs::File::open(&file).await {
        Ok(f) => f,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let len = match file.metadata().await {
        Ok(m) => m.len(),
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    if len == 0 {
        return err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "This audio file is empty. Choose another track, then repair or re-import the source in Library.",
        );
    }
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
    match attachment {
        Attachment::Inline => {}
        Attachment::Original => {
            h.insert(
                "content-disposition",
                format!("attachment; filename=amp-verse-original.{extension}")
                    .parse()
                    .unwrap(),
            );
        }
        Attachment::Export => {
            h.insert(
                "content-disposition",
                "attachment; filename=amp-verse-export.mp3".parse().unwrap(),
            );
            h.insert("content-type", "audio/mpeg".parse().unwrap());
        }
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
        Ok(file) => file_response(file, headers, Attachment::Inline).await,
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
                Attachment::Export,
            )
            .await
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
struct PlaylistReadFailure {
    stage: &'static str,
    kind: &'static str,
    message: String,
    detail: String,
}
impl PlaylistReadFailure {
    fn new(stage: &'static str, kind: &'static str, message: &str, detail: impl ToString) -> Self {
        Self {
            stage,
            kind,
            message: message.into(),
            detail: detail.to_string(),
        }
    }
    fn parse(stage: &'static str, error: serde_json::Error) -> Self {
        // Schema errors can quote metadata values. Keep the location/category, not those values.
        Self::new(
            stage,
            "parse",
            "Cannot read Smart playlist settings",
            format!(
                "JSON {:?} at line {} column {}",
                error.classify(),
                error.line(),
                error.column()
            ),
        )
    }
    fn catalog(stage: &'static str, detail: String) -> Self {
        let kind = if detail == "Shared record busy; retry after reloading" {
            "busy"
        } else {
            "storage"
        };
        Self::new(stage, kind, &detail, &detail)
    }
    fn response(self) -> Response {
        let code = if self.kind == "busy" {
            "catalog_busy"
        } else {
            "playlist_read_failed"
        };
        // Local diagnostics only: no request headers, metadata documents, or raw paths in JSON.
        let detail: String = self
            .detail
            .chars()
            .take(512)
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        eprintln!(
            "AMP Studio playlist read failed: stage={} kind={} detail={detail}",
            self.stage, self.kind
        );
        let mut response = (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"Cannot read playlists", "code":code, "stage":self.stage, "kind":self.kind}))).into_response();
        if self.kind == "busy" {
            response
                .headers_mut()
                .insert("retry-after", axum::http::HeaderValue::from_static("1"));
        }
        response
    }
}
fn raw_smart_settings(cast: &Cast) -> Result<Value, String> {
    read_smart_settings(cast).map_err(|error| error.message)
}
fn read_smart_settings(cast: &Cast) -> Result<Value, PlaylistReadFailure> {
    let path = cast.root.join("ampdj/smart-playlists.json");
    let bytes = if let Some(bytes) = amp_core::catalog_db::read(&path)
        .map_err(|error| PlaylistReadFailure::catalog("smart_settings_catalog_read", error))?
    {
        Some(bytes)
    } else {
        match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match std::fs::read(cast.root.join(".ampverse/show.json")) {
                    Ok(bytes) => Some(bytes),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => {
                        return Err(PlaylistReadFailure::new(
                            "legacy_smart_settings_read",
                            "storage",
                            "Cannot read legacy Smart playlist settings",
                            error,
                        ));
                    }
                }
            }
            Err(error) => {
                return Err(PlaylistReadFailure::new(
                    "smart_settings_read",
                    "storage",
                    "Cannot read Smart playlist settings",
                    error,
                ));
            }
        }
    };
    let mut value = match bytes {
        Some(bytes) => {
            if bytes.len() > 4 * 1024 * 1024 {
                return Err(PlaylistReadFailure::new(
                    "smart_settings_size",
                    "metadata",
                    "Smart settings exceed supported size",
                    "Size limit exceeded",
                ));
            }
            serde_json::from_slice(&bytes)
                .map_err(|error| PlaylistReadFailure::parse("smart_settings_parse", error))?
        }
        None => serde_json::to_value(amp_core::show::Settings::default()).map_err(|error| {
            PlaylistReadFailure::new(
                "smart_settings_defaults",
                "metadata",
                "Cannot read Smart playlist settings",
                error,
            )
        })?,
    };
    amp_core::coordination::adopt_legacy(&mut value).map_err(|error| {
        PlaylistReadFailure::new("smart_settings_revision", "metadata", &error, &error)
    })?;
    Ok(value)
}
async fn playlists(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = tokio::task::spawn_blocking(move || -> Result<Value, PlaylistReadFailure> {
        let settings = read_smart_settings(&cast)?;
        let _: amp_core::show::Settings = serde_json::from_value(settings.clone())
            .map_err(|error| PlaylistReadFailure::parse("smart_settings_schema", error))?;
        let mut lists = sharing::lists(&cast)
            .map_err(|error| PlaylistReadFailure::catalog("playlist_listing", error))?;
        for list in &mut lists {
            if let Some(id) = list["id"]
                .as_str()
                .and_then(|id| id.strip_prefix("normal_"))
                && safe_id(id)
            {
                let p = cast.root.join("ampdj/playlists").join(format!("{id}.json"));
                let (playlist, mut raw) = sharing::normal_playlist_data(&cast, &p)
                    .map_err(|error| PlaylistReadFailure::catalog("normal_playlist_read", error))?;
                amp_core::coordination::adopt_legacy(&mut raw).map_err(|error| {
                    PlaylistReadFailure::new("normal_playlist_revision", "metadata", &error, &error)
                })?;
                list["revision"] = raw.get("_amp_sync").cloned().unwrap_or(Value::Null);
                list["track_paths"] = json!(
                    playlist
                        .tracks
                        .iter()
                        .map(|p| player_control::portable_path(p))
                        .collect::<Vec<_>>()
                );
                list["companion_available"] = json!(playlist.companion_available);
            }
        }
        Ok(json!({"playlists":lists,"smart_revision":settings}))
    })
    .await;
    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(error)) => error.response(),
        Err(error) => PlaylistReadFailure::new(
            "playlist_read_task",
            "internal",
            "Cannot read playlists",
            error,
        )
        .response(),
    }
}
#[derive(Deserialize)]
struct PlaylistEdit {
    library_id: Option<String>,
    track_paths: Option<Vec<String>>,
    id: Option<String>,
    name: String,
    #[serde(default)]
    tracks: Vec<usize>,
    rules: Option<Value>,
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
    if let Some(paths) = &e.track_paths {
        if e.library_id.as_deref() != cast.library_id.as_deref() || cast.library_id.is_none() {
            return err(
                StatusCode::CONFLICT,
                "Library changed. Refresh before saving this playlist.",
            );
        }
        if paths.len() > 20000 {
            return err(StatusCode::BAD_REQUEST, "Playlist has too many songs.");
        }
    }
    if e.name.trim().is_empty() || e.name.len() > 200 || e.tracks.len() > 20000 {
        return err(StatusCode::BAD_REQUEST, "Invalid playlist");
    }
    let result = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        if let Some(mut rules) = e.rules {
            let mut current = raw_smart_settings(&cast)?;
            if e.revision.as_ref() != Some(&current) {
                return Err("Smart playlists changed; reload settings revision".into());
            }
            if !rules.is_object() {return Err("Invalid Smart rules".into());}
            rules["name"] = json!(e.name);
            let id = e.id.unwrap_or_else(amp_core::coordination::token);
            let id = id.strip_prefix("smart_").unwrap_or(&id);
            if !safe_id(id) {
                return Err("Invalid playlist ID".into());
            }
            rules["id"] = json!(id);
            // Validate a typed copy, but preserve the caller's full raw nested rules.
            let _:amp_core::show::SmartPlaylist=serde_json::from_value(rules.clone()).map_err(|e|e.to_string())?;
            let rule_value = rules;
            let entries = current["playlists"]
                .as_array_mut()
                .ok_or("Invalid Smart playlist container")?;
            if let Some(existing) = entries.iter_mut().find(|p| p["id"] == id) {
                existing
                    .as_object_mut()
                    .ok_or("Invalid Smart playlist")?
                    .extend(rule_value.as_object().ok_or("Invalid rules")?.clone());
            } else {
                entries.push(rule_value);
            }
            let settings: amp_core::show::Settings =
                serde_json::from_value(current.clone()).map_err(|e| e.to_string())?;
            settings.validate()?;
            let library = cast
                .library_id
                .as_deref()
                .ok_or("Library identity unavailable")?;
            amp_core::coordination::save_document(
                &cast.root.join("ampdj/smart-playlists.json"),
                &current,
                library,
            )?;
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
            let existing = if path.exists() || amp_core::catalog_db::read(&path)?.is_some() {
                let (playlist,mut raw)=sharing::normal_playlist_data(&cast,&path)?;
                amp_core::coordination::adopt_legacy(&mut raw)?;
                Some((playlist,raw))
            } else {None};
            let current=existing.as_ref().and_then(|(_,v)|v.get("_amp_sync")).cloned();
            if current!=e.revision {return Err("Playlist changed; reload before saving".into());}
            let library=cast.library_id.as_deref().ok_or("Library identity unavailable")?;
            let paths = match e.track_paths {
                Some(paths)=>paths,
                None=>e.tracks.iter().map(|i|cast.tracks.get(*i).map(|t|player_control::portable_path(&t.path)).ok_or("Track unavailable")).collect::<Result<Vec<_>,_>>()?,
            };
            let old:std::collections::HashSet<_>=existing.as_ref().map(|(p,_)|p.tracks.iter().map(|p|player_control::portable_path(p)).collect()).unwrap_or_default();
            let mut portable=Vec::with_capacity(paths.len());
            for path in paths {
                let relative=path.replace('\\',"/");
                let reference=amp_core::playlist_sync_contract::TrackRef{library_id:library.into(),track_key:format!("song_{:x}",Sha256::digest(relative.as_bytes())),relative_path:relative.clone()};
                reference.validate(library)?;
                if player_control::track_index(&cast,&relative).is_none()&&!old.contains(&relative){return Err("A new song is not in this library. Existing unavailable references may be retained, moved or removed.".into());}
                portable.push(relative);
            }
            let mut value=existing.map(|(_,v)|v).unwrap_or_else(||json!({"version":1,"companion_available":false}));
            value["name"]=json!(e.name);value["tracks"]=json!(portable);
            amp_core::coordination::save_document(&path,&value,library)?;
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
    #[tokio::test]
    async fn unrelated_busy_errors_do_not_advertise_catalog_retry() {
        let response = PlaylistReadFailure::catalog(
            "playlist_listing",
            "Some unrelated storage is busy".into(),
        )
        .response();
        assert!(!response.headers().contains_key("retry-after"));
        let error = value(response).await;
        assert_eq!(error["code"], "playlist_read_failed");
    }
    #[tokio::test]
    async fn catalog_lock_busy_read_is_retryable_and_release_restores_saved_playlists() {
        let (root, _cast, app) = fixture();
        let created = write(
            &app,
            "/api/studio/playlists",
            json!({"name":"Saved normal", "tracks":[0]}),
        )
        .await;
        assert_eq!(created.status(), StatusCode::OK);
        amp_core::catalog_db::ensure(root.path()).unwrap();
        let before = value(read(&app, "/api/studio/playlists").await).await;
        assert!(before["playlists"].as_array().unwrap().len() >= 2);
        let catalog = amp_core::catalog_db::path(root.path());
        let bytes = std::fs::read(&catalog).unwrap();
        let lock =
            amp_core::coordination::Lock::acquire(&root.path().join("ampdj/catalog-publication"))
                .unwrap();
        let response = read(&app, "/api/studio/playlists").await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()["retry-after"], "1");
        let error = value(response).await;
        assert_eq!(error["error"], "Cannot read playlists");
        assert_eq!(error["code"], "catalog_busy");
        assert_eq!(error["kind"], "busy");
        assert_eq!(error["stage"], "smart_settings_catalog_read");
        assert!(!error.to_string().contains(root.path().to_str().unwrap()));
        drop(lock);
        let response = read(&app, "/api/studio/playlists").await;
        assert_eq!(response.status(), StatusCode::OK);
        let after = value(response).await;
        assert_eq!(after, before);
        assert_eq!(
            std::fs::read(catalog).unwrap(),
            bytes,
            "GET cannot rewrite the catalog"
        );
    }
    #[tokio::test]
    async fn playlist_parse_schema_and_storage_failures_never_advertise_retry() {
        for (contents, stage) in [
            ("{ invalid", "smart_settings_parse"),
            (
                r#"{"version":2,"playlists":"invalid"}"#,
                "smart_settings_schema",
            ),
        ] {
            let (root, _cast, app) = fixture();
            let file = root.path().join("ampdj/smart-playlists.json");
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, contents).unwrap();
            let response = read(&app, "/api/studio/playlists").await;
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert!(!response.headers().contains_key("retry-after"));
            let error = value(response).await;
            assert_eq!(error["error"], "Cannot read playlists");
            assert_eq!(error["code"], "playlist_read_failed");
            assert_eq!(error["kind"], "parse");
            assert_eq!(error["stage"], stage);
            assert!(!error.to_string().contains(contents));
            assert_eq!(std::fs::read_to_string(file).unwrap(), contents);
        }
        let (root, _cast, app) = fixture();
        std::fs::create_dir_all(root.path().join("ampdj/smart-playlists.json")).unwrap();
        let response = read(&app, "/api/studio/playlists").await;
        assert!(!response.headers().contains_key("retry-after"));
        let error = value(response).await;
        assert_eq!(error["code"], "playlist_read_failed");
        assert_eq!(error["kind"], "storage");
        assert_eq!(error["stage"], "smart_settings_read");
    }
    #[tokio::test]
    async fn empty_audio_is_a_source_error_instead_of_successful_silence() {
        let (_root, cast, app) = fixture();
        let file = path(&cast, 0).unwrap();
        std::fs::write(&file, []).unwrap();
        for method in ["GET", "HEAD"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri("/api/studio/tracks/0/audio")
                        .header("x-amp-password", "secret")
                        .header("range", "bytes=0-63")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
            if method == "GET" {
                let body = axum::body::to_bytes(response.into_body(), 4096)
                    .await
                    .unwrap();
                let value: Value = serde_json::from_slice(&body).unwrap();
                assert!(value["error"].as_str().unwrap().contains("empty"));
            }
        }
        assert_eq!(std::fs::metadata(file).unwrap().len(), 0);
    }
    #[tokio::test]
    async fn browser_session_sign_in_refresh_media_logout_and_csrf_are_scoped() {
        let (_root, cast, app) = fixture();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/studio/bootstrap")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/studio/session")
                    .header("x-amp-password", "wrong")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/studio/session")
                    .header("x-amp-password", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let set_cookie = response.headers()["set-cookie"].to_str().unwrap();
        assert!(
            set_cookie.contains("HttpOnly")
                && set_cookie.contains("SameSite=Strict")
                && set_cookie.contains("Path=/api")
        );
        assert!(!set_cookie.contains("secret"));
        let cookie = set_cookie.split(';').next().unwrap().to_owned();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/studio/session")
                    .header("cookie", &cookie)
                    .header("x-amp-password", "wrong")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "valid prior cookie cannot authorize a wrong explicit sign-in code"
        );
        for uri in [
            "/api/studio/bootstrap",
            "/api/studio/tracks",
            "/api/studio/tracks/0/audio",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(uri)
                        .header("cookie", &cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
        }
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/studio/session/logout")
                    .header("cookie", &cookie)
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
                Request::builder()
                    .uri("/api/studio/bootstrap")
                    .header("cookie", &cookie)
                    .header("x-amp-station", "other")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let (_other_root, _other_cast, other) = fixture();
        let response = other
            .oneshot(
                Request::builder()
                    .uri("/api/studio/bootstrap")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/studio/session/logout")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            response.headers()["set-cookie"]
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/studio/bootstrap")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        drop(cast);
    }
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
    #[tokio::test]
    async fn smart_creation_preserves_opaque_container_rules_hints_version_and_rejects_stale_revision()
     {
        let (root, _cast, app) = fixture();
        let path = root.path().join("ampdj/smart-playlists.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut original = serde_json::to_value(amp_core::show::Settings::default()).unwrap();
        original["version"] = json!(2);
        original["opaque_container"] = json!({"provider":"keep"});
        original["playlists"][0]["opaque_rule"] = json!([1, 2, 3]);
        original["playlists"][0]["source_library"] = json!("foreign-library");
        original["playlists"][0]["source_root_hint"] = json!("generated-foreign-source");
        amp_core::coordination::atomic_write(&path, &original).unwrap();
        let snapshot =
            value(read(&app, "/api/studio/playlists").await).await["smart_revision"].clone();
        let rules =
            serde_json::to_value(amp_core::show::Settings::default().playlists.remove(0)).unwrap();
        let response = write(
            &app,
            "/api/studio/playlists",
            json!({"name":"New Smart","rules":rules,"revision":snapshot}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let saved = raw_smart_settings(&_cast).unwrap();
        assert_eq!(saved["version"], 2);
        assert_eq!(saved["opaque_container"], original["opaque_container"]);
        assert_eq!(
            saved["playlists"][0]["opaque_rule"],
            original["playlists"][0]["opaque_rule"]
        );
        assert_eq!(saved["playlists"][0]["source_library"], "foreign-library");
        assert_eq!(
            saved["playlists"][0]["source_root_hint"],
            "generated-foreign-source"
        );
        assert_eq!(saved["playlists"].as_array().unwrap().len(), 2);
        let response = write(
            &app,
            "/api/studio/playlists",
            json!({"name":"Stale Smart","rules":rules,"revision":snapshot}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let after = raw_smart_settings(&_cast).unwrap();
        assert_eq!(saved, after);
    }
    #[tokio::test]
    async fn browser_smart_rename_retains_nested_opaque_rules_and_all_genres() {
        let (root, cast, app) = fixture();
        let file = root.path().join("ampdj/smart-playlists.json");
        let mut settings = serde_json::to_value(amp_core::show::Settings::default()).unwrap();
        settings["version"] = json!(2);
        settings["playlists"][0]["genres"] = json!(["R&B", "club music shit", "club"]);
        settings["playlists"][0]["filters"] =
            json!([{"type":"Flag","field":"Analyzed","value":true,"future_filter":{"retain":1}}]);
        settings["playlists"][0]["playlist_sources"] =
            json!([{"name":"Saved source","songs":[],"future_source":{"retain":2}}]);
        settings["playlists"][0]["energy_progression"] = json!({"target":7.,"transitions":4,"hold":true,"arc":"Linear","future_progression":{"retain":3}});
        amp_core::coordination::atomic_write(&file, &settings).unwrap();
        let snapshot =
            value(read(&app, "/api/studio/playlists").await).await["smart_revision"].clone();
        let mut rules = snapshot["playlists"][0].clone();
        rules["id"] = json!("not-the-target");
        let response = write(
            &app,
            "/api/studio/playlists",
            json!({"id":"smart_demo","name":"Renamed","rules":rules,"revision":snapshot}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let saved = raw_smart_settings(&cast).unwrap();
        let actual = &saved["playlists"][0];
        assert_eq!(actual["id"], "demo");
        assert_eq!(actual["name"], "Renamed");
        for field in [
            "genres",
            "filters",
            "playlist_sources",
            "energy_progression",
        ] {
            assert_eq!(
                actual[field], settings["playlists"][0][field],
                "{field} must survive a rename"
            );
        }
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
    async fn browser_normal_edit_preserves_unavailable_paths_companion_and_extensions() {
        let (root, cast, app) = fixture();
        let file = root.path().join("ampdj/playlists/retained.json");
        let library = cast.library_id.as_deref().unwrap();
        amp_core::coordination::save_document(&file,&json!({"version":1,"name":"Retained","tracks":["track-0.mp3","unavailable/song.mp3","track-0.mp3"],"companion_available":true,"future_setting":{"keep":[1,2]}}),library).unwrap();
        let data = value(read(&app, "/api/studio/playlists").await).await;
        let saved = data["playlists"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "normal_retained")
            .unwrap();
        assert_eq!(
            saved["track_paths"],
            json!(["track-0.mp3", "unavailable/song.mp3", "track-0.mp3"])
        );
        assert_eq!(saved["companion_available"], true);
        let request = json!({"id":"normal_retained","name":"Renamed","library_id":library,"track_paths":["unavailable/song.mp3","track-0.mp3","track-0.mp3"],"revision":saved["revision"]});
        assert_eq!(
            write(&app, "/api/studio/playlists", request.clone())
                .await
                .status(),
            StatusCode::OK
        );
        let (_, raw) = sharing::normal_playlist_data(&cast, &file).unwrap();
        assert_eq!(raw["name"], "Renamed");
        assert_eq!(raw["tracks"], request["track_paths"]);
        assert_eq!(raw["companion_available"], true);
        assert_eq!(raw["future_setting"], json!({"keep":[1,2]}));
        assert_eq!(
            write(&app, "/api/studio/playlists", request).await.status(),
            StatusCode::CONFLICT
        );
        for unsafe_path in ["../outside.mp3", "/absolute.mp3", "new-missing.mp3"] {
            assert_eq!(write(&app,"/api/studio/playlists",json!({"id":"normal_retained","name":"Rejected","library_id":library,"track_paths":[unsafe_path],"revision":raw["_amp_sync"]})).await.status(),StatusCode::CONFLICT);
        }
        assert_eq!(sharing::normal_playlist_data(&cast, &file).unwrap().1, raw);
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
