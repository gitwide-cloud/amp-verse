//! One portable metadata directory per music root. Never relocates source audio.
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
fn roots() -> &'static Mutex<Vec<PathBuf>> {
    static ROOTS: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
    ROOTS.get_or_init(Default::default)
}
pub fn register(root: &Path) {
    if let Ok(mut roots) = roots().lock()
        && !roots.iter().any(|r| r == root)
    {
        roots.push(root.into());
    }
}
pub fn root_for(audio: &Path) -> PathBuf {
    if let Ok(roots) = roots().lock()
        && let Some(root) = roots
            .iter()
            .filter(|r| audio.starts_with(r))
            .max_by_key(|r| r.components().count())
    {
        return root.clone();
    }
    for parent in audio.ancestors().skip(1) {
        if parent.join("ampdj/library.json").is_file()
            || parent.join(".ampverse/library.json").is_file()
        {
            register(parent);
            return parent.into();
        }
    }
    audio.parent().unwrap_or(Path::new(".")).into()
}
pub fn enroll(root: &Path) -> Result<(), String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    register(&root);
    let new = root.join("ampdj/library.json");
    ensure_parent(&new)?;
    if new.exists() {
        return Ok(());
    }
    let old = root.join(".ampverse/library.json");
    if old.is_file() {
        let _lock = crate::coordination::Lock::acquire(&new)?;
        if !new.exists() {
            let value: Value = serde_json::from_slice(&fs::read(old).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            if value["version"] != 1
                || value["id"]
                    .as_str()
                    .and_then(|id| uuid::Uuid::parse_str(id).ok())
                    .is_none()
            {
                return Err("Unsupported legacy library identity".into());
            }
            crate::coordination::atomic_write(&new, &value)?;
        }
    }
    Ok(())
}
/// Embedded file copy in Documents, grouped by persistent library ID.
pub fn export_rules(root: &Path) -> Result<PathBuf, String> {
    let id = crate::coordination::library_id(root)?;
    let docs = std::env::var_os("AMP_DOCUMENTS_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|p| PathBuf::from(p).join("Documents"))
        })
        .ok_or("Documents folder unavailable")?;
    let folder = docs.join("ampdj").join(id);
    let file = folder.join("smart-playlists.ampdb");
    let rules = root.join("ampdj/smart-playlists.json");
    let bytes = if let Some(bytes) = crate::catalog_db::read(&rules)? {
        bytes
    } else {
        fs::read(rules).map_err(|e| e.to_string())?
    };
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    ensure_parent(&file)?;
    let old = folder.join("smart-playlists.json");
    if old.is_file() {
        let legacy: Value = serde_json::from_slice(&fs::read(&old).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        crate::catalog_db::save_local_record(&file, "recovery/smart-playlists.json", &legacy)?;
    }
    crate::catalog_db::save_local_record(&file, "smart-playlists.json", &value)?;
    if crate::catalog_db::local_record(&file, "smart-playlists.json")? != Some(value) {
        return Err("Exported rules did not verify; original retained".into());
    }
    if old.is_file() {
        fs::remove_file(old).map_err(|e| e.to_string())?;
    }
    Ok(file)
}

fn ensure_parent(file: &Path) -> Result<(), String> {
    let parent = file.parent().ok_or("Missing metadata directory")?;
    for dir in parent.ancestors() {
        if fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Metadata directories must not be symlinks".into());
        }
    }
    fs::create_dir_all(parent).map_err(|e| e.to_string())
}
fn copy_record(source: &Path, target: &Path) -> Result<bool, String> {
    let value: Value = serde_json::from_slice(&fs::read(source).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{}: {e}", source.display()))?;
    if target.exists() {
        return Ok(false);
    }
    ensure_parent(target)?;
    crate::coordination::atomic_write(target, &value)?;
    Ok(true)
}
/// Offline conversion only. Refuse active analysis; retain old lock files and every old JSON in a central archive.
pub fn organize(root: &Path) -> Result<usize, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    enroll(&root)?;
    let _guard = crate::coordination::Lock::acquire(&root.join("ampdj/organize"))?;
    fn visit(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() && entry.file_name() != "ampdj" {
                visit(&entry.path(), files)?;
            } else if kind.is_file() {
                files.push(entry.path());
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(&root, &mut files)?;
    let now = crate::coordination::now()?;
    for file in &files {
        if file.extension().is_some_and(|e| e == "lease") {
            let value: Value = serde_json::from_slice(&fs::read(file).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            if value["state"] == "active" && value["expires_ms"].as_u64().unwrap_or(u64::MAX) > now
            {
                return Err(
                    "Pause analysis on all devices before organizing the shared library".into(),
                );
            }
        }
    }
    // Import root configuration before retiring the old store. Its identity is retained.
    let old_rules = root.join(".ampverse/show.json");
    if old_rules.is_file() {
        copy_record(&old_rules, &root.join("ampdj/smart-playlists.json"))?;
    }
    let old_identity = root.join(".ampverse/library.json");
    let id = crate::coordination::library_id(&root)?;
    ensure_parent(&old_identity)?;
    crate::coordination::atomic_write(
        &old_identity,
        &serde_json::json!({"version":2,"id":id,"moved_to":"ampdj"}),
    )?;
    // Preserve the existing reader's precedence: canonical legacy records beat adjacent sidecars.
    files.sort_by_key(|file| {
        usize::from(
            file.file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".amp.json")),
        )
    });
    let moved = std::sync::atomic::AtomicUsize::new(0);
    for sidecar_phase in [false, true] {
        let phase: Vec<_> = files
            .iter()
            .filter(|file| {
                file.file_name()
                    .is_some_and(|n| n.to_string_lossy().ends_with(".amp.json"))
                    == sidecar_phase
            })
            .collect();
        let error = Mutex::new(None);
        crate::batch::for_each(&phase, 4, |file| match migrate_file(&root, file) {
            Ok(count) => {
                let total = moved.fetch_add(count, std::sync::atomic::Ordering::Relaxed) + count;
                if count > 0 && total.is_multiple_of(100) {
                    eprintln!("Organized {total} metadata records");
                }
                true
            }
            Err(message) => {
                *error.lock().unwrap() = Some(message);
                false
            }
        });
        if let Some(error) = error.into_inner().unwrap() {
            return Err(error);
        }
    }
    Ok(moved.load(std::sync::atomic::Ordering::Relaxed))
}
fn migrate_file(root: &Path, file: &Path) -> Result<usize, String> {
    let relative = file.strip_prefix(root).map_err(|e| e.to_string())?;
    if relative == Path::new(".ampverse/library.json") {
        return Ok(0);
    } // compatibility fence for old writers
    let parts: Vec<_> = relative.components().collect();
    let old_index = parts.iter().position(|c| c.as_os_str() == ".ampverse");
    let sidecar = file
        .file_name()
        .is_some_and(|n| n.to_string_lossy().ends_with(".amp.json"));
    if (old_index.is_none() && !sidecar) || file.extension().is_some_and(|e| e == "lock") {
        return Ok(0);
    }
    let json_record = file.extension().is_some_and(|e| e == "json");
    let _lock = crate::coordination::Lock::acquire(file)?;
    let mut target = None;
    if let Some(index) = old_index {
        let tail: PathBuf = parts[index + 1..].iter().collect();
        if json_record && tail.starts_with("tracks") {
            let prefix: PathBuf = parts[..index].iter().collect();
            let record_relative = prefix.join(tail.strip_prefix("tracks").unwrap());
            let name = record_relative.to_string_lossy();
            if let Some(audio_relative) = name.strip_suffix(".json") {
                let audio = root.join(audio_relative);
                if audio.is_file() && crate::supported(&audio) {
                    target = Some(root.join("ampdj/tracks").join(&record_relative));
                }
            }
        } else if json_record && index == 0 && tail.starts_with("playlists") {
            target = Some(root.join("ampdj").join(&tail));
        }
    } else if sidecar {
        let text = relative.to_string_lossy();
        target = Some(
            root.join("ampdj/tracks")
                .join(format!("{}.json", text.strip_suffix(".amp.json").unwrap())),
        );
    }
    if let Some(target) = &target {
        if target.starts_with(root.join("ampdj/tracks")) {
            let prep: crate::Preparation =
                serde_json::from_slice(&fs::read(file).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            prep.validate()?;
        }
        copy_record(file, target)?;
    }
    let archive = root.join("ampdj/legacy").join(relative);
    if let Some(parent) = archive.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if archive.exists() {
        if fs::read(&archive).map_err(|e| e.to_string())?
            != fs::read(file).map_err(|e| e.to_string())?
        {
            return Err(format!("Existing archive differs: {}", archive.display()));
        }
        // Resume after an interrupted copy without deleting an unverified record.
        fs::remove_file(file).map_err(|e| e.to_string())?;
    } else {
        fs::rename(file, &archive).map_err(|e| e.to_string())?;
    }
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn organize_nested_legacy_records_keeps_identity_cues_and_duplicate_names() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = crate::coordination::token();
        fs::create_dir_all(root.join(".ampverse")).unwrap();
        fs::write(
            root.join(".ampverse/library.json"),
            serde_json::to_vec(&serde_json::json!({"version":1,"id":id})).unwrap(),
        )
        .unwrap();
        for (folder, cue) in [("a", 2.), ("b", 4.)] {
            fs::create_dir_all(root.join(folder).join(".ampverse/tracks")).unwrap();
            fs::write(root.join(folder).join("song.mp3"), b"audio unchanged").unwrap();
            let prep = crate::Preparation {
                cue_in: cue,
                bpm: Some(123.),
                manual_override: true,
                ..Default::default()
            };
            fs::write(
                root.join(folder).join(".ampverse/tracks/song.mp3.json"),
                serde_json::to_vec(&prep).unwrap(),
            )
            .unwrap();
        }
        let older = crate::Preparation {
            cue_in: 99.,
            ..Default::default()
        };
        fs::write(
            root.join("a/song.mp3.amp.json"),
            serde_json::to_vec(&older).unwrap(),
        )
        .unwrap();
        assert_eq!(organize(root).unwrap(), 3);
        assert_eq!(crate::coordination::library_id(root).unwrap(), id);
        for (folder, cue) in [("a", 2.), ("b", 4.)] {
            let audio = root.join(folder).join("song.mp3");
            assert_eq!(crate::preparation::load(&audio).unwrap().cue_in, cue);
            assert!(crate::preparation::portable(&audio).starts_with(root.join("ampdj/tracks")));
            assert_eq!(fs::read(audio).unwrap(), b"audio unchanged");
            assert!(
                !root
                    .join(folder)
                    .join(".ampverse/tracks/song.mp3.json")
                    .exists()
            );
            assert!(
                root.join("ampdj/legacy")
                    .join(folder)
                    .join(".ampverse/tracks/song.mp3.json")
                    .exists()
            );
        }
        assert_eq!(organize(root).unwrap(), 0);
    }
    #[test]
    fn active_analysis_blocks_conversion_and_existing_canonical_records_win() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".ampverse/tracks")).unwrap();
        let lease = root.join(".ampverse/tracks/active.lease");
        fs::write(&lease,serde_json::to_vec(&serde_json::json!({"state":"active","expires_ms":crate::coordination::now().unwrap()+60000})).unwrap()).unwrap();
        assert!(organize(root).unwrap_err().contains("Pause analysis"));
        assert!(lease.exists());
    }
}
