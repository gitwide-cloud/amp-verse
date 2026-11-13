//! Portable speech transcripts; independent of musical analysis and playback snapshots.
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transcript {
    pub version: u32,
    pub source_sha256: String,
    pub model: String,
    pub language: String,
    pub duration: f64,
    pub input: String,
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub device_id: String,
}
impl Transcript {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.source_sha256.len() != 64
            || !self.source_sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || !self.duration.is_finite()
            || self.duration <= 0.
            || self.duration > 86400.
            || self.segments.is_empty()
            || self.segments.len() > 100_000
        {
            return Err("Invalid or empty transcript".into());
        }
        let mut last = 0.;
        let mut size = 0;
        for s in &self.segments {
            size += s.text.len();
            if !s.start.is_finite()
                || !s.end.is_finite()
                || s.start < last
                || s.start < 0.
                || s.end <= s.start
                || s.end > self.duration + 1.
                || s.text.contains('\0')
                || size > 8 * 1024 * 1024
            {
                return Err("Invalid transcript timestamps or text".into());
            }
            last = s.start;
        }
        Ok(())
    }
    pub fn text(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.text.trim())
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn srt(&self) -> String {
        self.segments
            .iter()
            .enumerate()
            .map(|(i, s)| {
                format!(
                    "{}\n{} --> {}\n{}\n",
                    i + 1,
                    time(s.start),
                    time(s.end),
                    s.text.trim()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
fn time(s: f64) -> String {
    let ms = (s.max(0.) * 1000.).round() as u64;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}
fn location(root: &Path, relative: &Path) -> Result<(std::path::PathBuf, String), String> {
    let (root, relative) =
        crate::music_sources::owner(root, relative).map_err(|e| e.to_string())?;
    let key = relative
        .to_str()
        .ok_or("Transcript path is not UTF-8")?
        .replace('\\', "/");
    if key.is_empty()
        || key.contains(':')
        || Path::new(&key)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("Transcript references must stay relative".into());
    }
    Ok((root, key))
}
fn read(file: &Path, key: &str) -> Result<Option<Transcript>, String> {
    if !file.is_file() {
        return Ok(None);
    }
    let value = crate::catalog_db::local_record(file, key)?;
    value
        .map(|v| {
            let doc: Transcript = serde_json::from_value(v).map_err(|e| e.to_string())?;
            doc.validate()?;
            Ok(doc)
        })
        .transpose()
}
pub fn load(root: &Path, relative: &Path) -> Result<Option<Transcript>, String> {
    let (root, key) = location(root, relative)?;
    let file = root.join("ampdj/transcripts.ampdb");
    if !file.is_file() {
        return Ok(None);
    }
    let _lock = crate::coordination::Lock::acquire(&file)?;
    read(&file, &key)
}
pub fn save(
    root: &Path,
    relative: &Path,
    doc: &Transcript,
    expected: u64,
) -> Result<Transcript, String> {
    doc.validate()?;
    let (root, key) = location(root, relative)?;
    crate::storage::enroll(&root)?;
    let file = root.join("ampdj/transcripts.ampdb");
    let _lock = crate::coordination::Lock::acquire(&file)?;
    let current = read(&file, &key)?;
    if current.as_ref().map_or(0, |d| d.revision) != expected {
        return Err("Transcript changed on another device. Existing version retained; export your local draft or reload before replacing it.".into());
    }
    let mut saved = doc.clone();
    saved.revision = expected
        .checked_add(1)
        .ok_or("Transcript revision overflow")?;
    saved.device_id = crate::coordination::device_id()?;
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let local = work.path().join("transcripts.ampdb");
    if file.is_file() {
        std::fs::copy(&file, &local).map_err(|e| e.to_string())?;
    }
    crate::catalog_db::save_local_record(
        &local,
        &key,
        &serde_json::to_value(&saved).map_err(|e| e.to_string())?,
    )?;
    let pending = root.join(format!(
        "ampdj/.transcript-{}.pending",
        crate::coordination::token()
    ));
    let outcome = (|| {
        std::fs::copy(local, &pending).map_err(|e| e.to_string())?;
        std::fs::OpenOptions::new()
            .write(true)
            .open(&pending)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        crate::catalog_db::replace_snapshot(&pending, &file)
    })();
    if outcome.is_err() {
        let _ = std::fs::remove_file(pending);
    }
    outcome?;
    Ok(saved)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn doc() -> Transcript {
        Transcript {
            version: 1,
            source_sha256: "a".repeat(64),
            model: "base".into(),
            language: "en".into(),
            duration: 3.,
            input: "vocals".into(),
            segments: vec![Segment {
                start: 0.5,
                end: 2.,
                text: "Hello library".into(),
            }],
            revision: 0,
            device_id: String::new(),
        }
    }
    #[test]
    fn captions_and_validation() {
        let mut d = doc();
        d.validate().unwrap();
        assert!(d.srt().contains("00:00:00,500 --> 00:00:02,000"));
        d.segments[0].end = f64::NAN;
        assert!(d.validate().is_err());
    }
    #[test]
    fn portable_store_detects_conflicts_without_touching_audio() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("speech.mp3"), b"original").unwrap();
        crate::storage::register(dir.path());
        let saved = save(dir.path(), Path::new("speech.mp3"), &doc(), 0).unwrap();
        assert_eq!(saved.revision, 1);
        assert!(save(dir.path(), Path::new("speech.mp3"), &doc(), 0).is_err());
        assert_eq!(
            load(dir.path(), Path::new("speech.mp3"))
                .unwrap()
                .unwrap()
                .text(),
            "Hello library"
        );
        assert_eq!(
            std::fs::read(dir.path().join("speech.mp3")).unwrap(),
            b"original"
        );
    }
}
