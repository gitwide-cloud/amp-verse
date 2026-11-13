//! Immutable 500-song snapshots, selected by one atomically published index.
//! Callers hold catalog-publication. A crash before index publication leaves only
//! an unreferenced snapshot; readers continue seeing the previous complete revision.
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    fs,
    path::{Path, PathBuf},
};
const LIMIT: i64 = 500;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn read_db(file: &Path) -> Result<Connection, String> {
    if fs::symlink_metadata(file)
        .map_err(err)?
        .file_type()
        .is_symlink()
    {
        return Err("Catalog snapshot cannot be a symbolic link".into());
    }
    Connection::open_with_flags(file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(err)
}
pub fn active(root: &Path) -> Result<bool, String> {
    let db = read_db(&super::catalog_db::path(root))?;
    let v: i64 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(err)?;
    match v {
        1 => Ok(false),
        2 => Ok(true),
        _ => Err("Unsupported catalog version".into()),
    }
}
fn snapshot(root: &Path, name: &str) -> Result<PathBuf, String> {
    if !name.starts_with("songs-")
        || !name.ends_with(".ampdb")
        || Path::new(name).components().count() != 1
    {
        return Err("Invalid song snapshot location".into());
    }
    Ok(root.join("ampdj/catalogs").join(name))
}
fn publish(local: &Path, target: &Path) -> Result<(), String> {
    let db = read_db(local)?;
    let check: String = db
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(err)?;
    if check != "ok" {
        return Err("Catalog snapshot integrity failure".into());
    }
    drop(db);
    fs::create_dir_all(target.parent().ok_or("Catalog folder missing")?).map_err(err)?;
    let pending = target.with_extension(format!("{}.pending", crate::coordination::token()));
    let result = (|| {
        fs::copy(local, &pending).map_err(err)?;
        fs::OpenOptions::new()
            .write(true)
            .open(&pending)
            .and_then(|f| f.sync_all())
            .map_err(err)?;
        super::catalog_db::replace_snapshot(&pending, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(pending);
    }
    result
}
fn records(file: &Path) -> Result<Connection, String> {
    let db = Connection::open(file).map_err(err)?;
    db.execute_batch("PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS records(key TEXT PRIMARY KEY,document TEXT NOT NULL); PRAGMA user_version=1;").map_err(err)?;
    Ok(db)
}
/// A one-time bounded streaming conversion. The full original is retained for recovery.
pub fn migrate(root: &Path, force: bool) -> Result<(), String> {
    if active(root)? {
        return Ok(());
    }
    let source = read_db(&super::catalog_db::path(root))?;
    let n: i64 = source
        .query_row(
            "SELECT COUNT(*) FROM records WHERE key LIKE 'tracks/%'",
            [],
            |r| r.get(0),
        )
        .map_err(err)?;
    if !force && n < LIMIT {
        return Ok(());
    }
    let work = tempfile::tempdir().map_err(err)?;
    let index_file = work.path().join("index.ampdb");
    let index = records(&index_file)?;
    index.execute_batch("CREATE TABLE locations(key TEXT PRIMARY KEY,shard INTEGER NOT NULL); CREATE TABLE shards(id INTEGER PRIMARY KEY,file TEXT NOT NULL,count INTEGER NOT NULL); PRAGMA user_version=2; BEGIN;").map_err(err)?;
    index
        .execute_batch(crate::analysis_queue::SCHEMA)
        .map_err(err)?;
    let mut q = source
        .prepare("SELECT key,document FROM records ORDER BY key")
        .map_err(err)?;
    let mut shard: Option<Connection> = None;
    let mut id = 0;
    let mut count = 0;
    let mut shard_file = PathBuf::new();
    let mut shard_name = String::new();
    let flush = |db: Option<Connection>,
                 id: i64,
                 count: i64,
                 file: &Path,
                 name: &str|
     -> Result<(), String> {
        if let Some(db) = db {
            db.execute_batch("COMMIT").map_err(err)?;
            db.close().map_err(|(_, e)| err(e))?;
            publish(file, &snapshot(root, name)?)?;
            index
                .execute(
                    "INSERT INTO shards VALUES(?1,?2,?3)",
                    params![id, name, count],
                )
                .map_err(err)?;
        }
        Ok(())
    };
    for row in q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(err)?
    {
        let (key, document) = row.map_err(err)?;
        if key.starts_with("tracks/") {
            let cached: serde_json::Value = serde_json::from_str(&document).map_err(err)?;
            let prepared: crate::Preparation =
                serde_json::from_value(cached.clone()).map_err(err)?;
            if crate::media_kind::is_music(
                Path::new(key.strip_prefix("tracks/").unwrap_or(&key)),
                &prepared,
            ) && cached["analysis"].is_object()
                && cached["source_sha256"].is_string()
                && let Some(size) = cached["analysis_source_stat"]["size"].as_u64()
                && let Some(modified) = cached["analysis_source_stat"]["modified_ns"].as_str()
                && let Some(path) = key
                    .strip_prefix("tracks/")
                    .and_then(|p| p.strip_suffix(".json"))
            {
                index
                    .execute(
                        "INSERT INTO analysis_jobs(path,stamp,state) VALUES(?1,?2,'done')",
                        params![path, format!("{size}:{modified}")],
                    )
                    .map_err(err)?;
            }
            if shard.is_none() || count == LIMIT {
                flush(shard.take(), id, count, &shard_file, &shard_name)?;
                id += 1;
                count = 0;
                shard_name = format!("songs-{id:06}-{}.ampdb", crate::coordination::token());
                shard_file = work.path().join(&shard_name);
                let db = records(&shard_file)?;
                db.execute_batch("BEGIN").map_err(err)?;
                shard = Some(db);
            }
            shard
                .as_ref()
                .unwrap()
                .execute("INSERT INTO records VALUES(?1,?2)", params![key, document])
                .map_err(err)?;
            index
                .execute("INSERT INTO locations VALUES(?1,?2)", params![key, id])
                .map_err(err)?;
            count += 1;
        } else {
            index
                .execute("INSERT INTO records VALUES(?1,?2)", params![key, document])
                .map_err(err)?;
        }
    }
    drop(q);
    drop(source);
    flush(shard.take(), id, count, &shard_file, &shard_name)?;
    index.execute_batch("COMMIT").map_err(err)?;
    index.close().map_err(|(_, e)| err(e))?;
    let backup = root.join(format!(
        "ampdj/catalog-before-shards-{}.ampdb",
        crate::coordination::token()
    ));
    fs::copy(super::catalog_db::path(root), &backup).map_err(err)?;
    fs::OpenOptions::new()
        .write(true)
        .open(&backup)
        .and_then(|f| f.sync_all())
        .map_err(err)?;
    publish(&index_file, &super::catalog_db::path(root))
}
pub fn read(root: &Path, key: &str) -> Result<Option<Vec<u8>>, String> {
    let index = read_db(&super::catalog_db::path(root))?;
    let db = if key.starts_with("tracks/") {
        let file:Option<String>=index.query_row("SELECT shards.file FROM locations JOIN shards ON locations.shard=shards.id WHERE locations.key=?1",[key],|r|r.get(0)).optional().map_err(err)?;
        let Some(file) = file else { return Ok(None) };
        read_db(&snapshot(root, &file)?)?
    } else {
        index
    };
    db.query_row("SELECT document FROM records WHERE key=?1", [key], |r| {
        r.get::<_, String>(0)
    })
    .optional()
    .map(|v| v.map(String::into_bytes))
    .map_err(err)
}
pub fn write(
    root: &Path,
    key: &str,
    value: &str,
    verify: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let work = tempfile::tempdir().map_err(err)?;
    let local = work.path().join("index.ampdb");
    fs::copy(super::catalog_db::path(root), &local).map_err(err)?;
    let index = Connection::open(&local).map_err(err)?;
    let mut retired = None;
    if key.starts_with("tracks/") {
        let known: Option<i64> = index
            .query_row("SELECT shard FROM locations WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(err)?;
        let id = if let Some(id) = known {
            id
        } else {
            index
                .query_row(
                    "SELECT id FROM shards WHERE count<500 ORDER BY id DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(err)?
                .unwrap_or(
                    index
                        .query_row("SELECT COALESCE(MAX(id),0)+1 FROM shards", [], |r| r.get(0))
                        .map_err(err)?,
                )
        };
        let old: Option<String> = index
            .query_row("SELECT file FROM shards WHERE id=?1", [id], |r| r.get(0))
            .optional()
            .map_err(err)?;
        let name = format!("songs-{id:06}-{}.ampdb", crate::coordination::token());
        let song_file = work.path().join(&name);
        if let Some(old) = old {
            let path = snapshot(root, &old)?;
            fs::copy(&path, &song_file).map_err(err)?;
            retired = Some(path);
        }
        let songs = records(&song_file)?;
        songs.execute("INSERT INTO records VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET document=excluded.document",params![key,value]).map_err(err)?;
        let count: i64 = songs
            .query_row("SELECT COUNT(*) FROM records", [], |r| r.get(0))
            .map_err(err)?;
        if count > LIMIT {
            return Err("Song catalog capacity exceeded".into());
        }
        songs.close().map_err(|(_, e)| err(e))?;
        index.execute("INSERT INTO locations VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET shard=excluded.shard",params![key,id]).map_err(err)?;
        index.execute("INSERT INTO shards VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET file=excluded.file,count=excluded.count",params![id,name,count]).map_err(err)?;
        verify()?;
        publish(&song_file, &snapshot(root, &name)?)?;
    } else {
        index.execute("INSERT INTO records VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET document=excluded.document",params![key,value]).map_err(err)?;
        verify()?;
    }
    index.close().map_err(|(_, e)| err(e))?;
    publish(&local, &super::catalog_db::path(root))?;
    if let Some(path) = retired {
        let _ = fs::remove_file(path);
    }
    Ok(())
}
pub fn preparations(
    root: &Path,
) -> Result<std::collections::BTreeMap<String, crate::Preparation>, String> {
    let index = read_db(&super::catalog_db::path(root))?;
    let mut result = std::collections::BTreeMap::new();
    let mut q = index
        .prepare("SELECT file FROM shards ORDER BY id")
        .map_err(err)?;
    for row in q.query_map([], |r| r.get::<_, String>(0)).map_err(err)? {
        let db = read_db(&snapshot(root, &row.map_err(err)?)?)?;
        let mut q = db
            .prepare("SELECT key,document FROM records")
            .map_err(err)?;
        for row in q
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(err)?
        {
            let (key, text) = row.map_err(err)?;
            let p: crate::Preparation = serde_json::from_str(&text).map_err(err)?;
            p.validate()?;
            result.insert(key, p);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn five_thousand_songs_roll_over_without_a_catalog_count_limit() {
        let root = tempfile::tempdir().unwrap();
        crate::storage::register(root.path());
        fs::create_dir(root.path().join("ampdj")).unwrap();
        let db = records(&crate::catalog_db::path(root.path())).unwrap();
        let prep = serde_json::to_string(&crate::Preparation::default()).unwrap();
        db.execute_batch("BEGIN").unwrap();
        for i in 0..5001 {
            db.execute(
                "INSERT INTO records VALUES(?1,?2)",
                params![format!("tracks/{i}.mp3.json"), prep],
            )
            .unwrap();
        }
        let mut seeded: serde_json::Value = serde_json::from_str(&prep).unwrap();
        seeded["analysis"] = serde_json::json!({"engine":crate::engine_api::ENGINE_ID,"duration":10.,"tempo_confidence":0.8,"key_confidence":0.8,"reviewed":false});
        seeded["source_sha256"] = serde_json::json!("a".repeat(64));
        seeded["analysis_source_stat"] = serde_json::json!({"size":5,"modified_ns":"1234"});
        db.execute(
            "UPDATE records SET document=?1 WHERE key='tracks/5000.mp3.json'",
            [seeded.to_string()],
        )
        .unwrap();
        db.execute(
            "INSERT INTO records VALUES('smart-playlists.json','{}')",
            [],
        )
        .unwrap();
        db.execute_batch("COMMIT").unwrap();
        drop(db);
        let before = fs::read(crate::catalog_db::path(root.path())).unwrap();
        let _guard =
            crate::coordination::Lock::acquire(&root.path().join("ampdj/catalog-publication"))
                .unwrap();
        migrate(root.path(), true).unwrap();
        assert!(active(root.path()).unwrap());
        let index = read_db(&crate::catalog_db::path(root.path())).unwrap();
        let n: i64 = index
            .query_row("SELECT COUNT(*) FROM shards", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 11);
        assert_eq!(
            index
                .query_row(
                    "SELECT state FROM analysis_jobs WHERE path='5000.mp3'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "done"
        );
        let mut query = index.prepare("SELECT file,count FROM shards").unwrap();
        let shards: Vec<(String, i64)> = query
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        drop(query);
        drop(index);
        for (name, count) in &shards {
            assert!(*count <= 500);
            let db = read_db(&snapshot(root.path(), name).unwrap()).unwrap();
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM records", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                *count
            );
        }
        assert_eq!(preparations(root.path()).unwrap().len(), 5001);
        let backups: Vec<_> = fs::read_dir(root.path().join("ampdj"))
            .unwrap()
            .map(Result::unwrap)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("catalog-before-shards-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(backups[0].path()).unwrap(), before);
        assert_eq!(
            String::from_utf8(read(root.path(), "smart-playlists.json").unwrap().unwrap()).unwrap(),
            "{}"
        );
        let mut p = crate::Preparation {
            bpm: Some(123.),
            manual_override: true,
            ..Default::default()
        };
        p.manual_cues.push(crate::preparation::Cue {
            id: "manual-drop".into(),
            seconds: 3.,
            label: "Keep cue".into(),
            ..Default::default()
        });
        write(
            root.path(),
            "tracks/0.mp3.json",
            &serde_json::to_string(&p).unwrap(),
            || Ok(()),
        )
        .unwrap();
        let current = fs::read(crate::catalog_db::path(root.path())).unwrap();
        assert!(
            write(root.path(), "tracks/0.mp3.json", "{}", || Err(
                "lease expired".into()
            ))
            .is_err()
        );
        assert_eq!(
            fs::read(crate::catalog_db::path(root.path())).unwrap(),
            current
        );
        assert_eq!(
            serde_json::from_slice::<crate::Preparation>(
                &read(root.path(), "tracks/0.mp3.json").unwrap().unwrap()
            )
            .unwrap(),
            p
        );
        for (name, _) in shards.iter().skip(1) {
            assert!(
                snapshot(root.path(), name).unwrap().is_file(),
                "Unrelated snapshot replaced"
            );
        }
        write(root.path(), "tracks/new.mp3.json", &prep, || Ok(())).unwrap();
        assert_eq!(preparations(root.path()).unwrap().len(), 5002);
    }
}
