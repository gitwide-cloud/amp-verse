//! Device-local startup catalog. A failed NAS refresh never replaces a completed scan.
use amp_core::{Library, Playlist};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    version: u32,
    root: PathBuf,
    pub library: Library,
    #[serde(default)]
    pub sources: Vec<amp_core::music_sources::Source>,
    pub settings: Option<amp_core::show::Settings>,
    pub playlists: Vec<(PathBuf, Playlist)>,
}
fn path(root: &Path) -> Option<PathBuf> {
    // No canonicalize/stat here: startup must not wait for a sleeping NAS.
    let key = Sha256::digest(root.to_string_lossy().as_bytes());
    Some(
        crate::theme::settings_path()?
            .parent()?
            .join("catalogs")
            .join(format!("{key:x}.json")),
    )
}
pub fn scan(root: &Path) -> Result<Snapshot, String> {
    let library = amp_core::scan(root).map_err(|e| e.to_string())?;
    let settings = amp_core::show::Settings::load(root).ok();
    let mut playlists = Vec::new();
    for path in amp_core::catalog_db::playlist_paths(root)? {
        if let Ok(list) = Playlist::load(&path, root) {
            playlists.push((path, list));
        }
    }
    Ok(Snapshot {
        version: 1,
        root: root.into(),
        library,
        sources: amp_core::music_sources::list(root)?,
        settings,
        playlists,
    })
}
fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|p| matches!(p, Component::Normal(_)))
}
fn valid(snapshot: &Snapshot, root: &Path) -> bool {
    snapshot.version == 1
        && snapshot.root == root
        && snapshot
            .library
            .tracks
            .iter()
            .all(|t| relative(&t.path) && t.preparation.validate().is_ok())
        && valid_folder(&snapshot.library.tree, snapshot.library.tracks.len())
        && snapshot.playlists.iter().all(|(p, list)| {
            p.starts_with(root.join("ampdj/playlists")) && list.tracks.iter().all(|t| relative(t))
        })
}
fn valid_folder(folder: &amp_core::Folder, tracks: usize) -> bool {
    folder.tracks.iter().all(|&i| i < tracks)
        && folder.folders.iter().all(|f| valid_folder(f, tracks))
}
fn load_at(file: &Path, root: &Path) -> Option<Snapshot> {
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .ok()?
        .take(128 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 128 * 1024 * 1024 {
        return None;
    }
    let mut snapshot: Snapshot = serde_json::from_slice(&bytes).ok()?;
    for (path, _) in &mut snapshot.playlists {
        if let Ok(relative) = path.strip_prefix(root.join(".ampverse/playlists")) {
            *path = root.join("ampdj/playlists").join(relative);
        }
    }
    valid(&snapshot, root).then_some(snapshot)
}
pub fn load(root: &Path) -> Option<Snapshot> {
    load_at(&path(root)?, root)
}
fn save_at(file: &Path, root: &Path, snapshot: &Snapshot) -> Result<(), String> {
    if !valid(snapshot, root) {
        return Err("Invalid library snapshot".into());
    }
    let parent = file.parent().ok_or("Catalog folder unavailable")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut temp, snapshot).map_err(|e| e.to_string())?;
    temp.flush().map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(file).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn save(root: &Path, snapshot: &Snapshot) -> Result<(), String> {
    save_at(
        &path(root).ok_or("Catalog folder unavailable")?,
        root,
        snapshot,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_catalog_survives_offline_share_and_additions_refresh_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("music");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("one.mp3"), b"fixture").unwrap();
        amp_core::preparation::save(
            &root,
            Path::new("one.mp3"),
            &amp_core::Preparation {
                cue_in: 1.,
                bpm: Some(120.),
                manual_override: true,
                ..Default::default()
            },
        )
        .unwrap();
        let file = dir.path().join("catalog.json");
        save_at(&file, &root, &scan(&root).unwrap()).unwrap();
        assert_eq!(load_at(&file, &root).unwrap().library.tracks.len(), 1);
        std::fs::write(root.join("two.mp3"), b"fixture").unwrap();
        save_at(&file, &root, &scan(&root).unwrap()).unwrap();
        std::fs::rename(&root, dir.path().join("offline")).unwrap();
        assert!(scan(&root).is_err());
        let offline = load_at(&file, &root).unwrap();
        assert_eq!(offline.library.tracks.len(), 2);
        assert_eq!(offline.library.tracks[0].preparation.cue_in, 1.);
        assert_eq!(offline.library.tracks[0].preparation.bpm, Some(120.));
        assert!(offline.library.tracks[0].preparation.manual_override);
        assert!(load_at(&file, &dir.path().join("other")).is_none());
        std::fs::write(&file, b"partial").unwrap();
        assert!(load_at(&file, &root).is_none());
    }
}
