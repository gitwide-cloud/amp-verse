//! Offline-first library, preparation data and process audio adapter. No GUI or network dependency.
pub mod dj_engine;
pub mod dj_logic;
pub mod library_engine;
#[cfg(feature = "analysis")]
pub use library_engine::analysis;
pub mod arrangement;
pub mod background;
pub mod batch;
pub(crate) use library_engine::catalog_shards;
pub mod coordination;
pub mod discovery;
pub mod podcast_feed;
pub mod energy_profile;
pub mod engine_api;
pub mod show;
pub mod smart;
pub mod smart_filters;
// Compatibility names keep existing integrations on the same implementation.
pub use dj_engine::{stem_archive, stem_mix};
pub use dj_logic::{dj_phrasing, mix_schedule, transition_plan};
pub use library_engine::{
    analysis_queue, artwork, catalog_db, consolidate, genres, media_kind, music_sources, offline,
    preparation, review, storage,
};

use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Preparation {
    pub cue_in: f64,
    pub cue_out: Option<f64>,
    pub bpm: Option<f64>,
    pub beat_offset: Option<f64>,
    #[serde(default)]
    pub stems: Vec<String>,
    #[serde(default)]
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub harmonic_key: Option<String>,
    #[serde(default)]
    pub cues: Vec<preparation::Cue>,
    #[serde(default)]
    pub manual_cues: Vec<preparation::Cue>,
    #[serde(default)]
    pub manual_override: bool,
    #[serde(default)]
    pub analysis: Option<preparation::AnalysisInfo>,
    #[serde(default)]
    pub source_sha256: Option<String>,
    #[serde(flatten)]
    pub extensions: std::collections::BTreeMap<String, serde_json::Value>,
}
impl Preparation {
    pub fn validate(&self) -> Result<(), String> {
        if !self.cue_in.is_finite() || self.cue_in < 0.0 {
            return Err("Cue IN must be finite and nonnegative".into());
        }
        if self
            .cue_out
            .is_some_and(|x| !x.is_finite() || x <= self.cue_in)
        {
            return Err("Cue OUT must follow IN".into());
        }
        if self.bpm.is_some_and(|x| !x.is_finite() || x <= 0.0) {
            return Err("BPM must be positive".into());
        }
        if self.beat_offset.is_some_and(|x| !x.is_finite() || x < 0.0) {
            return Err("Beat offset must be nonnegative".into());
        }
        preparation::validate(self)?;
        stem_mix::knobs(self)?;
        energy_profile::validate(self)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// Relative to the configured music root; never a remote executable or URL.
    pub path: PathBuf,
    pub title: String,
    pub preparation: Preparation,
}
impl Track {
    /// Stable identity for this track, derived from its library-relative path.
    /// Two scans (full or incremental) of the same file always produce the same
    /// ID; it is not stored, so it needs no migration and survives file moves
    /// only if the path is unchanged. Position in `Library::tracks` is not an ID.
    pub fn id(&self) -> String {
        track_id(&self.path)
    }
}
/// Deterministic stable ID for a library-relative track path. Not a secret: it
/// only needs to be stable and collision-resistant for one library's track count.
pub fn track_id(relative: &Path) -> String {
    use sha2::{Digest, Sha256};
    let normalized = relative.to_string_lossy().replace('\\', "/");
    let digest = Sha256::digest(normalized.as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub name: String,
    pub folders: Vec<Folder>,
    pub tracks: Vec<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub tracks: Vec<Track>,
    pub tree: Folder,
}

pub fn sidecar(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_os_string();
    p.push(".amp.json");
    p.into()
}

pub fn scan(root: &Path) -> io::Result<Library> {
    scan_mode(root, true)
}
/// Enumerate job inputs without reading audio tags or preparation for every file.
/// Analysis workers reload preparation under the shared lease; these are not playback snapshots.
pub fn scan_job_inputs(root: &Path) -> io::Result<Library> {
    scan_mode(root, false)
}
fn scan_mode(root: &Path, metadata: bool) -> io::Result<Library> {
    let mut library = scan_single(root, metadata)?;
    music_sources::append(root, &mut library, metadata)?;
    Ok(library)
}
/// Stat fingerprint recorded on every scanned track so a later refresh can tell,
/// without reopening the audio file, whether its tags need rereading.
fn source_stat(entry: &fs::DirEntry) -> Option<serde_json::Value> {
    let stat = entry.metadata().ok()?;
    let modified_ns = stat
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos()
        .to_string();
    Some(serde_json::json!({ "size": stat.len(), "modified_ns": modified_ns }))
}

/// Build a `Track` by reading tags (when `metadata`) or deferring to defaults, then
/// stamping the current stat fingerprint so a future refresh can detect "unchanged".
fn tag_track(
    entry: &fs::DirEntry,
    path: &Path,
    root: &Path,
    catalog: &mut std::collections::BTreeMap<String, Preparation>,
    metadata: bool,
) -> io::Result<Track> {
    let relative: PathBuf = path.strip_prefix(root).unwrap().into();
    let key = format!("tracks/{}.json", relative.to_string_lossy().replace('\\', "/"));
    let mut preparation = if !metadata {
        Preparation::default()
    } else if let Some(prep) = catalog.remove(&key) {
        prep
    } else {
        preparation::load(path).map_err(io::Error::other)?
    };
    if metadata {
        genres::read_embedded(path, &mut preparation);
    }
    if let Some(stat) = source_stat(entry) {
        preparation
            .extensions
            .insert("scan_source_stat".into(), stat);
    }
    let content = media_kind::kind(path, &preparation);
    if content != media_kind::Kind::Music {
        preparation
            .extensions
            .insert("media_kind_detected".into(), serde_json::json!(content.id()));
    }
    let title = preparation
        .extensions
        .get("title")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into());
    Ok(Track {
        path: relative,
        title,
        preparation,
    })
}

fn scan_single(root: &Path, metadata: bool) -> io::Result<Library> {
    let root = root.canonicalize()?;
    storage::register(&root);
    let mut tracks = Vec::new();
    let mut catalog = if metadata {
        catalog_db::preparations(&root).map_err(io::Error::other)?
    } else {
        Default::default()
    };
    fn visit(
        dir: &Path,
        root: &Path,
        tracks: &mut Vec<Track>,
        catalog: &mut std::collections::BTreeMap<String, Preparation>,
        metadata: bool,
    ) -> io::Result<Folder> {
        let mut folder = Folder {
            name: dir.file_name().unwrap_or_default().to_string_lossy().into(),
            folders: vec![],
            tracks: vec![],
        };
        let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let ty = entry.file_type()?;
            // Do not follow symlinks, including links outside the root or cyclic directories.
            if ty.is_symlink() {
                continue;
            }
            let path = entry.path();
            if ty.is_dir() && (entry.file_name() == "ampdj" || entry.file_name() == ".ampverse") {
                continue;
            }
            if ty.is_dir() {
                folder
                    .folders
                    .push(visit(&path, root, tracks, catalog, metadata)?);
            } else if ty.is_file() && supported(&path) {
                folder.tracks.push(tracks.len());
                tracks.push(tag_track(&entry, &path, root, catalog, metadata)?);
            }
        }
        Ok(folder)
    }
    let tree = visit(&root, &root, &mut tracks, &mut catalog, metadata)?;
    Ok(Library { tracks, tree })
}

/// Counts from an incremental [`refresh`], distinguishing files whose tags were
/// actually reread from files whose cached entry was reused unchanged.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RefreshStats {
    /// Files visited on disk during this refresh.
    pub considered: usize,
    /// Files reused from `previous` without reopening/retagging the audio file.
    pub reused: usize,
    /// Files that were new, changed, or otherwise required a fresh tag read.
    pub tagged: usize,
    /// Files present in `previous` that are no longer on disk.
    pub removed: usize,
}

/// Incrementally reconcile `previous` against the current state of `root`: unchanged
/// files (same size and modification time) are reused verbatim, including their
/// preparation data and stable [`Track::id`]; only added/changed files are retagged,
/// and files no longer present are dropped. Does not touch `root`'s additional
/// configured music sources beyond the existing full-rescan behavior for those.
pub fn refresh(previous: &Library, root: &Path) -> io::Result<(Library, RefreshStats)> {
    refresh_mode(previous, root, true)
}
/// Incremental counterpart to [`scan_job_inputs`].
pub fn refresh_job_inputs(previous: &Library, root: &Path) -> io::Result<(Library, RefreshStats)> {
    refresh_mode(previous, root, false)
}
fn refresh_mode(
    previous: &Library,
    root: &Path,
    metadata: bool,
) -> io::Result<(Library, RefreshStats)> {
    let (mut library, stats) = refresh_single(previous, root, metadata)?;
    music_sources::append(root, &mut library, metadata)?;
    Ok((library, stats))
}
fn refresh_single(
    previous: &Library,
    root: &Path,
    metadata: bool,
) -> io::Result<(Library, RefreshStats)> {
    let root = root.canonicalize()?;
    storage::register(&root);
    let mut previous_by_path = std::collections::HashMap::with_capacity(previous.tracks.len());
    for track in &previous.tracks {
        previous_by_path.insert(track.path.as_path(), track);
    }
    let mut tracks = Vec::new();
    let mut seen = std::collections::HashSet::with_capacity(previous.tracks.len());
    let mut stats = RefreshStats::default();
    let mut catalog = if metadata {
        catalog_db::preparations(&root).map_err(io::Error::other)?
    } else {
        Default::default()
    };
    fn visit(
        dir: &Path,
        root: &Path,
        tracks: &mut Vec<Track>,
        catalog: &mut std::collections::BTreeMap<String, Preparation>,
        previous_by_path: &std::collections::HashMap<&Path, &Track>,
        seen: &mut std::collections::HashSet<PathBuf>,
        stats: &mut RefreshStats,
        metadata: bool,
    ) -> io::Result<Folder> {
        let mut folder = Folder {
            name: dir.file_name().unwrap_or_default().to_string_lossy().into(),
            folders: vec![],
            tracks: vec![],
        };
        let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let ty = entry.file_type()?;
            if ty.is_symlink() {
                continue;
            }
            let path = entry.path();
            if ty.is_dir() && (entry.file_name() == "ampdj" || entry.file_name() == ".ampverse") {
                continue;
            }
            if ty.is_dir() {
                folder.folders.push(visit(
                    &path,
                    root,
                    tracks,
                    catalog,
                    previous_by_path,
                    seen,
                    stats,
                    metadata,
                )?);
            } else if ty.is_file() && supported(&path) {
                stats.considered += 1;
                let relative: PathBuf = path.strip_prefix(root).unwrap().into();
                seen.insert(relative.clone());
                let current_stat = source_stat(&entry);
                let previous_track = previous_by_path.get(relative.as_path()).copied();
                let unchanged = metadata
                    && current_stat.is_some()
                    && previous_track.is_some_and(|prev| {
                        prev.preparation.extensions.get("scan_source_stat") == current_stat.as_ref()
                    });
                folder.tracks.push(tracks.len());
                if unchanged {
                    stats.reused += 1;
                    tracks.push(previous_track.unwrap().clone());
                } else {
                    stats.tagged += 1;
                    tracks.push(tag_track(&entry, &path, root, catalog, metadata)?);
                }
            }
        }
        Ok(folder)
    }
    let tree = visit(
        &root,
        &root,
        &mut tracks,
        &mut catalog,
        &previous_by_path,
        &mut seen,
        &mut stats,
        metadata,
    )?;
    stats.removed = previous
        .tracks
        .iter()
        .filter(|t| !seen.contains(&t.path))
        .count();
    Ok((Library { tracks, tree }, stats))
}
pub fn supported(p: &Path) -> bool {
    p.extension().is_some_and(|e| {
        matches!(
            e.to_string_lossy().to_lowercase().as_str(),
            "mp3" | "wav" | "flac" | "ogg" | "opus" | "m4a" | "m4b" | "aac"
        )
    })
}

/// Component-aware folder selection. Windows folder casing is not significant;
/// Unix libraries retain their native case-sensitive behavior.
pub fn folder_contains(path: &Path, folder: &Path) -> bool {
    if cfg!(windows) {
        let mut parts = path.components();
        folder.components().all(|wanted| {
            parts.next().is_some_and(|part| {
                part.as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&wanted.as_os_str().to_string_lossy())
            })
        })
    } else {
        path.starts_with(folder)
    }
}
/// Return the original spelling of the relative portion, without substring-prefix matches.
pub fn relative_folder(path: &Path, root: &Path) -> Option<PathBuf> {
    if !folder_contains(path, root) {
        return music_sources::relative(root, path);
    }
    Some(path.components().skip(root.components().count()).collect())
}

/// Resolve an untrusted playlist entry without escaping the library root.
pub fn resolve(root: &Path, relative: &Path) -> io::Result<PathBuf> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(io::Error::other("Expected a relative library path"));
    }
    let (root, relative) = music_sources::owner(root, relative)?;
    let file = root.join(relative).canonicalize()?;
    if !file.starts_with(root) || !file.is_file() {
        return Err(io::Error::other("Track outside library"));
    }
    Ok(file)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Playlist {
    #[serde(default)]
    pub companion_available: bool,
    #[serde(default, rename = "_amp_sync", skip_serializing_if = "Option::is_none")]
    pub sync: Option<coordination::Revision>,
    pub version: u32,
    pub name: String,
    pub tracks: Vec<PathBuf>,
}
impl Playlist {
    pub fn load(path: &Path, root: &Path) -> io::Result<Self> {
        let bytes = if let Some(bytes) = catalog_db::read(path).map_err(io::Error::other)? {
            bytes
        } else {
            fs::read(path)?
        };
        let mut value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        coordination::adopt_legacy(&mut value).map_err(io::Error::other)?;
        let mut list: Self = serde_json::from_value(value).map_err(io::Error::other)?;
        for path in &mut list.tracks {
            *path = PathBuf::from(path.to_string_lossy().replace('\\', "/"));
        }
        if list.version != 1 {
            return Err(io::Error::other("Expected version 1 playlist"));
        }
        for p in &list.tracks {
            resolve(root, p)?;
        }
        Ok(list)
    }
    pub fn save(&self, path: &Path) -> io::Result<()> {
        self.save_in_library(
            path,
            path.parent()
                .ok_or_else(|| io::Error::other("Playlist needs a parent"))?,
        )
    }
    pub fn save_in_library(&self, path: &Path, root: &Path) -> io::Result<()> {
        let mut portable = self.clone();
        for path in &mut portable.tracks {
            *path = PathBuf::from(path.to_string_lossy().replace('\\', "/"));
        }
        if portable.version != 1 {
            return Err(io::Error::other("Expected version 1 playlist"));
        }
        let proposed = serde_json::to_value(&portable).map_err(io::Error::other)?;
        let result = (|| {
            for track in &portable.tracks {
                resolve(root, track).map_err(|e| e.to_string())?;
            }
            let library_id = coordination::library_id(root)?;
            coordination::save_document(path, &proposed, &library_id)
        })();
        result.map_err(|e| {
            io::Error::other(coordination::retain_document(
                path,
                &proposed,
                portable.sync.as_ref().map_or("", |r| r.library_id.as_str()),
                e,
            ))
        })
    }
}

pub fn command(name: &str) -> Command {
    // Installed audio tools live beside the app; development can still use PATH.
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.parent().map(|p| {
                p.join(if cfg!(windows) {
                    format!("{name}.exe")
                } else {
                    name.to_owned()
                })
            })
        })
        .filter(|p| p.is_file());
    let executable = bundled.as_deref().unwrap_or_else(|| Path::new(name));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = Command::new(executable);
        cmd.creation_flags(0x08000000);
        cmd
    }
    #[cfg(not(windows))]
    {
        Command::new(executable)
    }
}
pub fn cue_args(prep: &Preparation) -> Result<Vec<String>, String> {
    prep.validate()?;
    let mut args = vec!["-ss".into(), prep.cue_in.to_string()];
    if let Some(end) = prep.cue_out {
        args.extend(["-t".into(), (end - prep.cue_in).to_string()]);
    }
    Ok(args)
}

/// Small-footprint prototype adapter inspired by MP3 Lab's process lifecycle.
#[derive(Default)]
pub struct Deck {
    child: Option<Child>,
}
impl Deck {
    pub fn play(&mut self, root: &Path, track: &Track, volume: u8) -> Result<(), String> {
        self.stop();
        let path = resolve(root, &track.path).map_err(|e| e.to_string())?;
        let child = command("ffplay")
            .args(["-nodisp", "-autoexit", "-loglevel", "error", "-volume"])
            .arg(volume.min(100).to_string())
            .args(cue_args(&track.preparation)?)
            .arg("-i")
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Install FFmpeg/ffplay and add it to PATH: {e}"))?;
        self.child = Some(child);
        Ok(())
    }
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    pub fn playing(&mut self) -> bool {
        match self.child.as_mut().map(Child::try_wait) {
            Some(Ok(None)) => true,
            _ => {
                self.stop();
                false
            }
        }
    }
}
impl Drop for Deck {
    fn drop(&mut self) {
        self.stop();
    }
}

pub use library_engine::transcripts;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires ffplay and SDL_AUDIODRIVER=dummy; enabled by Linux CI"]
    fn deck_plays_prepared_cues_and_cleans_up() {
        use std::time::{Duration, Instant};
        let dir = tempfile::tempdir().unwrap();
        let samples = 44100u32 * 8;
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend((36 + samples * 2).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(44100u32.to_le_bytes());
        wav.extend(88200u32.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend((samples * 2).to_le_bytes());
        wav.resize(wav.len() + samples as usize * 2, 0);
        fs::write(dir.path().join("silence.wav"), wav).unwrap();
        let track = Track {
            path: "silence.wav".into(),
            title: "silence".into(),
            preparation: Preparation {
                cue_in: 2.,
                cue_out: Some(3.),
                ..Default::default()
            },
        };
        let mut deck = Deck::default();
        deck.play(dir.path(), &track, 0).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        assert!(deck.playing(), "FFplay exited before audio playback");
        deck.stop();
        assert!(!deck.playing());
        let started = Instant::now();
        deck.play(dir.path(), &track, 0).unwrap();
        while deck.playing() {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "Cue OUT was not applied"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            started.elapsed() >= Duration::from_millis(500),
            "Decoder exited too early"
        );
    }
    #[test]
    fn cues_reject_bad_ranges_and_numbers() {
        for p in [
            Preparation {
                cue_in: -1.,
                ..Default::default()
            },
            Preparation {
                cue_out: Some(0.),
                ..Default::default()
            },
            Preparation {
                bpm: Some(f64::NAN),
                ..Default::default()
            },
        ] {
            assert!(p.validate().is_err());
        }
        assert_eq!(
            cue_args(&Preparation {
                cue_in: 2.,
                cue_out: Some(5.),
                ..Default::default()
            })
            .unwrap(),
            ["-ss", "2", "-t", "3"]
        );
    }
    #[test]
    fn tree_playlist_and_sidecars_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("Albums")).unwrap();
        fs::write(root.join("Albums/a.MP3"), []).unwrap();
        fs::write(root.join("ignore.txt"), []).unwrap();
        let prep = Preparation {
            cue_in: 1.,
            cue_out: Some(3.),
            ..Default::default()
        };
        fs::write(
            sidecar(&root.join("Albums/a.MP3")),
            serde_json::to_vec(&prep).unwrap(),
        )
        .unwrap();
        let library = scan(root).unwrap();
        assert_eq!(library.tracks.len(), 1);
        assert_eq!(library.tree.folders[0].tracks, [0]);
        // A scan always stamps a stat fingerprint (used by `refresh` to detect
        // unchanged files later); strip it before comparing sidecar fidelity.
        let mut scanned = library.tracks[0].preparation.clone();
        scanned.extensions.remove("scan_source_stat");
        assert_eq!(scanned, prep);
        let list = Playlist {
            companion_available: false,
            sync: None,
            version: 1,
            name: "Evening".into(),
            tracks: vec!["Albums/a.MP3".into()],
        };
        list.save(&root.join("evening.json")).unwrap();
        assert_eq!(
            Playlist::load(&root.join("evening.json"), root)
                .unwrap()
                .tracks,
            list.tracks
        );
        assert!(resolve(root, Path::new("../outside.mp3")).is_err());
        assert!(resolve(root, &root.join("Albums/a.MP3")).is_err());
    }
    #[test]
    fn incremental_refresh_reuses_unchanged_tags_updates_changed_and_drops_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("keep.mp3"), []).unwrap();
        fs::write(root.join("change.mp3"), []).unwrap();
        fs::write(root.join("remove.mp3"), []).unwrap();
        let baseline = scan(root).unwrap();
        assert_eq!(baseline.tracks.len(), 3);

        // Nothing changed on disk: every file is reused, none is reopened/retagged.
        let (same, stats) = refresh(&baseline, root).unwrap();
        assert_eq!((stats.considered, stats.reused, stats.tagged, stats.removed), (3, 3, 0, 0));
        assert_eq!(same.tracks.len(), 3);

        let keep = |lib: &Library| {
            lib.tracks
                .iter()
                .find(|t| t.path == Path::new("keep.mp3"))
                .unwrap()
                .id()
        };
        let keep_id_before = keep(&baseline);
        // Stable across two consecutive no-op refreshes.
        assert_eq!(keep_id_before, keep(&same));

        fs::write(root.join("change.mp3"), b"different bytes").unwrap();
        fs::remove_file(root.join("remove.mp3")).unwrap();
        fs::write(root.join("added.mp3"), []).unwrap();
        let (updated, stats) = refresh(&same, root).unwrap();
        assert_eq!((stats.considered, stats.reused, stats.tagged, stats.removed), (3, 1, 2, 1));
        assert_eq!(updated.tracks.len(), 3);
        assert!(updated.tracks.iter().any(|t| t.path == Path::new("added.mp3")));
        assert!(!updated.tracks.iter().any(|t| t.path == Path::new("remove.mp3")));
        // The untouched track's stable ID survives the refresh that changed its siblings.
        assert_eq!(keep_id_before, keep(&updated));
    }
    #[test]
    #[ignore = "measurement fixture, not a correctness check; run with --ignored --nocapture"]
    fn fixture_library_incremental_refresh_avoids_retagging_most_of_a_large_library() {
        use std::time::Instant;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        const COUNT: usize = 4000;
        for i in 0..COUNT {
            fs::write(root.join(format!("track_{i:05}.mp3")), []).unwrap();
        }
        let started = Instant::now();
        let baseline = scan(root).unwrap();
        let full_scan = started.elapsed();
        assert_eq!(baseline.tracks.len(), COUNT);

        // What the old refresh path did: another unconditional full scan/retag of everything.
        let started = Instant::now();
        let _ = scan(root).unwrap();
        let full_rescan = started.elapsed();

        // A handful of real changes: 3 added, 2 removed, 2 modified; COUNT-4 files untouched.
        for i in 0..2 {
            fs::remove_file(root.join(format!("track_{i:05}.mp3"))).unwrap();
        }
        for i in 2..4 {
            fs::write(root.join(format!("track_{i:05}.mp3")), b"changed").unwrap();
        }
        for i in 0..3 {
            fs::write(root.join(format!("new_{i:03}.mp3")), []).unwrap();
        }
        let started = Instant::now();
        let (updated, stats) = refresh(&baseline, root).unwrap();
        let incremental = started.elapsed();

        assert_eq!(updated.tracks.len(), COUNT - 2 + 3);
        assert_eq!(stats.removed, 2);
        assert_eq!(stats.tagged, 5); // 2 modified + 3 added
        assert_eq!(stats.reused, COUNT - 2 - 2); // everything else

        println!(
            "fixture library evidence: {COUNT} files; full scan {full_scan:?}; full rescan \
             (old refresh path) {full_rescan:?}; incremental refresh after 2 removed/2 \
             modified/3 added {incremental:?} (considered {}, reused {}, tagged {}, removed {})",
            stats.considered, stats.reused, stats.tagged, stats.removed
        );
        assert!(
            incremental < full_rescan,
            "incremental refresh ({incremental:?}) should be faster than a full rescan \
             ({full_rescan:?}) when only 5 of {COUNT} files changed"
        );
    }
    #[test]
    fn windows_playlist_paths_enroll_as_portable_paths() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("Albums")).unwrap();
        fs::write(root.path().join("Albums/a.mp3"), b"fixture").unwrap();
        let file = root.path().join("show.json");
        fs::write(
            &file,
            serde_json::to_vec(
                &serde_json::json!({"version":1,"name":"Show","tracks":["Albums\\a.mp3"]}),
            )
            .unwrap(),
        )
        .unwrap();
        let list = Playlist::load(&file, root.path()).unwrap();
        assert_eq!(list.tracks, vec![PathBuf::from("Albums/a.mp3")]);
        list.save_in_library(&file, root.path()).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(value["tracks"][0], "Albums/a.mp3");
    }
    #[test]
    fn malformed_preparation_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.wav"), []).unwrap();
        fs::write(tmp.path().join("a.wav.amp.json"), "broken").unwrap();
        assert!(scan(tmp.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_escape_or_recurse() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.mp3"), []).unwrap();
        symlink(
            outside.path().join("secret.mp3"),
            root.path().join("link.mp3"),
        )
        .unwrap();
        symlink(root.path(), root.path().join("cycle")).unwrap();
        assert!(scan(root.path()).unwrap().tracks.is_empty());
        assert!(resolve(root.path(), Path::new("link.mp3")).is_err());
    }
}
