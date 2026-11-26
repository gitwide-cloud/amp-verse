//! Device-local startup catalog. A failed NAS refresh never replaces a completed scan.
use amp_core::{Library, Playlist};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
// Save and load must agree: a successful save must remain usable at restart.
const MAX_SNAPSHOT_BYTES: u64 = 256 * 1024 * 1024;

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
/// Retain a newer published snapshot while an earlier scan completes.
pub fn keep_current_preparation(
    current: &amp_core::Preparation,
    incoming: &amp_core::Preparation,
) -> bool {
    let Some(current) = current.extensions.get("_amp_sync") else {
        return false;
    };
    let Some(revision) = current["revision"].as_u64() else {
        return false;
    };
    let Some(id) = current["library_id"].as_str().filter(|id| !id.is_empty()) else {
        return false;
    };
    let Some(incoming) = incoming.extensions.get("_amp_sync") else {
        return true;
    };
    incoming["library_id"].as_str() == Some(id)
        && incoming["revision"]
            .as_u64()
            .is_some_and(|other| revision > other)
}
/// A scan may finish after the user saves newer rules. Never roll those back.
pub fn accept_settings(
    current: &amp_core::show::Settings,
    incoming: &amp_core::show::Settings,
) -> bool {
    match (&current.sync, &incoming.sync) {
        (Some(current), Some(incoming)) if current.library_id == incoming.library_id => {
            incoming.revision >= current.revision
        }
        (Some(_), None) => false,
        _ => true,
    }
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
    let sources = amp_core::music_sources::list(root)?;
    require_complete_sources(&library, &sources)?;
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
        sources,
        settings,
        playlists,
    })
}
/// Core scans can omit offline additional roots. Never publish that partial scan
/// over a completed startup catalog (an empty but accessible root is valid).
fn require_complete_sources(
    library: &Library,
    sources: &[amp_core::music_sources::Source],
) -> Result<(), String> {
    let group = library
        .tree
        .folders
        .iter()
        .find(|f| f.name == amp_core::music_sources::PREFIX);
    let missing: Vec<_> = sources
        .iter()
        .filter(|source| {
            !group.is_some_and(|group| group.folders.iter().any(|f| f.name == source.id))
        })
        .map(|source| source.root.display().to_string())
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Music source unavailable: {}. Your saved library is retained. Reconnect the source, then rescan.",
            missing.join(", ")
        ))
    }
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
    load_at_with_limit(file, root, MAX_SNAPSHOT_BYTES)
}
fn load_at_with_limit(file: &Path, root: &Path, limit: u64) -> Option<Snapshot> {
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .ok()?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > limit {
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
struct LimitedWriter<'a, W> {
    inner: &'a mut W,
    remaining: u64,
}
impl<W: Write> Write for LimitedWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Library snapshot exceeds catalog size limit; previous catalog retained",
            ));
        }
        let written = self.inner.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
fn save_at(file: &Path, root: &Path, snapshot: &Snapshot) -> Result<(), String> {
    save_at_with_limit(file, root, snapshot, MAX_SNAPSHOT_BYTES)
}
fn save_at_with_limit(
    file: &Path,
    root: &Path,
    snapshot: &Snapshot,
    limit: u64,
) -> Result<(), String> {
    if !valid(snapshot, root) {
        return Err("Invalid library snapshot".into());
    }
    let parent = file.parent().ok_or("Catalog folder unavailable")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    serde_json::to_writer(
        LimitedWriter {
            inner: &mut temp,
            remaining: limit,
        },
        snapshot,
    )
    .map_err(|e| e.to_string())?;
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
    fn bounded_save_and_load_agree_and_oversize_keeps_complete_metadata() {
        assert_eq!(MAX_SNAPSHOT_BYTES, 256 * 1024 * 1024);
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("offline-nas-root");
        let file = dir.path().join("catalog.json");
        let mut preparation = amp_core::Preparation {
            bpm: Some(123.),
            manual_override: true,
            analysis: Some(amp_core::preparation::AnalysisInfo {
                engine: "fixture-engine".into(),
                duration: 180.,
                tempo_confidence: 0.9,
                key_confidence: 0.8,
                reviewed: true,
            }),
            ..Default::default()
        };
        preparation.extensions.insert(
            "opaque-analysis".into(),
            serde_json::json!({"bins":[1,2,3],"keep":"all fields"}),
        );
        let mut snapshot = Snapshot {
            version: 1,
            root: root.clone(),
            library: Library {
                tracks: vec![amp_core::Track {
                    path: "artist/album/song.mp3".into(),
                    title: "Preserved title".into(),
                    preparation,
                }],
                tree: amp_core::Folder {
                    name: "music".into(),
                    tracks: vec![0],
                    folders: vec![],
                },
            },
            sources: vec![amp_core::music_sources::Source {
                id: "extra-source".into(),
                root: "other-nas/music".into(),
                library_id: "source-library".into(),
                locations: Default::default(),
            }],
            settings: Some(amp_core::show::Settings::default()),
            playlists: vec![(
                root.join("ampdj/playlists/normal.json"),
                Playlist {
                    version: 1,
                    name: "Duplicates retained".into(),
                    tracks: vec![
                        "artist/album/song.mp3".into(),
                        "artist/album/song.mp3".into(),
                    ],
                    sync: None,
                    companion_available: true,
                },
            )],
        };
        let expected = serde_json::to_value(&snapshot).unwrap();
        let serialized = serde_json::to_vec(&snapshot).unwrap();
        let limit = serialized.len() as u64;
        save_at_with_limit(&file, &root, &snapshot, limit).unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().len(), limit);
        assert_eq!(
            serde_json::to_value(load_at_with_limit(&file, &root, limit).unwrap()).unwrap(),
            expected,
            "No analysis, settings, source refs, or duplicate playlist members may be truncated"
        );
        assert!(load_at_with_limit(&file, &root, limit - 1).is_none());
        assert!(
            save_at_with_limit(&file, &root, &snapshot, limit - 1)
                .unwrap_err()
                .contains("catalog size limit")
        );
        assert_eq!(std::fs::read(&file).unwrap(), serialized);
        snapshot.library.tracks[0]
            .preparation
            .extensions
            .insert("larger-analysis".into(), serde_json::json!("x".repeat(256)));
        assert!(
            save_at_with_limit(&file, &root, &snapshot, limit)
                .unwrap_err()
                .contains("catalog size limit")
        );
        assert_eq!(
            std::fs::read(&file).unwrap(),
            serialized,
            "Oversize replacement leaves previous complete cache byte-for-byte intact"
        );
        assert_eq!(
            serde_json::to_value(load_at_with_limit(&file, &root, limit).unwrap()).unwrap(),
            expected
        );
        let absent = dir.path().join("never-published.json");
        assert!(save_at_with_limit(&absent, &root, &snapshot, limit).is_err());
        assert!(
            !absent.exists(),
            "An oversize first save must not publish a partial cache"
        );
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "Failed temp writes clean up without leaving partial catalog files"
        );
    }
    #[test]
    fn offline_additional_source_cannot_replace_completed_catalog_but_empty_source_can() {
        let source = amp_core::music_sources::Source {
            id: "plex-music".into(),
            root: "nas/plex/music".into(),
            library_id: "plex".into(),
            locations: Default::default(),
        };
        let mut library = Library {
            tracks: vec![],
            tree: amp_core::Folder {
                name: "music".into(),
                tracks: vec![],
                folders: vec![],
            },
        };
        assert!(
            require_complete_sources(&library, std::slice::from_ref(&source))
                .unwrap_err()
                .contains("saved library is retained")
        );
        library.tree.folders.push(amp_core::Folder {
            name: amp_core::music_sources::PREFIX.into(),
            tracks: vec![],
            folders: vec![amp_core::Folder {
                name: source.id.clone(),
                tracks: vec![],
                folders: vec![],
            }],
        });
        assert!(require_complete_sources(&library, &[source]).is_ok());
        assert!(require_complete_sources(&library, &[]).is_ok());
    }
    #[test]
    fn slow_scan_preserves_new_analysis_and_manual_revisions_but_accepts_newer_shared_data() {
        let mut current = amp_core::Preparation::default();
        current.extensions.insert(
            "_amp_sync".into(),
            serde_json::json!({"library_id":"same-library","revision":8}),
        );
        current.bpm = Some(123.);
        current.manual_override = true;
        let mut scanned = current.clone();
        scanned.extensions.get_mut("_amp_sync").unwrap()["revision"] = serde_json::json!(7);
        assert!(keep_current_preparation(&current, &scanned));
        assert!(keep_current_preparation(
            &current,
            &amp_core::Preparation::default()
        ));
        scanned.extensions.get_mut("_amp_sync").unwrap()["revision"] = serde_json::json!(9);
        assert!(!keep_current_preparation(&current, &scanned));
        scanned.extensions.get_mut("_amp_sync").unwrap()["library_id"] =
            serde_json::json!("another-library");
        assert!(!keep_current_preparation(&current, &scanned));
    }

    #[test]
    fn delayed_scan_cannot_hide_a_newly_saved_smart_playlist() {
        let old = amp_core::show::Settings {
            sync: Some(amp_core::coordination::Revision {
                library_id: "library".into(),
                revision: 8,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut saved = old.clone();
        saved.sync.as_mut().unwrap().revision = 9;
        saved.playlists[0].name = "R&B".into();
        assert!(!accept_settings(&saved, &old));
        assert!(!accept_settings(
            &saved,
            &amp_core::show::Settings::default()
        ));
        assert!(accept_settings(&old, &saved));
        assert!(accept_settings(&saved, &saved));
    }
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
