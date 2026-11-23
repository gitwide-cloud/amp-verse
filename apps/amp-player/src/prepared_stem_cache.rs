//! Explicit selected-playlist preparation. Live playback never calls this writer.
//! Three gapless MP3 streams; no WAV/PCM files and no legacy prepared-audio format.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    time::{Duration, Instant},
};
const RATE: u64 = 44100;
const WINDOW: usize = 485100;
const OVERLAP: usize = 22050;
pub(crate) const MODEL: &str = "98228931494151762a1c4ab1ec7899a894b1f81fd4a509921a9d2f9bacc50845";
const NAMES: [&str; 3] = ["vocals.mp3", "instruments.mp3", "drums.mp3"];
const TRACK_LIMIT: u64 = 250 * 1024 * 1024;
const CACHE_LIMIT: u64 = 10 * 1024 * 1024 * 1024;
const FRAME_LIMIT: u64 = RATE * 7200;
type Parts = [Vec<f32>; 3];

/// Caller snapshots exactly one selected normal or resolved SmartPlaylist.
/// Changes to playlist rules during preparation do not expand this snapshot.
pub(crate) struct PlaylistSnapshot {
    pub name: String,
    pub tracks: Vec<PathBuf>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Part {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
    /// Multiply decoded PCM by this value, preserving the separated part's gain.
    pub restore_gain: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Manifest {
    pub version: u32,
    pub model_sha256: String,
    pub source_sha256: String,
    pub sample_rate: u32,
    pub frames: u64,
    pub codec: String,
    pub bitrate: u32,
    pub parts: [Part; 3],
}
#[derive(Debug)]
pub(crate) struct Prepared {
    pub directory: PathBuf,
    pub manifest: Manifest,
}
pub(crate) struct Progress {
    pub completed: usize,
    pub total: usize,
    pub phase: &'static str,
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn check(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Relaxed) {
        Err("Playlist stem preparation canceled".into())
    } else {
        Ok(())
    }
}
fn safe(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        if let Ok(meta) = fs::symlink_metadata(ancestor) {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("Linked cache/source paths are unsupported".into());
                }
            }
            if meta.file_type().is_symlink() {
                return Err("Linked cache/source paths are unsupported".into());
            }
        }
    }
    Ok(())
}
fn usage(path: &Path) -> Result<u64, String> {
    safe(path)?;
    let meta = fs::symlink_metadata(path).map_err(err)?;
    if meta.is_file() {
        return Ok(meta.len());
    }
    if !meta.is_dir() {
        return Err("Unexpected cache member".into());
    }
    fs::read_dir(path)
        .map_err(err)?
        .try_fold(0u64, |sum, item| {
            sum.checked_add(usage(&item.map_err(err)?.path())?)
                .ok_or_else(|| "Cache accounting overflow".into())
        })
}
fn digest(path: &Path, cancel: &AtomicBool) -> Result<String, String> {
    safe(path)?;
    if !fs::metadata(path).map_err(err)?.is_file() {
        return Err("Expected regular source/cache file".into());
    }
    let mut reader = fs::File::open(path).map_err(err)?;
    let mut sha = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        check(cancel)?;
        let n = reader.read(&mut buffer).map_err(err)?;
        if n == 0 {
            break;
        }
        sha.update(&buffer[..n]);
    }
    Ok(format!("{:x}", sha.finalize()))
}
/// Every ffmpeg is kill-on-owner-drop, with cancellation and disk watchdog.
struct Process {
    child: Arc<Mutex<crate::receiver_process::Decoder>>,
    finished: Arc<AtomicBool>,
    watcher: Option<std::thread::JoinHandle<()>>,
    aborted: Arc<AtomicBool>,
}
impl Process {
    fn spawn(
        command: &mut std::process::Command,
        cancel: Arc<AtomicBool>,
        stage: Option<PathBuf>,
    ) -> Result<Self, String> {
        let child = Arc::new(Mutex::new(
            crate::receiver_process::spawn(command).map_err(err)?,
        ));
        let finished = Arc::new(AtomicBool::new(false));
        let aborted = Arc::new(AtomicBool::new(false));
        let c = child.clone();
        let done = finished.clone();
        let bad = aborted.clone();
        let watcher = std::thread::spawn(move || {
            let start = Instant::now();
            while !done.load(Relaxed) {
                if cancel.load(Relaxed)
                    || start.elapsed() > Duration::from_secs(7200)
                    || stage
                        .as_ref()
                        .is_some_and(|p| usage(p).map_or(true, |n| n > TRACK_LIMIT))
                {
                    bad.store(true, Relaxed);
                    if let Ok(mut p) = c.lock() {
                        let _ = p.kill();
                    }
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
        Ok(Self {
            child,
            finished,
            watcher: Some(watcher),
            aborted,
        })
    }
    fn wait(&mut self) -> Result<(), String> {
        loop {
            let status = self.child.lock().map_err(err)?.try_wait().map_err(err)?;
            if let Some(status) = status {
                if !status.success() || self.aborted.load(Relaxed) {
                    return Err(
                        "Preparation encoder/decoder failed, canceled, or exceeded its budget"
                            .into(),
                    );
                }
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.finished.store(true, Relaxed);
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some(w) = self.watcher.take() {
            let _ = w.join();
        }
    }
}
fn decoder(
    path: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<(Process, std::process::ChildStdout), String> {
    let mut cmd = amp_core::command("ffmpeg");
    cmd.args(["-nostdin", "-v", "error", "-i"])
        .arg(path)
        .args([
            "-map", "0:a:0", "-vn", "-ac", "2", "-ar", "44100", "-f", "f32le", "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let process = Process::spawn(&mut cmd, cancel, None)?;
    let output = process
        .child
        .lock()
        .map_err(err)?
        .stdout
        .take()
        .ok_or("Decoder stdout missing")?;
    Ok((process, output))
}
fn read_pcm(input: &mut impl Read, frames: usize) -> Result<Vec<f32>, String> {
    let mut bytes = vec![0u8; frames * 8];
    let mut n = 0;
    while n < bytes.len() {
        let count = input.read(&mut bytes[n..]).map_err(err)?;
        if count == 0 {
            break;
        }
        n += count;
    }
    if n % 8 != 0 {
        return Err("Partial stereo PCM frame".into());
    }
    let pcm: Vec<_> = bytes[..n]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    if pcm.iter().any(|v| !v.is_finite()) {
        return Err("Nonfinite PCM".into());
    }
    Ok(pcm)
}
fn blend(previous: &Parts, current: &mut Parts) {
    let frames = (previous[0].len().min(current[0].len())) / 2;
    for part in 0..3 {
        for frame in 0..frames {
            let a = if frames > 1 {
                frame as f32 / (frames - 1) as f32
            } else {
                1.
            };
            for ch in 0..2 {
                let i = frame * 2 + ch;
                current[part][i] = previous[part][i] * (1. - a) + current[part][i] * a;
            }
        }
    }
}
fn windows(
    mut input: impl Read,
    cancel: &AtomicBool,
    mut separate: impl FnMut(Vec<f32>) -> Result<Parts, String>,
    mut emit: impl FnMut(&Parts, usize) -> Result<(), String>,
) -> Result<u64, String> {
    let mut carry = Vec::new();
    let mut tail: Option<Parts> = None;
    let mut total = 0;
    loop {
        check(cancel)?;
        let new = read_pcm(
            &mut input,
            if carry.is_empty() {
                WINDOW
            } else {
                WINDOW - OVERLAP
            },
        )?;
        if new.is_empty() {
            if let Some(t) = tail.take() {
                let n = t[0].len() / 2;
                emit(&t, n)?;
                total += n as u64;
            }
            break;
        }
        let mut pcm = std::mem::take(&mut carry);
        pcm.extend(new);
        let frames = pcm.len() / 2;
        let full = frames == WINDOW;
        if full {
            carry = pcm[(frames - OVERLAP) * 2..].to_vec();
        }
        let mut parts = separate(pcm)?;
        if parts
            .iter()
            .any(|v| v.len() != frames * 2 || v.iter().any(|x| !x.is_finite()))
        {
            return Err("Invalid separated PCM shape".into());
        }
        if let Some(old) = tail.take() {
            blend(&old, &mut parts);
        }
        let n = if full { frames - OVERLAP } else { frames };
        total += n as u64;
        if total > FRAME_LIMIT {
            return Err("Selected track exceeds two-hour preparation limit".into());
        }
        emit(&parts, n)?;
        if full {
            tail = Some(std::array::from_fn(|p| parts[p][n * 2..].to_vec()));
        } else {
            break;
        }
    }
    if total == 0 || total > FRAME_LIMIT {
        return Err("Empty or oversized source".into());
    }
    Ok(total)
}
trait Separator {
    fn separate(&mut self, pcm: Vec<f32>, cancel: Arc<AtomicBool>) -> Result<Parts, String>;
}
struct Native;
impl Separator for Native {
    fn separate(&mut self, pcm: Vec<f32>, cancel: Arc<AtomicBool>) -> Result<Parts, String> {
        crate::onnx_stem_worker::separate(pcm, cancel, Duration::from_secs(20))
    }
}
fn pass(
    path: &Path,
    cancel: Arc<AtomicBool>,
    separator: &mut impl Separator,
    emit: impl FnMut(&Parts, usize) -> Result<(), String>,
) -> Result<u64, String> {
    let (mut process, output) = decoder(path, cancel.clone())?;
    let frames = windows(
        std::io::BufReader::new(output),
        &cancel,
        |pcm| separator.separate(pcm, cancel.clone()),
        emit,
    )?;
    process.wait()?;
    Ok(frames)
}
fn verify_entry(root: &Path, source_hash: &str, cancel: &AtomicBool) -> Result<Prepared, String> {
    safe(root)?;
    let path = root.join("manifest.json");
    if fs::metadata(&path).map_err(err)?.len() > 16384 {
        return Err("Cache manifest oversized".into());
    }
    let m: Manifest = serde_json::from_slice(&fs::read(&path).map_err(err)?).map_err(err)?;
    if m.version != 1
        || m.model_sha256 != MODEL
        || m.source_sha256 != source_hash
        || m.sample_rate != 44100
        || m.frames == 0
        || m.frames > FRAME_LIMIT
        || m.codec != "mp3-gapless"
        || m.bitrate != 256000
    {
        return Err("Cache contract mismatch".into());
    }
    for (i, p) in m.parts.iter().enumerate() {
        if p.name != NAMES[i]
            || !p.restore_gain.is_finite()
            || p.restore_gain < 1.
            || p.bytes == 0
            || p.bytes > TRACK_LIMIT
            || fs::metadata(root.join(&p.name)).map_err(err)?.len() != p.bytes
            || digest(&root.join(&p.name), cancel)? != p.sha256
        {
            return Err("Prepared part integrity mismatch".into());
        }
    }
    if usage(root)? > TRACK_LIMIT || fs::read_dir(root).map_err(err)?.count() != 4 {
        return Err("Unexpected prepared cache contents".into());
    }
    Ok(Prepared {
        directory: root.to_path_buf(),
        manifest: m,
    })
}
fn key(hash: &str) -> String {
    format!("{hash}-mp3256-v1")
}
pub(crate) fn ready(
    root: &Path,
    source: &Path,
    cancel: &AtomicBool,
) -> Result<Option<Prepared>, String> {
    cache_root(root)?;
    let hash = digest(source, cancel)?;
    let target = root.join(key(&hash));
    if !target.exists() {
        return Ok(None);
    }
    verify_entry(&target, &hash, cancel).map(Some)
}
#[derive(Serialize, Deserialize)]
struct Lookup {
    source: PathBuf,
    bytes: u64,
    modified_ns: u64,
    key: String,
}
fn identity(source: &Path) -> Result<Lookup, String> {
    safe(source)?;
    let source = source.canonicalize().map_err(err)?;
    let m = fs::metadata(&source).map_err(err)?;
    if !m.is_file() {
        return Err("Expected source file".into());
    }
    let modified_ns = u64::try_from(
        m.modified()
            .map_err(err)?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(err)?
            .as_nanos(),
    )
    .map_err(err)?;
    Ok(Lookup {
        source,
        bytes: m.len(),
        modified_ns,
        key: String::new(),
    })
}
fn lookup_path(root: &Path, source: &Path) -> PathBuf {
    let mut h = Sha256::new();
    h.update(source.to_string_lossy().as_bytes());
    root.join(format!("lookup-{:x}.json", h.finalize()))
}
fn write_lookup(root: &Path, source: &Path, hash: &str) -> Result<(), String> {
    // Reserve the full bounded metadata write even when refreshing an existing index.
    // The single cache-job lock also covers the reuse branch and atomic replacement.
    if usage(root)?
        .checked_add(16384)
        .is_none_or(|n| n > CACHE_LIMIT)
    {
        return Err("Prepared cache is full; lookup metadata reservation unavailable".into());
    }
    let mut record = identity(source)?;
    record.key = key(hash);
    let mut file = tempfile::NamedTempFile::new_in(root).map_err(err)?;
    let bytes = serde_json::to_vec(&record).map_err(err)?;
    if bytes.len() > 16384 {
        return Err("Prepared lookup metadata exceeds its reserved budget".into());
    }
    file.write_all(&bytes).map_err(err)?;
    file.as_file().sync_all().map_err(err)?;
    file.persist(lookup_path(root, &record.source))
        .map_err(err)?;
    Ok(())
}
/// Cheap UNVERIFIED UI hint only. Decoder MUST call ready() before trusting any PCM.
pub(crate) fn candidate(root: &Path, source: &Path) -> Option<Prepared> {
    cache_root(root).ok()?;
    let id = identity(source).ok()?;
    let path = lookup_path(root, &id.source);
    safe(&path).ok()?;
    if fs::metadata(&path).ok()?.len() > 16384 {
        return None;
    }
    let record: Lookup = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    if record.source != id.source
        || record.bytes != id.bytes
        || record.modified_ns != id.modified_ns
        || !owned_name(&record.key)
        || record.key.starts_with('.')
    {
        return None;
    }
    let directory = root.join(record.key);
    let path = directory.join("manifest.json");
    safe(&path).ok()?;
    if fs::metadata(&path).ok()?.len() > 16384 {
        return None;
    }
    let manifest: Manifest = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    Some(Prepared {
        directory,
        manifest,
    })
}
/// Background-only cache inventory. Counts complete manifest directories; ready() performs integrity verification before playback.
pub(crate) fn stats(root: &Path) -> Result<(usize, u64), String> {
    cache_root(root)?;
    if !root.exists() {
        return Ok((0, 0));
    }
    let bytes = usage(root)?;
    let mut count = 0;
    let mut members = 0;
    for item in fs::read_dir(root).map_err(err)? {
        members += 1;
        if members > 20000 {
            return Err("Cache inventory member limit exceeded".into());
        }
        let p = item.map_err(err)?.path();
        if p.is_dir()
            && p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| owned_name(n) && !n.starts_with('.'))
            && p.join("manifest.json").is_file()
        {
            count += 1
        }
    }
    Ok((count, bytes))
}
/// Explicit clear action only. Uses the same lock as preparation; never touches originals.
fn cache_root(root: &Path) -> Result<(), String> {
    safe(root)?;
    if !root.is_absolute() || root.file_name().is_none_or(|n| n != "prepared-stems-v1") {
        return Err("Expected device-local prepared-stems-v1 cache directory".into());
    }
    Ok(())
}
fn owned_name(name: &str) -> bool {
    name.strip_suffix("-mp3256-v1")
        .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        || name.starts_with(".preparing-")
}
pub(crate) fn clear(root: &Path) -> Result<(), String> {
    cache_root(root)?;
    fs::create_dir_all(root).map_err(err)?;
    let _lock = amp_core::coordination::Lock::acquire(&root.join("cache-job"))?;
    let mut removals = Vec::new();
    for item in fs::read_dir(root).map_err(err)? {
        let p = item.map_err(err)?.path();
        if p.file_name().is_some_and(|v| v == "cache-job.lock") {
            continue;
        }
        safe(&p)?;
        let lookup = p.is_file()
            && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.strip_prefix("lookup-")
                    .and_then(|s| s.strip_suffix(".json"))
                    .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            });
        if !lookup
            && (!p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(owned_name)
                || !p.is_dir())
        {
            return Err("Unexpected cache member; refusing destructive clear".into());
        }
        usage(&p)?;
        removals.push(p);
    }
    for p in removals {
        safe(&p)?;
        if p.is_dir() {
            fs::remove_dir_all(&p).map_err(err)?
        } else {
            fs::remove_file(&p).map_err(err)?
        }
    }

    Ok(())
}
/// One serial worker, opt-in only, reserving the complete per-track staging budget.
/// Progress callbacks must not block; cancellation is shared with supervised subprocesses.
pub(crate) fn prepare_selected(
    root: &Path,
    selection: PlaylistSnapshot,
    cancel: Arc<AtomicBool>,
    progress: impl FnMut(Progress),
) -> Result<Vec<Prepared>, String> {
    prepare_with(root, selection, cancel, progress, &mut Native)
}
fn prepare_with(
    root: &Path,
    selection: PlaylistSnapshot,
    cancel: Arc<AtomicBool>,
    mut progress: impl FnMut(Progress),
    separator: &mut impl Separator,
) -> Result<Vec<Prepared>, String> {
    if selection.name.trim().is_empty()
        || selection.tracks.is_empty()
        || selection.tracks.len() > 1000
    {
        return Err("Select one nonempty playlist (at most 1000 tracks)".into());
    }
    cache_root(root)?;
    fs::create_dir_all(root).map_err(err)?;
    let root = root.canonicalize().map_err(err)?;
    let _lock = amp_core::coordination::Lock::acquire(&root.join("cache-job"))?;
    let mut seen = HashSet::new();
    let mut tracks = Vec::new();
    for path in selection.tracks {
        safe(&path)?;
        let path = path.canonicalize().map_err(err)?;
        if path.starts_with(&root) {
            return Err("Cache output cannot be a preparation source".into());
        }
        if seen.insert(path.clone()) {
            tracks.push(path)
        }
    }
    let total = tracks.len();
    let mut results = Vec::new();
    for (index, path) in tracks.iter().enumerate() {
        check(&cancel)?;
        let hash = digest(path, &cancel)?;
        let target = root.join(key(&hash));
        if target.exists() {
            results.push(verify_entry(&target, &hash, &cancel)?);
            write_lookup(&root, path, &hash)?;
            continue;
        }
        if usage(&root)?
            .checked_add(TRACK_LIMIT + 16384)
            .is_none_or(|n| n > CACHE_LIMIT)
        {
            return Err("Prepared cache is full; explicitly clear it before preparing more".into());
        }
        progress(Progress {
            completed: index,
            total,
            phase: "measuring stem peaks",
        });
        let mut peaks = [0f32; 3];
        let frames = pass(path, cancel.clone(), separator, |parts, n| {
            for p in 0..3 {
                for x in &parts[p][..n * 2] {
                    peaks[p] = peaks[p].max(x.abs())
                }
            }
            Ok(())
        })?;
        if digest(path, &cancel)? != hash {
            return Err("Original changed during preparation".into());
        }
        if frames.saturating_mul(96000) / RATE + 1024 * 1024 > TRACK_LIMIT {
            return Err("Selected track exceeds compressed per-track cache budget".into());
        }
        let gains = peaks.map(|p| (p / 0.90).max(1.));
        let stage = tempfile::Builder::new()
            .prefix(".preparing-")
            .tempdir_in(&root)
            .map_err(err)?;
        let mut encoders = Vec::new();
        let mut inputs = Vec::new();
        for name in NAMES {
            let mut cmd = amp_core::command("ffmpeg");
            cmd.args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "f32le",
                "-ar",
                "44100",
                "-ac",
                "2",
                "-i",
                "pipe:0",
                "-map_metadata",
                "-1",
                "-c:a",
                "libmp3lame",
                "-b:a",
                "256k",
                "-write_xing",
                "1",
                "-n",
            ])
            .args(["-fs", &((TRACK_LIMIT - 16384) / 3).to_string()])
            .arg(stage.path().join(name))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
            let process =
                Process::spawn(&mut cmd, cancel.clone(), Some(stage.path().to_path_buf()))?;
            inputs.push(
                process
                    .child
                    .lock()
                    .map_err(err)?
                    .stdin
                    .take()
                    .ok_or("Encoder stdin missing")?,
            );
            encoders.push(process);
        }
        progress(Progress {
            completed: index,
            total,
            phase: "encoding 256 kbps MP3 stems",
        });
        let encoded = pass(path, cancel.clone(), separator, |parts, n| {
            for p in 0..3 {
                let mut bytes = Vec::with_capacity(n * 8);
                for x in &parts[p][..n * 2] {
                    let v = *x / gains[p];
                    if v.abs() > 0.99 {
                        return Err(
                            "Model output changed between passes; refusing clipped cache".into(),
                        );
                    }
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
                inputs[p].write_all(&bytes).map_err(err)?;
            }
            Ok(())
        })?;
        drop(inputs);
        for p in &mut encoders {
            p.wait()?;
        }
        if encoded != frames {
            return Err("Source frame count changed between passes".into());
        }
        progress(Progress {
            completed: index,
            total,
            phase: "verifying gapless alignment",
        });
        let mut parts = Vec::new();
        for (i, name) in NAMES.iter().enumerate() {
            let path = stage.path().join(name);
            let (mut p, mut output) = decoder(&path, cancel.clone())?;
            let mut decoded = 0u64;
            loop {
                let pcm = read_pcm(&mut output, WINDOW)?;
                if pcm.is_empty() {
                    break;
                }
                decoded += pcm.len() as u64 / 2;
                if decoded > frames {
                    return Err("MP3 gapless sample count mismatch".into());
                }
            }
            p.wait()?;
            if decoded != frames {
                return Err("MP3 delay/padding was not removed exactly".into());
            }
            parts.push(Part {
                name: (*name).into(),
                bytes: fs::metadata(&path).map_err(err)?.len(),
                sha256: digest(&path, &cancel)?,
                restore_gain: gains[i],
            });
        }
        let manifest = Manifest {
            version: 1,
            model_sha256: MODEL.into(),
            source_sha256: hash.clone(),
            sample_rate: 44100,
            frames,
            codec: "mp3-gapless".into(),
            bitrate: 256000,
            parts: parts.try_into().map_err(|_| "Part count")?,
        };
        let mut file = fs::File::create(stage.path().join("manifest.json")).map_err(err)?;
        file.write_all(&serde_json::to_vec(&manifest).map_err(err)?)
            .map_err(err)?;
        file.sync_all().map_err(err)?;
        let _ = verify_entry(stage.path(), &hash, &cancel)?;
        if digest(path, &cancel)? != hash {
            return Err("Original changed before publication".into());
        }
        check(&cancel)?;
        if usage(&root)? > CACHE_LIMIT {
            return Err("Cache quota exceeded before publication".into());
        }
        fs::rename(stage.path(), &target).map_err(err)?;
        write_lookup(&root, path, &hash)?;
        results.push(Prepared {
            directory: target,
            manifest,
        });
        progress(Progress {
            completed: index + 1,
            total,
            phase: "prepared",
        });
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Split;
    impl Separator for Split {
        fn separate(&mut self, pcm: Vec<f32>, _: Arc<AtomicBool>) -> Result<Parts, String> {
            Ok([
                pcm.iter().map(|v| v * 0.2).collect(),
                pcm.iter().map(|v| v * 0.5).collect(),
                pcm.iter().map(|v| v * 0.3).collect(),
            ])
        }
    }
    struct LoudSplit;
    impl Separator for LoudSplit {
        fn separate(&mut self, pcm: Vec<f32>, _: Arc<AtomicBool>) -> Result<Parts, String> {
            Ok([
                pcm.iter().map(|v| v * 3.).collect(),
                pcm.iter().map(|v| v * 0.5).collect(),
                pcm.iter().map(|v| v * 0.3).collect(),
            ])
        }
    }
    fn fixture(path: &Path, frames: usize) {
        // Test original is raw PCM, explicitly decoded by FFmpeg through a WAV header.
        // This input fixture is not a prepared stem output.
        let mut f = fs::File::create(path).unwrap();
        let bytes = (frames * 4) as u32;
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + bytes).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&2u16.to_le_bytes()).unwrap();
        f.write_all(&44100u32.to_le_bytes()).unwrap();
        f.write_all(&176400u32.to_le_bytes()).unwrap();
        f.write_all(&4u16.to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&bytes.to_le_bytes()).unwrap();
        for n in 0..frames {
            let v = if n == 100 || n == frames.saturating_sub(101) || n == 463050 || n == 926100 {
                20000i16
            } else {
                0
            };
            f.write_all(&v.to_le_bytes()).unwrap();
            f.write_all(&v.to_le_bytes()).unwrap();
        }
    }
    #[test]
    fn overlap_matches_live_linear_crossfade_and_preserves_exact_tail() {
        let previous = [vec![2.; 12], vec![4.; 12], vec![6.; 12]];
        let mut current = [vec![4.; 16], vec![8.; 16], vec![12.; 16]];
        blend(&previous, &mut current);
        for p in 0..3 {
            for frame in 0..6 {
                let a = frame as f32 / 5.;
                for ch in 0..2 {
                    let i = frame * 2 + ch;
                    assert_eq!(
                        current[p][i],
                        previous[p][i] * (1. - a) + (p as f32 + 1.) * 4. * a
                    )
                }
            }
        }
        let frames = WINDOW * 2 + 123;
        let original: Vec<f32> = (0..frames * 2).map(|i| (i % 137) as f32 / 137.).collect();
        let bytes: Vec<u8> = original.iter().flat_map(|v| v.to_le_bytes()).collect();
        let mut emitted = Vec::new();
        let total = windows(
            &bytes[..],
            &AtomicBool::new(false),
            |pcm| Ok([pcm.clone(), pcm.clone(), pcm]),
            |parts, n| {
                emitted.extend_from_slice(&parts[0][..n * 2]);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(total, frames as u64);
        assert_eq!(emitted.len(), original.len());
        assert!(
            emitted
                .iter()
                .zip(original)
                .all(|(a, b)| (*a - b).abs() < 0.000001)
        );
    }
    #[test]
    fn canceled_or_malformed_windows_never_emit() {
        let cancel = AtomicBool::new(true);
        let mut emitted = false;
        assert!(
            windows(
                &[][..],
                &cancel,
                |_| panic!(),
                |_, _| {
                    emitted = true;
                    Ok(())
                }
            )
            .is_err()
        );
        assert!(!emitted);
        assert!(
            windows(
                &[0u8; 7][..],
                &AtomicBool::new(false),
                |_| panic!(),
                |_, _| panic!()
            )
            .is_err()
        );
    }
    #[test]
    fn cache_clear_refuses_unowned_roots_and_members() {
        let t = tempfile::tempdir().unwrap();
        assert!(clear(t.path()).is_err());
        let root = t.path().join("prepared-stems-v1");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("unrelated"), b"retain").unwrap();
        assert!(clear(&root).is_err());
        assert_eq!(fs::read(root.join("unrelated")).unwrap(), b"retain");
    }
    #[test]
    fn ffmpeg_gapless_three_parts_roundtrip_and_source_unchanged() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("original.wav");
        fixture(&source, WINDOW * 2 + 137);
        let original = fs::read(&source).unwrap();
        let root = t.path().join("prepared-stems-v1");
        let cancel = Arc::new(AtomicBool::new(false));
        let prepared = prepare_with(
            &root,
            PlaylistSnapshot {
                name: "selected".into(),
                tracks: vec![source.clone(), source.clone()],
            },
            cancel.clone(),
            |_| {},
            &mut LoudSplit,
        )
        .unwrap();
        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].manifest.frames, (WINDOW * 2 + 137) as u64);
        assert!(candidate(&root, &source).is_some());
        assert!(ready(&root, &source, &cancel).unwrap().is_some());
        assert_eq!(fs::read(&source).unwrap(), original);
        for part in &prepared[0].manifest.parts {
            if part.name == "vocals.mp3" {
                assert!(part.restore_gain > 1.);
            }
            let (mut child, mut output) =
                decoder(&prepared[0].directory.join(&part.name), cancel.clone()).unwrap();
            let mut pcm = Vec::new();
            loop {
                let next = read_pcm(&mut output, WINDOW).unwrap();
                if next.is_empty() {
                    break;
                }
                pcm.extend(next);
            }
            child.wait().unwrap();
            assert_eq!(pcm.len(), (WINDOW * 2 + 137) * 2);
            // LAME gapless removal must preserve the transient's source-sample origin.
            let restored = pcm[200].abs() * part.restore_gain;
            let scale = if part.name == "vocals.mp3" {
                3.
            } else if part.name == "instruments.mp3" {
                0.5
            } else {
                0.3
            };
            assert!(
                (restored / (20000. / 32768. * scale) - 1.).abs() < 0.30,
                "gain restoration failed {restored}"
            );
            for expected in [100usize, 463050, 926100, WINDOW * 2 + 137 - 101] {
                let lo = expected - 12;
                let hi = expected + 13;
                let peak = (lo..hi)
                    .max_by(|a, b| pcm[a * 2].abs().total_cmp(&pcm[b * 2].abs()))
                    .unwrap();
                assert!(
                    peak.abs_diff(expected) <= 1,
                    "MP3 impulse shifted from {expected} to {peak}"
                );
            }
        }
        fs::write(prepared[0].directory.join("vocals.mp3"), b"changed").unwrap();
        assert!(ready(&root, &source, &cancel).is_err());
        clear(&root).unwrap();
        assert_eq!(fs::read(&source).unwrap(), original);
    }
    #[test]
    fn cancel_cleans_stage_and_never_publishes() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("original.wav");
        fixture(&source, 2000);
        let root = t.path().join("prepared-stems-v1");
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        assert!(
            prepare_with(
                &root,
                PlaylistSnapshot {
                    name: "selected".into(),
                    tracks: vec![source]
                },
                cancel,
                move |p| {
                    if p.phase == "encoding 256 kbps MP3 stems" {
                        c.store(true, Relaxed)
                    }
                },
                &mut Split
            )
            .is_err()
        );
        assert!(
            fs::read_dir(root)
                .unwrap()
                .all(|p| p.unwrap().file_name() == "cache-job.lock")
        );
    }
    #[test]
    fn lookup_refresh_reserves_metadata_even_for_reused_audio() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("source");
        fs::write(&source, b"original").unwrap();
        let root = t.path().join("prepared-stems-v1");
        fs::create_dir(&root).unwrap();
        let usage_file = fs::File::create(root.join("budget-fixture")).unwrap();
        usage_file.set_len(CACHE_LIMIT - 8192).unwrap();
        assert!(
            write_lookup(&root, &source, &"a".repeat(64))
                .unwrap_err()
                .contains("reservation")
        );
        assert!(!lookup_path(&root, &source.canonicalize().unwrap()).exists());
        assert_eq!(fs::read(source).unwrap(), b"original");
    }
    #[test]
    fn disk_quota_rejects_before_inference_and_does_not_delete() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("original.wav");
        fixture(&source, 10);
        let root = t.path().join("prepared-stems-v1");
        fs::create_dir(&root).unwrap();
        let file = fs::File::create(root.join("budget-fixture")).unwrap();
        file.set_len(CACHE_LIMIT).unwrap();
        struct Never;
        impl Separator for Never {
            fn separate(&mut self, _: Vec<f32>, _: Arc<AtomicBool>) -> Result<Parts, String> {
                panic!("quota must reject before inference")
            }
        }
        let result = prepare_with(
            &root,
            PlaylistSnapshot {
                name: "selected".into(),
                tracks: vec![source],
            },
            Arc::new(AtomicBool::new(false)),
            |_| {},
            &mut Never,
        );
        assert!(result.unwrap_err().contains("cache is full"));
        assert_eq!(file.metadata().unwrap().len(), CACHE_LIMIT);
    }
    #[test]
    #[ignore = "Requires explicitly configured native model and AMP_PREP_TEST_SOURCE short original; writes only temporary opt-in MP3 cache"]
    fn native_selected_playlist_preparation_roundtrips_compressed_stems() {
        let source = PathBuf::from(
            std::env::var_os("AMP_PREP_TEST_SOURCE")
                .expect("Set AMP_PREP_TEST_SOURCE to a short read-only original"),
        );
        let cancel = Arc::new(AtomicBool::new(false));
        let original = digest(&source, &cancel).unwrap();
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("prepared-stems-v1");
        let result = prepare_selected(
            &root,
            PlaylistSnapshot {
                name: "explicit native acceptance selection".into(),
                tracks: vec![source.clone()],
            },
            cancel.clone(),
            |p| println!("{}/{} {}", p.completed, p.total, p.phase),
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].manifest.frames > RATE * 11,
            "Use a >11sec native fixture to exercise window overlap"
        );
        let verified = ready(&root, &source, &cancel).unwrap().unwrap();
        assert_eq!(verified.manifest.source_sha256, original);
        assert!(
            verified
                .manifest
                .parts
                .iter()
                .all(|p| p.bytes > 0 && p.restore_gain >= 1.)
        );
        assert_eq!(digest(&source, &cancel).unwrap(), original);
        assert!(usage(&root).unwrap() <= CACHE_LIMIT);
        assert_eq!(fs::read_dir(&verified.directory).unwrap().count(), 4);
        assert!(
            verified
                .manifest
                .parts
                .iter()
                .all(|p| p.name.ends_with(".mp3"))
        );
        clear(&root).unwrap();
        assert!(ready(&root, &source, &cancel).unwrap().is_none());
        let canceled = Arc::new(AtomicBool::new(false));
        let token = canceled.clone();
        assert!(
            prepare_selected(
                &root,
                PlaylistSnapshot {
                    name: "native canceled selection".into(),
                    tracks: vec![source.clone()]
                },
                canceled,
                move |p| {
                    if p.phase == "encoding 256 kbps MP3 stems" {
                        token.store(true, Relaxed)
                    }
                }
            )
            .is_err()
        );
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .all(|p| p.unwrap().file_name() == "cache-job.lock")
        );
        assert_eq!(digest(&source, &cancel).unwrap(), original);
    }
}
