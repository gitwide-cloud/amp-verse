//! Device-local playback snapshot: a NAS refresh must not delay live ingest.
use amp_core::{Folder, Library};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Component, Path},
};

#[derive(Serialize, Deserialize)]
struct Snapshot {
    version: u32,
    root: std::path::PathBuf,
    library: Library,
}
fn valid_folder(folder: &Folder, count: usize) -> bool {
    folder.tracks.iter().all(|&i| i < count)
        && folder.folders.iter().all(|f| valid_folder(f, count))
}
pub fn load(file: &Path, root: &Path) -> Option<Library> {
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .ok()?
        .take(128 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 128 * 1024 * 1024 {
        return None;
    }
    let snapshot: Snapshot = match serde_json::from_slice(&bytes) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!("Startup catalog unreadable: {error}");
            return None;
        }
    };
    if snapshot.root != root && snapshot.root.canonicalize().ok().as_deref() != Some(root) {
        eprintln!(
            "Startup catalog belongs to another root: {:?} / {:?}",
            snapshot.root, root
        );
    }
    for track in &snapshot.library.tracks {
        if let Err(error) = track.preparation.validate() {
            eprintln!("Startup catalog preparation rejected: {error}");
            break;
        }
    }
    (snapshot.version == 1
        && (snapshot.root == root || snapshot.root.canonicalize().ok().as_deref() == Some(root))
        && valid_folder(&snapshot.library.tree, snapshot.library.tracks.len())
        && snapshot.library.tracks.iter().all(|t| {
            !t.path.as_os_str().is_empty()
                && t.path
                    .components()
                    .all(|c| matches!(c, Component::Normal(_)))
                && t.preparation.validate().is_ok()
        }))
    .then_some(snapshot.library)
}
pub fn save(file: &Path, root: &Path, library: &Library) -> Result<(), String> {
    amp_core::coordination::atomic_write(
        file,
        &Snapshot {
            version: 1,
            root: root.into(),
            library: library.clone(),
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires an explicit local catalog fixture"]
    fn configured_catalog_import() {
        let file = std::path::PathBuf::from(std::env::var_os("AMP_TEST_CATALOG").unwrap());
        let root = std::path::PathBuf::from(std::env::var_os("AMP_TEST_ROOT").unwrap())
            .canonicalize()
            .unwrap();
        assert!(load(&file, &root).is_some());
    }
    #[test]
    fn snapshots_are_root_bound_and_reject_unsafe_tracks() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("catalog.json");
        let mut library = Library {
            tracks: vec![],
            tree: Folder {
                name: "Music".into(),
                tracks: vec![],
                folders: vec![],
            },
        };
        save(&file, dir.path(), &library).unwrap();
        assert!(load(&file, dir.path()).is_some());
        assert!(load(&file, &dir.path().join("other")).is_none());
        library.tracks.push(amp_core::Track {
            path: "../outside.mp3".into(),
            title: "Unsafe".into(),
            preparation: Default::default(),
        });
        save(&file, dir.path(), &library).unwrap();
        assert!(load(&file, dir.path()).is_none());
        std::fs::write(&file, b"interrupted").unwrap();
        assert!(load(&file, dir.path()).is_none());
    }
}
