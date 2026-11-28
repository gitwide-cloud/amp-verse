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

/// Build a fresh device-local export store with only selected immutable artwork.
/// The caller owns this staging file; source stores are opened read-only.
pub fn copy_selected_to_local(
    selections: impl IntoIterator<Item = (std::path::PathBuf, String)>,
    destination: &Path,
    byte_budget: u64,
    mut checkpoint: impl FnMut() -> Result<(), String>,
) -> Result<usize, String> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut owners: BTreeMap<std::path::PathBuf, BTreeSet<String>> = BTreeMap::new();
    for (root, id) in selections {
        if id.len() == 64 && id.bytes().all(|c| c.is_ascii_hexdigit()) {
            owners.entry(root).or_default().insert(id);
        }
    }
    if owners.is_empty() {
        return Ok(0);
    }
    fs::create_dir_all(
        destination
            .parent()
            .ok_or("Export artwork folder unavailable")?,
    )
    .map_err(err)?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(err)?;
    let mut out = Connection::open(destination).map_err(err)?;
    out.execute_batch(
        "PRAGMA synchronous=FULL;CREATE TABLE artwork(id TEXT PRIMARY KEY,data BLOB NOT NULL);",
    )
    .map_err(err)?;
    let tx = out.transaction().map_err(err)?;
    let mut bytes = 0u64;
    let mut copied = 0;
    let mut seen = BTreeSet::new();
    for (root, ids) in owners {
        checkpoint()?;
        let source = root.join("ampdj/artwork.ampdb");
        if !source.is_file() {
            continue;
        }
        if fs::symlink_metadata(&source)
            .map_err(err)?
            .file_type()
            .is_symlink()
        {
            return Err("Artwork store cannot be a symlink".into());
        }
        let _guard = crate::coordination::Lock::acquire(&source)?;
        let db = Connection::open_with_flags(&source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(err)?;
        for id in ids {
            checkpoint()?;
            if seen.contains(&id) {
                continue;
            }
            let data: Option<Vec<u8>> = db
                .query_row(
                    "SELECT data FROM artwork WHERE id=?1 AND length(data)<=8388608",
                    [&id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(err)?;
            if let Some(data) = data {
                if format!("{:x}", Sha256::digest(&data)) != id {
                    return Err("Export artwork checksum failed".into());
                }
                bytes = bytes
                    .checked_add(data.len() as u64)
                    .ok_or("Artwork size overflow")?;
                if bytes > byte_budget.min(512 * 1024 * 1024) {
                    return Err(
                        "Selected artwork exceeds the export limit. Choose fewer songs.".into(),
                    );
                }
                seen.insert(id.clone());
                copied += tx
                    .execute(
                        "INSERT OR IGNORE INTO artwork VALUES(?1,?2)",
                        params![id, data],
                    )
                    .map_err(err)?;
            }
        }
    }
    tx.commit().map_err(err)?;
    out.close().map_err(|(_, e)| err(e))?;
    Ok(copied)
}
#[cfg(test)]
mod selected_export_tests {
    #[test]
    fn exported_artwork_contains_only_the_selected_assets() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let a = super::save(source.path(), b"selected").unwrap();
        let b = super::save(source.path(), b"private-unselected").unwrap();
        std::fs::create_dir(target.path().join("ampdj")).unwrap();
        let dest = target.path().join("ampdj/artwork.ampdb");
        assert_eq!(
            super::copy_selected_to_local(
                [(source.path().to_path_buf(), a.clone())],
                &dest,
                1024 * 1024,
                || Ok(())
            )
            .unwrap(),
            1
        );
        assert_eq!(
            super::read(target.path(), &a).unwrap(),
            Some(b"selected".to_vec())
        );
        assert!(super::read(target.path(), &b).unwrap().is_none());
        assert_eq!(
            super::read(source.path(), &b).unwrap(),
            Some(b"private-unselected".to_vec())
        );
    }
}
