//! Conservative playlist sync. Pending intents are never replayed after interruption.
use super::*;
use amp_core::playlist_sync_contract::{Action, Kind, Operation, TrackRef};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_SNAPSHOT: usize = 4 * 1024 * 1024;
const MAX_RECORD: usize = 2 * 1024 * 1024;
pub fn routes() -> Router<Arc<Cast>> {
    Router::new()
        .route("/api/sync/v1/playlists", get(snapshot))
        .route("/api/sync/v1/playlist-operations", post(apply))
}
fn error(code: StatusCode, message: impl ToString) -> Response {
    (code, Json(json!({"error":message.to_string()}))).into_response()
}
fn authority(c: &Cast, h: &HeaderMap) -> bool {
    c.password.as_ref().is_none_or(|p| !p.is_empty())
        && c.authorized(h)
        && h.get("x-amp-server").and_then(|v| v.to_str().ok()) == Some(c.server_id.as_str())
        && h.get("x-amp-library").and_then(|v| v.to_str().ok()) == c.library_id.as_deref()
        && c.library_id.is_some()
}
fn raw(path: &Path) -> Result<Option<Value>, String> {
    let bytes = if let Some(b) = amp_core::catalog_db::read(path)? {
        Some(b)
    } else {
        match fs::File::open(path) {
            Ok(file) => {
                if file.metadata().map_err(|e| e.to_string())?.len() > MAX_RECORD as u64 {
                    return Err("Playlist record exceeds read bound".into());
                }
                let mut b = Vec::new();
                file.take(MAX_RECORD as u64 + 1)
                    .read_to_end(&mut b)
                    .map_err(|e| e.to_string())?;
                Some(b)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        }
    };
    bytes
        .map(|b| {
            if b.len() > MAX_RECORD {
                return Err("Playlist record exceeds read bound".into());
            }
            let mut v: Value = serde_json::from_slice(&b).map_err(|e| e.to_string())?;
            amp_core::coordination::adopt_legacy(&mut v)?;
            Ok(v)
        })
        .transpose()
}
fn rev(v: &Value) -> Value {
    v.get("_amp_sync").cloned().unwrap_or(Value::Null)
}
fn normal_path(c: &Cast, id: &str) -> Result<PathBuf, String> {
    let stem = id
        .strip_prefix("normal_")
        .filter(|s| amp_core::playlist_sync_contract::safe_existing_id(s))
        .ok_or("Invalid playlist ID")?;
    let folder = c.root.join("ampdj/playlists");
    checked_path(c, &folder)?;
    if folder.exists()
        && !folder
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(&c.root)
    {
        return Err("Playlist folder leaves library".into());
    }
    let p = folder.join(format!("{stem}.json"));
    if p.exists()
        && !p
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(&c.root)
    {
        return Err("Playlist leaves library".into());
    }
    Ok(p)
}
fn checked_path(c: &Cast, path: &Path) -> Result<(), String> {
    let existing = path
        .ancestors()
        .find(|p| p.exists())
        .ok_or("Missing library parent")?;
    if !existing
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(&c.root)
    {
        return Err("Playlist path leaves library".into());
    }
    Ok(())
}
fn smart_path(c: &Cast) -> PathBuf {
    c.root.join("ampdj/smart-playlists.json")
}
fn smart_raw(c: &Cast) -> Result<Option<Value>, String> {
    let path = smart_path(c);
    checked_path(c, &path)?;
    raw(&path)
}
fn wire_normal(c: &Cast, id: &str, stored: &Value) -> Result<Value, String> {
    if stored["version"].as_u64() != Some(1) {
        return Err("Unsupported normal playlist version".into());
    }
    let mut document = stored.clone();
    document
        .as_object_mut()
        .ok_or("Invalid playlist")?
        .remove("_amp_sync");
    let entries = stored["tracks"]
        .as_array()
        .ok_or("Invalid ordered tracks")?;
    if entries.len() > 20000 {
        return Err("Playlist exceeds ordered-reference bound".into());
    }
    let tracks = entries
        .iter()
        .map(|p| {
            let path = p.as_str().ok_or("Invalid track path")?.replace('\\', "/");
            let reference = TrackRef {
                library_id: c.library_id.clone().ok_or("Missing Library ID")?,
                track_key: format!("song_{:x}", Sha256::digest(path.as_bytes())),
                relative_path: path,
            };
            reference.validate(c.library_id.as_deref().unwrap())?;
            serde_json::to_value(reference).map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    document["tracks"] = json!(tracks);
    Ok(
        json!({"document_id":id,"kind":"normal","document_version":1,"revision":rev(stored),"document":document}),
    )
}
fn canonical(c: &Cast, op: &Operation) -> Result<Value, String> {
    match op.kind {
        Kind::Normal => Ok(match raw(&normal_path(c, &op.document_id)?)? {
            Some(v) => wire_normal(c, &op.document_id, &v)?,
            None => Value::Null,
        }),
        Kind::Smart => {
            let Some(settings) = smart_raw(c)? else {
                return Ok(Value::Null);
            };
            Ok(settings["playlists"].as_array().ok_or("Invalid smart settings")?.iter()
                .find(|p|p["id"].as_str()==op.document_id.strip_prefix("smart_"))
                .map(|p|json!({"document_id":op.document_id,"kind":"smart","document_version":settings["version"],"revision":rev(&settings),"document":p})).unwrap_or(Value::Null))
        }
    }
}
fn verify_library(c: &Cast) -> Result<(), String> {
    let path = c.root.join("ampdj/library.json");
    checked_path(c, &path)?;
    let identity = raw(&path)?.ok_or("Canonical library identity unavailable")?;
    if identity["id"].as_str() != c.library_id.as_deref() {
        return Err("Mounted library identity changed".into());
    }
    Ok(())
}
struct SnapshotBudget {
    used: usize,
    hash: Sha256,
}
struct BudgetWriter<'a> {
    remaining: usize,
    written: usize,
    hash: &'a mut Sha256,
}
impl Write for BudgetWriter<'_> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        if b.len() > self.remaining {
            return Err(std::io::Error::other(
                "Playlist snapshot exceeds response bound",
            ));
        }
        self.remaining -= b.len();
        self.written += b.len();
        self.hash.update(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl SnapshotBudget {
    fn new() -> Self {
        let mut hash = Sha256::new();
        hash.update(b"[");
        Self { used: 2, hash }
    }
    fn push(&mut self, documents: &mut Vec<Value>, document: Value) -> Result<(), String> {
        if documents.len() >= 1000 {
            return Err("Playlist snapshot exceeds document bound".into());
        }
        if !documents.is_empty() {
            self.used = self.used.checked_add(1).ok_or("Snapshot bound overflow")?;
            self.hash.update(b",");
        }
        let remaining = MAX_SNAPSHOT
            .checked_sub(self.used)
            .ok_or("Playlist snapshot exceeds response bound")?;
        let mut writer = BudgetWriter {
            remaining,
            written: 0,
            hash: &mut self.hash,
        };
        serde_json::to_writer(&mut writer, &document).map_err(|e| e.to_string())?;
        self.used += writer.written;
        documents.push(document);
        Ok(())
    }
    fn finish(mut self) -> String {
        self.hash.update(b"]");
        format!("{:x}", self.hash.finalize())
    }
}
fn collect(c: &Cast) -> Result<Value, String> {
    verify_library(c)?;
    let mut documents = Vec::new();
    let mut budget = SnapshotBudget::new();
    let mut paths = amp_core::catalog_db::playlist_paths(&c.root)?;
    paths.sort();
    if paths.len() > 1000 {
        return Err("Playlist snapshot exceeds document bound".into());
    }
    for path in paths {
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().contains(".json.conflict-"))
        {
            continue;
        }
        if path.parent() != Some(c.root.join("ampdj/playlists").as_path()) {
            return Err(
                "unsupported_unenrolled_playlist_path: outside conventional playlist root".into(),
            );
        }
        let id = format!(
            "normal_{}",
            path.file_stem()
                .and_then(|s| s.to_str())
                .ok_or("Invalid playlist filename")?
        );
        let checked = normal_path(c, &id)?;
        if checked != path {
            return Err("unsupported_unenrolled_playlist_path: exact mapping unavailable".into());
        }
        if let Some(v) = raw(&checked)? {
            budget.push(&mut documents, wire_normal(c, &id, &v)?)?;
        }
    }
    let mut smart_revision = Value::Null;
    if let Some(settings) = smart_raw(c)? {
        let typed: amp_core::show::Settings =
            serde_json::from_value(settings.clone()).map_err(|e| e.to_string())?;
        typed.validate()?;
        smart_revision = rev(&settings);
        for rules in settings["playlists"]
            .as_array()
            .ok_or("Invalid smart settings")?
        {
            budget.push(&mut documents,json!({"document_id":format!("smart_{}",rules["id"].as_str().ok_or("Invalid smart ID")?),"kind":"smart","document_version":settings["version"],"revision":smart_revision,"document":rules}))?;
        }
    }
    let token = budget.finish();
    let response = json!({"protocol":1,"server_id":c.server_id,"library_id":c.library_id,"snapshot_token":token,"documents":documents,"smart_revision":smart_revision,"deletes_supported":false,"automatic_client_sync":false});
    let mut hash = Sha256::new();
    let mut writer = BudgetWriter {
        remaining: MAX_SNAPSHOT,
        written: 0,
        hash: &mut hash,
    };
    serde_json::to_writer(&mut writer, &response).map_err(|e| e.to_string())?;
    Ok(response)
}
async fn snapshot(State(c): State<Arc<Cast>>, h: HeaderMap) -> Response {
    if !authority(&c, &h) {
        return error(
            StatusCode::FORBIDDEN,
            "Authentication and exact server/library identity required",
        );
    }
    match tokio::task::spawn_blocking(move || collect(&c)).await {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => error(StatusCode::SERVICE_UNAVAILABLE, e),
        Err(e) => error(StatusCode::SERVICE_UNAVAILABLE, e),
    }
}
#[derive(serde::Serialize, Deserialize)]
struct Ledger {
    request_digest: String,
    proposal: Operation,
    #[serde(default)]
    result: Option<Value>,
}
fn sync_parent(p: &Path) -> Result<(), String> {
    #[cfg(not(unix))]
    let _ = p;
    #[cfg(unix)]
    fs::File::open(p.parent().ok_or("Missing ledger parent")?)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn uncertain(c: &Cast, op: &Operation, reason: impl ToString) -> Value {
    let current = canonical(c, op);
    json!({"status":"uncertain","operation_id":op.operation_id,"proposal":op,"current":current.as_ref().ok(),"current_error":current.err(),"reason":reason.to_string(),"retry_publication":false})
}
fn publish(c: &Cast, op: &Operation) -> Result<(), String> {
    verify_library(c)?;
    let (path, current, mut proposed) = match op.kind {
        Kind::Normal => {
            let path = normal_path(c, &op.document_id)?;
            fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
            let path = normal_path(c, &op.document_id)?;
            let current = raw(&path)?;
            if (op.action == Action::Create) != current.is_none() {
                return Err("Create/update existence conflict".into());
            }
            let mut p = op.document.clone().ok_or("Missing document")?;
            let refs: Vec<TrackRef> =
                serde_json::from_value(p["tracks"].clone()).map_err(|e| e.to_string())?;
            p["tracks"] = json!(
                refs.into_iter()
                    .map(|r| r.relative_path)
                    .collect::<Vec<_>>()
            );
            (path, current, p)
        }
        Kind::Smart => {
            let path = smart_path(c);
            checked_path(c, &path)?;
            fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
            if !path
                .parent()
                .unwrap()
                .canonicalize()
                .map_err(|e| e.to_string())?
                .starts_with(&c.root)
            {
                return Err("Smart folder leaves library".into());
            }
            if path.exists()
                && !path
                    .canonicalize()
                    .map_err(|e| e.to_string())?
                    .starts_with(&c.root)
            {
                return Err("Smart settings leave library".into());
            }
            let current = raw(&path)?;
            if current
                .as_ref()
                .and_then(|v| v["version"].as_u64())
                .is_some_and(|v| v > u64::from(op.document_version))
            {
                return Err("Smart document schema downgrade rejected".into());
            }
            let mut p = current
                .clone()
                .unwrap_or_else(|| json!({"version":op.document_version,"playlists":[]}));
            let rules = p["playlists"]
                .as_array_mut()
                .ok_or("Invalid smart settings")?;
            let index = rules
                .iter()
                .position(|r| r["id"].as_str() == op.document_id.strip_prefix("smart_"));
            let previous_hint = index
                .and_then(|i| rules[i].get("source_root_hint"))
                .cloned()
                .unwrap_or(Value::Null);
            let requested_hint = op
                .document
                .as_ref()
                .and_then(|d| d.get("source_root_hint"))
                .cloned()
                .unwrap_or(Value::Null);
            if previous_hint != requested_hint {
                return Err(
                    "Source root hints are opaque and cannot be remapped by synchronization".into(),
                );
            }
            if (op.action == Action::Create) != index.is_none() {
                return Err("Create/update existence conflict".into());
            }
            if let Some(i) = index {
                rules[i] = op.document.clone().unwrap()
            } else {
                rules.push(op.document.clone().unwrap())
            }
            p["version"] = json!(op.document_version);
            let typed: amp_core::show::Settings =
                serde_json::from_value(p.clone()).map_err(|e| e.to_string())?;
            typed.validate()?;
            (path, current, p)
        }
    };
    let current_rev = current.as_ref().map(rev).filter(|r| !r.is_null());
    if current_rev != op.base_revision {
        return Err("Revision conflict; canonical version retained".into());
    }
    if let Some(base) = &op.base_revision {
        proposed["_amp_sync"] = base.clone();
    } else {
        proposed
            .as_object_mut()
            .ok_or("Expected document")?
            .remove("_amp_sync");
    }
    // Existing writer locks and rechecks the baseline, including catalog-aware publication.
    amp_core::coordination::save_document(&path, &proposed, &op.library_id)
}
fn execute(c: &Cast, op: Operation) -> Result<(StatusCode, Value), String> {
    op.validate(
        &c.server_id,
        c.library_id.as_deref().ok_or("Missing Library ID")?,
    )?;
    verify_library(c)?;
    let digest = op.request_digest()?;
    let config = c
        .config_dir
        .lock()
        .unwrap()
        .clone()
        .ok_or("Private server config required")?;
    let dir = config.join("playlist-sync/operations");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    sync_parent(&dir)?;
    sync_parent(dir.parent().unwrap())?;
    let key = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&json!([
                op.server_id,
                op.library_id,
                op.device_id,
                op.operation_id
            ]))
            .unwrap()
        )
    );
    let path = dir.join(format!("{key}.json"));
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let old: Ledger = match fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
            {
                Some(v) => v,
                None => {
                    return Ok((
                        StatusCode::CONFLICT,
                        uncertain(c, &op, "Incomplete intent; review retained request"),
                    ));
                }
            };
            if old.request_digest != digest {
                return Ok((
                    StatusCode::CONFLICT,
                    json!({"status":"operation_id_reused","operation_id":op.operation_id}),
                ));
            }
            return Ok(if let Some(result) = old.result {
                // Windows FlushFileBuffers requires a handle opened with write access.
                if let Err(e) = fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .and_then(|f| f.sync_all())
                    .map_err(|e| e.to_string())
                    .and_then(|_| sync_parent(&path))
                {
                    return Ok((StatusCode::CONFLICT, uncertain(c, &op, e)));
                }
                (StatusCode::OK, result)
            } else {
                (
                    StatusCode::CONFLICT,
                    uncertain(
                        c,
                        &old.proposal,
                        "Unacknowledged intent; publication must not be replayed",
                    ),
                )
            });
        }
        Err(e) => return Err(e.to_string()),
    };
    let mut ledger = Ledger {
        request_digest: digest,
        proposal: op.clone(),
        result: None,
    };
    file.write_all(&serde_json::to_vec(&ledger).map_err(|e| e.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    sync_parent(&path)?;
    if let Err(e) = publish(c, &op) {
        return Ok((StatusCode::CONFLICT, uncertain(c, &op, e)));
    }
    // Read-back is necessary but not sufficient if another writer interleaved.
    let current = match canonical(c, &op) {
        Ok(v) => v,
        Err(e) => return Ok((StatusCode::CONFLICT, uncertain(c, &op, e))),
    };
    let expected = op.document.as_ref().unwrap();
    let mut read_doc = current["document"].clone();
    // Publication adds internal revision only; wire document retains all user fields.
    if op.kind == Kind::Normal {
        read_doc.as_object_mut().map(|o| o.remove("_amp_sync"));
    }
    if &read_doc != expected {
        return Ok((
            StatusCode::CONFLICT,
            uncertain(c, &op, "Publication interleaved before acknowledgment"),
        ));
    }
    let result = json!({"protocol":1,"server_id":c.server_id,"library_id":c.library_id,"status":"applied","operation_id":op.operation_id,"canonical":current});
    ledger.result = Some(result.clone());
    if let Err(e) = amp_core::coordination::atomic_write(&path, &ledger) {
        return Ok((StatusCode::CONFLICT, uncertain(c, &op, e)));
    }
    Ok((StatusCode::OK, result))
}
async fn apply(State(c): State<Arc<Cast>>, h: HeaderMap, Json(op): Json<Operation>) -> Response {
    if !authority(&c, &h) {
        return error(
            StatusCode::FORBIDDEN,
            "Authentication and exact server/library identity required",
        );
    }
    if let Err(e) = op.validate(&c.server_id, c.library_id.as_deref().unwrap()) {
        return error(StatusCode::BAD_REQUEST, e);
    }
    match tokio::task::spawn_blocking(move || execute(&c, op)).await {
        Ok(Ok((code, v))) => (code, Json(v)).into_response(),
        Ok(Err(e)) => error(StatusCode::SERVICE_UNAVAILABLE, e),
        Err(e) => error(StatusCode::SERVICE_UNAVAILABLE, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amp_core::playlist_sync_contract::PROTOCOL;
    use axum::http::Request;
    use tower::ServiceExt;
    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        config: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("music");
            let config = dir.path().join("private");
            fs::create_dir_all(&root).unwrap();
            fs::create_dir_all(&config).unwrap();
            let root = root.canonicalize().unwrap();
            Self {
                _dir: dir,
                root,
                config,
            }
        }
        fn cast(&self) -> Arc<Cast> {
            let c = Cast::with_identity(
                self.root.clone(),
                amp_core::scan(&self.root).unwrap(),
                Some("secret".into()),
                "server".into(),
                "station".into(),
                "Fixture".into(),
            );
            *c.config_dir.lock().unwrap() = Some(self.config.clone());
            c
        }
        fn op(&self, id: &str) -> Operation {
            let c = self.cast();
            let library = c.library_id.clone().unwrap();
            let path = "missing/song.mp3";
            let track = json!({"library_id":library,"relative_path":path,"track_key":format!("song_{:x}",Sha256::digest(path.as_bytes()))});
            Operation {
                protocol: PROTOCOL,
                operation_id: id.into(),
                device_id: "phone".into(),
                server_id: "server".into(),
                library_id: library,
                document_id: "normal_saved".into(),
                kind: Kind::Normal,
                document_version: 1,
                action: Action::Create,
                base_revision: None,
                document: Some(
                    json!({"version":1,"name":"Keep","companion_available":true,"tracks":[track.clone(),track],"future":{"preserved":true}}),
                ),
            }
        }
        fn intent(&self, op: &Operation) {
            let dir = self.config.join("playlist-sync/operations");
            fs::create_dir_all(&dir).unwrap();
            let key = format!(
                "{:x}",
                Sha256::digest(
                    serde_json::to_vec(&json!([
                        op.server_id,
                        op.library_id,
                        op.device_id,
                        op.operation_id
                    ]))
                    .unwrap()
                )
            );
            amp_core::coordination::atomic_write(
                &dir.join(format!("{key}.json")),
                &Ledger {
                    request_digest: op.request_digest().unwrap(),
                    proposal: op.clone(),
                    result: None,
                },
            )
            .unwrap();
        }
    }
    #[test]
    fn normal_roundtrip_keeps_order_duplicates_unresolved_and_extensions() {
        let f = Fixture::new();
        let c = f.cast();
        let op = f.op("one");
        let (status, result) = execute(&c, op.clone()).unwrap();
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["canonical"]["document"], op.document.unwrap());
        let snapshot = collect(&c).unwrap();
        assert_eq!(snapshot["documents"].as_array().unwrap().len(), 1);
        let stored = raw(&f.root.join("ampdj/playlists/saved.json"))
            .unwrap()
            .unwrap();
        assert_eq!(
            stored["tracks"],
            json!(["missing/song.mp3", "missing/song.mp3"])
        );
        assert_eq!(stored["companion_available"], true);
        assert_eq!(stored["future"]["preserved"], true);
        amp_core::catalog_db::compact(&f.root).unwrap();
        assert_eq!(
            collect(&f.cast()).unwrap()["documents"],
            snapshot["documents"]
        );
    }
    #[test]
    fn smart_roundtrip_preserves_container_and_rule_extensions() {
        let f = Fixture::new();
        let c = f.cast();
        fs::create_dir_all(f.root.join("ampdj")).unwrap();
        let mut settings = serde_json::to_value(amp_core::show::Settings::default()).unwrap();
        settings["future_container"] = json!({"keep":1});
        settings["playlists"][0]["source_root_hint"] = json!("C:\\Music");
        amp_core::coordination::atomic_write(&smart_path(&c), &settings).unwrap();
        let current = raw(&smart_path(&c)).unwrap().unwrap();
        let mut op = f.op("smart");
        op.kind = Kind::Smart;
        op.action = Action::Update;
        op.document_id = "smart_demo".into();
        op.base_revision = Some(rev(&current));
        let mut rules = current["playlists"][0].clone();
        rules["future_rule"] = json!({"keep":2});
        rules["companion_available"] = json!(true);
        op.document = Some(rules.clone());
        let (status, result) = execute(&c, op).unwrap();
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["canonical"]["document"], rules);
        assert_eq!(
            raw(&smart_path(&c)).unwrap().unwrap()["future_container"],
            json!({"keep":1})
        );
    }
    #[test]
    fn durable_ack_replays_exact_request_but_rejects_reused_id() {
        let f = Fixture::new();
        let op = f.op("repeat");
        let (status, ack) = execute(&f.cast(), op.clone()).unwrap();
        assert_eq!(status, StatusCode::OK);
        let (status, replayed) = execute(&f.cast(), op.clone()).unwrap();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ack, replayed);
        let mut changed = op;
        changed.document.as_mut().unwrap()["name"] = json!("Changed");
        let (status, error) = execute(&f.cast(), changed).unwrap();
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(error["status"], "operation_id_reused");
        assert_eq!(
            collect(&f.cast()).unwrap()["documents"][0]["document"]["name"],
            "Keep"
        );
    }
    #[test]
    fn stale_cas_preserves_latest_and_unacknowledged_proposal() {
        let f = Fixture::new();
        let (_, ack) = execute(&f.cast(), f.op("create")).unwrap();
        let mut first = f.op("first");
        first.action = Action::Update;
        first.base_revision = Some(ack["canonical"]["revision"].clone());
        first.document.as_mut().unwrap()["name"] = json!("First");
        let mut stale = first.clone();
        stale.operation_id = "stale".into();
        stale.document.as_mut().unwrap()["name"] = json!("Stale");
        assert_eq!(execute(&f.cast(), first).unwrap().0, StatusCode::OK);
        let (status, result) = execute(&f.cast(), stale.clone()).unwrap();
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(result["proposal"]["document"]["name"], "Stale");
        assert_eq!(result["current"]["document"]["name"], "First");
        assert_eq!(execute(&f.cast(), stale).unwrap().1["status"], "uncertain");
    }
    #[test]
    fn interrupted_intent_never_publishes_or_replays() {
        let f = Fixture::new();
        let op = f.op("crash-intent");
        f.intent(&op);
        let (status, result) = execute(&f.cast(), op).unwrap();
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(result["status"], "uncertain");
        assert!(!f.root.join("ampdj/playlists/saved.json").exists());
    }
    #[test]
    fn interrupted_after_publish_never_infers_ack_or_overwrites_later_edit() {
        let f = Fixture::new();
        let op = f.op("crash-publish");
        f.intent(&op);
        publish(&f.cast(), &op).unwrap();
        let (_, interrupted) = execute(&f.cast(), op.clone()).unwrap();
        assert_eq!(interrupted["status"], "uncertain");
        let old = canonical(&f.cast(), &op).unwrap();
        let mut later = f.op("later");
        later.action = Action::Update;
        later.base_revision = Some(old["revision"].clone());
        later.document.as_mut().unwrap()["name"] = json!("Desktop later");
        assert_eq!(execute(&f.cast(), later).unwrap().0, StatusCode::OK);
        let (status, result) = execute(&f.cast(), op.clone()).unwrap();
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(result["status"], "uncertain");
        assert_eq!(result["current"]["document"]["name"], "Desktop later");
        assert_eq!(
            canonical(&f.cast(), &op).unwrap()["revision"],
            result["current"]["revision"]
        );
    }
    #[test]
    fn concurrent_duplicate_operation_publishes_once() {
        let f = Fixture::new();
        let c = f.cast();
        let op = f.op("concurrent");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let a = {
            let c = c.clone();
            let op = op.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                execute(&c, op).unwrap()
            })
        };
        barrier.wait();
        let b = execute(&c, op.clone()).unwrap();
        let a = a.join().unwrap();
        assert!(a.0 == StatusCode::OK || b.0 == StatusCode::OK);
        assert_eq!(canonical(&c, &op).unwrap()["revision"]["revision"], 1);
    }
    #[test]
    fn smart_create_uses_container_baseline_and_does_not_replace_other_rules() {
        let f = Fixture::new();
        let c = f.cast();
        let mut op = f.op("smart-first");
        op.kind = Kind::Smart;
        op.document_id = "smart_demo".into();
        op.document =
            Some(serde_json::to_value(&amp_core::show::Settings::default().playlists[0]).unwrap());
        let (_, ack) = execute(&c, op.clone()).unwrap();
        assert_eq!(ack["status"], "applied");
        op.operation_id = "smart-next".into();
        op.document_id = "smart_second".into();
        op.base_revision = Some(ack["canonical"]["revision"].clone());
        op.document.as_mut().unwrap()["id"] = json!("second");
        assert_eq!(execute(&c, op).unwrap().0, StatusCode::OK);
        assert_eq!(
            collect(&c).unwrap()["documents"].as_array().unwrap().len(),
            2
        );
    }
    #[test]
    fn wrong_mounted_library_and_conflict_artifacts_are_not_published() {
        let f = Fixture::new();
        let c = f.cast();
        let op = f.op("library-changed");
        fs::create_dir_all(f.root.join("ampdj/playlists")).unwrap();
        fs::write(
            f.root
                .join("ampdj/playlists/saved.json.conflict-retained.json"),
            b"not a canonical playlist",
        )
        .unwrap();
        assert_eq!(collect(&c).unwrap()["documents"], json!([]));
        amp_core::coordination::atomic_write(
            &f.root.join("ampdj/library.json"),
            &json!({"version":1,"id":"other"}),
        )
        .unwrap();
        assert!(execute(&c, op).is_err());
        assert!(!f.config.join("playlist-sync").exists());
    }
    #[cfg(unix)]
    #[test]
    fn symbolic_link_cannot_redirect_playlist_publication() {
        let f = Fixture::new();
        let c = f.cast();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), f.root.join("ampdj/playlists")).unwrap();
        assert!(execute(&c, f.op("symlink")).unwrap().1["status"] == "uncertain");
        assert!(!outside.path().join("saved.json").exists());
    }
    #[tokio::test]
    async fn auth_authority_and_delete_reject_before_ledger_writes() {
        let f = Fixture::new();
        let c = f.cast();
        let op = f.op("unauthorized");
        for (password, server, library) in [
            ("wrong", "server", op.library_id.as_str()),
            ("secret", "other", op.library_id.as_str()),
            ("secret", "server", "other"),
        ] {
            let request = Request::post("/api/sync/v1/playlist-operations")
                .header("content-type", "application/json")
                .header("x-amp-password", password)
                .header("x-amp-server", server)
                .header("x-amp-library", library)
                .body(Body::from(serde_json::to_vec(&op).unwrap()))
                .unwrap();
            assert_eq!(
                router(c.clone()).oneshot(request).await.unwrap().status(),
                StatusCode::FORBIDDEN
            );
        }
        let mut delete = op;
        delete.action = Action::Delete;
        let request = Request::post("/api/sync/v1/playlist-operations")
            .header("content-type", "application/json")
            .header("x-amp-password", "secret")
            .header("x-amp-server", "server")
            .header("x-amp-library", &delete.library_id)
            .body(Body::from(serde_json::to_vec(&delete).unwrap()))
            .unwrap();
        assert_eq!(
            router(c).oneshot(request).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
        assert!(!f.config.join("playlist-sync").exists());
    }

    #[tokio::test]
    async fn invalid_empty_configured_secret_cannot_read_or_create_ledger() {
        let f = Fixture::new();
        let op = f.op("public-identities");
        {
            let password = Some(String::new());
            let c = Cast::with_identity(
                f.root.clone(),
                amp_core::scan(&f.root).unwrap(),
                password,
                "server".into(),
                "station".into(),
                "Fixture".into(),
            );
            *c.config_dir.lock().unwrap() = Some(f.config.clone());
            for (method, path, body) in [
                ("GET", "/api/sync/v1/playlists", Body::empty()),
                (
                    "POST",
                    "/api/sync/v1/playlist-operations",
                    Body::from(serde_json::to_vec(&op).unwrap()),
                ),
            ] {
                let req = Request::builder()
                    .method(method)
                    .uri(path)
                    .header("content-type", "application/json")
                    .header("x-amp-server", "server")
                    .header("x-amp-library", &op.library_id)
                    .header("x-amp-password", "")
                    .body(body)
                    .unwrap();
                assert_eq!(
                    router(c.clone()).oneshot(req).await.unwrap().status(),
                    StatusCode::FORBIDDEN
                );
            }
        }
        assert!(!f.config.join("playlist-sync").exists());
    }
    #[tokio::test]
    async fn trusted_lan_sync_honors_no_code_mode_but_keeps_identity_and_cross_site_guards() {
        let f = Fixture::new();
        let op = f.op("trusted-lan");
        let c = Cast::with_identity(
            f.root.clone(),
            amp_core::scan(&f.root).unwrap(),
            None,
            "server".into(),
            "station".into(),
            "Fixture".into(),
        );
        *c.config_dir.lock().unwrap() = Some(f.config.clone());
        for (server, library, cross_site) in [
            ("other", op.library_id.as_str(), false),
            ("server", "other", false),
            ("server", op.library_id.as_str(), true),
        ] {
            let mut request = Request::post("/api/sync/v1/playlist-operations")
                .header("content-type", "application/json")
                .header("x-amp-server", server)
                .header("x-amp-library", library);
            if cross_site {
                request = request.header("sec-fetch-site", "cross-site");
            }
            let response = router(c.clone())
                .oneshot(
                    request
                        .body(Body::from(serde_json::to_vec(&op).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        assert!(!f.config.join("playlist-sync").exists());
        let response = router(c.clone())
            .oneshot(
                Request::get("/api/sync/v1/playlists")
                    .header("x-amp-server", "server")
                    .header("x-amp-library", &op.library_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = router(c)
            .oneshot(
                Request::post("/api/sync/v1/playlist-operations")
                    .header("content-type", "application/json")
                    .header("x-amp-server", "server")
                    .header("x-amp-library", &op.library_id)
                    .body(Body::from(serde_json::to_vec(&op).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        let reply: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reply["status"], "applied");
    }
    #[test]
    fn legacy_space_dot_unicode_ids_roundtrip_exactly_without_new_id_relaxation() {
        let f = Fixture::new();
        let c = f.cast();
        fs::create_dir_all(f.root.join("ampdj/playlists")).unwrap();
        let stem = "Party mix.v2 – café";
        let path = f.root.join("ampdj/playlists").join(format!("{stem}.json"));
        amp_core::coordination::atomic_write(&path,&json!({"version":1,"name":"Legacy","tracks":["missing/song.mp3"],"companion_available":true})).unwrap();
        let snapshot = collect(&c).unwrap();
        let original = &snapshot["documents"][0];
        assert_eq!(original["document_id"], format!("normal_{stem}"));
        let mut op = f.op("legacy");
        op.action = Action::Update;
        op.document_id = format!("normal_{stem}");
        op.base_revision = Some(original["revision"].clone());
        op.document = Some(original["document"].clone());
        op.document.as_mut().unwrap()["name"] = json!("Edited");
        assert_eq!(execute(&c, op.clone()).unwrap().0, StatusCode::OK);
        assert_eq!(raw(&path).unwrap().unwrap()["name"], "Edited");
        op.action = Action::Create;
        op.base_revision = None;
        assert!(op.validate("server", &op.library_id).is_err());
        amp_core::catalog_db::compact(&f.root).unwrap();
        assert_eq!(
            collect(&f.cast()).unwrap()["documents"][0]["document_id"],
            format!("normal_{stem}")
        );
    }
    #[test]
    fn nested_packed_playlist_is_reported_instead_of_flattened() {
        let f = Fixture::new();
        let c = f.cast();
        amp_core::catalog_db::compact(&f.root).unwrap();
        let nested = f.root.join("ampdj/playlists/nested/saved.json");
        assert!(
            amp_core::catalog_db::write(
                &nested,
                &serde_json::to_vec(&json!({"version":1,"name":"Nested","tracks":[]})).unwrap()
            )
            .unwrap()
        );
        assert!(
            collect(&c)
                .unwrap_err()
                .contains("unsupported_unenrolled_playlist_path")
        );
        assert!(!f.root.join("ampdj/playlists/saved.json").exists());
    }
    #[test]
    fn snapshot_budget_stops_before_append_and_loose_reads_are_bounded() {
        let mut budget = SnapshotBudget::new();
        let mut documents = Vec::new();
        let first = json!({"padding":"x".repeat(MAX_SNAPSHOT/2)});
        budget.push(&mut documents, first.clone()).unwrap();
        assert!(budget.push(&mut documents, first).is_err());
        assert_eq!(documents.len(), 1);
        let mut small = SnapshotBudget::new();
        let mut values = Vec::new();
        small.push(&mut values, json!({"a":1})).unwrap();
        small.push(&mut values, json!({"b":2})).unwrap();
        assert_eq!(
            small.finish(),
            format!("{:x}", Sha256::digest(serde_json::to_vec(&values).unwrap()))
        );
        let f = Fixture::new();
        let path = f._dir.path().join("oversized.json");
        fs::File::create(&path)
            .unwrap()
            .set_len(MAX_RECORD as u64 + 1)
            .unwrap();
        assert!(raw(&path).unwrap_err().contains("read bound"));
    }
}
