//! One local SQLite archive containing prepared stem audio and its manifest.
//! BLOBs are streamed in bounded chunks; no separation engine is needed to restore them.
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering::Relaxed},
};
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn store(
    file: &Path,
    key: &str,
    manifest: &serde_json::Value,
    folder: &Path,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut db = Connection::open(file).map_err(error)?;
    db.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(error)?;
    db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS records(key TEXT PRIMARY KEY,document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS stem_audio(stem TEXT NOT NULL,name TEXT NOT NULL,audio BLOB NOT NULL,UNIQUE(stem,name));").map_err(error)?;
    let tx = db.transaction().map_err(error)?;
    for name in ["vocals", "instrumental", "drums"] {
        let mut input = fs::File::open(folder.join(format!("{name}.wav"))).map_err(error)?;
        let size = input.metadata().map_err(error)?.len();
        if size > 900_000_000 {
            return Err("Stem is too large for this archive format".into());
        }
        tx.execute(
            "INSERT OR REPLACE INTO stem_audio(stem,name,audio) VALUES(?1,?2,zeroblob(?3))",
            params![key, name, size],
        )
        .map_err(error)?;
        let row = tx.last_insert_rowid();
        let mut blob = tx
            .blob_open("main", "stem_audio", "audio", row, false)
            .map_err(error)?;
        let mut bytes = [0; 65536];
        loop {
            if cancel.load(Relaxed) {
                return Err("Stem archive cancelled".into());
            }
            let n = input.read(&mut bytes).map_err(error)?;
            if n == 0 {
                break;
            }
            blob.write_all(&bytes[..n]).map_err(error)?;
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO records VALUES(?1,?2)",
        params![key, manifest.to_string()],
    )
    .map_err(error)?;
    tx.commit().map_err(error)
}
pub fn has_name(file: &Path, name: &str) -> bool {
    if !file.is_file() {
        return false;
    }
    let result = (|| -> Result<bool, String> {
        let db = Connection::open_with_flags(file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(error)?;
        db.query_row(
            "SELECT 1 FROM records WHERE json_extract(document,'$.source_name')=?1 LIMIT 1",
            [name],
            |_| Ok(true),
        )
        .optional()
        .map(|v| v.unwrap_or(false))
        .map_err(error)
    })();
    result.unwrap_or(false)
}
pub fn contains(file: &Path, key: &str) -> bool {
    if !file.is_file() {
        return false;
    }
    let result = (|| -> Result<bool, String> {
        let db = Connection::open_with_flags(file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(error)?;
        db.query_row(
            "SELECT 1 FROM stem_audio WHERE stem=?1 LIMIT 1",
            [key],
            |_| Ok(true),
        )
        .optional()
        .map(|v| v.unwrap_or(false))
        .map_err(error)
    })();
    result.unwrap_or(false)
}
pub fn restore(
    file: &Path,
    source_hash: &str,
    folder: &Path,
) -> Result<Option<serde_json::Value>, String> {
    if !file.is_file() {
        return Ok(None);
    }
    let db = Connection::open_with_flags(file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(error)?;
    let row:Option<(String,String)>=db.query_row("SELECT key,document FROM records WHERE json_extract(document,'$.source_sha256')=?1 AND key IN (SELECT stem FROM stem_audio) LIMIT 1",[source_hash],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(error)?;
    let Some((key, document)) = row else {
        return Ok(None);
    };
    fs::create_dir_all(folder).map_err(error)?;
    for name in ["vocals", "instrumental", "drums"] {
        let row: i64 = db
            .query_row(
                "SELECT rowid FROM stem_audio WHERE stem=?1 AND name=?2",
                params![key, name],
                |r| r.get(0),
            )
            .map_err(error)?;
        let mut blob = db
            .blob_open("main", "stem_audio", "audio", row, true)
            .map_err(error)?;
        if blob.len() > 900_000_000 {
            return Err("Archived stem is too large".into());
        }
        let mut output = fs::File::create(folder.join(format!("{name}.wav"))).map_err(error)?;
        std::io::copy(&mut blob, &mut output).map_err(error)?;
        output.sync_all().map_err(error)?;
    }
    serde_json::from_str(&document).map(Some).map_err(error)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_audio_restores_without_separator_and_cancel_rolls_back() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        for name in ["vocals", "instrumental", "drums"] {
            fs::write(source.join(format!("{name}.wav")), name.as_bytes()).unwrap();
        }
        let file = root.path().join("stems.ampdb");
        let manifest = serde_json::json!({"source_sha256":"hash","source_name":"song.wav"});
        store(&file, "song", &manifest, &source, &AtomicBool::new(false)).unwrap();
        assert!(has_name(&file, "song.wav"));
        let restored = root.path().join("restored");
        assert_eq!(
            restore(&file, "hash", &restored).unwrap(),
            Some(manifest.clone())
        );
        assert_eq!(fs::read(restored.join("vocals.wav")).unwrap(), b"vocals");
        assert!(store(&file, "song", &manifest, &source, &AtomicBool::new(true)).is_err());
        assert_eq!(restore(&file, "hash", &restored).unwrap(), Some(manifest));
    }
}
