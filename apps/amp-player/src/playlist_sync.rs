//! Saved-playlist synchronization only. Never receives queues, decks or editor drafts.
use amp_core::playlist_sync_contract::{Action, Kind, Operation, TrackRef};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};
const MAX: u64 = 4 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
pub struct Binding {
    base: String,
    server_id: String,
    library_id: String,
    root: PathBuf,
    #[serde(default)]
    ui_root: PathBuf,
    #[serde(default = "protected_default")]
    password_required: bool,
    secret: Vec<u8>,
}
#[derive(Clone)]
pub struct Candidate {
    pub address: String,
    pub password: String,
    pub server_id: String,
    pub library_id: String,
    pub password_required: bool,
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
struct Document {
    document_id: String,
    kind: Kind,
    document_version: u32,
    revision: Value,
    document: Value,
}
fn legacy_smart_version() -> u32 {
    2
}
#[derive(Deserialize)]
struct Snapshot {
    #[serde(default = "legacy_smart_version")]
    max_smart_document_version: u32,
    protocol: u32,
    server_id: String,
    library_id: String,
    documents: Vec<Document>,
    smart_revision: Value,
}
#[derive(Clone, Serialize, Deserialize, Default)]
struct Record {
    #[serde(default)]
    revision: Value,
    baseline: Option<Value>,
    #[serde(default)]
    desired: Option<Value>,
    pending: Option<Operation>,
    attempted: bool,
    conflict: Option<String>,
}
#[derive(Serialize, Deserialize, Default)]
struct State {
    records: BTreeMap<String, Record>,
}
#[derive(Default)]
pub struct Client {
    binding: Option<Binding>,
    initialized: bool,
    pending: Option<mpsc::Receiver<Result<Completed, String>>>,
    next: Option<Instant>,
    pub status: String,
}
struct Completed {
    binding: Option<Binding>,
    status: String,
    saved: Option<SavedView>,
}
pub struct SavedView {
    pub settings: Option<amp_core::show::Settings>,
    pub normal: Vec<(PathBuf, amp_core::Playlist)>,
}
fn protected_default() -> bool {
    true
}
fn private_dir(path: &Path) -> Result<(), String> {
    amp_core::coordination::directory(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(err)?;
    }
    Ok(())
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn config() -> Result<PathBuf, String> {
    Ok(amp_core::coordination::device_config_dir()?.join("playlist-sync"))
}
fn save(path: &Path, value: &impl Serialize) -> Result<(), String> {
    private_dir(path.parent().ok_or("Missing private directory")?)?;
    if serde_json::to_vec(value).map_err(err)?.len() as u64 > MAX * 4 {
        return Err("Private playlist state exceeds bound; existing data retained".into());
    }
    amp_core::coordination::atomic_write(path, value)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(err)?;
    }
    Ok(())
}
fn state_path(binding: &Binding, directory: &Path) -> PathBuf {
    let key = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(&binding.server_id, &binding.library_id, &binding.root)).unwrap()
        )
    );
    directory.join(format!("{key}.json"))
}
fn bounded(reader: impl Read, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > limit {
        return Err("Playlist data exceeds read bound".into());
    }
    Ok(bytes)
}
fn read(path: &Path) -> Result<Option<Value>, String> {
    let bytes = if let Some(bytes) = amp_core::catalog_db::read(path)? {
        Some(bytes)
    } else {
        match std::fs::File::open(path) {
            Ok(file) => Some(bounded(file, MAX)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(err(e)),
        }
    };
    bytes
        .map(|b| {
            if b.len() as u64 > MAX {
                return Err("Playlist exceeds read limit".into());
            }
            serde_json::from_slice(&b).map_err(err)
        })
        .transpose()
}
fn checked(root: &Path, target: &Path) -> Result<(), String> {
    if !target.starts_with(root) {
        return Err("Playlist path leaves paired library".into());
    }
    let ancestor = target
        .ancestors()
        .find(|p| p.exists())
        .ok_or("Missing library path")?;
    if !ancestor.canonicalize().map_err(err)?.starts_with(root) {
        return Err("Linked playlist path leaves library".into());
    }
    Ok(())
}
fn local_identity(root: &Path) -> Result<String, String> {
    checked(root, &root.join("ampdj/library.json"))?;
    let v = read(&root.join("ampdj/library.json"))?
        .ok_or("Save a playlist before connecting this library")?;
    v["id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or("Missing library identity".into())
}
fn clean(mut value: Value) -> Value {
    if let Some(map) = value.as_object_mut() {
        map.retain(|k, _| !k.starts_with("_amp_"));
        map.remove("source_root_hint");
        map.remove("source_unavailable");
    }
    value
}
fn semantic(value: &Value) -> Value {
    clean(value.clone())
}
fn path(root: &Path, id: &str) -> Result<PathBuf, String> {
    let stem = id.strip_prefix("normal_").ok_or("Invalid normal ID")?;
    if stem.is_empty()
        || stem.len() > 128
        || stem == "."
        || stem == ".."
        || stem.contains(['/', '\\', ':', '\0', '*', '?', '<', '>', '|'])
    {
        return Err("Unsafe normal playlist ID".into());
    }
    let path = root.join("ampdj/playlists").join(format!("{stem}.json"));
    for ancestor in path.ancestors() {
        if ancestor == root {
            break;
        }
        if std::fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Linked playlist path".into());
        }
    }
    checked(root, &path)?;
    Ok(path)
}
fn local(binding: &Binding) -> Result<BTreeMap<String, Document>, String> {
    if local_identity(&binding.root)? != binding.library_id {
        return Err("Local library identity changed; sync paused".into());
    }
    let mut result = BTreeMap::new();
    for file in amp_core::catalog_db::playlist_paths(&binding.root)? {
        if file
            .file_name()
            .is_some_and(|n| n.to_string_lossy().contains(".json.conflict-"))
        {
            continue;
        }
        if file.parent() != Some(binding.root.join("ampdj/playlists").as_path()) {
            continue;
        }
        let id = format!(
            "normal_{}",
            file.file_stem()
                .and_then(|s| s.to_str())
                .ok_or("Invalid playlist name")?
        );
        let Some(value) = read(&path(&binding.root, &id)?)? else {
            continue;
        };
        let mut document = clean(value.clone());
        document["tracks"] = json!(
            value["tracks"]
                .as_array()
                .ok_or("Invalid playlist tracks")?
                .iter()
                .map(|p| {
                    let relative_path = p.as_str().ok_or("Invalid track path")?.replace('\\', "/");
                    let reference = TrackRef {
                        library_id: binding.library_id.clone(),
                        track_key: format!("song_{:x}", Sha256::digest(relative_path.as_bytes())),
                        relative_path,
                    };
                    reference.validate(&binding.library_id)?;
                    serde_json::to_value(reference).map_err(err)
                })
                .collect::<Result<Vec<_>, String>>()?
        );
        result.insert(
            id.clone(),
            Document {
                document_id: id,
                kind: Kind::Normal,
                document_version: 1,
                revision: value["_amp_sync"].clone(),
                document,
            },
        );
    }
    checked(
        &binding.root,
        &binding.root.join("ampdj/smart-playlists.json"),
    )?;
    if let Some(settings) = read(&binding.root.join("ampdj/smart-playlists.json"))? {
        for rules in settings["playlists"]
            .as_array()
            .ok_or("Invalid Smart Playlist settings")?
        {
            let id = format!(
                "smart_{}",
                rules["id"].as_str().ok_or("Missing Smart Playlist ID")?
            );
            result.insert(
                id.clone(),
                Document {
                    document_id: id,
                    kind: Kind::Smart,
                    document_version: settings["version"].as_u64().unwrap_or(1) as u32,
                    revision: settings["_amp_sync"].clone(),
                    document: clean(rules.clone()),
                },
            );
        }
    }
    if result.len() > 1000 {
        return Err("Playlist count exceeds sync bound".into());
    }
    Ok(result)
}
struct Transport {
    agent: ureq::Agent,
    binding: Binding,
    password: String,
}
impl Transport {
    fn new(binding: &Binding) -> Result<Self, String> {
        let password = if binding.secret.is_empty() {
            String::new()
        } else {
            match crate::playlist_sync_secret::open(&binding.secret)
                .and_then(|bytes| String::from_utf8(bytes).map_err(err))
            {
                Ok(value) => value,
                Err(_) if !binding.password_required => String::new(),
                Err(error) => return Err(error),
            }
        };
        if binding.password_required && password.is_empty() {
            return Err("The selected protected station requires its access code".into());
        }
        Ok(Self {
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(5)))
                .max_redirects(0)
                .proxy(None)
                .build()
                .into(),
            binding: binding.clone(),
            password,
        })
    }
    fn get(&self) -> Result<Snapshot, String> {
        let mut response = self
            .agent
            .get(format!("{}/api/sync/v1/playlists", self.binding.base))
            .header("x-amp-password", &self.password)
            .header("x-amp-server", &self.binding.server_id)
            .header("x-amp-library", &self.binding.library_id)
            .call()
            .map_err(|_| "Playlist server unavailable or access denied; local changes retained")?;
        let bytes = bounded(response.body_mut().as_reader(), MAX)?;
        if bytes.len() as u64 > MAX {
            return Err("Server playlist snapshot exceeds bound".into());
        }
        let result: Snapshot = serde_json::from_slice(&bytes).map_err(err)?;
        if result.protocol != 1
            || result.server_id != self.binding.server_id
            || result.library_id != self.binding.library_id
            || result.documents.len() > 1000
        {
            return Err("Server/library changed; sync paused".into());
        }
        let mut seen = BTreeSet::new();
        for doc in &result.documents {
            if !seen.insert(doc.document_id.clone()) {
                return Err("Duplicate server playlist ID".into());
            }
            let op = operation(&self.binding, doc, doc.revision.clone(), Action::Update);
            op.validate(&self.binding.server_id, &self.binding.library_id)?;
        }
        Ok(result)
    }
    fn post(&self, operation: &Operation) -> Result<Value, String> {
        let response = self
            .agent
            .post(format!(
                "{}/api/sync/v1/playlist-operations",
                self.binding.base
            ))
            .header("x-amp-password", &self.password)
            .header("x-amp-server", &self.binding.server_id)
            .header("x-amp-library", &self.binding.library_id)
            .send_json(operation);
        let mut response = match response {
            Ok(r) => r,
            Err(ureq::Error::StatusCode(409)) => {
                return Err("Playlist conflict or uncertain receipt; retained for review".into());
            }
            Err(_) => return Err("Playlist publication unacknowledged; retained for review".into()),
        };
        let result: Value = response.body_mut().read_json().map_err(err)?;
        if result["protocol"] != 1
            || result["canonical"]["document_id"] != operation.document_id
            || !result["canonical"]["revision"].is_object()
            || result["status"] != "applied"
            || result["server_id"] != self.binding.server_id
            || result["library_id"] != self.binding.library_id
            || result["operation_id"] != operation.operation_id
            || semantic(&result["canonical"]["document"])
                != semantic(operation.document.as_ref().unwrap())
        {
            return Err("Playlist receipt did not match request; retained for review".into());
        }
        Ok(result)
    }
}
fn operation(binding: &Binding, doc: &Document, revision: Value, action: Action) -> Operation {
    Operation {
        protocol: 1,
        operation_id: amp_core::coordination::token(),
        device_id: amp_core::coordination::device_id().unwrap_or_default(),
        server_id: binding.server_id.clone(),
        library_id: binding.library_id.clone(),
        document_id: doc.document_id.clone(),
        kind: doc.kind.clone(),
        document_version: doc.document_version,
        action,
        base_revision: (!revision.is_null()).then_some(revision),
        document: Some(clean(doc.document.clone())),
    }
}
#[derive(Debug, PartialEq)]
enum Decision {
    Equal,
    Pull,
    Push,
    Conflict,
    Removed,
}
fn decide(base: Option<&Value>, local: Option<&Value>, remote: Option<&Value>) -> Decision {
    if local.map(semantic) == remote.map(semantic) {
        return Decision::Equal;
    }
    match (base, local, remote) {
        (None, Some(_), None) => Decision::Push,
        (None, None, Some(_)) => Decision::Pull,
        (Some(base), Some(local), Some(_remote)) if semantic(local) == semantic(base) => {
            Decision::Pull
        }
        (Some(base), Some(_local), Some(remote)) if semantic(remote) == semantic(base) => {
            Decision::Push
        }
        (Some(_), None, _) | (Some(_), _, None) => Decision::Removed,
        _ => Decision::Conflict,
    }
}
fn hydrate(binding: &Binding, doc: &Document, expected: Option<&Document>) -> Result<(), String> {
    if local_identity(&binding.root)? != binding.library_id {
        return Err("Local library identity changed".into());
    }
    let target = match doc.kind {
        Kind::Normal => path(&binding.root, &doc.document_id)?,
        Kind::Smart => binding.root.join("ampdj/smart-playlists.json"),
    };
    checked(&binding.root, &target)?;
    let mut before = read(&target)?;
    if let Some(current) = &mut before {
        amp_core::coordination::adopt_legacy(current)?;
    }
    let mut value = match doc.kind {
        Kind::Normal => {
            let current=before.as_ref().map(|v|{let mut wire=clean(v.clone());wire["tracks"]=json!(v["tracks"].as_array().ok_or("Invalid local tracks")?.iter().map(|p|{let relative=p.as_str().ok_or("Invalid local path")?.replace('\\',"/");Ok(json!({"library_id":binding.library_id,"track_key":format!("song_{:x}",Sha256::digest(relative.as_bytes())),"relative_path":relative}))}).collect::<Result<Vec<_>,String>>()?);Ok::<_,String>(wire)}).transpose()?;
            if current
                .as_ref()
                .is_some_and(|v| semantic(v) == semantic(&doc.document))
            {
                return Ok(());
            }
            if current.as_ref().map(semantic) != expected.map(|d| semantic(&d.document)) {
                return Err("Local playlist changed while pulling; retained for review".into());
            }
            let mut value = doc.document.clone();
            value["tracks"] = json!(
                serde_json::from_value::<Vec<TrackRef>>(value["tracks"].clone())
                    .map_err(err)?
                    .into_iter()
                    .map(|t| t.relative_path)
                    .collect::<Vec<_>>()
            );
            value
        }
        Kind::Smart => {
            let mut value = before
                .clone()
                .unwrap_or_else(|| json!({"version":doc.document_version,"playlists":[]}));
            let entries = value["playlists"]
                .as_array_mut()
                .ok_or("Invalid local Smart Playlists")?;
            let current = entries.iter().find(|v| v["id"] == doc.document["id"]);
            if current.is_some_and(|v| semantic(v) == semantic(&doc.document)) {
                return Ok(());
            }
            if current.map(semantic) != expected.map(|d| semantic(&d.document)) {
                return Err("Local playlist changed while pulling; retained for review".into());
            }
            if let Some(entry) = entries.iter_mut().find(|v| v["id"] == doc.document["id"]) {
                *entry = doc.document.clone();
            } else {
                entries.push(doc.document.clone());
            }
            value["version"] =
                json!((value["version"].as_u64().unwrap_or(1) as u32).max(doc.document_version));
            value
        }
    };
    // Use exactly the revision read with the compared document. Never rebase over a later Save.
    if let Some(revision) = before.as_ref().and_then(|v| v.get("_amp_sync")) {
        value["_amp_sync"] = revision.clone();
    }
    amp_core::coordination::save_document(&target, &value, &binding.library_id)
}

fn load_state(path: &Path) -> Result<State, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
        Err(e) => return Err(err(e)),
    };
    let bytes = bounded(file, MAX * 4)?;
    if bytes.len() as u64 > MAX * 4 {
        return Err("Private playlist outbox exceeds bound; retained for review".into());
    }
    serde_json::from_slice(&bytes).map_err(err)
}
fn cycle(binding: &Binding, directory: &Path) -> Result<String, String> {
    let transport = Transport::new(binding)?;
    let state_file = state_path(binding, directory);
    private_dir(directory)?;
    let _guard = amp_core::coordination::Lock::acquire(&state_file)?;
    let mut state = load_state(&state_file)?;
    // Persist unsent local saved changes even if the server is currently offline.
    let mine = local(binding)?;
    for (id, doc) in &mine {
        let record = state.records.entry(id.clone()).or_default();
        if record.baseline.as_ref().map(semantic) != Some(semantic(&doc.document)) {
            record.desired = Some(doc.document.clone());
        }
    }
    save(&state_file, &state)?;
    let remote = transport.get()?;
    let remote_docs: BTreeMap<_, _> = remote
        .documents
        .into_iter()
        .map(|d| (d.document_id.clone(), d))
        .collect();
    let ids: BTreeSet<_> = mine
        .keys()
        .chain(remote_docs.keys())
        .chain(state.records.keys())
        .cloned()
        .collect();
    let mut synchronized = 0;
    for id in ids {
        let local_doc = mine.get(&id);
        let remote_doc = remote_docs.get(&id);
        let record = state.records.entry(id.clone()).or_default();
        if record.conflict.is_some() || record.attempted {
            let bytes = serde_json::to_vec(record).map_err(err)?;
            let name = format!("{:x}.json", Sha256::digest(bytes));
            save(&directory.join("conflicts").join(name), record)?;
        }
        match decide(
            record.baseline.as_ref(),
            local_doc.map(|d| &d.document),
            remote_doc.map(|d| &d.document),
        ) {
            _ if record.conflict.is_some() || record.attempted => {}
            Decision::Equal => {
                record.baseline = remote_doc.map(|d| d.document.clone());
                record.revision = remote_doc.map_or(Value::Null, |d| d.revision.clone());
                record.desired = None;
                record.pending = None;
                record.attempted = false;
                record.conflict = None;
                synchronized += 1;
            }
            Decision::Pull => {
                let doc = remote_doc.unwrap();
                match hydrate(binding, doc, local_doc) {
                    Ok(()) => {
                        record.baseline = Some(doc.document.clone());
                        record.revision = doc.revision.clone();
                        synchronized += 1;
                    }
                    Err(error) => record.conflict = Some(error),
                }
            }
            Decision::Push => {
                let doc = local_doc.unwrap();
                // Whole-container Smart CAS: fetch immediately before each serialized operation.
                let fresh = transport.get()?;
                let canonical = fresh.documents.iter().find(|d| d.document_id == id);
                if canonical.map(|d| semantic(&d.document))
                    != remote_doc.map(|d| semantic(&d.document))
                {
                    record.conflict = Some("Canonical playlist changed; review required".into());
                } else {
                    let revision = if doc.kind == Kind::Smart {
                        fresh.smart_revision
                    } else {
                        canonical.map_or(Value::Null, |d| d.revision.clone())
                    };
                    let action = if canonical.is_some() {
                        Action::Update
                    } else {
                        Action::Create
                    };
                    if doc.kind == Kind::Smart
                        && doc.document_version > fresh.max_smart_document_version
                    {
                        record.conflict = Some("Selected server needs a compatible update before syncing Clean rules; local rules retained".into());
                        continue;
                    }
                    let mut op = operation(binding, doc, revision, action);
                    if op.kind == Kind::Smart {
                        // An empty settings-v2 snapshot has no document version. Create
                        // with known-supported v2 so it cannot downgrade that container.
                        if op.action == Action::Create {
                            op.document_version = op.document_version.max(2);
                        }
                        op.document_version = op.document_version.max(
                            fresh
                                .documents
                                .iter()
                                .filter(|d| d.kind == Kind::Smart)
                                .map(|d| d.document_version)
                                .max()
                                .unwrap_or(1),
                        );
                    }
                    if op.kind == Kind::Smart
                        && let Some(hint) =
                            canonical.and_then(|d| d.document.get("source_root_hint"))
                    {
                        // Preserve the server's opaque compatibility hint; never upload
                        // a different device-local path from the desktop copy.
                        op.document.as_mut().unwrap()["source_root_hint"] = hint.clone();
                    }
                    op.validate(&binding.server_id, &binding.library_id)?;
                    record.pending = Some(op.clone());
                    record.attempted = true;
                    save(&state_file, &state)?; // Durable ambiguity boundary before any network write.
                    let outcome = transport.post(&op);
                    let record = state.records.get_mut(&id).unwrap();
                    match outcome {
                        Ok(ack) => {
                            record.revision = ack["canonical"]["revision"].clone();
                            record.baseline = Some(ack["canonical"]["document"].clone());
                            record.desired = None;
                            record.pending = None;
                            record.attempted = false;
                            synchronized += 1;
                        }
                        Err(error) => record.conflict = Some(error),
                    }
                }
            }
            Decision::Conflict => {
                record.conflict = Some(
                    "Both versions changed or first pairing differs; saved versions retained"
                        .into(),
                )
            }
            Decision::Removed => {
                record.conflict =
                    Some("Deletion synchronization is disabled; saved versions retained".into())
            }
        }
        save(&state_file, &state)?;
    }
    let conflicts = state
        .records
        .values()
        .filter(|r| r.conflict.is_some() || r.attempted)
        .count();
    Ok(format!(
        "{synchronized} saved playlists synchronized; {conflicts} need review. Playing music and unsaved drafts unchanged."
    ))
}
fn view(binding: &Binding) -> Result<SavedView, String> {
    let settings = read(&binding.root.join("ampdj/smart-playlists.json"))?
        .map(serde_json::from_value)
        .transpose()
        .map_err(err)?;
    let normal = local(binding)?
        .into_values()
        .filter(|d| d.kind == Kind::Normal)
        .map(|d| {
            let path = path(&binding.root, &d.document_id)?;
            let mut value = d.document;
            value["tracks"] = json!(
                serde_json::from_value::<Vec<TrackRef>>(value["tracks"].clone())
                    .map_err(err)?
                    .into_iter()
                    .map(|t| t.relative_path)
                    .collect::<Vec<_>>()
            );
            let mut p = read(&path)?;
            if let Some(current) = &mut p {
                amp_core::coordination::adopt_legacy(current)?;
            }
            if let Some(rev) = p.as_ref().and_then(|v| v.get("_amp_sync")) {
                value["_amp_sync"] = rev.clone();
            }
            Ok((path, serde_json::from_value(value).map_err(err)?))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(SavedView { settings, normal })
}
fn root_key(path: &Path) -> String {
    let value = path.to_string_lossy();
    #[cfg(windows)]
    {
        let value = value.replace('/', "\\");
        let value = if let Some(tail) = value.strip_prefix("\\\\?\\UNC\\") {
            format!("\\\\{tail}")
        } else {
            value.strip_prefix("\\\\?\\").unwrap_or(&value).to_owned()
        };
        value.trim_end_matches('\\').to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        value.trim_end_matches('/').to_owned()
    }
}
fn matches_root(binding: &Binding, root: &Path) -> bool {
    root.is_absolute()
        && (root_key(root) == root_key(&binding.root)
            || (!binding.ui_root.as_os_str().is_empty()
                && root_key(root) == root_key(&binding.ui_root)))
}
fn run_binding(binding: Binding, pair: bool) -> Result<Completed, String> {
    let directory = config()?;
    private_dir(&directory)?;
    let mut status = cycle(&binding, &directory)?;
    if pair {
        save(&directory.join("binding.json"), &binding)?;
    }
    let saved = match view(&binding) {
        Ok(saved) => Some(saved),
        Err(error) => {
            status.push_str(&format!(" Saved browser refresh unavailable: {error}"));
            None
        }
    };
    Ok(Completed {
        binding: Some(binding),
        status,
        saved,
    })
}
impl Client {
    fn spawn(&mut self, work: impl FnOnce() -> Result<Completed, String> + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(work());
        });
    }
    pub fn poll(&mut self, root: &Path) -> Option<SavedView> {
        if !self.initialized {
            self.initialized = true;
            self.spawn(|| {
                let binding = match read(&config()?.join("binding.json"))? {
                    Some(value) if !value.is_null() => {
                        Some(serde_json::from_value(value).map_err(err)?)
                    }
                    _ => None,
                };
                Ok(Completed {
                    binding,
                    status: String::new(),
                    saved: None,
                })
            });
        }
        let mut refreshed = None;
        let result = match self.pending.as_ref().map(|rx| rx.try_recv()) {
            Some(Ok(result)) => Some(result),
            Some(Err(mpsc::TryRecvError::Disconnected)) => Some(Err(
                "Playlist sync worker stopped; saved proposals retained".into(),
            )),
            _ => None,
        };
        if let Some(result) = result {
            self.pending = None;
            match result {
                Ok(done) => {
                    self.status = done.status;
                    if done.binding.as_ref().is_some_and(|b| matches_root(b, root)) {
                        refreshed = done.saved;
                    }
                    self.binding = done.binding;
                }
                Err(error) => self.status = error,
            }
            self.next = Some(Instant::now() + Duration::from_secs(15));
        }
        if self.pending.is_none()
            && self.next.is_none_or(|t| Instant::now() >= t)
            && let Some(binding) = self.binding.clone()
        {
            if matches_root(&binding, root) {
                self.spawn(move || run_binding(binding, false));
            } else {
                self.status = "Playlist sync paused: choose the paired music library".into();
            }
        }
        refreshed
    }
    pub fn show(&mut self, ui: &mut eframe::egui::Ui, candidate: Option<Candidate>, root: &Path) {
        ui.separator();
        ui.strong("Automatic saved-playlist sync");
        ui.label("Uses the selected station. Keeps playback and unsaved edits unchanged. Deletions are not synchronized.");
        if self.binding.is_some() {
            if ui
                .add_enabled(
                    self.pending.is_none(),
                    eframe::egui::Button::new("Pause playlist sync"),
                )
                .clicked()
            {
                self.spawn(|| {
                    save(&config()?.join("binding.json"), &Value::Null)?;
                    Ok(Completed {
                        binding: None,
                        status: "Playlist sync paused; saved proposals retained".into(),
                        saved: None,
                    })
                });
            }
        } else if let Some(candidate) = candidate {
            if ui
                .add_enabled(
                    self.pending.is_none() && !root.as_os_str().is_empty(),
                    eframe::egui::Button::new("Enable sync with this station"),
                )
                .clicked()
            {
                let ui_root = root.to_path_buf();
                self.status = "Checking the selected station and music library…".into();
                self.spawn(move||{
                    if !ui_root.is_absolute(){return Err("Choose this music folder with Library's folder picker before enabling sync".into());}
                    let root=ui_root.canonicalize().map_err(err)?;
                    if local_identity(&root)?!=candidate.library_id{return Err("This is a different library. Explicit source remapping is required; nothing was changed.".into());}
                    if candidate.password_required&&candidate.password.is_empty(){return Err("Reconnect the selected protected station with its access code first".into());}
                    let binding=Binding{base:format!("http://{}",candidate.address),server_id:candidate.server_id,library_id:candidate.library_id,root,ui_root,password_required:candidate.password_required,secret:if candidate.password.is_empty(){Vec::new()}else{crate::playlist_sync_secret::seal(candidate.password.as_bytes())?}};
                    run_binding(binding,true)
                });
            }
        } else {
            ui.weak("Connect to your chosen station first. Protected stations also need their access code.");
        }
        if self.pending.is_some() {
            ui.spinner();
        }
        ui.label(&self.status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Binding) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("music");
        std::fs::create_dir(&root).unwrap();
        let library_id = amp_core::coordination::library_id(&root).unwrap();
        (
            dir,
            Binding {
                base: "http://127.0.0.1:1".into(),
                server_id: "fixture-server".into(),
                library_id,
                root: root.canonicalize().unwrap(),
                ui_root: root.clone(),
                password_required: true,
                secret: crate::playlist_sync_secret::seal(b"test-secret").unwrap(),
            },
        )
    }
    fn normal(binding: &Binding, id: &str, name: &str) -> Document {
        let relative_path = "missing/song.mp3";
        Document {
            document_id: format!("normal_{id}"),
            kind: Kind::Normal,
            document_version: 1,
            revision: json!({"revision":1,"library_id":binding.library_id,"device_id":"fixture","fields":{},"field_devices":{}}),
            document: json!({"version":1,"name":name,"tracks":[{"library_id":binding.library_id,"track_key":format!("song_{:x}",Sha256::digest(relative_path.as_bytes())),"relative_path":relative_path}],"future_setting":{"keep":true}}),
        }
    }
    #[test]
    fn blocked_storage_worker_never_blocks_ui_or_probes_a_missing_root_on_completion() {
        let (_dir, mut binding) = fixture();
        binding.root = std::env::temp_dir().join(format!(
            "unmounted-storage-fixture-{}",
            amp_core::coordination::token()
        ));
        binding.ui_root = binding.root.clone();
        let selected = binding.ui_root.clone();
        let ui_thread = std::thread::current().id();
        let mut client = Client {
            initialized: true,
            ..Default::default()
        };
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        client.spawn(move || {
            assert_ne!(std::thread::current().id(), ui_thread);
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(Completed {
                binding: Some(binding),
                status: "Fixture completed".into(),
                saved: Some(SavedView {
                    settings: Some(amp_core::show::Settings::default()),
                    normal: vec![],
                }),
            })
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        // Completion remains blocked, but the UI can keep polling and rendering.
        for _ in 0..5 {
            assert!(client.poll(&selected).is_none());
        }
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(saved) = client.poll(&selected) {
                assert!(saved.settings.is_some());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Worker completion was not applied without filesystem access"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!matches_root(
            client.binding.as_ref().unwrap(),
            Path::new("other-unmounted-root")
        ));
        assert!(!matches_root(
            client.binding.as_ref().unwrap(),
            Path::new("")
        ));
    }
    #[cfg(windows)]
    #[test]
    fn lexical_windows_root_keys_match_raw_and_verbatim_unc_without_io() {
        assert_eq!(
            root_key(Path::new(r"\\?\UNC\NAS\Music")),
            root_key(Path::new(r"\\nas\music"))
        );
        assert_eq!(
            root_key(Path::new(r"\\?\C:\Music")),
            root_key(Path::new(r"c:\music"))
        );
    }
    #[test]
    fn delayed_worker_models_never_replace_a_newer_local_saved_revision() {
        let mut player = crate::Player::default();
        let newer = amp_core::coordination::Revision {
            revision: 3,
            library_id: "fixture-library".into(),
            ..Default::default()
        };
        let mut older = newer.clone();
        older.revision = 2;
        player.run.settings.sync = Some(newer.clone());
        player.run.settings.playlists[0].name = "Newly saved".into();
        let mut incoming = player.run.settings.clone();
        incoming.sync = Some(older.clone());
        incoming.playlists[0].name = "Delayed snapshot".into();
        let path = PathBuf::from("fixture-playlist.json");
        let saved = amp_core::Playlist {
            companion_available: false,
            sync: Some(newer),
            version: 1,
            name: "Newly saved normal".into(),
            tracks: vec![],
        };
        let mut stale = saved.clone();
        stale.sync = Some(older);
        stale.name = "Delayed normal".into();
        player.dj_browser.playlists.push((path.clone(), saved));
        player.dj_browser.refresh_sync_saved(
            SavedView {
                settings: Some(incoming),
                normal: vec![(path, stale)],
            },
            &mut player.run.settings,
            &mut player.show_editor,
        );
        assert_eq!(player.run.settings.playlists[0].name, "Newly saved");
        assert_eq!(player.dj_browser.playlists[0].1.name, "Newly saved normal");
    }
    #[test]
    fn open_lan_binding_needs_no_credential_but_protected_binding_still_does() {
        let (_dir, mut binding) = fixture();
        binding.secret.clear();
        assert!(Transport::new(&binding).is_err());
        binding.password_required = false;
        assert!(Transport::new(&binding).unwrap().password.is_empty());
        let mut old = serde_json::to_value(&binding).unwrap();
        old.as_object_mut().unwrap().remove("password_required");
        let old: Binding = serde_json::from_value(old).unwrap();
        assert!(old.password_required);
        assert!(Transport::new(&old).is_err());
    }
    #[test]
    fn three_way_conflicts_and_deletions_are_never_guessed() {
        let a = json!({"name":"a"});
        let b = json!({"name":"b"});
        let c = json!({"name":"c"});
        assert_eq!(decide(Some(&a), Some(&b), Some(&a)), Decision::Push);
        assert_eq!(decide(Some(&a), Some(&a), Some(&b)), Decision::Pull);
        assert_eq!(decide(Some(&a), Some(&b), Some(&c)), Decision::Conflict);
        assert_eq!(decide(None, Some(&a), Some(&b)), Decision::Conflict);
        assert_eq!(decide(Some(&a), None, Some(&a)), Decision::Removed);
        assert_eq!(decide(Some(&a), Some(&a), None), Decision::Removed);
        assert_eq!(decide(Some(&a), Some(&b), Some(&b)), Decision::Equal);
    }
    #[test]
    fn missing_tracks_duplicates_and_future_fields_survive_raw_hydration() {
        let (_dir, binding) = fixture();
        let mut doc = normal(&binding, "saved", "Missing stays");
        let track = doc.document["tracks"][0].clone();
        doc.document["tracks"].as_array_mut().unwrap().push(track);
        hydrate(&binding, &doc, None).unwrap();
        let saved = local(&binding).unwrap().remove(&doc.document_id).unwrap();
        assert_eq!(semantic(&saved.document), semantic(&doc.document));
        assert!(
            amp_core::Playlist::load(
                &path(&binding.root, &doc.document_id).unwrap(),
                &binding.root
            )
            .is_err()
        );
        assert_eq!(view(&binding).unwrap().normal[0].1.tracks.len(), 2);
    }
    #[test]
    fn raw_smart_rules_preserve_full_definition_and_extensions() {
        let (_dir, binding) = fixture();
        let settings = amp_core::show::Settings::default();
        let mut rules = serde_json::to_value(&settings.playlists[0]).unwrap();
        rules["genres"] = json!(["R&B"]);
        rules["future_setting"] = json!({"retain":[1,2,3]});
        let doc = Document {
            document_id: "smart_demo".into(),
            kind: Kind::Smart,
            document_version: 1,
            revision: Value::Null,
            document: rules,
        };
        hydrate(&binding, &doc, None).unwrap();
        assert_eq!(
            semantic(&local(&binding).unwrap()["smart_demo"].document),
            semantic(&doc.document)
        );
    }
    #[test]
    fn pulling_rechecks_local_saved_baseline_instead_of_overwriting_newer_save() {
        let (_dir, binding) = fixture();
        let first = normal(&binding, "saved", "Original");
        hydrate(&binding, &first, None).unwrap();
        let expected = local(&binding).unwrap().remove(&first.document_id).unwrap();
        let mut local_edit = expected.clone();
        local_edit.document["name"] = json!("Changed while network ran");
        hydrate(&binding, &local_edit, Some(&expected)).unwrap();
        let mut remote = first.clone();
        remote.document["name"] = json!("Remote");
        assert!(
            hydrate(&binding, &remote, Some(&expected))
                .unwrap_err()
                .contains("changed")
        );
        assert_eq!(
            local(&binding).unwrap()[&first.document_id].document["name"],
            "Changed while network ran"
        );
        let before = read(&path(&binding.root, &first.document_id).unwrap()).unwrap();
        hydrate(&binding, &local_edit, Some(&expected)).unwrap();
        assert_eq!(
            read(&path(&binding.root, &first.document_id).unwrap()).unwrap(),
            before,
            "Already canonical shared data must not be rewritten or report conflict"
        );
    }
    #[test]
    fn legacy_saved_documents_keep_guarded_baselines_without_requiring_audio() {
        let (_dir, binding) = fixture();
        let file = path(&binding.root, "normal_legacy").unwrap();
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            serde_json::to_vec(&json!({"version":1,"name":"Legacy","tracks":["missing/song.mp3"]}))
                .unwrap(),
        )
        .unwrap();
        assert!(view(&binding).unwrap().normal[0].1.sync.is_some());
        let expected = local(&binding).unwrap().remove("normal_legacy").unwrap();
        let mut incoming = expected.clone();
        incoming.document["name"] = json!("Updated from server");
        hydrate(&binding, &incoming, Some(&expected)).unwrap();
        assert_eq!(
            local(&binding).unwrap()["normal_legacy"].document["name"],
            "Updated from server"
        );
        assert!(read(&file).unwrap().unwrap()["_amp_sync"].is_object());
    }
    #[test]
    fn offline_saved_changes_are_durable_without_attempting_publication() {
        let (dir, binding) = fixture();
        let doc = normal(&binding, "offline", "Offline");
        hydrate(&binding, &doc, None).unwrap();
        let state_dir = dir.path().join("private");
        assert!(cycle(&binding, &state_dir).is_err());
        let saved: State =
            serde_json::from_value(read(&state_path(&binding, &state_dir)).unwrap().unwrap())
                .unwrap();
        let record = &saved.records[&doc.document_id];
        assert!(record.desired.is_some());
        assert!(!record.attempted);
        assert!(record.pending.is_none());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(state_path(&binding, &state_dir))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn saved_view_refresh_does_not_touch_playing_regular_reservations_or_smart_drafts() {
        let mut player = crate::Player {
            queue: vec![4, 4, 2],
            ..Default::default()
        };
        player
            .run
            .load_regular_playlist("Current performance".into());
        let mut changed = amp_core::show::Settings::default();
        changed.playlists[0].name = "Incoming saved name".into();
        let original = amp_core::show::Settings::default();
        let dir = tempfile::tempdir().unwrap();
        player.show_editor.new_playlist(dir.path(), &original);
        assert!(player.show_editor.has_unsaved());
        player.dj_browser.refresh_sync_saved(
            SavedView {
                settings: Some(changed),
                normal: vec![],
            },
            &mut player.run.settings,
            &mut player.show_editor,
        );
        assert_eq!(player.queue, vec![4, 4, 2]);
        assert!(player.run.has_regular_playlist());
        assert_eq!(player.run.playlist.as_deref(), Some("Current performance"));
        assert!(player.show_editor.has_unsaved());
        assert_eq!(player.run.settings.playlists[0].name, "Incoming saved name");
        player.run.profile = Some(original.playlists[0].clone());
        player.run.preloaded = Some((1, 4));
        let playing_profile = player.run.profile.as_ref().unwrap().name.clone();
        player.dj_browser.refresh_sync_saved(
            SavedView {
                settings: Some(original),
                normal: vec![],
            },
            &mut player.run.settings,
            &mut player.show_editor,
        );
        assert_eq!(player.run.profile.as_ref().unwrap().name, playing_profile);
        assert_eq!(player.run.preloaded, Some((1, 4)));
        assert_eq!(player.queue, vec![4, 4, 2]);
    }
    struct Server {
        _dir: tempfile::TempDir,
        binding: Binding,
        directory: PathBuf,
        stop: Option<tokio::sync::oneshot::Sender<()>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl Server {
        fn start() -> Self {
            Self::start_password(Some("test-secret"))
        }
        fn start_password(password: Option<&str>) -> Self {
            let (dir, mut binding) = fixture();
            binding.password_required = password.is_some();
            binding.secret = match password {
                Some(value) if !value.is_empty() => {
                    crate::playlist_sync_secret::seal(value.as_bytes()).unwrap()
                }
                _ => Vec::new(),
            };
            let config = dir.path().join("server");
            std::fs::create_dir(&config).unwrap();
            let station = amp_cast::Cast::with_identity(
                binding.root.clone(),
                amp_core::scan(&binding.root).unwrap(),
                password.map(str::to_owned),
                binding.server_id.clone(),
                "station".into(),
                "Fixture".into(),
            );
            *station.config_dir.lock().unwrap() = Some(config.clone());
            let (tx, rx) = mpsc::channel();
            let (stop, stopped) = tokio::sync::oneshot::channel();
            let thread = std::thread::spawn(move || {
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(async move {
                        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                        tx.send(listener.local_addr().unwrap()).unwrap();
                        axum::serve(listener, amp_cast::router(station))
                            .with_graceful_shutdown(async {
                                let _ = stopped.await;
                            })
                            .await
                            .unwrap();
                    })
            });
            binding.base = format!("http://{}", rx.recv().unwrap());
            Self {
                directory: config,
                binding,
                _dir: dir,
                stop: Some(stop),
                thread: Some(thread),
            }
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.stop.take().unwrap().send(());
            self.thread.take().unwrap().join().unwrap();
        }
    }
    #[test]
    fn open_lan_real_backend_syncs_without_code_and_still_pins_authority() {
        let server = Server::start_password(None);
        assert!(server.binding.secret.is_empty());
        assert!(Transport::new(&server.binding).unwrap().get().is_ok());
        let mut wrong = server.binding.clone();
        wrong.server_id = "not-the-selected-server".into();
        assert!(Transport::new(&wrong).unwrap().get().is_err());
        wrong = server.binding.clone();
        wrong.library_id = "not-the-selected-library".into();
        assert!(Transport::new(&wrong).unwrap().get().is_err());
        let root = server._dir.path().join("open-copy");
        std::fs::create_dir_all(root.join("ampdj")).unwrap();
        std::fs::copy(
            server.binding.root.join("ampdj/library.json"),
            root.join("ampdj/library.json"),
        )
        .unwrap();
        let mut binding = server.binding.clone();
        binding.root = root.canonicalize().unwrap();
        binding.ui_root = binding.root.clone();
        let doc = normal(&binding, "open-created", "Saved without a code");
        hydrate(&binding, &doc, None).unwrap();
        cycle(&binding, &server._dir.path().join("open-client")).unwrap();
        assert_eq!(
            local(&server.binding).unwrap()[&doc.document_id].document["name"],
            "Saved without a code"
        );
    }
    #[test]
    fn protected_and_invalid_empty_backend_modes_still_reject_no_code_clients() {
        for password in [Some("test-secret"), Some("")] {
            let server = Server::start_password(password);
            let mut unauthorized = server.binding.clone();
            unauthorized.password_required = false;
            unauthorized.secret.clear();
            assert!(Transport::new(&unauthorized).unwrap().get().is_err());
            assert!(!server.directory.join("playlist-sync/operations").exists());
        }
    }
    #[test]
    fn wrong_server_or_library_binding_never_publishes() {
        let server = Server::start();
        let mut wrong = server.binding.clone();
        wrong.server_id = "different-server".into();
        assert!(Transport::new(&wrong).unwrap().get().is_err());
        wrong = server.binding.clone();
        wrong.library_id = "different-library".into();
        assert!(
            cycle(&wrong, &server._dir.path().join("client"))
                .unwrap_err()
                .contains("identity")
        );
        assert!(!server.directory.join("playlist-sync/operations").exists());
    }
    #[test]
    fn shared_storage_changes_are_observed_without_post_or_revision_churn() {
        let server = Server::start();
        let doc = normal(&server.binding, "shared", "Already canonical");
        hydrate(&server.binding, &doc, None).unwrap();
        let before = read(&path(&server.binding.root, &doc.document_id).unwrap()).unwrap();
        let state_dir = server._dir.path().join("client");
        cycle(&server.binding, &state_dir).unwrap();
        cycle(&server.binding, &state_dir).unwrap();
        assert_eq!(
            read(&path(&server.binding.root, &doc.document_id).unwrap()).unwrap(),
            before
        );
        assert!(!server.directory.join("playlist-sync/operations").exists());
    }
    #[test]
    fn copied_smart_create_does_not_downgrade_existing_server_container() {
        let server = Server::start();
        let rules = amp_core::show::Settings::default().playlists.remove(0);
        let existing = Document {
            document_id: "smart_demo".into(),
            kind: Kind::Smart,
            document_version: 2,
            revision: Value::Null,
            document: serde_json::to_value(&rules).unwrap(),
        };
        hydrate(&server.binding, &existing, None).unwrap();
        let root = server._dir.path().join("copy-smart");
        std::fs::create_dir_all(root.join("ampdj")).unwrap();
        std::fs::copy(
            server.binding.root.join("ampdj/library.json"),
            root.join("ampdj/library.json"),
        )
        .unwrap();
        let mut binding = server.binding.clone();
        binding.root = root.canonicalize().unwrap();
        let mut new = existing;
        new.document_id = "smart_custom".into();
        new.document_version = 1;
        new.document["id"] = json!("custom");
        hydrate(&binding, &new, None).unwrap();
        cycle(&binding, &server._dir.path().join("smart-client")).unwrap();
        let settings = read(&server.binding.root.join("ampdj/smart-playlists.json"))
            .unwrap()
            .unwrap();
        assert_eq!(settings["version"], 2);
        assert_eq!(settings["playlists"].as_array().unwrap().len(), 2);
    }
    #[test]
    fn smart_create_preserves_an_empty_version_two_server_container() {
        let server = Server::start();
        amp_core::coordination::save_document(
            &server.binding.root.join("ampdj/smart-playlists.json"),
            &json!({"version":2,"playlists":[]}),
            &server.binding.library_id,
        )
        .unwrap();
        let root = server._dir.path().join("copy-empty-smart");
        std::fs::create_dir_all(root.join("ampdj")).unwrap();
        std::fs::copy(
            server.binding.root.join("ampdj/library.json"),
            root.join("ampdj/library.json"),
        )
        .unwrap();
        let mut binding = server.binding.clone();
        binding.root = root.canonicalize().unwrap();
        let rules = amp_core::show::Settings::default().playlists.remove(0);
        let doc = Document {
            document_id: "smart_demo".into(),
            kind: Kind::Smart,
            document_version: 1,
            revision: Value::Null,
            document: serde_json::to_value(rules).unwrap(),
        };
        hydrate(&binding, &doc, None).unwrap();
        cycle(&binding, &server._dir.path().join("empty-smart-client")).unwrap();
        assert_eq!(
            read(&server.binding.root.join("ampdj/smart-playlists.json"))
                .unwrap()
                .unwrap()["version"],
            2
        );
    }
    #[test]
    fn equal_or_both_missing_after_restart_never_clear_uncertain_intent() {
        let server = Server::start();
        let doc = normal(
            &server.binding,
            "uncertain-equal",
            "Committed before lost receipt",
        );
        hydrate(&server.binding, &doc, None).unwrap();
        let directory = server._dir.path().join("uncertain-client");
        let operation = operation(&server.binding, &doc, doc.revision.clone(), Action::Update);
        let mut state = State::default();
        state.records.insert(
            doc.document_id.clone(),
            Record {
                baseline: Some(json!({"name":"Older"})),
                pending: Some(operation.clone()),
                attempted: true,
                ..Default::default()
            },
        );
        save(&state_path(&server.binding, &directory), &state).unwrap();
        cycle(&server.binding, &directory).unwrap();
        let mut resumed = load_state(&state_path(&server.binding, &directory)).unwrap();
        let saved = &resumed.records[&doc.document_id];
        assert!(saved.attempted);
        assert_eq!(saved.pending.as_ref().unwrap(), &operation);
        let path = path(&server.binding.root, &doc.document_id).unwrap();
        std::fs::remove_file(path).unwrap();
        resumed.records.get_mut(&doc.document_id).unwrap().conflict =
            Some("Uncertain retained".into());
        save(&state_path(&server.binding, &directory), &resumed).unwrap();
        cycle(&server.binding, &directory).unwrap();
        let resumed = load_state(&state_path(&server.binding, &directory)).unwrap();
        let saved = &resumed.records[&doc.document_id];
        assert!(saved.attempted);
        assert!(saved.conflict.is_some());
        assert_eq!(saved.pending.as_ref().unwrap(), &operation);
        assert!(!server.directory.join("playlist-sync/operations").exists());
    }
    #[test]
    fn hinted_smart_name_edit_preserves_canonical_hint_not_a_local_path() {
        let server = Server::start();
        let mut rules =
            serde_json::to_value(amp_core::show::Settings::default().playlists.remove(0)).unwrap();
        rules["source_library"] = json!(server.binding.library_id);
        rules["source_root_hint"] = json!("original-server-hint");
        let doc = Document {
            document_id: "smart_demo".into(),
            kind: Kind::Smart,
            document_version: 1,
            revision: Value::Null,
            document: rules,
        };
        hydrate(&server.binding, &doc, None).unwrap();
        let root = server._dir.path().join("hint-copy");
        std::fs::create_dir_all(root.join("ampdj")).unwrap();
        std::fs::copy(
            server.binding.root.join("ampdj/library.json"),
            root.join("ampdj/library.json"),
        )
        .unwrap();
        let mut binding = server.binding.clone();
        binding.root = root.canonicalize().unwrap();
        let directory = server._dir.path().join("hint-client");
        cycle(&binding, &directory).unwrap();
        let before = local(&binding).unwrap().remove("smart_demo").unwrap();
        let mut edit = before.clone();
        edit.document["name"] = json!("Renamed on desktop");
        edit.document["source_root_hint"] = json!("different-desktop-hint");
        hydrate(&binding, &edit, Some(&before)).unwrap();
        cycle(&binding, &directory).unwrap();
        let canonical = read(&server.binding.root.join("ampdj/smart-playlists.json"))
            .unwrap()
            .unwrap();
        assert_eq!(canonical["playlists"][0]["name"], "Renamed on desktop");
        assert_eq!(
            canonical["playlists"][0]["source_root_hint"],
            "original-server-hint"
        );
        let saved = load_state(&state_path(&binding, &directory)).unwrap();
        assert!(!saved.records["smart_demo"].attempted);
        assert!(saved.records["smart_demo"].conflict.is_none());
    }
    #[test]
    fn copied_library_pushes_once_and_retains_uncertain_pending_without_replay() {
        let server = Server::start();
        let client_root = server._dir.path().join("copy");
        std::fs::create_dir_all(client_root.join("ampdj")).unwrap();
        std::fs::copy(
            server.binding.root.join("ampdj/library.json"),
            client_root.join("ampdj/library.json"),
        )
        .unwrap();
        let mut binding = server.binding.clone();
        binding.root = client_root.canonicalize().unwrap();
        let doc = normal(&binding, "copied", "Copied library");
        hydrate(&binding, &doc, None).unwrap();
        let private = server._dir.path().join("client");
        cycle(&binding, &private).unwrap();
        assert_eq!(
            local(&server.binding).unwrap()[&doc.document_id].document["name"],
            "Copied library"
        );
        let mut state: State =
            serde_json::from_value(read(&state_path(&binding, &private)).unwrap().unwrap())
                .unwrap();
        let old = local(&binding).unwrap().remove(&doc.document_id).unwrap();
        let mut changed = old.clone();
        changed.document["name"] = json!("Uncertain edit");
        hydrate(&binding, &changed, Some(&old)).unwrap();
        let record = state.records.get_mut(&doc.document_id).unwrap();
        record.pending = Some(operation(
            &binding,
            &changed,
            old.revision.clone(),
            Action::Update,
        ));
        record.attempted = true;
        save(&state_path(&binding, &private), &state).unwrap();
        let count = std::fs::read_dir(server.directory.join("playlist-sync/operations"))
            .unwrap()
            .count();
        cycle(&binding, &private).unwrap();
        assert_eq!(
            std::fs::read_dir(server.directory.join("playlist-sync/operations"))
                .unwrap()
                .count(),
            count
        );
        assert_eq!(
            local(&server.binding).unwrap()[&doc.document_id].document["name"],
            "Copied library"
        );
    }
}
