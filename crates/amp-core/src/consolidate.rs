//! Content-verified library consolidation. No source is removed until the new library is complete.
use crate::{Playlist, Preparation, Track, coordination, preparation, show};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Serialize, Deserialize)]
pub struct Entry {
    #[serde(default)]
    pub organize_names: bool,
    pub original: PathBuf,
    pub relative: PathBuf,
    pub hash: String,
    pub bytes: u64,
    pub preparation: Preparation,
    pub revision: Option<Vec<u8>>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Group {
    pub members: Vec<usize>,
    pub keeper: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub root: PathBuf,
    pub files: Vec<Entry>,
    pub groups: Vec<Group>,
    pub settings: show::Settings,
    pub playlists: Vec<Playlist>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Report {
    pub destination: PathBuf,
    pub moved: usize,
    pub retained: usize,
    pub warnings: Vec<String>,
}
pub fn resume_plan(destination: &Path) -> Result<Plan, String> {
    let file = destination.join("ampdj/consolidation.ampdb");
    let value = crate::catalog_db::local_record(&file, "plan")?
        .ok_or("No consolidation journal was found in that folder")?;
    let plan: Plan = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let state = crate::catalog_db::local_record(&file, "migration")?
        .ok_or("Consolidation journal is incomplete")?;
    if state["plan_id"] != plan.id {
        return Err("Consolidation journal identity mismatch".into());
    }
    Ok(plan)
}
fn playlist_record(path: &Path) -> Result<Playlist, String> {
    let bytes = if let Some(bytes) = crate::catalog_db::read(path)? {
        bytes
    } else {
        fs::read(path).map_err(|e| e.to_string())?
    };
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    coordination::adopt_legacy(&mut value)?;
    let mut list: Playlist = serde_json::from_value(value).map_err(|e| e.to_string())?;
    for path in &mut list.tracks {
        *path = PathBuf::from(path.to_string_lossy().replace('\\', "/"));
    }
    if list.version != 1
        || list.tracks.iter().any(|p| {
            p.components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        })
    {
        return Err("Invalid source playlist record".into());
    }
    Ok(list)
}
fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Cancelled. Completed destination files and all unmoved originals were retained; retry with this preview to continue.".into())
    } else {
        Ok(())
    }
}
fn hash(path: &Path, cancel: &AtomicBool) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        cancelled(cancel)?;
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
pub fn scan(
    root: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(String),
) -> Result<Plan, String> {
    let library = crate::scan(root).map_err(|e| e.to_string())?;
    let settings = show::Settings::load(root)?;
    let mut playlists = Vec::new();
    for path in crate::catalog_db::playlist_paths(root)? {
        playlists.push(Playlist::load(&path, root).map_err(|e| e.to_string())?);
    }
    let mut files = Vec::new();
    let mut hashes = BTreeMap::<String, Vec<usize>>::new();
    for (index, track) in library.tracks.iter().enumerate() {
        cancelled(cancel)?;
        progress(format!(
            "Checking song {} of {}: {}",
            index + 1,
            library.tracks.len(),
            track.title
        ));
        let original = crate::resolve(root, &track.path).map_err(|e| e.to_string())?;
        let digest = hash(&original, cancel)?;
        let metadata = fs::metadata(&original).map_err(|e| e.to_string())?;
        let (mut prepared, revision) = preparation::load_with_revision(&original)?;
        for key in ["embedded_tags", "embedded_genres", "audio_properties"] {
            if let Some(value) = track.preparation.extensions.get(key) {
                prepared
                    .extensions
                    .entry(key.into())
                    .or_insert_with(|| value.clone());
            }
        }
        hashes.entry(digest.clone()).or_default().push(files.len());
        files.push(Entry {
            organize_names: false,
            original,
            relative: track.path.clone(),
            hash: digest,
            bytes: metadata.len(),
            preparation: prepared,
            revision,
        });
    }
    let groups = hashes
        .into_values()
        .map(|members| {
            let keeper = *members
                .iter()
                .max_by_key(|&&i| {
                    let p = &files[i].preparation;
                    (
                        p.manual_override,
                        p.manual_cues.len(),
                        p.analysis.is_some(),
                        p.cues.len(),
                    )
                })
                .unwrap();
            Group { members, keeper }
        })
        .collect();
    Ok(Plan {
        id: uuid::Uuid::new_v4().to_string(),
        root: root.canonicalize().map_err(|e| e.to_string())?,
        files,
        groups,
        settings,
        playlists,
    })
}
pub fn target_relative(entry: &Entry) -> PathBuf {
    if entry.organize_names {
        let tag = |key: &str, embedded: &str, fallback: &str| {
            let v = entry.preparation.extensions.get(key).or_else(|| {
                entry
                    .preparation
                    .extensions
                    .get("embedded_tags")?
                    .get(embedded)
            });
            let value = v
                .and_then(|v| v.as_str().or_else(|| v.as_array()?.first()?.as_str()))
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(fallback);
            safe_component(value)
        };
        let title = entry
            .original
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Track");
        let ext = entry
            .original
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("mp3");
        return PathBuf::from(tag("artist", "TrackArtist", "Unknown artist"))
            .join(tag("album", "AlbumTitle", "Unknown album"))
            .join(format!(
                "{} - {}.{}",
                tag("title", "TrackTitle", title),
                entry
                    .hash
                    .get(..12)
                    .filter(|v| v.bytes().all(|c| c.is_ascii_hexdigit()))
                    .unwrap_or("unverified"),
                ext
            ));
    }
    destination_path(&entry.relative)
}
fn safe_component(value: &str) -> String {
    let value: String = value
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(80)
        .collect();
    let value = value.trim().trim_matches('.').trim();
    let upper = value.to_ascii_uppercase();
    if value.is_empty() {
        "Unknown".into()
    } else if matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        format!("_{value}")
    } else {
        value.into()
    }
}
fn destination_path(relative: &Path) -> PathBuf {
    let path = if let Ok(tail) = relative.strip_prefix(crate::music_sources::PREFIX) {
        PathBuf::from("Sources").join(tail)
    } else {
        PathBuf::from("Library").join(relative)
    };
    PathBuf::from(path.to_string_lossy().replace('\\', "/"))
}

fn track(entry: &Entry, path: PathBuf) -> Track {
    Track {
        path,
        title: String::new(),
        preparation: entry.preparation.clone(),
    }
}
fn journal(
    destination: &Path,
    plan: &Plan,
    state: &str,
    report: Option<&Report>,
) -> Result<(), String> {
    let file = destination.join("ampdj/consolidation.ampdb");
    if crate::catalog_db::local_record(&file, "plan")?.is_none() {
        crate::catalog_db::save_local_record(
            &file,
            "plan",
            &serde_json::to_value(plan).map_err(|e| e.to_string())?,
        )?;
    }
    crate::catalog_db::save_local_record(
        &file,
        "migration",
        &serde_json::json!({"plan_id":plan.id,"state":state,"report":report}),
    )
}
fn ensure_source(entry: &Entry, cancel: &AtomicBool) -> Result<(), String> {
    let metadata = fs::symlink_metadata(&entry.original).map_err(|e| e.to_string())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != entry.bytes
        || hash(&entry.original, cancel)? != entry.hash
    {
        return Err(format!(
            "Source changed since the preview: {}. Scan again.",
            entry.original.display()
        ));
    }
    if preparation::load_with_revision(&entry.original)?.1 != entry.revision {
        return Err(format!(
            "Cues or analysis changed since the preview: {}. Scan again.",
            entry.original.display()
        ));
    }
    Ok(())
}
fn copy_verified(entry: &Entry, destination: &Path, cancel: &AtomicBool) -> Result<(), String> {
    if destination.exists() {
        if fs::symlink_metadata(destination).is_ok_and(|m| m.file_type().is_symlink())
            || hash(destination, cancel)? != entry.hash
        {
            return Err(format!(
                "Destination collision: {}. No file was overwritten.",
                destination.display()
            ));
        }
        return Ok(());
    }
    let parent = destination.parent().ok_or("Missing destination folder")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut source = fs::File::open(&entry.original).map_err(|e| e.to_string())?;
    let mut buffer = [0; 65536];
    let mut digest = Sha256::new();
    loop {
        cancelled(cancel)?;
        let n = source.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        temporary
            .write_all(&buffer[..n])
            .map_err(|e| e.to_string())?;
        digest.update(&buffer[..n]);
    }
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    if format!("{:x}", digest.finalize()) != entry.hash
        || hash(temporary.path(), cancel)? != entry.hash
    {
        return Err("Audio verification failed; original retained.".into());
    }
    temporary
        .persist_noclobber(destination)
        .map_err(|e| e.to_string())?;
    Ok(())
}
/// Optional callback preserves device-local resources (such as stem caches) before originals are removed.
pub fn execute(
    plan: &Plan,
    destination: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(String),
    mut resources: impl FnMut(&Path, &Path) -> Result<(), String>,
) -> Result<Report, String> {
    cancelled(cancel)?;
    crate::storage::register(&plan.root);
    if plan.files.is_empty() {
        return Err("No songs in this preview".into());
    }
    let destination = destination.canonicalize().map_err(|e| e.to_string())?;
    if !destination.is_dir()
        || fs::symlink_metadata(&destination).is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("Choose a real, empty destination folder.".into());
    }
    if crate::folder_contains(&destination, &plan.root)
        || crate::folder_contains(&plan.root, &destination)
        || plan.files.iter().any(|f| {
            crate::folder_contains(&destination, f.original.parent().unwrap())
                || crate::folder_contains(&f.original, &destination)
        })
    {
        return Err("The destination must be separate from every source folder.".into());
    }
    let previous = crate::catalog_db::local_record(
        &destination.join("ampdj/consolidation.ampdb"),
        "migration",
    )?;
    let resume = previous.as_ref().is_some_and(|v| v["plan_id"] == plan.id);
    if !resume
        && fs::read_dir(&destination)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err("Destination is not empty. Choose an empty folder; existing files will not be overwritten.".into());
    }
    if resume
        && crate::catalog_db::local_record(&destination.join("ampdj/consolidation.ampdb"), "plan")?
            .as_ref()
            != Some(&serde_json::to_value(plan).map_err(|e| e.to_string())?)
    {
        return Err("The keeper choices changed after this migration started. Restore the original choices before retrying, or choose a new empty destination.".into());
    }
    let current = show::Settings::load(&plan.root)?;
    if serde_json::to_value(&current).map_err(|e| e.to_string())?
        != serde_json::to_value(&plan.settings).map_err(|e| e.to_string())?
    {
        return Err(
            "Smart Playlists changed after the preview. Scan again before consolidating.".into(),
        );
    }
    let mut current_lists = Vec::new();
    for path in crate::catalog_db::playlist_paths(&plan.root)? {
        current_lists.push(playlist_record(&path)?);
    }
    if serde_json::to_value(&current_lists).map_err(|e| e.to_string())?
        != serde_json::to_value(&plan.playlists).map_err(|e| e.to_string())?
    {
        return Err("Playlists changed after the preview. Scan again.".into());
    }
    let mut mapping = HashMap::new();
    let mut ids = HashMap::new();
    let mut keepers = Vec::new();
    for group in &plan.groups {
        if !group.members.contains(&group.keeper) {
            return Err("Invalid keeper selection".into());
        }
        let keeper = plan
            .files
            .get(group.keeper)
            .ok_or("Invalid keeper selection")?;
        let relative = target_relative(keeper);
        for &index in &group.members {
            let entry = plan.files.get(index).ok_or("Invalid duplicate preview")?;
            if entry.hash != keeper.hash {
                return Err("Duplicate contents differ".into());
            }
            mapping.insert(entry.relative.clone(), relative.clone());
            ids.insert(
                show::song_id(&track(entry, entry.relative.clone())),
                show::song_id(&track(keeper, relative.clone())),
            );
        }
        keepers.push((group.keeper, relative));
    }
    if mapping.len() != plan.files.len() {
        return Err("Preview does not cover all songs".into());
    }
    for entry in &plan.files {
        if !crate::supported(&entry.original) {
            return Err("Migration journals may only refer to supported music files".into());
        }
        if entry.original.exists()
            && crate::resolve(&plan.root, &entry.relative).map_err(|e| e.to_string())?
                != entry.original
        {
            return Err("Music source locations changed after the preview. Scan again.".into());
        }
    }
    journal(&destination, plan, "copying", None)?;
    for (slot, (index, relative)) in keepers.iter().enumerate() {
        cancelled(cancel)?;
        let entry = &plan.files[*index];
        progress(format!(
            "Verifying song {} of {}: {}",
            slot + 1,
            keepers.len(),
            entry.original.display()
        ));
        // A retry may find an original already removed after a fully published migration.
        if entry.original.exists() {
            ensure_source(entry, cancel)?;
        }
        if relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("Invalid destination path".into());
        }
        let dest = destination.join(relative);
        fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
        if !dest
            .parent()
            .unwrap()
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(&destination)
            || fs::symlink_metadata(&dest).is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(
                "Destination path escaped the consolidation folder; originals retained.".into(),
            );
        }

        if !dest.exists() {
            copy_verified(entry, &dest, cancel)?;
        } else if hash(&dest, cancel)? != entry.hash {
            return Err("Destination changed; originals retained.".into());
        }
        let mut prep = entry.preparation.clone();
        if let Some(id) = prep.extensions.get("artwork_id").and_then(|v| v.as_str()) {
            let (owner, _) = crate::music_sources::owner(&plan.root, &entry.relative)
                .map_err(|e| e.to_string())?;
            if let Some(data) = crate::artwork::read(&owner, id)? {
                crate::artwork::save(&destination, &data)?;
            } else {
                return Err("A selected artwork asset is missing; originals retained".into());
            }
        }
        prep.extensions.remove("_amp_sync");
        if !resume || preparation::revision(&dest).is_none() {
            preparation::save(&destination, relative, &prep)?;
        }
        if entry.original.exists() {
            resources(&entry.original, &dest)?;
        }
    }
    let mut settings = plan.settings.clone();
    settings.sync = None;
    let library_id = coordination::library_id(&destination)?;
    let remap_ids = |list: &mut Vec<String>| {
        for id in list.iter_mut() {
            if let Some(new) = ids.get(id) {
                *id = new.clone();
            }
        }
        list.sort();
        list.dedup();
    };
    for profile in &mut settings.playlists {
        let displaced: Vec<_> = plan
            .files
            .iter()
            .filter(|f| {
                let old = track(f, f.relative.clone());
                show::eligible(profile, &old)
                    && mapping
                        .get(&f.relative)
                        .is_some_and(|new| new != &destination_path(&f.relative))
            })
            .filter_map(|f| {
                ids.get(&show::song_id(&track(f, f.relative.clone())))
                    .cloned()
            })
            .collect();
        for folder in &mut profile.folders {
            *folder = destination_path(folder);
        }
        remap_ids(&mut profile.allowed);
        remap_ids(&mut profile.excluded);
        remap_ids(&mut profile.preferred);
        if let Some(id) = &mut profile.recognition_song
            && let Some(new) = ids.get(id)
        {
            *id = new.clone();
        }
        for source in &mut profile.playlist_sources {
            remap_ids(&mut source.songs);
        }
        if !displaced.is_empty() {
            profile.playlist_sources.push(show::PlaylistSource {
                name: "Consolidated duplicate references".into(),
                songs: displaced,
            });
        }
        if !profile.source_unavailable {
            profile.source_library = Some(library_id.clone());
            profile.source_root_hint = Some(destination.clone());
        }
    }
    // Retry uses the destination's current revision, but only within this migration's private destination.
    if resume {
        let mut current = show::Settings::load(&destination)?;
        let sync = current.sync.take();
        if sync.is_some()
            && serde_json::to_value(&current).map_err(|e| e.to_string())?
                != serde_json::to_value(&settings).map_err(|e| e.to_string())?
        {
            return Err("Destination Smart Playlists were edited; originals retained. Review the destination before retrying.".into());
        }
        settings.sync = sync;
    }
    settings.save(&destination)?;
    fs::create_dir_all(destination.join("ampdj/playlists")).map_err(|e| e.to_string())?;
    for (index, list) in plan.playlists.iter().enumerate() {
        let mut list = list.clone();
        list.sync = None;
        list.tracks = list
            .tracks
            .iter()
            .map(|p| {
                mapping
                    .get(p)
                    .cloned()
                    .ok_or_else(|| format!("Unresolved playlist song: {}", p.display()))
            })
            .collect::<Result<_, _>>()?;
        let path = destination.join(format!("ampdj/playlists/list-{index}.json"));
        if resume && let Ok(mut current) = Playlist::load(&path, &destination) {
            let sync = current.sync.take();
            if serde_json::to_value(&current).map_err(|e| e.to_string())?
                != serde_json::to_value(&list).map_err(|e| e.to_string())?
            {
                return Err("Destination playlist was edited; originals retained.".into());
            }
            list.sync = sync;
        }
        list.save_in_library(&path, &destination)
            .map_err(|e| e.to_string())?;
    }
    if !destination.join("ampdj/catalog.ampdb").is_file() {
        crate::catalog_db::compact(&destination)?;
    }
    let loaded = crate::scan(&destination).map_err(|e| e.to_string())?;
    if loaded.tracks.len() != keepers.len() {
        return Err("Consolidated catalog count mismatch; originals retained.".into());
    }
    journal(&destination, plan, "published", None)?;
    let mut report = Report {
        destination: destination.clone(),
        moved: 0,
        retained: plan.files.len() - keepers.len(),
        warnings: vec![],
    };
    for (index, relative) in &keepers {
        cancelled(cancel)?;
        let entry = &plan.files[*index];
        progress(format!("Finishing move: {}", entry.original.display()));
        if !entry.original.exists() {
            report.moved += 1;
            continue;
        }
        // Coordinate with writers of the original preparation record; never remove newer edits.
        let result = (|| {
            let record = preparation::portable(&entry.original);
            coordination::directory(record.parent().ok_or("Missing metadata parent")?)?;
            let _guard = coordination::Lock::acquire(&record)?;
            ensure_source(entry, cancel)?;
            if hash(&destination.join(relative), cancel)? != entry.hash {
                return Err("Destination verification failed".into());
            }
            fs::remove_file(&entry.original).map_err(|e| e.to_string())
        })();
        match result {
            Ok(()) => report.moved += 1,
            Err(error) => {
                report.retained += 1;
                report.warnings.push(format!(
                    "Original retained: {} ({error})",
                    entry.original.display()
                ));
            }
        }
        journal(&destination, plan, "moving", Some(&report))?;
    }
    cancelled(cancel)?;
    journal(&destination, plan, "complete", Some(&report))?;
    Ok(report)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn artist_album_rename_preserves_portable_artwork_and_cues() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        fs::write(source.path().join("song.mp3"), b"unique music").unwrap();
        let id = crate::artwork::save(source.path(), b"cover bytes").unwrap();
        let mut prep = Preparation {
            cue_in: 5.,
            bpm: Some(120.),
            manual_override: true,
            ..Default::default()
        };
        prep.extensions
            .insert("title".into(), serde_json::json!("My song"));
        prep.extensions
            .insert("artist".into(), serde_json::json!("A/B"));
        prep.extensions
            .insert("album".into(), serde_json::json!("CON"));
        prep.extensions
            .insert("artwork_id".into(), serde_json::json!(id));
        preparation::save(source.path(), Path::new("song.mp3"), &prep).unwrap();
        let cancel = AtomicBool::new(false);
        let mut plan = scan(source.path(), &cancel, |_| {}).unwrap();
        plan.files[0].organize_names = true;
        let relative = target_relative(&plan.files[0]);
        assert!(relative.starts_with("A_B/_CON"));
        execute(&plan, dest.path(), &cancel, |_| {}, |_, _| Ok(())).unwrap();
        let saved = preparation::load(&dest.path().join(&relative)).unwrap();
        assert_eq!(saved.cue_in, 5.);
        assert_eq!(saved.bpm, Some(120.));
        assert_eq!(
            crate::artwork::read(dest.path(), &id).unwrap().unwrap(),
            b"cover bytes"
        );
    }
    #[test]
    fn interrupted_deletion_resumes_even_when_source_playlist_tracks_have_moved() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        for (name, audio) in [("one.mp3", b"one"), ("two.mp3", b"two")] {
            fs::write(source.path().join(name), audio).unwrap();
        }
        fs::create_dir_all(source.path().join("ampdj/playlists")).unwrap();
        let list = Playlist {
            companion_available: false,
            sync: None,
            version: 1,
            name: "Both".into(),
            tracks: vec!["one.mp3".into(), "two.mp3".into()],
        };
        list.save_in_library(
            &source.path().join("ampdj/playlists/both.json"),
            source.path(),
        )
        .unwrap();
        let cancel = AtomicBool::new(false);
        let plan = scan(source.path(), &cancel, |_| {}).unwrap();
        let mut finishing = 0;
        assert!(
            execute(
                &plan,
                dest.path(),
                &cancel,
                |text| {
                    if text.starts_with("Finishing move:") {
                        finishing += 1;
                        if finishing == 2 {
                            cancel.store(true, Ordering::Relaxed);
                        }
                    }
                },
                |_, _| Ok(())
            )
            .is_err()
        );
        assert_eq!(
            ["one.mp3", "two.mp3"]
                .iter()
                .filter(|name| source.path().join(name).exists())
                .count(),
            1
        );
        cancel.store(false, Ordering::Relaxed);
        let recovered = resume_plan(dest.path()).unwrap();
        let report = execute(&recovered, dest.path(), &cancel, |_| {}, |_, _| Ok(())).unwrap();
        assert_eq!(report.moved, 2);
    }
    #[test]
    fn multiple_sources_and_identical_names_keep_unique_contents_and_song_references() {
        let main = tempfile::tempdir().unwrap();
        let extra = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        fs::write(main.path().join("song.mp3"), b"main song").unwrap();
        fs::write(extra.path().join("song.mp3"), b"different song").unwrap();
        crate::music_sources::add(main.path(), extra.path()).unwrap();
        let library = crate::scan(main.path()).unwrap();
        let external = library
            .tracks
            .iter()
            .find(|t| t.path.starts_with(crate::music_sources::PREFIX))
            .unwrap();
        let mut settings = show::Settings::default();
        settings.playlists[0].allowed = vec![show::song_id(external)];
        settings.playlists[0].restrict_allowed = true;
        settings.playlists[0].recognition_song = Some(show::song_id(external));
        settings.save(main.path()).unwrap();
        let cancel = AtomicBool::new(false);
        let plan = scan(main.path(), &cancel, |_| {}).unwrap();
        let report = execute(&plan, dest.path(), &cancel, |_| {}, |_, _| Ok(())).unwrap();
        assert_eq!(report.moved, 2);
        let songs = crate::scan(dest.path()).unwrap();
        assert_eq!(songs.tracks.len(), 2);
        let rules = show::Settings::load(dest.path()).unwrap();
        let eligible: Vec<_> = songs
            .tracks
            .iter()
            .filter(|t| show::eligible(&rules.playlists[0], t))
            .collect();
        assert_eq!(eligible.len(), 1);
        assert_eq!(
            rules.playlists[0].recognition_song,
            Some(show::song_id(eligible[0]))
        );
        assert_eq!(resume_plan(dest.path()).unwrap().id, plan.id);
    }
    #[test]
    fn interrupted_copy_can_resume_without_removing_unverified_originals() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        fs::write(source.path().join("one.mp3"), b"one").unwrap();
        fs::write(source.path().join("two.mp3"), b"two").unwrap();
        let cancel = AtomicBool::new(false);
        let plan = scan(source.path(), &cancel, |_| {}).unwrap();
        assert!(
            execute(
                &plan,
                dest.path(),
                &cancel,
                |_| {},
                |_, _| {
                    cancel.store(true, Ordering::Relaxed);
                    Ok(())
                }
            )
            .is_err()
        );
        assert!(source.path().join("one.mp3").exists() && source.path().join("two.mp3").exists());
        cancel.store(false, Ordering::Relaxed);
        let report = execute(&plan, dest.path(), &cancel, |_| {}, |_, _| Ok(())).unwrap();
        assert_eq!(report.moved, 2, "{:?}", report.warnings);
    }
    #[test]
    fn metadata_edited_during_copy_is_preserved_at_its_original_location() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        fs::write(source.path().join("song.mp3"), b"song").unwrap();
        let cancel = AtomicBool::new(false);
        let plan = scan(source.path(), &cancel, |_| {}).unwrap();
        let report = execute(
            &plan,
            dest.path(),
            &cancel,
            |_| {},
            |_, _| {
                preparation::save(
                    source.path(),
                    Path::new("song.mp3"),
                    &Preparation {
                        cue_in: 3.,
                        manual_override: true,
                        ..Default::default()
                    },
                )
            },
        )
        .unwrap();
        assert_eq!(report.moved, 0);
        assert!(source.path().join("song.mp3").exists());
        assert_eq!(
            crate::scan(source.path()).unwrap().tracks[0]
                .preparation
                .cue_in,
            3.
        );
    }
    #[test]
    fn unique_audio_moves_duplicates_remain_and_cues_and_playlists_follow() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        fs::create_dir(source.path().join("Rock")).unwrap();
        fs::write(source.path().join("Rock/a.mp3"), b"unique audio").unwrap();
        fs::write(source.path().join("copy.mp3"), b"unique audio").unwrap();
        fs::write(source.path().join("b.mp3"), b"another audio").unwrap();
        let prep = Preparation {
            bpm: Some(123.),
            cue_in: 2.,
            manual_override: true,
            analysis: Some(preparation::AnalysisInfo {
                engine: "fixture".into(),
                duration: 30.,
                tempo_confidence: 0.9,
                key_confidence: 0.8,
                reviewed: true,
            }),
            ..Default::default()
        };
        preparation::save(source.path(), Path::new("Rock/a.mp3"), &prep).unwrap();
        let library = crate::scan(source.path()).unwrap();
        let mut settings = show::Settings::default();
        settings.playlists[0].folders = vec!["Rock".into()];
        settings.playlists[0].restrict_folders = true;
        settings.save(source.path()).unwrap();
        fs::create_dir_all(source.path().join("ampdj/playlists")).unwrap();
        let list = Playlist {
            companion_available: false,
            sync: None,
            version: 1,
            name: "Favorites".into(),
            tracks: vec!["copy.mp3".into(), "b.mp3".into()],
        };
        list.save_in_library(
            &source.path().join("ampdj/playlists/favs.json"),
            source.path(),
        )
        .unwrap();
        let cancel = AtomicBool::new(false);
        let plan = scan(source.path(), &cancel, |_| {}).unwrap();
        assert_eq!(plan.groups.len(), 2);
        let report = execute(&plan, dest.path(), &cancel, |_| {}, |_, _| Ok(())).unwrap();
        assert_eq!(report.moved, 2, "{:?}", report.warnings);
        assert_eq!(report.retained, 1);
        assert!(source.path().join("copy.mp3").exists());
        assert!(!source.path().join("Rock/a.mp3").exists());
        let migrated = crate::scan(dest.path()).unwrap();
        assert_eq!(migrated.tracks.len(), 2);
        let a = migrated
            .tracks
            .iter()
            .find(|t| t.preparation.bpm == Some(123.))
            .unwrap();
        assert_eq!(a.preparation.cue_in, 2.);
        assert_eq!(a.preparation.analysis.as_ref().unwrap().engine, "fixture");
        let rules = show::Settings::load(dest.path()).unwrap();
        assert!(show::eligible(&rules.playlists[0], a));
        let lists = crate::catalog_db::playlist_paths(dest.path()).unwrap();
        let list = Playlist::load(&lists[0], dest.path()).unwrap();
        assert_eq!(list.tracks.len(), 2);
        assert!(list.tracks.contains(&a.path));
        assert_eq!(library.tracks.len(), 3);
    }
    #[test]
    fn cancellation_collision_and_changed_source_never_delete_originals() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        fs::write(source.path().join("a.mp3"), b"original").unwrap();
        let cancel = AtomicBool::new(false);
        let plan = scan(source.path(), &cancel, |_| {}).unwrap();
        fs::write(dest.path().join("existing"), b"keep").unwrap();
        assert!(execute(&plan, dest.path(), &cancel, |_| {}, |_, _| Ok(())).is_err());
        assert!(source.path().join("a.mp3").exists());
        cancel.store(true, Ordering::Relaxed);
        assert!(execute(&plan, dest.path(), &cancel, |_| {}, |_, _| Ok(())).is_err());
        assert!(source.path().join("a.mp3").exists());
        let empty = tempfile::tempdir().unwrap();
        cancel.store(false, Ordering::Relaxed);
        fs::write(source.path().join("a.mp3"), b"changed").unwrap();
        assert!(execute(&plan, empty.path(), &cancel, |_| {}, |_, _| Ok(())).is_err());
        assert!(source.path().join("a.mp3").exists());
    }
}
