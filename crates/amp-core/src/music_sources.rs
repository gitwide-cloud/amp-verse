//! Explicit additional roots. Audio and preparation stay in each originating library.
use crate::{Folder, Library, coordination};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};
pub const PREFIX: &str = "__amp_sources";
pub fn display_name(name: &str) -> String {
    if name == PREFIX {
        "Additional music sources".into()
    } else {
        if let Ok(cached) = cache().read()
            && let Some(source) = cached.values().flatten().find(|source| source.id == name)
        {
            let leaf = source
                .root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            if let Some(parent) = source.root.parent().and_then(Path::file_name) {
                return format!("{} / {leaf}", parent.to_string_lossy());
            }
        }
        if name.len() > 37
            && name.is_char_boundary(name.len() - 37)
            && name.as_bytes()[name.len() - 37] == b'-'
            && uuid::Uuid::parse_str(&name[name.len() - 36..]).is_ok()
        {
            return name[..name.len() - 37].into();
        }
        name.into()
    }
}
fn cache() -> &'static std::sync::RwLock<std::collections::HashMap<PathBuf, Vec<Source>>> {
    static CACHE: std::sync::OnceLock<
        std::sync::RwLock<std::collections::HashMap<PathBuf, Vec<Source>>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}
fn known(root: &Path) -> Result<Vec<Source>, String> {
    if let Ok(cache) = cache().read()
        && let Some(sources) = cache.get(root)
    {
        return Ok(sources.clone());
    }
    list(root)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub root: PathBuf,
    pub library_id: String,
    #[serde(default)]
    pub locations: std::collections::BTreeMap<String, PathBuf>,
}
#[derive(Default, Serialize, Deserialize)]
struct Sources {
    #[serde(default, rename = "_amp_sync", skip_serializing_if = "Option::is_none")]
    sync: Option<coordination::Revision>,
    #[serde(default)]
    sources: Vec<Source>,
}
fn document(root: &Path) -> PathBuf {
    root.join("ampdj/music-sources.json")
}
fn read(root: &Path) -> Result<Sources, String> {
    let file = document(root);
    let bytes = match crate::catalog_db::read(&file)? {
        Some(bytes) => bytes,
        None => match fs::read(&file) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Sources::default()),
            Err(e) => return Err(e.to_string()),
        },
    };
    let list: Sources = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let mut ids = std::collections::HashSet::new();
    if list.sources.len() > 128
        || list.sources.iter().any(|s| {
            s.id.is_empty()
                || s.id.len() > 180
                || s.id.contains(['/', '\\'])
                || s.id == "."
                || s.id == ".."
                || !ids.insert(&s.id)
                || s.library_id.is_empty()
        })
    {
        return Err("Invalid saved music sources; existing music was retained.".into());
    }
    Ok(list)
}
pub fn list(root: &Path) -> Result<Vec<Source>, String> {
    let mut sources = read(root)?.sources;
    if !sources.is_empty()
        && let Ok(device) = coordination::device_id()
    {
        for source in &mut sources {
            if let Some(path) = source.locations.get(&device) {
                source.root = path.clone();
            }
        }
    }
    if let Ok(mut cache) = cache().write() {
        cache.insert(root.into(), sources.clone());
    }
    Ok(sources)
}
fn save(root: &Path, list: &Sources) -> Result<(), String> {
    crate::storage::enroll(root)?;
    coordination::save_document(
        &document(root),
        &serde_json::to_value(list).map_err(|e| e.to_string())?,
        &coordination::library_id(root)?,
    )?;
    if let Ok(mut cached) = cache().write() {
        cached.clear();
    }
    Ok(())
}
fn identity_matches(root: &Path, id: &str) -> bool {
    fs::read(root.join("ampdj/library.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|value| value["id"].as_str() == Some(id))
}
pub fn add(root: &Path, folder: &Path) -> Result<Vec<Source>, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let folder = folder.canonicalize().map_err(|e| e.to_string())?;
    if !folder.is_dir() {
        return Err("Select a music folder.".into());
    }
    if crate::folder_contains(&folder, &root) {
        return Err(
            "That folder is already included in the main library, including its subfolders.".into(),
        );
    }
    if root.join(PREFIX).exists() {
        return Err(format!(
            "The reserved folder {PREFIX} already exists in the main library."
        ));
    }
    let mut sources = read(&root)?;
    let device = coordination::device_id()?;
    if sources
        .sources
        .iter()
        .any(|s| crate::folder_contains(&folder, s.locations.get(&device).unwrap_or(&s.root)))
    {
        return Err("That folder is already included in a music source.".into());
    }
    let library_id = coordination::library_id(&folder)?;
    if let Some(source) = sources
        .sources
        .iter_mut()
        .find(|s| s.library_id == library_id)
    {
        source.locations.insert(device, folder);
    } else {
        let name: String = folder
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .take(24)
            .collect();
        let id = format!(
            "{}-{library_id}",
            if name.is_empty() { "Music" } else { &name }
        );
        sources.sources.push(Source {
            id,
            root: folder.clone(),
            library_id,
            locations: std::collections::BTreeMap::from([(device, folder)]),
        });
    }
    save(&root, &sources)?;
    list(&root)
}
pub fn remove(root: &Path, id: &str) -> Result<Vec<Source>, String> {
    let mut sources = read(root)?;
    sources.sources.retain(|s| s.id != id);
    save(root, &sources)?;
    list(root)
}
/// Returns the owning library for a virtual source path. No traversal or symlink escape.
pub fn owner(root: &Path, relative: &Path) -> io::Result<(PathBuf, PathBuf)> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(io::Error::other("Expected a relative library path"));
    }
    let mut components = relative.components();
    if components.next().is_some_and(|c| c.as_os_str() == PREFIX) {
        let id = components
            .next()
            .ok_or_else(|| io::Error::other("Missing music source"))?;
        let source = known(root)
            .map_err(io::Error::other)?
            .into_iter()
            .find(|s| std::ffi::OsStr::new(&s.id) == id.as_os_str())
            .ok_or_else(|| io::Error::other("Music source is no longer connected"))?;
        let base = source.root.canonicalize()?;
        if !identity_matches(&base, &source.library_id) {
            return Err(io::Error::other(
                "Music source identity changed; reconnect it in Library > Sources.",
            ));
        }
        crate::storage::register(&base);
        Ok((base, components.collect()))
    } else {
        Ok((root.canonicalize()?, relative.into()))
    }
}
pub fn relative(root: &Path, path: &Path) -> Option<PathBuf> {
    for source in known(root).ok()? {
        let base = source.root;
        if crate::folder_contains(path, &base) {
            let tail: PathBuf = path.components().skip(base.components().count()).collect();
            return Some(PathBuf::from(PREFIX).join(source.id).join(tail));
        }
    }
    None
}
/// Useful for existing callers needing an absolute display/drag path. Actual reads still
/// pass through resolve and its containment checks.
pub fn audio_path(root: &Path, path: &Path) -> PathBuf {
    let mut components = path.components();
    if components.next().is_some_and(|c| c.as_os_str() == PREFIX)
        && let Some(id) = components.next()
        && let Ok(sources) = known(root)
        && let Some(source) = sources
            .into_iter()
            .find(|s| std::ffi::OsStr::new(&s.id) == id.as_os_str())
    {
        return source.root.join(components.collect::<PathBuf>());
    }
    root.join(path)
}
pub(crate) fn append(root: &Path, library: &mut Library, metadata: bool) -> io::Result<()> {
    let mut group = Folder {
        name: PREFIX.into(),
        folders: vec![],
        tracks: vec![],
    };
    // scan_single enumerates real entries and rejects symlinks. Canonicalize the
    // roots once, not every song: per-song NAS round trips can stall preparation.
    let base = root.canonicalize()?;
    let mut seen: std::collections::HashSet<_> =
        library.tracks.iter().map(|t| base.join(&t.path)).collect();
    for source in list(root).map_err(io::Error::other)? {
        if !identity_matches(&source.root, &source.library_id) {
            if !metadata {
                return Err(io::Error::other(format!(
                    "Music source unavailable or identity changed: {}. Reconnect it in Library > Sources.",
                    source.root.display()
                )));
            }
            continue;
        }
        let mut extra = match crate::scan_single(&source.root, metadata) {
            Ok(extra) => extra,
            Err(error) if !metadata => {
                return Err(io::Error::other(format!(
                    "Cannot scan music source {}: {error}",
                    source.root.display()
                )));
            }
            Err(_) => continue,
        };
        let source_base = source.root.canonicalize()?;
        let mut indexes = std::collections::HashMap::new();
        for (old, mut track) in extra.tracks.into_iter().enumerate() {
            let absolute = source_base.join(&track.path);
            if !seen.insert(absolute) {
                continue;
            }
            indexes.insert(old, library.tracks.len());
            track.path = PathBuf::from(PREFIX).join(&source.id).join(track.path);
            library.tracks.push(track);
        }
        fn remap(folder: &mut Folder, indexes: &std::collections::HashMap<usize, usize>) {
            folder.tracks = folder
                .tracks
                .iter()
                .filter_map(|i| indexes.get(i).copied())
                .collect();
            for f in &mut folder.folders {
                remap(f, indexes);
            }
        }
        remap(&mut extra.tree, &indexes);
        extra.tree.name = source.id;
        group.folders.push(extra.tree);
    }
    if !group.folders.is_empty() {
        library.tree.folders.push(group);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn job_enumeration_does_not_parse_metadata_and_reports_disconnected_sources() {
        let main = tempfile::tempdir().unwrap();
        let extra = tempfile::tempdir().unwrap();
        fs::create_dir_all(extra.path().join("Artist/Album")).unwrap();
        fs::write(extra.path().join("Artist/Album/song.mp3"), b"audio").unwrap();
        fs::write(
            extra.path().join("Artist/Album/song.mp3.amp.json"),
            b"broken preparation",
        )
        .unwrap();
        add(main.path(), extra.path()).unwrap();
        let jobs = crate::scan_job_inputs(main.path()).unwrap();
        assert_eq!(jobs.tracks.len(), 1);
        let mut input = jobs.tracks[0].preparation.clone();
        assert!(input.extensions.remove("scan_source_stat").is_some());
        assert_eq!(input, crate::Preparation::default());
        assert!(crate::scan(extra.path()).is_err());
        fs::rename(
            extra.path().join("ampdj/library.json"),
            extra.path().join("ampdj/library.offline"),
        )
        .unwrap();
        assert!(
            crate::scan_job_inputs(main.path())
                .unwrap_err()
                .to_string()
                .contains("Music source unavailable")
        );
    }
    #[test]
    fn added_roots_persist_combine_and_keep_their_metadata_and_ids() {
        let main = tempfile::tempdir().unwrap();
        let extra = tempfile::tempdir().unwrap();
        fs::write(main.path().join("a.mp3"), b"audio").unwrap();
        fs::create_dir_all(extra.path().join("Rock/Live")).unwrap();
        fs::write(extra.path().join("Rock/Live/b.mp3"), b"audio").unwrap();
        let sources = add(main.path(), extra.path()).unwrap();
        assert_eq!(list(main.path()).unwrap().len(), 1);
        let lib = crate::scan(main.path()).unwrap();
        assert_eq!(lib.tracks.len(), 2);
        assert_eq!(
            crate::scan_job_inputs(main.path())
                .unwrap()
                .tracks
                .iter()
                .map(|t| &t.path)
                .collect::<Vec<_>>(),
            lib.tracks.iter().map(|t| &t.path).collect::<Vec<_>>()
        );
        let b = lib.tracks.iter().find(|t| t.title == "b").unwrap();
        let id = crate::show::song_id(b);
        assert_eq!(
            crate::resolve(main.path(), &b.path).unwrap(),
            extra.path().join("Rock/Live/b.mp3").canonicalize().unwrap()
        );
        let p = crate::Preparation {
            bpm: Some(123.),
            ..Default::default()
        };
        crate::preparation::save(main.path(), &b.path, &p).unwrap();
        assert_eq!(
            crate::scan(extra.path()).unwrap().tracks[0].preparation.bpm,
            Some(123.)
        );
        let refreshed = crate::scan(main.path()).unwrap();
        let b = refreshed.tracks.iter().find(|t| t.title == "b").unwrap();
        assert_eq!(b.preparation.bpm, Some(123.));
        assert_eq!(crate::show::song_id(b), id);
        let mut rules = crate::show::Settings::default().playlists.remove(0);
        rules.folders = vec![b.path.parent().unwrap().into()];
        rules.restrict_folders = true;
        assert!(crate::show::eligible(&rules, b));
        assert!(!crate::show::eligible(&rules, &refreshed.tracks[0]));
        let playlist = crate::Playlist {
            version: 1,
            name: "Combined".into(),
            tracks: refreshed.tracks.iter().map(|t| t.path.clone()).collect(),
            sync: None,
            companion_available: false,
        };
        let file = main.path().join("ampdj/playlists/combined.json");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        playlist.save_in_library(&file, main.path()).unwrap();
        assert_eq!(
            crate::Playlist::load(&file, main.path())
                .unwrap()
                .tracks
                .len(),
            2
        );
        crate::catalog_db::compact(main.path()).unwrap();
        assert!(
            crate::catalog_db::read(&document(main.path()))
                .unwrap()
                .is_some()
        );
        assert!(add(main.path(), extra.path()).is_err());
        assert!(add(main.path(), &extra.path().join("Rock")).is_err());
        assert!(
            crate::resolve(
                main.path(),
                &PathBuf::from(PREFIX).join(&sources[0].id).join("../a.mp3")
            )
            .is_err()
        );
        remove(main.path(), &sources[0].id).unwrap();
        assert_eq!(crate::scan(main.path()).unwrap().tracks.len(), 1);
        assert!(extra.path().join("Rock/Live/b.mp3").exists());
    }
    #[test]
    fn disconnected_sources_remain_saved_and_can_be_remapped_without_changing_ids() {
        let main = tempfile::tempdir().unwrap();
        let extra = tempfile::tempdir().unwrap();
        let moved = tempfile::tempdir().unwrap();
        fs::write(main.path().join("main.mp3"), b"audio").unwrap();
        fs::write(extra.path().join("extra.mp3"), b"audio").unwrap();
        let source = add(main.path(), extra.path()).unwrap().remove(0);
        let path = moved.path().join("new-location");
        fs::rename(extra.path(), &path).unwrap();
        assert_eq!(crate::scan(main.path()).unwrap().tracks.len(), 1);
        assert_eq!(list(main.path()).unwrap().len(), 1);
        let connected = add(main.path(), &path).unwrap();
        assert_eq!(connected[0].id, source.id);
        assert_eq!(crate::scan(main.path()).unwrap().tracks.len(), 2);
        assert_eq!(
            read(main.path()).unwrap().sources[0].root,
            source.root,
            "Device remap replaced the shared default path"
        );
        assert_eq!(
            list(main.path()).unwrap()[0].root,
            path.canonicalize().unwrap()
        );
        assert!(add(main.path(), &path).is_err());
    }
}
