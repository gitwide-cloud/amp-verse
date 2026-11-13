//! Embedded, portable SQLite documents. Never opens a writable SQLite connection on a NAS.
//! Updates edit a local copy under the shared publication lock, then replace the snapshot.
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn path(root: &Path) -> PathBuf {
    root.join("ampdj/catalog.ampdb")
}
/// Additional sources get their own portable catalog before automatic jobs write results.
pub fn ensure(root: &Path) -> Result<(), String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        if path(root).is_file() {
            return Ok(());
        }
        match compact(root) {
            Ok(_) => return Ok(()),
            Err(_) if path(root).is_file() => return Ok(()),
            Err(e) if e.contains("busy") && std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(100))
            }
            Err(e) => return Err(e),
        }
    }
}
/// Older JSON-only clients reject identity version 2 instead of creating stale metadata.
pub fn fence_legacy_clients(root: &Path) -> Result<(), String> {
    if !path(root).is_file() {
        return Err("No converted catalog to protect".into());
    }
    let file = root.join("ampdj/library.json");
    let _lock = crate::coordination::Lock::acquire(&file)?;
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&file).map_err(error)?).map_err(error)?;
    if !matches!(value["version"].as_u64(), Some(1 | 2))
        || value["id"]
            .as_str()
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .is_none()
    {
        return Err("Invalid library identity; unchanged".into());
    }
    value["version"] = serde_json::json!(2);
    value["store"] = serde_json::json!("catalog.ampdb");
    crate::coordination::atomic_write(&file, &value)
}
pub fn location(document: &Path) -> Option<(PathBuf, String)> {
    let root = crate::storage::root_for(document);
    let relative = document.strip_prefix(root.join("ampdj")).ok()?;
    let key = relative.to_string_lossy().replace('\\', "/");
    ((key.starts_with("tracks/")
        || key.starts_with("playlists/")
        || matches!(key.as_str(), "smart-playlists.json" | "music-sources.json"))
        && key.ends_with(".json"))
    .then_some((root, key))
}
fn open_copy(root: &Path) -> Result<(tempfile::TempDir, Connection), String> {
    let dir = tempfile::tempdir().map_err(error)?;
    let local = dir.path().join("catalog.ampdb");
    let existing = path(root).is_file();
    if existing {
        if fs::symlink_metadata(path(root))
            .map_err(error)?
            .file_type()
            .is_symlink()
        {
            return Err("Catalog cannot be a symbolic link".into());
        }
        fs::copy(path(root), &local).map_err(error)?;
    }
    let db = Connection::open(local).map_err(error)?;
    if existing {
        let version: i64 = db
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(error)?;
        if !matches!(version, 1 | 2) {
            return Err("Unsupported catalog version".into());
        }
    } else {
        db.execute_batch("CREATE TABLE records (key TEXT PRIMARY KEY, document TEXT NOT NULL); PRAGMA user_version=1;").map_err(error)?;
    }
    db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;")
        .map_err(error)?;
    Ok((dir, db))
}
/// Shared queue transactions edit only the small index snapshot, never a writable NAS DB.
pub(crate) fn queue_access<T>(
    root: &Path,
    write: bool,
    work: impl FnOnce(&mut Connection) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = crate::coordination::Lock::acquire(&root.join("ampdj/catalog-publication"))?;
    if write {
        crate::catalog_shards::migrate(root, true)?;
        let (dir, mut db) = open_copy(root)?;
        let value = work(&mut db)?;
        publish(root, dir, db)?;
        Ok(value)
    } else {
        let mut db = open_read(root)?;
        work(&mut db)
    }
}
// Readers hold the publication lock and open only a fully committed immutable snapshot.
// This reads indexed pages directly; it never creates a journal or writable NAS connection.
fn open_read(root: &Path) -> Result<Connection, String> {
    if fs::symlink_metadata(path(root))
        .map_err(error)?
        .file_type()
        .is_symlink()
    {
        return Err("Catalog cannot be a symbolic link".into());
    }
    let db = Connection::open_with_flags(path(root), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(error)?;
    let version: i64 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(error)?;
    if !matches!(version, 1 | 2) {
        return Err("Unsupported catalog version".into());
    }
    Ok(db)
}
pub fn read(document: &Path) -> Result<Option<Vec<u8>>, String> {
    let Some((root, key)) = location(document) else {
        return Ok(None);
    };
    if !path(&root).is_file() {
        return Ok(None);
    }
    let _guard = crate::coordination::Lock::acquire(&root.join("ampdj/catalog-publication"))?;
    if crate::catalog_shards::active(&root)? {
        return crate::catalog_shards::read(&root, &key);
    }
    let db = open_read(&root)?;
    db.query_row("SELECT document FROM records WHERE key=?1", [key], |r| {
        r.get::<_, String>(0)
    })
    .optional()
    .map(|s| s.map(String::into_bytes))
    .map_err(error)
}
pub fn write(document: &Path, bytes: &[u8]) -> Result<bool, String> {
    write_guarded(document, bytes, || Ok(()))
}
pub fn write_guarded(
    document: &Path,
    bytes: &[u8],
    verify: impl FnOnce() -> Result<(), String>,
) -> Result<bool, String> {
    let Some((root, key)) = location(document) else {
        return Ok(false);
    };
    if !path(&root).is_file() {
        return Ok(false);
    }
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(error)?;
    let _guard = crate::coordination::Lock::acquire(&root.join("ampdj/catalog-publication"))?;
    crate::catalog_shards::migrate(&root, false)?;
    if crate::catalog_shards::active(&root)? {
        crate::catalog_shards::write(&root, &key, &value.to_string(), verify)?;
        return Ok(true);
    }
    let (dir, db) = open_copy(&root)?;
    db.execute("INSERT INTO records(key,document) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET document=excluded.document", params![key,value.to_string()]).map_err(error)?;
    verify()?;
    publish(&root, dir, db)?;
    Ok(true)
}
pub fn preparations(
    root: &Path,
) -> Result<std::collections::BTreeMap<String, crate::Preparation>, String> {
    if !path(root).is_file() {
        return Ok(Default::default());
    }
    let _guard = crate::coordination::Lock::acquire(&root.join("ampdj/catalog-publication"))?;
    if crate::catalog_shards::active(root)? {
        return crate::catalog_shards::preparations(root);
    }
    let db = open_read(root)?;
    let mut query = db
        .prepare("SELECT key,document FROM records WHERE key LIKE 'tracks/%'")
        .map_err(error)?;
    let rows = query
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(error)?;
    let mut result = std::collections::BTreeMap::new();
    for row in rows {
        let (key, value) = row.map_err(error)?;
        let prep: crate::Preparation = serde_json::from_str(&value).map_err(error)?;
        prep.validate()?;
        result.insert(key, prep);
    }
    Ok(result)
}
pub fn playlist_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut result = Vec::new();
    if let Ok(entries) = fs::read_dir(root.join("ampdj/playlists")) {
        for entry in entries {
            let path = entry.map_err(error)?.path();
            if path.extension().is_some_and(|e| e == "json") {
                result.push(path);
            }
        }
    }
    if path(root).is_file() {
        let _guard = crate::coordination::Lock::acquire(&root.join("ampdj/catalog-publication"))?;
        let db = open_read(root)?;
        let mut query = db
            .prepare("SELECT key FROM records WHERE key LIKE 'playlists/%'")
            .map_err(error)?;
        for key in query
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(error)?
        {
            let key = key.map_err(error)?;
            if Path::new(&key)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
            {
                result.push(root.join("ampdj").join(key));
            }
        }
    }
    result.sort();
    result.dedup();
    Ok(result)
}
/// Device-local stem index. Audio remains in its managed cache; no per-song JSON is needed.
pub fn local_record(file: &Path, key: &str) -> Result<Option<serde_json::Value>, String> {
    if !file.is_file() {
        return Ok(None);
    }
    let db = Connection::open_with_flags(file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(error)?;
    let value: Option<String> = db
        .query_row("SELECT document FROM records WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()
        .map_err(error)?;
    value
        .map(|s| serde_json::from_str(&s).map_err(error))
        .transpose()
}
pub fn save_local_record(file: &Path, key: &str, value: &serde_json::Value) -> Result<(), String> {
    fs::create_dir_all(file.parent().ok_or("Index folder missing")?).map_err(error)?;
    let db = Connection::open(file).map_err(error)?;
    db.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(error)?;
    db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS records(key TEXT PRIMARY KEY,document TEXT NOT NULL);").map_err(error)?;
    db.execute("INSERT INTO records VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET document=excluded.document",params![key,value.to_string()]).map_err(error)?;
    Ok(())
}
fn publish(root: &Path, dir: tempfile::TempDir, db: Connection) -> Result<(), String> {
    let check: String = db
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(error)?;
    if check != "ok" {
        return Err("Catalog integrity check failed".into());
    }
    db.close().map_err(|(_, e)| error(e))?;
    let pending = root.join(format!(
        "ampdj/.catalog-{}.tmp",
        crate::coordination::token()
    ));
    let result = (|| {
        fs::copy(dir.path().join("catalog.ampdb"), &pending).map_err(error)?;
        fs::OpenOptions::new()
            .write(true)
            .open(&pending)
            .and_then(|f| f.sync_all())
            .map_err(error)?;
        fs::rename(&pending, path(root)).map_err(error)
    })();
    if result.is_err() {
        let _ = fs::remove_file(pending);
    }
    result
}
/// Explicit offline conversion. Imported values remain in the recovery_records table.
/// All clients must be upgraded before using this store; active playback is never migrated.
pub fn compact(root: &Path) -> Result<usize, String> {
    let canonical = root.canonicalize().map_err(error)?;
    let root = canonical.as_path();
    crate::storage::enroll(root)?;
    crate::coordination::library_id(root)?;
    let _migration = crate::coordination::Lock::acquire(&root.join("ampdj/catalog-migration"))?;
    let _guard = crate::coordination::Lock::acquire(&root.join("ampdj/catalog-publication"))?;
    if path(root).exists() {
        return Err("This library already uses a catalog database".into());
    }
    fn collect(dir: &Path, root: &Path, rows: &mut Vec<(String, String)>) -> Result<(), String> {
        if !dir.is_dir() {
            return Ok(());
        }
        for e in fs::read_dir(dir).map_err(error)? {
            let e = e.map_err(error)?;
            let kind = e.file_type().map_err(error)?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                collect(&e.path(), root, rows)?;
            } else if e.path().extension().is_some_and(|s| s == "lease") {
                let value: serde_json::Value =
                    serde_json::from_slice(&fs::read(e.path()).map_err(error)?).map_err(error)?;
                if value["state"] == "active"
                    && value["expires_ms"].as_u64().unwrap_or(u64::MAX)
                        > crate::coordination::now()?
                {
                    return Err("Stop all analysis workers before catalog conversion".into());
                }
            } else if e.path().extension().is_some_and(|s| s == "json") {
                let value: serde_json::Value =
                    serde_json::from_slice(&fs::read(e.path()).map_err(error)?).map_err(error)?;
                rows.push((
                    e.path()
                        .strip_prefix(root.join("ampdj"))
                        .map_err(error)?
                        .to_string_lossy()
                        .replace('\\', "/"),
                    value.to_string(),
                ));
            }
        }
        Ok(())
    }
    let mut rows = Vec::new();
    collect(&root.join("ampdj/tracks"), root, &mut rows)?;
    collect(&root.join("ampdj/playlists"), root, &mut rows)?;
    let rules = root.join("ampdj/smart-playlists.json");
    if rules.is_file() {
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(rules).map_err(error)?).map_err(error)?;
        rows.push(("smart-playlists.json".into(), value.to_string()));
    }
    let sources = root.join("ampdj/music-sources.json");
    if sources.is_file() {
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(sources).map_err(error)?).map_err(error)?;
        rows.push(("music-sources.json".into(), value.to_string()));
    }
    fn recovery_json(
        dir: &Path,
        rows: &mut Vec<(PathBuf, serde_json::Value)>,
    ) -> Result<(), String> {
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(dir).map_err(error)? {
            let entry = entry.map_err(error)?;
            let kind = entry.file_type().map_err(error)?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                recovery_json(&entry.path(), rows)?;
            } else if entry.path().extension().is_some_and(|s| s == "json") {
                rows.push((
                    entry.path(),
                    serde_json::from_slice(&fs::read(entry.path()).map_err(error)?)
                        .map_err(error)?,
                ));
            }
        }
        Ok(())
    }
    let mut legacy = Vec::new();
    for directory in [
        "ampdj/legacy",
        "ampdj/backups",
        ".ampverse/tracks",
        ".ampverse/backups",
    ] {
        recovery_json(&root.join(directory), &mut legacy)?;
    }
    let old_rules = root.join(".ampverse/show.json");
    if old_rules.is_file() {
        legacy.push((
            old_rules.clone(),
            serde_json::from_slice(&fs::read(old_rules).map_err(error)?).map_err(error)?,
        ));
    }
    let (dir, mut db) = open_copy(root)?;
    let transaction = db.transaction().map_err(error)?;
    for (key, document) in &rows {
        transaction
            .execute("INSERT INTO records VALUES(?1,?2)", params![key, document])
            .map_err(error)?;
    }
    transaction
        .execute_batch("CREATE TABLE recovery_records AS SELECT key,document FROM records;")
        .map_err(error)?;
    for (file, value) in &legacy {
        let key = format!(
            "legacy/{}",
            file.strip_prefix(root)
                .map_err(error)?
                .to_string_lossy()
                .replace('\\', "/")
        );
        transaction
            .execute(
                "INSERT INTO recovery_records VALUES(?1,?2)",
                params![key, value.to_string()],
            )
            .map_err(error)?;
    }
    transaction.commit().map_err(error)?;
    publish(root, dir, db)?;
    fence_legacy_clients(root)?;
    // New clients now use the catalog, and old JSON clients are fenced. Retiring
    // imported loose files must not block new index reads/writes for the whole cleanup.
    drop(_guard);
    drop(_migration);
    // The immutable recovery table retains the imported values before retiring loose JSON.
    // Never remove lock/lease files, source music, or a file changed by an older client.
    for (key, document) in &rows {
        let file = root.join("ampdj").join(key);
        let current: serde_json::Value =
            serde_json::from_slice(&fs::read(&file).map_err(error)?).map_err(error)?;
        if current != serde_json::from_str::<serde_json::Value>(document).map_err(error)? {
            return Err("An older client changed JSON during conversion; that file was retained. Close older clients and review the catalog recovery table.".into());
        }
        fs::remove_file(file).map_err(error)?;
    }
    for (file, value) in &legacy {
        let current: serde_json::Value =
            serde_json::from_slice(&fs::read(file).map_err(error)?).map_err(error)?;
        if current != *value {
            return Err("A legacy backup changed during conversion; it was retained".into());
        }
        fs::remove_file(file).map_err(error)?;
    }
    Ok(rows.len())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fresh_catalog_keeps_identity_and_demo_without_analysis() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(compact(root.path()).unwrap(), 0);
        let id = crate::coordination::library_id(root.path()).unwrap();
        assert_eq!(crate::coordination::library_id(root.path()).unwrap(), id);
        let settings = crate::show::Settings::load(root.path()).unwrap();
        assert_eq!(settings.playlists.len(), 1);
        assert_eq!(settings.playlists[0].name, "Demo");
    }
    #[test]
    fn local_database_retains_revisions_and_cues_without_a_server() {
        let root = tempfile::tempdir().unwrap();
        let rel = Path::new("song.wav");
        let audio = root.path().join(rel);
        fs::write(&audio, b"music").unwrap();
        let mut prep = crate::Preparation {
            cue_in: 2.,
            manual_override: true,
            ..Default::default()
        };
        crate::preparation::save(root.path(), rel, &prep).unwrap();
        assert_eq!(compact(root.path()).unwrap(), 1);
        assert!(!crate::preparation::portable(&audio).is_file());
        let (_dir, db) = open_copy(root.path()).unwrap();
        let recovery: String = db
            .query_row("SELECT document FROM recovery_records", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<crate::Preparation>(&recovery)
                .unwrap()
                .cue_in,
            2.
        );
        drop(db);
        let (_, base) = crate::preparation::load_with_revision(&audio).unwrap();
        prep = crate::preparation::load(&audio).unwrap();
        prep.cue_in = 3.;
        crate::preparation::save_if_unchanged(root.path(), rel, &prep, base.as_deref()).unwrap();
        assert_eq!(crate::preparation::load(&audio).unwrap().cue_in, 3.);
        assert!(
            write_guarded(
                &crate::preparation::portable(&audio),
                br#"{"cue_in":9}"#,
                || Err("lease expired".into())
            )
            .is_err()
        );
        assert_eq!(crate::preparation::load(&audio).unwrap().cue_in, 3.);
        prep.cue_in = 4.;
        assert!(
            crate::preparation::save_if_unchanged(root.path(), rel, &prep, base.as_deref())
                .is_err()
        );
        assert_eq!(crate::preparation::load(&audio).unwrap().cue_in, 3.);
        assert_eq!(fs::read(audio).unwrap(), b"music");
    }
}
