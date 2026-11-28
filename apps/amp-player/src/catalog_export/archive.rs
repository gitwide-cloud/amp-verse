//! Export selected source files directly to bounded native RAR volumes.
//! Staging contains only portable metadata, never copies of the source music.
use amp_core::{
    Playlist,
    show::{Settings, SmartPlaylist},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering::Relaxed},
        mpsc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub const PART_BYTES: u64 = 100_000_000;
const METADATA_LIMIT: u64 = 512 * 1024 * 1024;
const RESERVE: u64 = 64 * 1024 * 1024;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn check(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Relaxed) {
        Err("Export canceled; your music is unchanged.".into())
    } else {
        Ok(())
    }
}
pub fn rar_path() -> Option<PathBuf> {
    #[cfg(windows)]
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(base) = std::env::var_os(key) {
            let p = PathBuf::from(base).join("WinRAR/Rar.exe");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|v| std::env::split_paths(&v).collect::<Vec<_>>())
        .map(|p| p.join(if cfg!(windows) { "rar.exe" } else { "rar" }))
        .find(|p| p.is_file())
}
pub struct Selection {
    pub paths: Vec<PathBuf>,
    pub playlists: Vec<Playlist>,
    pub smart: Vec<SmartPlaylist>,
}
#[derive(Clone, Debug)]
struct Song {
    source: PathBuf,
    inputs: Vec<PathBuf>,
    owner: PathBuf,
    relative: PathBuf,
    member: PathBuf,
    size: u64,
    modified: SystemTime,
}
fn safe(path: &Path) -> Result<(), String> {
    let text = path
        .to_str()
        .ok_or("An export filename is not valid Unicode")?;
    let invalid_component = path.components().any(|part| {
        let Component::Normal(name) = part else {
            return true;
        };
        let name = name.to_string_lossy();
        let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
        name.ends_with(['.', ' '])
            || name
                .chars()
                .any(|c| c.is_control() || matches!(c, '<' | '>' | '"' | '|' | '?' | '*' | '\\'))
            || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.len() == 4
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    });
    if invalid_component
        || text.contains(['\r', '\n', '\0', '"'])
        || text.contains(':')
        || path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("An export filename is not portable; rename it or omit that song.".into());
    }
    Ok(())
}
// RAR stores a verbatim UNC prefix as server/share directories. Normalize only
// its input spelling; canonical source paths still own identity and validation.
fn rar_input(path: &Path) -> Result<String, String> {
    let text = path
        .to_str()
        .ok_or("An export filename is not valid Unicode")?;
    Ok(if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(text).to_owned()
    })
}
fn stored_absolute(path: &Path) -> PathBuf {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(v) => Some(v),
            _ => None,
        })
        .collect()
}
fn songs(root: &Path, paths: &[PathBuf], cancel: &AtomicBool) -> Result<Vec<Song>, String> {
    let root = root.canonicalize().map_err(error)?;
    let mut entries: Vec<Song> = Vec::new();
    let mut used = BTreeSet::new();
    let mut sources: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for path in paths {
        check(cancel)?;
        let source = amp_core::resolve(&root, path).map_err(error)?;
        if let Some(index) = sources.get(&source) {
            entries[*index].inputs.push(path.clone());
            continue;
        }
        sources.insert(source.clone(), entries.len());
        let (owner, relative) = amp_core::music_sources::owner(&root, path).map_err(error)?;
        let member = source
            .strip_prefix(&root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| stored_absolute(&source));
        safe(&member)?;
        if member
            .components()
            .next()
            .is_some_and(|c| c.as_os_str().eq_ignore_ascii_case("ampdj"))
            || !used.insert(member.to_string_lossy().replace('\\', "/").to_lowercase())
        {
            return Err(
                "Two selected sources use the same export path. Export those sources separately."
                    .into(),
            );
        }
        let metadata = fs::metadata(&source).map_err(error)?;
        entries.push(Song {
            source,
            inputs: vec![path.clone()],
            owner,
            relative,
            member,
            size: metadata.len(),
            modified: metadata.modified().map_err(error)?,
        });
    }
    if entries.is_empty() {
        return Err("Choose some music to export.".into());
    }
    Ok(entries)
}
fn unchanged(song: &Song) -> Result<(), String> {
    let meta = fs::metadata(&song.source).map_err(error)?;
    if meta.len() != song.size || meta.modified().map_err(error)? != song.modified {
        return Err(format!(
            "{} changed during export. Retry after it finishes saving.",
            song.member.display()
        ));
    }
    Ok(())
}
fn hash(song: &Song, cancel: &AtomicBool) -> Result<String, String> {
    let mut source = fs::File::open(&song.source).map_err(error)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 256 * 1024];
    loop {
        check(cancel)?;
        let n = source.read(&mut buffer).map_err(error)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    unchanged(song)?;
    Ok(format!("{:x}", digest.finalize()))
}
fn write_json(root: &Path, path: &Path, value: &impl serde::Serialize) -> Result<u64, String> {
    safe(path)?;
    let file = root.join(path);
    fs::create_dir_all(file.parent().ok_or("Export folder unavailable")?).map_err(error)?;
    let bytes = serde_json::to_vec(value).map_err(error)?;
    fs::write(file, &bytes).map_err(error)?;
    Ok(bytes.len() as u64)
}
fn metadata(
    root: &Path,
    entries: &[Song],
    selection: &Selection,
    cancel: &AtomicBool,
    progress: &mpsc::SyncSender<String>,
) -> Result<Vec<PathBuf>, String> {
    let mut owners: BTreeMap<PathBuf, Vec<&Song>> = BTreeMap::new();
    for song in entries {
        owners.entry(song.owner.clone()).or_default().push(song)
    }
    let mut size = 0u64;
    let mut manifest = Vec::new();
    let mut completed = 0;
    let mut covers = BTreeSet::new();
    for (owner, tracks) in owners {
        check(cancel)?;
        let mut saved = amp_core::catalog_db::preparations(&owner)?;
        for song in tracks {
            check(cancel)?;
            let _ = progress.try_send(format!(
                "Preparing catalog: {} / {} songs",
                completed + 1,
                entries.len()
            ));
            let key = format!(
                "tracks/{}.json",
                song.relative.to_string_lossy().replace('\\', "/")
            );
            let mut prep = saved
                .remove(&key)
                .map(Ok)
                .unwrap_or_else(|| amp_core::preparation::load(&song.source))?;
            let sha = hash(song, cancel)?;
            if prep.source_sha256.as_ref().is_some_and(|old| old != &sha) {
                return Err(format!(
                    "{} has changed since its saved analysis. Analyze it again or omit it before exporting.",
                    song.member.display()
                ));
            }
            for key in ["_amp_sync", "analysis_source_stat", "scan_source_stat"] {
                prep.extensions.remove(key);
            }
            prep.source_sha256 = Some(sha.clone());
            if let Some(id) = prep.extensions.get("artwork_id").and_then(|v| v.as_str()) {
                covers.insert((owner.clone(), id.to_owned()));
            }
            let mut record = PathBuf::from("ampdj/tracks")
                .join(&song.member)
                .into_os_string();
            record.push(".json");
            size = size
                .checked_add(write_json(root, Path::new(&record), &prep)?)
                .ok_or("Catalog size overflow")?;
            if size > METADATA_LIMIT {
                return Err(
                    "The selected catalog is too large for one export. Choose fewer songs.".into(),
                );
            }
            manifest.push(serde_json::json!({"path":song.member,"sha256":sha,"bytes":song.size}));
            completed += 1;
            if fs2::available_space(root).map_err(error)? < RESERVE {
                return Err(
                    "Not enough free space to finish the catalog. Choose another destination."
                        .into(),
                );
            }
        }
    }
    let artwork_budget = fs2::available_space(root)
        .map_err(error)?
        .saturating_sub(RESERVE)
        / 2;
    amp_core::artwork::copy_selected_to_local(
        covers,
        &root.join("ampdj/artwork.ampdb"),
        artwork_budget,
        || {
            check(cancel)?;
            if fs2::available_space(root).map_err(error)? < RESERVE {
                return Err("Not enough free space for the selected artwork.".into());
            }
            Ok(())
        },
    )?;
    let map: BTreeMap<_, _> = entries
        .iter()
        .flat_map(|song| {
            song.inputs
                .iter()
                .map(move |path| (path.clone(), song.member.clone()))
        })
        .collect();
    let mut lists = selection.playlists.clone();
    for list in &mut lists {
        list.sync = None;
        list.tracks = list
            .tracks
            .iter()
            .filter_map(|path| map.get(path).cloned())
            .collect();
    }
    lists.retain(|list| !list.tracks.is_empty());
    if lists.is_empty() {
        lists.push(Playlist {
            companion_available: false,
            sync: None,
            version: 1,
            name: "Exported music".into(),
            tracks: entries.iter().map(|s| s.member.clone()).collect(),
        });
    }
    for (index, list) in lists.iter().enumerate() {
        write_json(
            root,
            &PathBuf::from(format!("ampdj/playlists/playlist-{index:04}.json")),
            list,
        )?;
    }
    let mut smart = selection.smart.clone();
    for list in &mut smart {
        list.allowed = list
            .allowed
            .iter()
            .filter_map(|path| map.get(Path::new(path)))
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect();
        list.excluded = list
            .excluded
            .iter()
            .filter_map(|path| map.get(Path::new(path)))
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect();
        list.preferred = list
            .preferred
            .iter()
            .filter_map(|path| map.get(Path::new(path)))
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect();
        list.recognition_song = list
            .recognition_song
            .as_ref()
            .and_then(|path| map.get(Path::new(path)))
            .map(|path| path.to_string_lossy().replace('\\', "/"));
        list.restrict_allowed = true;
        list.folders.clear();
        list.restrict_folders = false;
        list.playlist_sources.clear();
        list.source_library = None;
        list.source_root_hint = None;
        list.source_unavailable = false;
    }
    smart.retain(|list| !list.allowed.is_empty());
    if !smart.is_empty() {
        let mut settings = Settings {
            sync: None,
            version: 1,
            playlists: smart,
        };
        settings.version = settings.required_version();
        settings.validate()?;
        write_json(root, Path::new("ampdj/smart-playlists.json"), &settings)?;
    }
    write_json(
        root,
        Path::new("AMP-Catalog.json"),
        &serde_json::json!({"format":"AMP Verse portable catalog","version":1,"part_limit_bytes":PART_BYTES,"tracks":manifest}),
    )?;
    if fs2::available_space(root).map_err(error)? < size.saturating_mul(3).saturating_add(RESERVE) {
        return Err("Not enough free space to build this catalog. Choose fewer songs or another destination.".into());
    }
    amp_core::catalog_db::compact(root)?;
    fs::write(root.join("READ ME.txt"),"Keep all RAR parts in one folder. Extract the first part with WinRAR or RAR. In AMP Verse, choose Add music folder and select the extracted folder. The catalog contains only the selected songs and their saved analysis and cues. Original music folders have not been changed.\r\n").map_err(error)?;
    let mut files = vec![
        PathBuf::from("ampdj/catalog.ampdb"),
        PathBuf::from("ampdj/library.json"),
        PathBuf::from("AMP-Catalog.json"),
        PathBuf::from("READ ME.txt"),
    ];
    if root.join("ampdj/artwork.ampdb").is_file() {
        files.push(PathBuf::from("ampdj/artwork.ampdb"));
    }
    Ok(files)
}
struct Owned(Box<dyn process_wrap::std::ChildWrapper>);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn run(mut cmd: std::process::Command, cancel: &AtomicBool, log: &Path) -> Result<(), String> {
    use std::process::Stdio;
    let output = fs::File::create(log).map_err(error)?;
    cmd.stdin(Stdio::null())
        .stdout(output.try_clone().map_err(error)?)
        .stderr(output);
    let mut wrapper = process_wrap::std::CommandWrap::from(cmd);
    #[cfg(windows)]
    wrapper
        .wrap(process_wrap::std::CreationFlags(
            windows::Win32::System::Threading::CREATE_NO_WINDOW
                | windows::Win32::System::Threading::BELOW_NORMAL_PRIORITY_CLASS,
        ))
        .wrap(process_wrap::std::JobObject);
    #[cfg(unix)]
    wrapper.wrap(process_wrap::std::ProcessGroup::leader());
    let mut owner = Owned(wrapper.spawn().map_err(error)?);
    loop {
        check(cancel)?;
        match owner.0.try_wait().map_err(error)? {
            Some(status) if status.success() => return Ok(()),
            Some(_) => {
                return Err(format!(
                    "RAR could not complete the export. {}",
                    fs::read_to_string(log)
                        .unwrap_or_default()
                        .lines()
                        .rev()
                        .take(2)
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}
pub fn export(
    root: &Path,
    selection: Selection,
    destination: &Path,
    rar: &Path,
    cancel: &AtomicBool,
    progress: &mpsc::SyncSender<String>,
) -> Result<PathBuf, String> {
    check(cancel)?;
    let _ = progress.try_send("Checking selected songs…".into());
    let entries = songs(root, &selection.paths, cancel)?;
    let total = entries.iter().try_fold(0u64, |sum, s| {
        sum.checked_add(s.size).ok_or("Export size overflow")
    })?;
    if fs2::available_space(destination).map_err(error)? < total.saturating_add(RESERVE) {
        return Err(
            "Not enough free space for these songs. Choose fewer songs or another destination."
                .into(),
        );
    }
    let stage = tempfile::Builder::new()
        .prefix(".amp-export-")
        .tempdir_in(destination)
        .map_err(error)?;
    let package = stage.path().join("metadata");
    fs::create_dir(&package).map_err(error)?;
    let metadata = metadata(&package, &entries, &selection, cancel, progress)?;
    let meta_size = metadata.iter().try_fold(0u64, |sum, p| {
        fs::metadata(package.join(p))
            .map(|m| sum.saturating_add(m.len()))
            .map_err(error)
    })?;
    if fs2::available_space(destination).map_err(error)?
        < total.saturating_add(meta_size).saturating_add(RESERVE)
    {
        return Err(
            "Not enough free space for the music and its catalog. Choose another destination."
                .into(),
        );
    }
    let mut lines = Vec::new();
    let mut expected = BTreeSet::new();
    let root = root.canonicalize().map_err(error)?;
    for song in &entries {
        safe(&song.member)?;
        let path = if song.source.starts_with(&root) {
            PathBuf::from(".").join(song.source.strip_prefix(&root).map_err(error)?)
        } else {
            song.source.clone()
        };
        let text = rar_input(&path)?;
        if text.contains(['\r', '\n', '\0', '"']) {
            return Err("A selected filename cannot be exported safely.".into());
        }
        lines.push(format!("\"{text}\""));
        expected.insert(
            song.member
                .to_string_lossy()
                .replace('\\', "/")
                .to_lowercase(),
        );
    }
    for path in &metadata {
        let full = package.join(path);
        lines.push(format!("\"{}\"", rar_input(&full)?));
        expected.insert(path.to_string_lossy().replace('\\', "/").to_lowercase());
    }
    let listing = stage.path().join("files.txt");
    fs::write(&listing, lines.join("\n")).map_err(error)?;
    let output = stage.path().join("parts");
    fs::create_dir(&output).map_err(error)?;
    let archive = output.join("AMP-Catalog.rar");
    let mut command = amp_core::command(rar.to_str().ok_or("RAR path is not valid Unicode")?);
    command
        .current_dir(&root)
        .args([
            "a",
            "-cfg-",
            "-idq",
            "-m1",
            "-md16m",
            "-mt2",
            "-scfl",
            "-scfc",
            "-v100000000b",
        ])
        .arg(format!("-ep4{}", stored_absolute(&package).display()))
        .arg(&archive)
        .arg(format!("@{}", listing.display()));
    let _ = progress.try_send("Creating 100 MB archive parts…".into());
    run(command, cancel, &stage.path().join("create.log"))?;
    for song in &entries {
        unchanged(song)?;
    }
    let mut parts: Vec<_> = fs::read_dir(&output)
        .map_err(error)?
        .map(|e| e.map(|e| e.path()).map_err(error))
        .collect::<Result<_, _>>()?;
    parts.sort();
    if parts.is_empty()
        || parts.iter().any(|p| {
            p.extension().is_none_or(|s| !s.eq_ignore_ascii_case("rar"))
                || !fs::metadata(p).is_ok_and(|m| m.len() <= PART_BYTES)
        })
    {
        return Err("RAR did not produce valid 100 MB parts; no export was published.".into());
    }
    let first = parts
        .iter()
        .find(|p| {
            p.file_name().is_some_and(|n| {
                n == "AMP-Catalog.part1.rar"
                    || n == "AMP-Catalog.part01.rar"
                    || n == "AMP-Catalog.part001.rar"
                    || n == "AMP-Catalog.rar"
            })
        })
        .unwrap_or(&parts[0]);
    let _ = progress.try_send("Verifying exported music and catalog…".into());
    let mut test = amp_core::command(rar.to_str().unwrap());
    test.args(["t", "-cfg-", "-idq"]).arg(first);
    run(test, cancel, &stage.path().join("verify.log"))?;
    let mut list = amp_core::command(rar.to_str().unwrap());
    list.args(["lb", "-cfg-", "-scfc"]).arg(first);
    let log = stage.path().join("members.txt");
    run(list, cancel, &log)?;
    let actual: BTreeSet<_> = fs::read_to_string(log)
        .map_err(error)?
        .lines()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.replace('\\', "/").to_lowercase())
        .collect();
    if actual != expected {
        return Err(
            "Archive paths did not match the portable index. No export was published.".into(),
        );
    }
    check(cancel)?;
    let id = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(error)?
                    .as_nanos(),
                std::process::id()
            )
            .as_bytes()
        )
    );
    let target = destination.join(format!("AMP Catalog {}", &id[..10]));
    if target.exists() {
        return Err("An export with this name already exists. Retry the export.".into());
    }
    fs::rename(&output, &target).map_err(error)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path) -> Vec<PathBuf> {
        fs::create_dir_all(root.join("Rock")).unwrap();
        fs::write(root.join("Rock/Selected.mp3"), b"selected original audio").unwrap();
        fs::write(
            root.join("Rock/Unselected.mp3"),
            b"unselected original audio",
        )
        .unwrap();
        let mut prep = amp_core::Preparation {
            bpm: Some(128.),
            manual_override: true,
            cue_in: 0.25,
            ..Default::default()
        };
        prep.extensions
            .insert("title".into(), serde_json::json!("Selected"));
        amp_core::preparation::save(root, Path::new("Rock/Selected.mp3"), &prep).unwrap();
        prep.extensions
            .insert("private_note".into(), serde_json::json!("UNSELECTED-NOTE"));
        amp_core::preparation::save(root, Path::new("Rock/Unselected.mp3"), &prep).unwrap();
        amp_core::catalog_db::compact(root).unwrap();
        vec![PathBuf::from("Rock/Selected.mp3")]
    }
    #[test]
    fn portable_index_contains_only_selected_music_and_retains_manual_cues() {
        let source = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let paths = fixture(source.path());
        let cancel = AtomicBool::new(false);
        let before = fs::read(amp_core::catalog_db::path(source.path())).unwrap();
        let entries = songs(source.path(), &paths, &cancel).unwrap();
        let (tx, _) = mpsc::sync_channel(8);
        metadata(
            destination.path(),
            &entries,
            &Selection {
                paths: paths.clone(),
                playlists: vec![],
                smart: vec![],
            },
            &cancel,
            &tx,
        )
        .unwrap();
        let index = amp_core::catalog_db::preparations(destination.path()).unwrap();
        assert_eq!(index.len(), 1);
        let prep = &index["tracks/Rock/Selected.mp3.json"];
        assert_eq!(prep.bpm, Some(128.));
        assert_eq!(prep.cue_in, 0.25);
        assert!(prep.manual_override);
        assert!(
            !serde_json::to_string(&index)
                .unwrap()
                .contains("UNSELECTED-NOTE")
        );
        assert_eq!(
            fs::read(amp_core::catalog_db::path(source.path())).unwrap(),
            before
        );
        assert_eq!(
            fs::read(source.path().join(&paths[0])).unwrap(),
            b"selected original audio"
        );
    }
    #[test]
    fn stopped_export_does_not_create_output_or_touch_originals() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let paths = fixture(source.path());
        let (tx, _) = mpsc::sync_channel(8);
        let result = export(
            source.path(),
            Selection {
                paths,
                playlists: vec![],
                smart: vec![],
            },
            target.path(),
            Path::new("not-started"),
            &AtomicBool::new(true),
            &tx,
        );
        assert!(result.unwrap_err().contains("canceled"));
        assert_eq!(fs::read_dir(target.path()).unwrap().count(), 0);
        assert_eq!(
            fs::read(source.path().join("Rock/Selected.mp3")).unwrap(),
            b"selected original audio"
        );
    }
    #[test]
    fn unsafe_member_paths_are_rejected() {
        for path in [
            "../outside.mp3",
            "/absolute.mp3",
            "a\nsecond.mp3",
            "amp:music.mp3",
            "\"quoted.mp3",
            "CON.mp3",
            "folder/track?.mp3",
            "folder/trailing.mp3 ",
            "folder\\..\\outside.mp3",
        ] {
            assert!(safe(Path::new(path)).is_err(), "{path}");
        }
    }
    #[test]
    #[ignore = "requires an installed RAR creator; set AMP_EXPORT_RAR"]
    fn native_rar_export_round_trip() {
        let rar = PathBuf::from(std::env::var_os("AMP_EXPORT_RAR").unwrap());
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let paths = fixture(source.path());
        let (tx, _) = mpsc::sync_channel(8);
        let folder = export(
            source.path(),
            Selection {
                paths,
                playlists: vec![],
                smart: vec![],
            },
            target.path(),
            &rar,
            &AtomicBool::new(false),
            &tx,
        )
        .unwrap();
        let first = fs::read_dir(&folder)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let extracted = tempfile::tempdir().unwrap();
        let mut command = amp_core::command(rar.to_str().unwrap());
        command
            .args(["x", "-cfg-", "-idq"])
            .arg(&first)
            .arg(format!(
                "{}{sep}",
                extracted.path().display(),
                sep = std::path::MAIN_SEPARATOR
            ));
        assert!(command.status().unwrap().success());
        assert_eq!(
            fs::read(extracted.path().join("Rock/Selected.mp3")).unwrap(),
            b"selected original audio"
        );
        assert!(!extracted.path().join("Rock/Unselected.mp3").exists());
        let prep =
            amp_core::preparation::load(&extracted.path().join("Rock/Selected.mp3")).unwrap();
        assert_eq!(prep.bpm, Some(128.));
        assert!(prep.manual_override);
        assert_eq!(prep.cue_in, 0.25);
    }
    #[test]
    #[ignore = "requires installed RAR and FFmpeg; set AMP_EXPORT_RAR"]
    fn native_rar_audio_and_playlists_round_trip() {
        let rar = PathBuf::from(std::env::var_os("AMP_EXPORT_RAR").unwrap());
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let path = PathBuf::from("Selected.mp3");
        assert!(
            amp_core::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=1",
                    "-b:a",
                    "256k"
                ])
                .arg(source.path().join(&path))
                .status()
                .unwrap()
                .success()
        );
        let before = fs::read(source.path().join(&path)).unwrap();
        let prep = amp_core::Preparation {
            bpm: Some(128.),
            manual_override: true,
            cue_in: 0.25,
            ..Default::default()
        };
        amp_core::preparation::save(source.path(), &path, &prep).unwrap();
        let list = Playlist {
            version: 1,
            name: "Selected set".into(),
            tracks: vec![path.clone(), "Unselected.mp3".into()],
            companion_available: false,
            sync: None,
        };
        let mut smart = Settings::default().playlists.remove(0);
        smart.name = "Selected smart".into();
        smart.allowed = vec!["Selected.mp3".into(), "Unselected.mp3".into()];
        let (tx, _) = mpsc::sync_channel(8);
        let folder = export(
            source.path(),
            Selection {
                paths: vec![path.clone()],
                playlists: vec![list],
                smart: vec![smart],
            },
            target.path(),
            &rar,
            &AtomicBool::new(false),
            &tx,
        )
        .unwrap();
        let first = fs::read_dir(&folder)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let extracted = tempfile::tempdir().unwrap();
        assert!(
            amp_core::command(rar.to_str().unwrap())
                .args(["x", "-cfg-", "-idq"])
                .arg(first)
                .arg(format!(
                    "{}{sep}",
                    extracted.path().display(),
                    sep = std::path::MAIN_SEPARATOR
                ))
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(fs::read(extracted.path().join(&path)).unwrap(), before);
        assert!(
            amp_core::command("ffmpeg")
                .args(["-v", "error", "-i"])
                .arg(extracted.path().join(&path))
                .args(["-f", "null", "-"])
                .status()
                .unwrap()
                .success()
        );
        let lists = amp_core::catalog_db::playlist_paths(extracted.path()).unwrap();
        assert_eq!(lists.len(), 1);
        let list = Playlist::load(&lists[0], extracted.path()).unwrap();
        assert_eq!(list.name, "Selected set");
        assert_eq!(list.tracks, vec![path]);
        let settings = Settings::load(extracted.path()).unwrap();
        assert_eq!(settings.playlists.len(), 1);
        assert_eq!(settings.playlists[0].allowed, vec!["Selected.mp3"]);
        assert!(settings.playlists[0].restrict_allowed);
    }
    #[test]
    fn windows_verbatim_paths_keep_rar_member_names_portable() {
        assert_eq!(
            rar_input(Path::new(r"\\?\UNC\server\share\music\song.mp3")).unwrap(),
            r"\\server\share\music\song.mp3"
        );
        assert_eq!(
            rar_input(Path::new(r"\\?\C:\Music\song.mp3")).unwrap(),
            r"C:\Music\song.mp3"
        );
        assert_eq!(
            rar_input(Path::new("Music/song.mp3")).unwrap(),
            "Music/song.mp3"
        );
    }
}
