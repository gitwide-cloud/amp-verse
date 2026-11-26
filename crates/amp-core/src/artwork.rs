//! Immutable artwork blobs in one portable snapshot, separate from the fast track catalog.
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn read(root: &Path, id: &str) -> Result<Option<Vec<u8>>, String> {
    if id.len() != 64 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid artwork ID".into());
    }
    let file = root.join("ampdj/artwork.ampdb");
    if !file.is_file() {
        return Ok(None);
    }
    let _lock = crate::coordination::Lock::acquire(&file)?;
    if fs::symlink_metadata(&file)
        .map_err(err)?
        .file_type()
        .is_symlink()
    {
        return Err("Artwork store cannot be a symlink".into());
    }
    let db = Connection::open_with_flags(file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(err)?;
    db.query_row("SELECT data FROM artwork WHERE id=?1", [id], |r| r.get(0))
        .optional()
        .map_err(err)
}
pub fn save(root: &Path, bytes: &[u8]) -> Result<String, String> {
    if bytes.is_empty() || bytes.len() > 8 * 1024 * 1024 {
        return Err("Artwork must be smaller than 8 MB".into());
    }
    let id = format!("{:x}", Sha256::digest(bytes));
    crate::coordination::directory(&root.join("ampdj"))?;
    let file = root.join("ampdj/artwork.ampdb");
    let _lock = crate::coordination::Lock::acquire(&file)?;
    let temp = tempfile::tempdir().map_err(err)?;
    let local = temp.path().join("artwork.ampdb");
    if file.exists() {
        if fs::symlink_metadata(&file)
            .map_err(err)?
            .file_type()
            .is_symlink()
        {
            return Err("Artwork store cannot be a symlink".into());
        }
        fs::copy(&file, &local).map_err(err)?;
    }
    let db = Connection::open(&local).map_err(err)?;
    db.execute_batch("PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS artwork(id TEXT PRIMARY KEY,data BLOB NOT NULL);").map_err(err)?;
    db.execute(
        "INSERT OR IGNORE INTO artwork VALUES(?1,?2)",
        params![id, bytes],
    )
    .map_err(err)?;
    db.close().map_err(|(_, e)| err(e))?;
    let pending = root.join(format!(
        "ampdj/.artwork-{}.tmp",
        crate::coordination::token()
    ));
    let result = (|| {
        fs::copy(local, &pending).map_err(err)?;
        fs::OpenOptions::new()
            .write(true)
            .open(&pending)
            .and_then(|f| f.sync_all())
            .map_err(err)?;
        fs::rename(&pending, &file).map_err(err)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&pending);
    }
    result?;
    Ok(id)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_assets_deduplicate_and_reopen() {
        let root = tempfile::tempdir().unwrap();
        let a = save(root.path(), b"first").unwrap();
        assert_eq!(save(root.path(), b"first").unwrap(), a);
        let b = save(root.path(), b"second").unwrap();
        assert_eq!(read(root.path(), &a).unwrap().unwrap(), b"first");
        assert_eq!(read(root.path(), &b).unwrap().unwrap(), b"second");
        assert!(read(root.path(), "../escape").is_err());
    }
}
