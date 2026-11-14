//! Device-local library identity, independent of the preview profile and analysis queue.
use amp_core::music_sources::Source;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Serialize, Deserialize)]
struct MainLibrary {
    version: u32,
    root: PathBuf,
}
fn file() -> Option<PathBuf> {
    if cfg!(windows)
        && let Some(home) = std::env::var_os("USERPROFILE")
    {
        return Some(
            PathBuf::from(home).join("AppData/Roaming/AMPVerse/ampdj/library-index.ampdb"),
        );
    }
    crate::theme::settings_path()?
        .parent()
        .map(|p| p.join("ampdj/library-index.ampdb"))
}
fn root_at(file: &Path) -> Option<PathBuf> {
    let value = amp_core::catalog_db::local_record(file, "main-library").ok()??;
    let value: MainLibrary = serde_json::from_value(value).ok()?;
    (value.version == 1 && !value.root.as_os_str().is_empty()).then_some(value.root)
}
fn source_key(root: &Path) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "sources/{:x}",
        Sha256::digest(root.to_string_lossy().as_bytes())
    )
}
fn remember_at(file: &Path, root: &Path, sources: Option<&[Source]>) -> Result<(), String> {
    if root.as_os_str().is_empty() {
        return Err("An empty library cannot replace the saved main path".into());
    }
    let value = serde_json::to_value(MainLibrary {
        version: 1,
        root: root.into(),
    })
    .map_err(|e| e.to_string())?;
    if amp_core::catalog_db::local_record(file, "main-library")?.as_ref() != Some(&value) {
        amp_core::catalog_db::save_local_record(file, "main-library", &value)?;
    }
    if let Some(sources) = sources {
        let key = source_key(root);
        let value = serde_json::to_value(sources).map_err(|e| e.to_string())?;
        if amp_core::catalog_db::local_record(file, &key)?.as_ref() != Some(&value) {
            amp_core::catalog_db::save_local_record(file, &key, &value)?;
        }
    }
    Ok(())
}
pub fn main_root() -> Option<PathBuf> {
    if cfg!(test) { None } else { root_at(&file()?) }
}
pub fn sources(root: &Path) -> Vec<Source> {
    if cfg!(test) {
        return Vec::new();
    }
    file()
        .and_then(|file| {
            amp_core::catalog_db::local_record(&file, &source_key(root))
                .ok()
                .flatten()
        })
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}
pub fn remember(root: &Path, sources: Option<&[Source]>) -> Result<(), String> {
    if cfg!(test) {
        return Ok(());
    }
    remember_at(
        &file().ok_or("Library index folder unavailable")?,
        root,
        sources,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_root_and_sources_survive_reopen_and_empty_choices_cannot_erase_them() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("library-index.ampdb");
        let root = Path::new(r"\\sleeping-nas\music");
        let sources = vec![Source {
            id: "extra".into(),
            root: "another-folder".into(),
            library_id: "lib-id".into(),
            locations: Default::default(),
        }];
        remember_at(&file, root, Some(&sources)).unwrap();
        assert_eq!(root_at(&file).as_deref(), Some(root));
        let before = amp_core::catalog_db::local_record(&file, &source_key(root)).unwrap();
        remember_at(&file, root, None).unwrap();
        assert!(remember_at(&file, Path::new(""), Some(&[])).is_err());
        assert_eq!(root_at(&file).as_deref(), Some(root));
        assert_eq!(
            amp_core::catalog_db::local_record(&file, &source_key(root)).unwrap(),
            before
        );
        remember_at(&file, root, Some(&[])).unwrap();
        assert_eq!(
            amp_core::catalog_db::local_record(&file, &source_key(root)).unwrap(),
            Some(serde_json::json!([]))
        );
    }
}
