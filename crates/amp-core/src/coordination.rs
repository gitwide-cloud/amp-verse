//! Shared-file coordination. Requires coherent atomic rename and OS byte-range
//! locks on the mounted filesystem (SMB/NFS configuration must be validated).
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn token() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .map_err(|e| e.to_string())
}
fn regular(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(m) if !m.is_file() || m.file_type().is_symlink() => {
            Err("Shared records and locks must be regular files".into())
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn directory(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink() || !m.is_dir()) {
        return Err("Shared metadata directories must be real directories".into());
    }
    fs::create_dir_all(path).map_err(|e| e.to_string())
}
/// Lock files remain in place forever: unlinking them would split the lock domain.
pub struct Lock {
    _file: fs::File,
}
impl Lock {
    pub fn acquire(record: &Path) -> Result<Self, String> {
        // Short record maintenance may briefly overlap another writer/process handoff.
        // Retry only contention; filesystem errors still fail immediately, and callers
        // must still perform their revision/ownership checks after obtaining the lock.
        let catalog = record
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| matches!(n, "catalog-publication" | "catalog-migration"));
        let deadline = std::time::Instant::now()
            + if catalog {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(250)
            };
        loop {
            if let Some(lock) = Self::try_acquire(record)? {
                return Ok(lock);
            }
            if std::time::Instant::now() >= deadline {
                return Err("Shared record busy; retry after reloading".into());
            }
            std::thread::sleep(Duration::from_millis(if catalog { 10 } else { 2 }));
        }
    }
    fn try_acquire(record: &Path) -> Result<Option<Self>, String> {
        let path = PathBuf::from(format!("{}.lock", record.display()));
        regular(&path)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            // flock is local-only on older SMB clients. OFD fcntl byte-range
            // locks interoperate with POSIX/Samba locks and survive unrelated
            // descriptor closes in our multithreaded worker pool.
            // SAFETY: flock64 is a plain C structure; zero initializes padding,
            // ranges and PID. The open file and structure live through syscall.
            let mut range: libc::flock64 = unsafe { std::mem::zeroed() };
            range.l_type = libc::F_WRLCK as libc::c_short;
            range.l_whence = libc::SEEK_SET as libc::c_short;
            #[cfg(target_pointer_width = "32")]
            let operation = libc::SYS_fcntl64;
            #[cfg(target_pointer_width = "64")]
            let operation = libc::SYS_fcntl;
            // Use the kernel's 64-bit lock layout on ARM32 as well as AMD64.
            let result =
                unsafe { libc::syscall(operation, file.as_raw_fd(), libc::F_OFD_SETLK, &range) };
            if result == 0 {
                return Ok(Some(Self { _file: file }));
            }
            let error = std::io::Error::last_os_error();
            if matches!(
                error.raw_os_error(),
                Some(libc::EACCES) | Some(libc::EAGAIN)
            ) {
                return Ok(None);
            }
            Err(format!(
                "Shared byte-range locking unavailable (Linux 3.15+ and compatible NAS mount required): {error}"
            ))
        }
        #[cfg(not(target_os = "linux"))]
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(e)) => Err(format!("NAS locking unavailable: {e}")),
        }
    }
}
pub fn atomic_write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let _migration = crate::catalog_db::location(path)
        .map(|(root, _)| Lock::acquire(&root.join("ampdj/catalog-migration")))
        .transpose()?;
    if crate::catalog_db::write(path, &serde_json::to_vec(value).map_err(|e| e.to_string())?)? {
        return Ok(());
    }
    regular(path)?;
    let temp = PathBuf::from(format!("{}.{}.tmp", path.display(), token()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temp, path).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        fs::File::open(path.parent().ok_or("Missing record parent")?)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    if let Some(bytes) = crate::catalog_db::read(path)? {
        return serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| e.to_string());
    }
    regular(path)?;
    match fs::read(path) {
        Ok(b) => serde_json::from_slice(&b)
            .map(Some)
            .map_err(|e| format!("Invalid shared record {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
#[derive(Serialize, Deserialize)]
struct Identity {
    version: u32,
    id: String,
}
fn identity(path: &Path) -> Result<String, String> {
    directory(path.parent().ok_or("Identity needs a parent")?)?;
    // Existing identities are immutable; avoid lock contention for ordinary reads.
    if let Some(id) = read::<Identity>(path)? {
        let supported = id.version == 1
            || (id.version == 2
                && path
                    .parent()
                    .is_some_and(|p| p.join("catalog.ampdb").is_file()));
        if !supported || uuid::Uuid::parse_str(&id.id).is_err() {
            return Err("Unsupported identity record".into());
        }
        return Ok(id.id);
    }
    let mut attempts = 0;
    let _lock = loop {
        match Lock::acquire(path) {
            Ok(lock) => break lock,
            Err(e) => {
                attempts += 1;
                if attempts >= 50 {
                    return Err(e);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    };
    if let Some(id) = read::<Identity>(path)? {
        return Ok(id.id);
    }
    let id = token();
    atomic_write(
        path,
        &Identity {
            version: 1,
            id: id.clone(),
        },
    )?;
    Ok(id)
}
pub fn library_id(root: &Path) -> Result<String, String> {
    if !root.is_dir() {
        return Err("Library unavailable; coordination paused".into());
    }
    crate::storage::enroll(root)?;
    identity(&root.join("ampdj/library.json"))
}
pub fn device_config_dir() -> Result<PathBuf, String> {
    #[cfg(test)]
    {
        static DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
        Ok(DIR
            .get_or_init(|| tempfile::tempdir().expect("test device directory"))
            .path()
            .to_path_buf())
    }
    #[cfg(not(test))]
    {
        let dir = std::env::var_os("AMP_DEVICE_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("amp-verse")))
            .or_else(|| {
                std::env::var_os("XDG_CONFIG_HOME").map(|p| PathBuf::from(p).join("amp-verse"))
            })
            .or_else(|| {
                std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config/amp-verse"))
            })
            .ok_or("Set AMP_DEVICE_CONFIG_DIR for this installation")?;
        Ok(dir)
    }
}

pub fn device_id() -> Result<String, String> {
    identity(&device_config_dir()?.join("device.json"))
}
pub fn server_id(config_dir: &Path) -> Result<String, String> {
    identity(&config_dir.join("server-id.json"))
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    pub revision: u64,
    pub device_id: String,
    pub library_id: String,
    pub fields: BTreeMap<String, u64>,
    #[serde(default)]
    pub field_devices: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_hash: Option<String>,
}
pub fn revision(value: &Value) -> Revision {
    value
        .get("_amp_sync")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}
pub fn adopt_legacy(value: &mut Value) -> Result<(), String> {
    if value.get("_amp_sync").is_none() {
        let hash = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(value).map_err(|e| e.to_string())?)
        );
        value.as_object_mut().ok_or("Expected object")?.insert(
            "_amp_sync".into(),
            serde_json::to_value(Revision {
                legacy_hash: Some(hash),
                ..Default::default()
            })
            .map_err(|e| e.to_string())?,
        );
    }
    Ok(())
}
fn checked_revision(value: &Value) -> Result<Revision, String> {
    let r = if let Some(sync) = value.get("_amp_sync") {
        serde_json::from_value::<Revision>(sync.clone())
            .map_err(|e| format!("Invalid revision record: {e}"))?
    } else {
        Revision::default()
    };
    if r.fields.values().any(|&v| v > r.revision) {
        return Err("Field revision exceeds record revision".into());
    }
    Ok(r)
}
fn fields(value: &Value) -> Result<serde_json::Map<String, Value>, String> {
    let mut map = value
        .as_object()
        .ok_or("Shared records must be objects")?
        .clone();
    map.remove("_amp_sync");
    Ok(map)
}
/// Three-way merge of independent JSON fields. Arrays (cue sets/order/rules)
/// are atomic fields: conflicting concurrent edits require explicit review.
pub fn merge(
    base: &Value,
    current: &Value,
    proposed: &Value,
    device: &str,
    library: &str,
) -> Result<Value, String> {
    let base_revision = checked_revision(base)?;
    let base = fields(base)?;
    let current_fields = fields(current)?;
    let proposed_fields = fields(proposed)?;
    let mut merged = current_fields.clone();
    let mut rev = checked_revision(current)?;
    if base_revision.revision > rev.revision {
        return Err(
            "Shared metadata is older than this snapshot; review recovery before saving".into(),
        );
    }
    if !rev.library_id.is_empty() && rev.library_id != library {
        return Err("Metadata belongs to a different Library ID".into());
    }
    let next = rev.revision.checked_add(1).ok_or("Revision overflow")?;
    let keys: std::collections::BTreeSet<_> = base.keys().chain(proposed_fields.keys()).collect();
    for key in keys {
        if proposed_fields.get(key) == base.get(key) {
            continue;
        }
        if rev.fields.get(key).copied().unwrap_or(0)
            > base_revision.fields.get(key).copied().unwrap_or(0)
            && current_fields.get(key) != proposed_fields.get(key)
        {
            return Err(format!("Conflict in {key}; its field revision changed"));
        }
        if current_fields.get(key) != base.get(key)
            && current_fields.get(key) != proposed_fields.get(key)
        {
            return Err(format!(
                "Conflict in {key}; shared revision {} retained",
                rev.revision
            ));
        }
        if let Some(value) = proposed_fields.get(key) {
            merged.insert(key.clone(), value.clone());
        } else {
            merged.remove(key);
        }
        rev.fields.insert(key.clone(), next);
        rev.field_devices.insert(key.clone(), device.into());
    }
    rev.legacy_hash = None;
    rev.revision = next;
    rev.device_id = device.into();
    rev.library_id = library.into();
    merged.insert(
        "_amp_sync".into(),
        serde_json::to_value(rev).map_err(|e| e.to_string())?,
    );
    Ok(Value::Object(merged))
}
/// Persist a failed explicit document save locally. Reconnection does not
/// publish this proposal; a future outbox UI must review and validate it.
pub fn retain_document(path: &Path, proposed: &Value, library: &str, error: String) -> String {
    let queued = (|| {
        let device = device_id()?;
        let file = device_config_dir()?
            .join("outbox")
            .join(format!("document-{}.json", token()));
        directory(file.parent().unwrap())?;
        atomic_write(
            &file,
            &serde_json::json!({"version":1,"kind":"document","library_id":library,"device_id":device,"target_hint":path,"base_revision":revision(proposed).revision,"proposed":proposed}),
        )?;
        Ok::<_, String>(file)
    })();
    match queued {
        Ok(file) => format!(
            "{error}. Local draft queued at {}; it will not synchronize automatically",
            file.display()
        ),
        Err(_) => error,
    }
}
/// Exact-revision publication for playlists/rule sets. A conflicting proposal is
/// retained as a separate file, never silently substituted for the current record.
pub fn save_document(path: &Path, proposed: &Value, library: &str) -> Result<(), String> {
    let parent = path.parent().ok_or("Document needs a parent")?;
    directory(parent)?;
    let device = device_id()?;
    let _lock = Lock::acquire(path)?;
    let current = read::<Value>(path)?.unwrap_or_else(|| serde_json::json!({}));
    let current_rev = checked_revision(&current)?;
    let proposed_rev = checked_revision(proposed)?;
    let exists = current.as_object().is_some_and(|m| !m.is_empty());
    let legacy_matches = proposed_rev.legacy_hash.as_ref().is_some_and(|h| {
        serde_json::to_vec(&current).is_ok_and(|b| format!("{:x}", Sha256::digest(b)) == *h)
    });
    if exists
        && (current_rev.revision != proposed_rev.revision
            || (current.get("_amp_sync").is_none() && !legacy_matches && proposed != &current))
    {
        let conflict = parent.join(format!(
            "{}.conflict-{}.json",
            path.file_name().unwrap_or_default().to_string_lossy(),
            token()
        ));
        let mut retained = proposed.clone();
        retained.as_object_mut().ok_or("Expected proposal object")?.insert("_amp_conflict".into(),serde_json::json!({"device_id":device,"shared_revision":current_rev.revision,"base_revision":proposed_rev.revision}));
        atomic_write(&conflict, &retained)?;
        return Err(format!(
            "Playlist/rules changed on another device; current version retained. Your proposal is saved at {}",
            conflict.display()
        ));
    }
    let value = merge(&current, &current, proposed, &device, library)?;
    atomic_write(path, &value)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Claim {
    library_id: String,
    device_id: String,
    analysis_type: String,
    token: String,
    generation: u64,
    expires_ms: u64,
    state: String,
}
#[derive(Clone)]
pub struct Lease {
    analysis_type: String,
    path: PathBuf,
    record: PathBuf,
    pub device: String,
    library: String,
    token: String,
    generation: u64,
    ttl_ms: u64,
}
#[derive(Debug, PartialEq)]
pub enum Skip {
    Busy,
    Complete,
    NonMusic,
}
pub enum Claimed {
    Owned(Lease),
    Skipped(Skip),
}
impl Lease {
    /// Serializes claims and metadata publication through the SAME track lock.
    pub fn claim(
        root: &Path,
        relative: &Path,
        analysis_type: &str,
        device: &str,
        ttl: Duration,
    ) -> Result<Claimed, String> {
        let (owner, path) =
            crate::music_sources::owner(root, relative).map_err(|e| e.to_string())?;
        if owner != root.canonicalize().map_err(|e| e.to_string())? {
            return Self::claim(&owner, &path, analysis_type, device, ttl);
        }
        crate::storage::enroll(root)?;
        let _migration = Lock::acquire(&root.join("ampdj/catalog-migration"))?;
        let library = library_id(root)?;
        let audio = crate::resolve(root, relative).map_err(|e| e.to_string())?;
        let record = crate::preparation::portable(&audio);
        directory(record.parent().unwrap().parent().unwrap())?;
        directory(record.parent().unwrap())?;
        let Some(_lock) = Lock::try_acquire(&record)? else {
            return Ok(Claimed::Skipped(Skip::Busy));
        };
        let hash = format!("{:x}", Sha256::digest(analysis_type.as_bytes()));
        let path = PathBuf::from(format!("{}.{}.lease", record.display(), hash));
        let previous = read::<Claim>(&path)?;
        let now = now()?;
        if previous
            .as_ref()
            .is_some_and(|c| c.state == "active" && c.expires_ms > now)
        {
            return Ok(Claimed::Skipped(Skip::Busy));
        }
        let generation = previous.map_or(Ok(1), |c| {
            c.generation
                .checked_add(1)
                .ok_or("Claim generation overflow")
        })?;
        let lease = Self {
            analysis_type: analysis_type.into(),
            path,
            record,
            device: device.into(),
            library,
            token: token(),
            generation,
            ttl_ms: ttl.as_millis().clamp(100, 3_600_000) as u64,
        };
        lease.write(now, "active")?;
        Ok(Claimed::Owned(lease))
    }
    fn write(&self, at: u64, state: &str) -> Result<(), String> {
        atomic_write(
            &self.path,
            &Claim {
                library_id: self.library.clone(),
                device_id: self.device.clone(),
                analysis_type: self.analysis_type.clone(),
                token: self.token.clone(),
                generation: self.generation,
                expires_ms: at.checked_add(self.ttl_ms).ok_or("Lease clock overflow")?,
                state: state.into(),
            },
        )
    }
    pub(crate) fn proposal_record(&self, candidate: &Value) -> (String, Value) {
        (
            self.token.clone(),
            serde_json::json!({"device_id":self.device,"analysis_type":self.analysis_type,"generation":self.generation,"proposal":candidate}),
        )
    }
    pub(crate) fn primary_analysis(&self) -> bool {
        self.analysis_type == crate::engine_api::JOB_TYPE
    }
    pub(crate) fn verify_record_locked(&self, record: &Path, library: &str) -> Result<(), String> {
        if self.record != record || self.library != library {
            return Err("Analysis lease belongs to a different track/library".into());
        }
        self.verify_locked()
    }
    pub(crate) fn verify_locked(&self) -> Result<(), String> {
        let claim: Claim = read(&self.path)?.ok_or("Analysis lease disappeared")?;
        if claim.state != "active"
            || claim.token != self.token
            || claim.generation != self.generation
            || claim.device_id != self.device
            || claim.library_id != self.library
            || claim.analysis_type != self.analysis_type
            || claim.expires_ms <= now()?
        {
            return Err("Analysis lease expired or ownership changed; result not published".into());
        }
        Ok(())
    }
    pub fn renew(&self) -> Result<(), String> {
        let _lock = Lock::acquire(&self.record)?;
        self.verify_locked()?;
        self.write(now()?, "active")
    }
    pub(crate) fn finish_locked(&self) -> Result<(), String> {
        self.verify_locked()?;
        self.write(now()?, "complete")
    }
    pub fn finish(&self) -> Result<(), String> {
        let _lock = Lock::acquire(&self.record)?;
        self.finish_locked()
    }
    #[cfg(feature = "analysis")]
    fn stamp(&self, p: &mut crate::Preparation) {
        if p.source_sha256.is_none() {
            p.source_sha256 = p
                .extensions
                .get("analysis_recommendation")
                .and_then(|r| r.get("source_sha256"))
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        p.extensions.insert(
            "analysis_engine_checked".into(),
            Value::String(crate::engine_api::ENGINE_ID.into()),
        );

        p.extensions.insert("analysis_origin".into(), serde_json::json!({"device_id": self.device,"library_id":self.library,"token":self.token,"generation":self.generation,"analysis_type":self.analysis_type}));
    }
    pub fn keep_alive(&self) -> Heartbeat {
        let signal = Arc::new((Mutex::new(false), Condvar::new()));
        let thread_signal = signal.clone();
        let lease = self.clone();
        let thread = std::thread::spawn(move || {
            let (lock, cv) = &*thread_signal;
            loop {
                let guard = lock.lock().unwrap();
                let (stop, _) = cv
                    .wait_timeout_while(
                        guard,
                        Duration::from_millis((lease.ttl_ms / 3).max(30)),
                        |stop| !*stop,
                    )
                    .unwrap();
                if *stop {
                    break;
                }
                drop(stop);
                // Errors fail closed at publication. Retry transient contention on next tick.
                let _ = lease.renew();
            }
        });
        Heartbeat {
            signal,
            thread: Some(thread),
        }
    }
}
pub struct Heartbeat {
    signal: Arc<(Mutex<bool>, Condvar)>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Heartbeat {
    fn drop(&mut self) {
        *self.signal.0.lock().unwrap() = true;
        self.signal.1.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Incremental, coordinated use of the existing analysis engine. This is shared
/// by Library, CLI and the headless server; no second NAS analyzer exists.
pub enum AnalysisResult {
    Prepared(Box<crate::Preparation>),
    Skipped(Skip),
}
#[cfg(feature = "analysis")]
fn source_stat(audio: &Path) -> Result<Value, String> {
    let m = fs::metadata(audio).map_err(|e| e.to_string())?;
    let ns = m
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos()
        .to_string();
    Ok(serde_json::json!({"size":m.len(),"modified_ns":ns}))
}
#[cfg(feature = "analysis")]
fn engine_version(engine: &str) -> Option<u64> {
    let (family, _) = crate::engine_api::ENGINE_ID.rsplit_once('-')?;
    engine.strip_prefix(&format!("{family}-"))?.parse().ok()
}
#[cfg(feature = "analysis")]
fn completed_version(p: &crate::Preparation) -> Option<u64> {
    p.analysis
        .as_ref()
        .and_then(|a| engine_version(&a.engine))
        .into_iter()
        .chain(
            p.extensions
                .get("analysis_engine_checked")
                .and_then(Value::as_str)
                .and_then(engine_version),
        )
        .max()
}
#[cfg(feature = "analysis")]
fn valid_engine(p: &crate::Preparation) -> bool {
    p.source_sha256.is_some()
        && completed_version(p)
            .is_some_and(|v| v >= engine_version(crate::engine_api::ENGINE_ID).unwrap())
}

#[cfg(feature = "analysis")]
pub fn analyze_shared(root: &Path, relative: &Path, force: bool) -> Result<AnalysisResult, String> {
    let (owner, path) = crate::music_sources::owner(root, relative).map_err(|e| e.to_string())?;
    if owner != root.canonicalize().map_err(|e| e.to_string())? {
        return analyze_shared(&owner, &path, force);
    }

    let audio = crate::resolve(root, relative).map_err(|e| e.to_string())?;
    let signature = source_stat(&audio)?;
    let cached = crate::preparation::load(&audio)?;
    if !force
        && valid_engine(&cached)
        && cached.extensions.get("analysis_source_stat") == Some(&signature)
    {
        return Ok(AnalysisResult::Skipped(Skip::Complete));
    }
    let device = device_id()?;
    let lease = match Lease::claim(
        root,
        relative,
        crate::engine_api::JOB_TYPE,
        &device,
        Duration::from_secs(120),
    )? {
        Claimed::Owned(l) => l,
        Claimed::Skipped(s) => return Ok(AnalysisResult::Skipped(s)),
    };
    let _heartbeat = lease.keep_alive();
    let audio = crate::resolve(root, relative).map_err(|e| e.to_string())?;
    let (mut before, bytes) = crate::preparation::load_with_revision(&audio)?;
    if !force {
        before.extensions.remove("media_kind_detected");
        if !before.extensions.contains_key("media_kind") {
            crate::genres::read_embedded(&audio, &mut before);
        }
        let kind = crate::media_kind::kind(&audio, &before);
        if kind != crate::media_kind::Kind::Music {
            before
                .extensions
                .insert("media_kind_detected".into(), serde_json::json!(kind.id()));
            lease.stamp(&mut before);
            crate::preparation::publish_claimed(root, relative, &before, bytes.as_deref(), &lease)?;
            return Ok(AnalysisResult::Skipped(Skip::NonMusic));
        }
    }
    if (!force
        || completed_version(&before)
            .is_some_and(|v| v > engine_version(crate::engine_api::ENGINE_ID).unwrap()))
        && valid_engine(&before)
        && before.source_sha256.as_ref().is_some_and(|s| {
            crate::preparation::fingerprint(&audio).is_ok_and(|actual| &actual == s)
        })
    {
        if before.extensions.get("analysis_source_stat") == Some(&source_stat(&audio)?) {
            lease.finish()?;
            return Ok(AnalysisResult::Skipped(Skip::Complete));
        }
        let mut cached = before.clone();
        cached
            .extensions
            .insert("analysis_source_stat".into(), source_stat(&audio)?);
        crate::preparation::publish_claimed(root, relative, &cached, bytes.as_deref(), &lease)?;
        return Ok(AnalysisResult::Skipped(Skip::Complete));
    }
    let mut result = crate::analysis::analyze(&audio, &before)?;
    lease.stamp(&mut result);
    result
        .extensions
        .insert("analysis_source_stat".into(), signature);
    crate::preparation::publish_claimed(root, relative, &result, bytes.as_deref(), &lease)?;
    Ok(AnalysisResult::Prepared(Box::new(
        crate::preparation::load(&audio)?,
    )))
}

/// Analyze an explicit editor draft under a shared claim, without publishing it.
#[cfg(feature = "analysis")]
pub fn analyze_draft(
    root: &Path,
    relative: &Path,
    draft: &crate::Preparation,
) -> Result<crate::Preparation, String> {
    let (owner, path) = crate::music_sources::owner(root, relative).map_err(|e| e.to_string())?;
    if owner != root.canonicalize().map_err(|e| e.to_string())? {
        return analyze_draft(&owner, &path, draft);
    }
    let device = device_id()?;
    let lease = match Lease::claim(
        root,
        relative,
        crate::engine_api::JOB_TYPE,
        &device,
        Duration::from_secs(120),
    )? {
        Claimed::Owned(lease) => lease,
        Claimed::Skipped(_) => {
            return Err(
                "Another device is analyzing this song; try again after it finishes".into(),
            );
        }
    };
    let _heartbeat = lease.keep_alive();
    let audio = crate::resolve(root, relative).map_err(|e| e.to_string())?;
    let mut result = crate::analysis::analyze(&audio, draft)?;
    lease.stamp(&mut result);
    result
        .extensions
        .insert("analysis_source_stat".into(), source_stat(&audio)?);
    lease.finish()?;
    Ok(result)
}

#[cfg(all(test, feature = "analysis"))]
mod tests {
    use super::*;
    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("song.wav"), b"audio-fixture").unwrap();
        library_id(root.path()).unwrap();
        root
    }
    fn claim(root: &Path, device: &str) -> Lease {
        match Lease::claim(
            root,
            Path::new("song.wav"),
            "test",
            device,
            Duration::from_secs(3),
        )
        .unwrap()
        {
            Claimed::Owned(l) => l,
            _ => panic!("expected ownership"),
        }
    }
    fn expire(lease: &Lease) {
        let _lock = Lock::acquire(&lease.record).unwrap();
        let mut c: Claim = read(&lease.path).unwrap().unwrap();
        c.expires_ms = 0;
        atomic_write(&lease.path, &c).unwrap();
    }
    #[cfg(feature = "analysis")]
    #[test]
    fn spoken_folder_is_classified_without_decoding_or_moving_audio() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("Speeches")).unwrap();
        let audio = root.path().join("Speeches/Talk.mp3");
        std::fs::write(&audio, b"not an audio codec fixture").unwrap();
        crate::catalog_db::compact(root.path()).unwrap();
        assert!(matches!(
            analyze_shared(root.path(), Path::new("Speeches/Talk.mp3"), false).unwrap(),
            AnalysisResult::Skipped(Skip::NonMusic)
        ));
        let saved = crate::preparation::load(&audio).unwrap();
        assert!(saved.analysis.is_none());
        assert_eq!(
            crate::media_kind::kind(Path::new("Speeches/Talk.mp3"), &saved),
            crate::media_kind::Kind::Speech
        );
        assert_eq!(std::fs::read(audio).unwrap(), b"not an audio codec fixture");
    }
    #[test]
    fn short_record_contention_retries_without_bypassing_the_lock() {
        let root = fixture();
        let record = root.path().join("record.json");
        let owner = Lock::acquire(&record).unwrap();
        assert!(Lock::try_acquire(&record).unwrap().is_none());
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(15));
            drop(owner);
        });
        let acquired = Lock::acquire(&record).unwrap();
        assert!(Lock::try_acquire(&record).unwrap().is_none());
        drop(acquired);
        release.join().unwrap();
    }
    #[test]
    fn unrelated_descriptor_close_does_not_release_another_workers_lock() {
        let root = fixture();
        let record = root.path().join("shared.json");
        let owner = Lock::acquire(&record).unwrap();
        let unrelated = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.path().join("shared.json.lock"))
            .unwrap();
        drop(unrelated);
        assert!(Lock::try_acquire(&record).unwrap().is_none());
        drop(owner);
        assert!(Lock::try_acquire(&record).unwrap().is_some());
    }
    #[test]
    fn identities_persist_and_libraries_remain_independent() {
        let a = fixture();
        let b = fixture();
        assert_eq!(library_id(a.path()).unwrap(), library_id(a.path()).unwrap());
        assert_ne!(library_id(a.path()).unwrap(), library_id(b.path()).unwrap());
        let id = a.path().join("device.json");
        assert_eq!(identity(&id).unwrap(), identity(&id).unwrap());
        assert_ne!(
            identity(&id).unwrap(),
            identity(&b.path().join("device.json")).unwrap()
        );
    }
    #[test]
    fn claim_race_has_one_owner_and_other_tracks_remain_available() {
        let root = fixture();
        fs::write(root.path().join("other.wav"), b"fixture").unwrap();
        let barrier = std::sync::Barrier::new(2);
        let results = std::thread::scope(|s| {
            let a = s.spawn(|| {
                barrier.wait();
                Lease::claim(
                    root.path(),
                    Path::new("song.wav"),
                    "test",
                    "device-a",
                    Duration::from_secs(3),
                )
                .unwrap()
            });
            let b = s.spawn(|| {
                barrier.wait();
                Lease::claim(
                    root.path(),
                    Path::new("song.wav"),
                    "test",
                    "device-b",
                    Duration::from_secs(3),
                )
                .unwrap()
            });
            [a.join().unwrap(), b.join().unwrap()]
        });
        assert_eq!(
            results
                .iter()
                .filter(|r| matches!(r, Claimed::Owned(_)))
                .count(),
            1
        );
        assert!(matches!(
            Lease::claim(
                root.path(),
                Path::new("other.wav"),
                "test",
                "device-b",
                Duration::from_secs(3)
            )
            .unwrap(),
            Claimed::Owned(_)
        ));
    }
    #[test]
    fn expired_worker_cannot_renew_or_publish_after_takeover() {
        let root = fixture();
        let old = claim(root.path(), "old");
        expire(&old);
        let new = claim(root.path(), "new");
        assert!(new.generation > old.generation);
        assert!(old.renew().is_err());
        let p = crate::Preparation {
            cue_in: 2.,
            manual_override: true,
            ..Default::default()
        };
        crate::preparation::publish_claimed(root.path(), Path::new("song.wav"), &p, None, &new)
            .unwrap();
        assert!(
            crate::preparation::publish_claimed(
                root.path(),
                Path::new("song.wav"),
                &crate::Preparation::default(),
                None,
                &old
            )
            .is_err()
        );
        assert_eq!(
            crate::preparation::load(&root.path().join("song.wav"))
                .unwrap()
                .cue_in,
            2.
        );
    }
    #[test]
    fn lease_cannot_publish_a_different_song() {
        let root = fixture();
        fs::write(root.path().join("other.wav"), b"fixture").unwrap();
        let lease = claim(root.path(), "device");
        assert!(
            crate::preparation::publish_claimed(
                root.path(),
                Path::new("other.wav"),
                &crate::Preparation::default(),
                None,
                &lease
            )
            .is_err()
        );
        assert!(crate::preparation::revision(&root.path().join("other.wav")).is_none());
    }
    #[test]
    fn unrelated_edits_merge_and_same_cue_conflicts() {
        let root = fixture();
        let rel = Path::new("song.wav");
        crate::preparation::save(root.path(), rel, &crate::Preparation::default()).unwrap();
        let (base, bytes) = crate::preparation::load_with_revision(&root.path().join(rel)).unwrap();
        let mut cue = base.clone();
        cue.cue_in = 2.;
        cue.manual_override = true;
        crate::preparation::save_if_unchanged(root.path(), rel, &cue, bytes.as_deref()).unwrap();
        let mut energy = base.clone();
        energy
            .extensions
            .insert("energy".into(), serde_json::json!(0.7));
        crate::preparation::save_if_unchanged(root.path(), rel, &energy, bytes.as_deref()).unwrap();
        let merged = crate::preparation::load(&root.path().join(rel)).unwrap();
        assert_eq!(merged.cue_in, 2.);
        assert_eq!(merged.extensions["energy"], 0.7);
        assert_eq!(
            revision(&serde_json::to_value(&merged).unwrap()).revision,
            3
        );
        let mut conflicting = base;
        conflicting.cue_in = 4.;
        assert!(
            crate::preparation::save_if_unchanged(root.path(), rel, &conflicting, bytes.as_deref())
                .is_err()
        );
        assert_eq!(
            crate::preparation::load(&root.path().join(rel)).unwrap(),
            merged
        );
    }
    #[test]
    fn approval_during_analysis_retains_current_and_stages_suggestion() {
        let root = fixture();
        let rel = Path::new("song.wav");
        let lease = claim(root.path(), "worker");
        let (base, bytes) = crate::preparation::load_with_revision(&root.path().join(rel)).unwrap();
        let mut manual = base.clone();
        manual.cue_in = 3.;
        manual.manual_override = true;
        manual
            .extensions
            .insert("loops".into(), serde_json::json!([1, 2]));
        crate::preparation::save_if_unchanged(root.path(), rel, &manual, bytes.as_deref()).unwrap();
        let mut automatic = base;
        automatic.cue_in = 1.;
        automatic.bpm = Some(120.);
        automatic.source_sha256 =
            Some(crate::preparation::fingerprint(&root.path().join(rel)).unwrap());
        lease.stamp(&mut automatic);
        automatic.extensions.insert(
            "analysis_source_stat".into(),
            source_stat(&root.path().join(rel)).unwrap(),
        );
        crate::preparation::publish_claimed(root.path(), rel, &automatic, bytes.as_deref(), &lease)
            .unwrap();
        let p = crate::preparation::load(&root.path().join(rel)).unwrap();
        assert_eq!(p.cue_in, 3.);
        assert_eq!(p.bpm, None);
        assert_eq!(p.extensions["loops"], serde_json::json!([1, 2]));
        assert_eq!(p.extensions["analysis_recommendation"]["cue_in"], 1.);
        assert!(matches!(
            analyze_shared(root.path(), rel, false).unwrap(),
            AnalysisResult::Skipped(Skip::Complete)
        ));
    }
    #[test]
    fn playlist_and_smart_rule_conflicts_preserve_both_versions() {
        let root = fixture();
        let path = root.path().join("playlist.json");
        let mut original = serde_json::json!({"version":1,"tracks":["song.wav"],"name":"Show"});
        save_document(&path, &original, "test").unwrap();
        original = read(&path).unwrap().unwrap();
        let mut a = original.clone();
        a["name"] = serde_json::json!("Device A");
        save_document(&path, &a, "test").unwrap();
        let mut b = original;
        b["name"] = serde_json::json!("Device B");
        assert!(save_document(&path, &b, "test").is_err());
        assert_eq!(read::<Value>(&path).unwrap().unwrap()["name"], "Device A");
        assert!(fs::read_dir(root.path()).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".conflict-")
        }));
        let rules = crate::show::Settings::default();
        rules.save(root.path()).unwrap();
        let mut a = crate::show::Settings::load(root.path()).unwrap();
        let mut b = a.clone();
        a.playlists[0].target_energy = Some(8.);
        a.save(root.path()).unwrap();
        b.playlists[0].target_energy = Some(3.);
        assert!(b.save(root.path()).is_err());
        assert_eq!(
            crate::show::Settings::load(root.path()).unwrap().playlists[0].target_energy,
            Some(8.)
        );
    }
    #[test]
    fn different_analysis_types_merge_without_replacing_each_other() {
        let root = fixture();
        let rel = Path::new("song.wav");
        let energy = match Lease::claim(
            root.path(),
            rel,
            "energy",
            "energy-device",
            Duration::from_secs(3),
        )
        .unwrap()
        {
            Claimed::Owned(l) => l,
            _ => panic!(),
        };
        let wave = match Lease::claim(
            root.path(),
            rel,
            "waveform",
            "wave-device",
            Duration::from_secs(3),
        )
        .unwrap()
        {
            Claimed::Owned(l) => l,
            _ => panic!(),
        };
        let mut a = crate::Preparation::default();
        a.extensions.insert("energy".into(), serde_json::json!(0.6));
        let mut b = crate::Preparation::default();
        b.extensions
            .insert("waveform".into(), serde_json::json!({"samples":[0.1,0.2]}));
        crate::preparation::publish_claimed(root.path(), rel, &a, None, &energy).unwrap();
        crate::preparation::publish_claimed(root.path(), rel, &b, None, &wave).unwrap();
        let saved = crate::preparation::load(&root.path().join(rel)).unwrap();
        assert_eq!(saved.extensions["energy"], 0.6);
        assert!(saved.extensions.contains_key("waveform"));
        let sync = revision(&serde_json::to_value(saved).unwrap());
        assert_eq!(sync.field_devices["energy"], "energy-device");
        assert_eq!(sync.field_devices["waveform"], "wave-device");
    }
    #[test]
    fn cue_edit_waveform_and_energy_proposals_coexist() {
        let root = fixture();
        let rel = Path::new("song.wav");
        let owner =
            |kind: &str| match Lease::claim(root.path(), rel, kind, kind, Duration::from_secs(3))
                .unwrap()
            {
                Claimed::Owned(l) => l,
                _ => panic!(),
            };
        let wave = owner("waveform");
        let energy = owner("energy");
        let mut manual = crate::Preparation {
            cue_in: 3.,
            manual_override: true,
            ..Default::default()
        };
        crate::preparation::save(root.path(), rel, &manual).unwrap();
        let mut a = crate::Preparation::default();
        a.extensions
            .insert("waveform".into(), serde_json::json!([0.1, 0.2]));
        crate::preparation::publish_claimed(root.path(), rel, &a, None, &wave).unwrap();
        let mut b = crate::Preparation::default();
        b.extensions.insert("energy".into(), serde_json::json!(0.6));
        crate::preparation::publish_claimed(root.path(), rel, &b, None, &energy).unwrap();
        let saved = crate::preparation::load(&root.path().join(rel)).unwrap();
        assert_eq!(saved.cue_in, 3.);
        assert_eq!(saved.extensions["waveform"], serde_json::json!([0.1, 0.2]));
        assert_eq!(saved.extensions["analysis_recommendation"]["cue_in"], 3.);
        assert_eq!(saved.extensions["analysis_recommendation"]["energy"], 0.6);
        manual = saved.clone();
        let after = owner("energy-next");
        let mut other = crate::Preparation::default();
        other
            .extensions
            .insert("energy".into(), serde_json::json!(0.8));
        crate::preparation::publish_claimed(root.path(), rel, &other, None, &after).unwrap();
        let latest = crate::preparation::load(&root.path().join(rel)).unwrap();
        assert_eq!(
            latest.extensions["analysis_proposals"]
                .as_object()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            latest.extensions["analysis_recommendation"],
            manual.extensions["analysis_recommendation"]
        );
    }
    #[test]
    fn older_engine_reuses_newer_completed_result_instead_of_downgrading_it() {
        let root = fixture();
        let rel = Path::new("song.wav");
        let audio = root.path().join(rel);
        let mut p = crate::Preparation {
            bpm: Some(126.),
            source_sha256: Some(crate::preparation::fingerprint(&audio).unwrap()),
            analysis: Some(crate::preparation::AnalysisInfo {
                engine: format!(
                    "amp-prep-heuristic-{}",
                    engine_version(crate::engine_api::ENGINE_ID).unwrap() + 1
                ),
                duration: 10.,
                tempo_confidence: 1.,
                key_confidence: 1.,
                reviewed: false,
            }),
            ..Default::default()
        };
        p.extensions
            .insert("analysis_source_stat".into(), source_stat(&audio).unwrap());
        crate::preparation::save(root.path(), rel, &p).unwrap();
        let before = crate::preparation::revision(&audio);
        assert!(matches!(
            analyze_shared(root.path(), rel, false).unwrap(),
            AnalysisResult::Skipped(Skip::Complete)
        ));
        assert!(matches!(
            analyze_shared(root.path(), rel, true).unwrap(),
            AnalysisResult::Skipped(Skip::Complete)
        ));
        assert_eq!(crate::preparation::revision(&audio), before);
    }
    #[test]
    fn legacy_rules_enroll_without_losing_concurrent_edit_protection() {
        let root = fixture();
        let file = root.path().join("ampdj/smart-playlists.json");
        fs::write(
            &file,
            serde_json::to_vec(&crate::show::Settings::default()).unwrap(),
        )
        .unwrap();
        let mut a = crate::show::Settings::load(root.path()).unwrap();
        let mut b = a.clone();
        a.playlists[0].name = "First".into();
        a.save(root.path()).unwrap();
        b.playlists[0].name = "Other".into();
        assert!(b.save(root.path()).is_err());
        assert_eq!(
            crate::show::Settings::load(root.path()).unwrap().playlists[0].name,
            "First"
        );
    }
    #[test]
    fn analysis_entry_point_never_bypasses_busy_or_broken_coordination() {
        let root = fixture();
        let rel = Path::new("song.wav");
        let approved = crate::Preparation {
            bpm: Some(127.0),
            ..Default::default()
        };
        crate::preparation::save(root.path(), rel, &approved).unwrap();
        let record = crate::preparation::load_with_revision(&root.path().join(rel))
            .unwrap()
            .1
            .unwrap();
        let lease = match Lease::claim(
            root.path(),
            rel,
            crate::engine_api::JOB_TYPE,
            "other-device",
            Duration::from_secs(120),
        )
        .unwrap()
        {
            Claimed::Owned(lease) => lease,
            _ => panic!("expected ownership"),
        };
        // Invalid fixture audio would fail decoding if either path bypassed the claim.
        assert!(matches!(
            analyze_shared(root.path(), rel, true).unwrap(),
            AnalysisResult::Skipped(_)
        ));
        assert_eq!(
            crate::preparation::load_with_revision(&root.path().join(rel))
                .unwrap()
                .1
                .unwrap(),
            record
        );
        fs::write(&lease.path, b"{interrupted").unwrap();
        assert!(analyze_shared(root.path(), rel, true).is_err());
        assert_eq!(
            crate::preparation::load_with_revision(&root.path().join(rel))
                .unwrap()
                .1
                .unwrap(),
            record
        );
    }
    #[test]
    fn disconnected_share_and_partial_records_fail_closed() {
        let root = fixture();
        let lease = claim(root.path(), "device");
        fs::rename(&lease.path, lease.path.with_extension("lost")).unwrap();
        assert!(lease.renew().is_err());
        assert!(
            crate::preparation::publish_claimed(
                root.path(),
                Path::new("song.wav"),
                &crate::Preparation::default(),
                None,
                &lease
            )
            .is_err()
        );
        fs::write(&lease.path, b"{partial").unwrap();
        assert!(
            Lease::claim(
                root.path(),
                Path::new("song.wav"),
                "test",
                "other",
                Duration::from_secs(3)
            )
            .is_err()
        );
        assert!(crate::preparation::revision(&root.path().join("song.wav")).is_none());
    }
    #[test]
    fn unavailable_library_rules_are_queued_locally_without_creating_a_share() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("disconnected");
        let mut settings = crate::show::Settings::default();
        let name = token();
        settings.playlists[0].name = name.clone();
        let error = settings.save(&missing).unwrap_err();
        assert!(error.contains("Local draft queued"));
        assert!(!missing.exists());
        let outbox = device_config_dir().unwrap().join("outbox");
        assert!(
            fs::read_dir(outbox)
                .unwrap()
                .filter_map(Result::ok)
                .filter_map(|f| read::<Value>(&f.path()).ok().flatten())
                .any(|v| v["kind"] == "document" && v["proposed"]["playlists"][0]["name"] == name)
        );
    }
    #[test]
    fn interrupted_temporary_file_does_not_replace_current_record() {
        let root = fixture();
        let path = root.path().join("record.json");
        atomic_write(&path, &serde_json::json!({"revision":2})).unwrap();
        fs::write(root.path().join("record.json.crashed.tmp"), b"incomplete").unwrap();
        assert_eq!(read::<Value>(&path).unwrap().unwrap()["revision"], 2);
    }
    #[test]
    #[ignore = "subprocess helper; invoked by process_recovery test"]
    fn child_claim_and_exit() {
        let Some(root) = std::env::var_os("AMP_COORD_TEST_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        assert!(matches!(
            Lease::claim(
                &root,
                Path::new("song.wav"),
                "test",
                "crashed-device",
                Duration::from_millis(150)
            )
            .unwrap(),
            Claimed::Owned(_)
        ));
        std::process::exit(0);
    }
    #[test]
    fn process_recovery_keeps_fencing_generation() {
        let root = fixture();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "coordination::tests::child_claim_and_exit",
                "--ignored",
            ])
            .env("AMP_COORD_TEST_ROOT", root.path())
            .status()
            .unwrap();
        assert!(status.success());
        let until = std::time::Instant::now() + Duration::from_secs(3);
        let lease = loop {
            match Lease::claim(
                root.path(),
                Path::new("song.wav"),
                "test",
                "recovery-device",
                Duration::from_secs(3),
            )
            .unwrap()
            {
                Claimed::Owned(lease) => break lease,
                Claimed::Skipped(_) => {
                    assert!(std::time::Instant::now() < until);
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        };
        assert_eq!(lease.generation, 2);
        lease.renew().unwrap();
    }
}
