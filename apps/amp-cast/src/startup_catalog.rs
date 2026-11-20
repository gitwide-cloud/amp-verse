//! Device-local playback snapshot: a NAS refresh must not delay live ingest.
use amp_core::{Folder, Library};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, BufWriter, Read, Write},
    path::{Component, Path},
};

#[derive(Serialize, Deserialize)]
struct Snapshot<T = Library> {
    version: u32,
    root: std::path::PathBuf,
    library: T,
}

const MAX_SNAPSHOT_BYTES: u64 = 128 * 1024 * 1024;

struct LimitedWriter<W> {
    inner: W,
    remaining: u64,
}

impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(io::Error::other(
                "Startup catalog exceeds the cache size limit",
            ));
        }
        let written = self.inner.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
fn valid_folder(folder: &Folder, count: usize) -> bool {
    folder.tracks.iter().all(|&i| i < count)
        && folder.folders.iter().all(|f| valid_folder(f, count))
}
pub fn load(file: &Path, root: &Path) -> Option<Library> {
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .ok()?
        .take(MAX_SNAPSHOT_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
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
    save_with_limit(file, root, library, MAX_SNAPSHOT_BYTES)
}

fn save_with_limit(file: &Path, root: &Path, library: &Library, limit: u64) -> Result<(), String> {
    let parent = file.parent().ok_or("Missing startup catalog directory")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut writer = LimitedWriter {
        inner: BufWriter::new(temporary.as_file_mut()),
        remaining: limit,
    };
    serde_json::to_writer(
        &mut writer,
        &Snapshot {
            version: 1,
            root: root.into(),
            library,
        },
    )
    .map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    drop(writer);
    let output = temporary.as_file();
    output.sync_all().map_err(|e| e.to_string())?;
    temporary.persist(file).map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_writer_rejects_overflow_without_replacing_the_snapshot() {
        let mut bounded = LimitedWriter {
            inner: Vec::new(),
            remaining: 3,
        };
        bounded.write_all(b"abc").unwrap();
        assert!(bounded.write_all(b"x").is_err());
        assert_eq!(bounded.inner.as_slice(), b"abc");

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("catalog.json");
        let library = Library {
            tracks: vec![],
            tree: Folder {
                name: "Music".into(),
                folders: vec![],
                tracks: vec![],
            },
        };
        save(&file, dir.path(), &library).unwrap();
        let previous = std::fs::read(&file).unwrap();
        let limit = previous.len() as u64;
        save_with_limit(&file, dir.path(), &library, limit).unwrap();
        assert!(save_with_limit(&file, dir.path(), &library, limit - 1).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), previous);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
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
        let compact = std::fs::read(&file).unwrap();
        assert!(!compact.contains(&b'\n'));
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
