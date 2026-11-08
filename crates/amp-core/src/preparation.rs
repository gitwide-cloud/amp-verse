//! Portable, versioned preparation beside the original audio file.
use crate::{Preparation, resolve, sidecar};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Cue {
    pub id: String,
    pub label: String,
    pub seconds: f64,
    pub kind: String,
    #[serde(default)]
    pub number: Option<u16>,
    #[serde(default, flatten)]
    pub extensions: std::collections::BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalysisInfo {
    pub engine: String,
    pub duration: f64,
    pub tempo_confidence: f64,
    pub key_confidence: f64,
    pub reviewed: bool,
}
/// Give legacy manual hot cues stable unused numbers without changing their IDs or positions.
pub fn assign_numbers(p: &mut Preparation) {
    let mut used: std::collections::HashSet<_> = p
        .manual_cues
        .iter()
        .chain(&p.cues)
        .filter_map(|c| c.number)
        .collect();
    for cue in &mut p.manual_cues {
        if cue.number.is_none()
            && let Some(number) = (1..=128).find(|n| !used.contains(n))
        {
            cue.number = Some(number);
            used.insert(number);
        }
    }
}
pub fn validate(p: &Preparation) -> Result<(), String> {
    if p.schema_version.is_some_and(|v| v != 1) {
        return Err("Unsupported preparation schema".into());
    }
    if let Some(v) = p.extensions.get("energy")
        && !v
            .as_f64()
            .is_some_and(|v| v.is_finite() && (0. ..=1.).contains(&v))
    {
        return Err("Energy must be between 0 and 1".into());
    }
    if let Some(v) = p.extensions.get("rating")
        && !v.as_u64().is_some_and(|v| v <= 5)
    {
        return Err("Rating must be 0 to 5".into());
    }
    if let Some(v) = p.extensions.get("playback_rate")
        && !v
            .as_f64()
            .is_some_and(|v| v.is_finite() && (0.84..=1.16).contains(&v))
    {
        return Err("Playback rate must be between 0.84 and 1.16".into());
    }
    let mut numbers = std::collections::HashSet::new();
    for cue in p.manual_cues.iter().chain(&p.cues) {
        if let Some(n) = cue.number
            && (n == 0 || n > 128 || !numbers.insert(n))
        {
            return Err("Saved hot-cue numbers must be unique and between 1 and 128".into());
        }
    }
    let mut ids = std::collections::HashSet::new();
    for cue in p.cues.iter().chain(&p.manual_cues) {
        if cue.id.is_empty() || !ids.insert(&cue.id) || !cue.seconds.is_finite() || cue.seconds < 0.
        {
            return Err("Cue IDs must be unique and positions nonnegative".into());
        }
    }
    if let Some(a) = &p.analysis {
        if !a.duration.is_finite()
            || a.duration <= 0.
            || ![a.tempo_confidence, a.key_confidence]
                .iter()
                .all(|x| x.is_finite() && (0. ..=1.).contains(x))
        {
            return Err("Invalid analysis measurements".into());
        }
        if p.cue_out.is_some_and(|v| v > a.duration + 0.1)
            || p.cues
                .iter()
                .chain(&p.manual_cues)
                .any(|c| c.seconds > a.duration + 0.1)
        {
            return Err("Cue exceeds audio duration".into());
        }
    }
    if p.source_sha256
        .as_ref()
        .is_some_and(|h| h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("Invalid source fingerprint".into());
    }
    Ok(())
}
pub fn fingerprint(path: &Path) -> Result<String, String> {
    let mut input = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
/// Preferred portable store. Legacy adjacent sidecars remain readable.
pub fn portable(path: &Path) -> std::path::PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".json");
    path.parent()
        .unwrap_or(Path::new("."))
        .join(".ampverse")
        .join("tracks")
        .join(name)
}
pub fn revision(path: &Path) -> Option<Vec<u8>> {
    fs::read(portable(path))
        .or_else(|_| fs::read(sidecar(path)))
        .ok()
}
pub fn load(path: &Path) -> Result<Preparation, String> {
    load_with_revision(path).map(|(p, _)| p)
}
pub fn load_with_revision(path: &Path) -> Result<(Preparation, Option<Vec<u8>>), String> {
    match fs::read(portable(path)).or_else(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            fs::read(sidecar(path))
        } else {
            Err(e)
        }
    }) {
        Ok(bytes) => {
            let mut p: Preparation = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            assign_numbers(&mut p);
            p.validate()?;
            Ok((p, Some(bytes)))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Preparation::default(), None)),
        Err(e) => Err(e.to_string()),
    }
}
pub fn save(root: &Path, relative: &Path, p: &Preparation) -> Result<(), String> {
    save_record(root, relative, p, None)
}
pub fn save_if_unchanged(
    root: &Path,
    relative: &Path,
    p: &Preparation,
    expected: Option<&[u8]>,
) -> Result<(), String> {
    save_record(root, relative, p, Some(expected))
}
fn save_record(
    root: &Path,
    relative: &Path,
    p: &Preparation,
    expected: Option<Option<&[u8]>>,
) -> Result<(), String> {
    p.validate()?;
    let audio = resolve(root, relative).map_err(|e| e.to_string())?;
    if let Some(expected) = &p.source_sha256
        && fingerprint(&audio)? != *expected
    {
        return Err("Audio changed since preparation; analyze it again".into());
    }
    let target = portable(&audio);
    let metadata_folder = audio.parent().unwrap().join(".ampverse");
    if fs::symlink_metadata(&metadata_folder).is_ok_and(|m| m.file_type().is_symlink())
        || fs::symlink_metadata(target.parent().unwrap()).is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("Portable metadata folders must not be symlinks".into());
    }
    fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
    if !target
        .parent()
        .unwrap()
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(
            audio
                .parent()
                .unwrap()
                .canonicalize()
                .map_err(|e| e.to_string())?,
        )
    {
        return Err("Portable metadata must stay beside its music folder".into());
    }

    let lock_path = target.with_extension("json.lock");
    if fs::symlink_metadata(&lock_path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Metadata lock must be a regular file".into());
    }
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|e| e.to_string())?;
    lock.try_lock()
        .map_err(|e| format!("Metadata is busy or cannot be locked: {e}"))?;
    if let Some(expected) = expected
        && revision(&audio).as_deref() != expected
    {
        return Err("Metadata changed since it was read; newer values retained".into());
    }
    // Refuse a metadata symlink instead of overwriting an unrelated file.
    if fs::symlink_metadata(&target).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Metadata sidecar must be a regular file".into());
    }
    let mut temp = target.as_os_str().to_os_string();
    temp.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    let temp = std::path::PathBuf::from(temp);
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        f.write_all(&serde_json::to_vec_pretty(p).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temp, &target).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_and_extensions_round_trip() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("song.wav"), b"fixture").unwrap();
        let mut p:Preparation=serde_json::from_str(r#"{"cue_in":2.0,"cue_out":5.0,"bpm":120,"beat_offset":0.0,"stems":[],"future_data":{"a":1}}"#).unwrap();
        p.schema_version = Some(1);
        p.source_sha256 = Some(fingerprint(&d.path().join("song.wav")).unwrap());
        p.manual_cues.push(Cue {
            id: "local-1".into(),
            label: "Mix".into(),
            seconds: 3.,
            kind: "hotcue".into(),
            ..Default::default()
        });
        p.extensions
            .insert("energy".into(), serde_json::json!(0.65));
        p.extensions.insert("rating".into(), serde_json::json!(4));
        p.extensions
            .insert("playback_rate".into(), serde_json::json!(1.02));
        p.extensions
            .insert("title".into(), serde_json::json!("Reviewed title"));
        assign_numbers(&mut p);
        save(d.path(), Path::new("song.wav"), &p).unwrap();
        assert_eq!(load(&d.path().join("song.wav")).unwrap(), p);
        assert_eq!(
            crate::scan(d.path()).unwrap().tracks[0].title,
            "Reviewed title"
        );
        save(d.path(), Path::new("song.wav"), &p).unwrap();
        fs::write(d.path().join("song.wav"), b"changed").unwrap();
        assert!(save(d.path(), Path::new("song.wav"), &p).is_err());
    }
    #[test]
    fn library_values_reject_out_of_range_data() {
        for (field, value) in [
            ("energy", serde_json::json!(1.5)),
            ("rating", serde_json::json!(6)),
            ("playback_rate", serde_json::json!(2.)),
        ] {
            let mut p = Preparation::default();
            p.extensions.insert(field.into(), value);
            assert!(p.validate().is_err());
        }
    }
    #[test]
    fn stale_background_writes_cannot_replace_newer_approved_metadata() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("song.wav"), b"fixture").unwrap();
        let first = Preparation::default();
        save(d.path(), Path::new("song.wav"), &first).unwrap();
        let before = revision(&d.path().join("song.wav")).unwrap();
        let corrected = Preparation {
            cue_in: 2.,
            manual_override: true,
            ..Default::default()
        };
        save(d.path(), Path::new("song.wav"), &corrected).unwrap();
        assert!(save_if_unchanged(d.path(), Path::new("song.wav"), &first, Some(&before)).is_err());
        assert_eq!(load(&d.path().join("song.wav")).unwrap().cue_in, 2.);
    }
    #[test]
    fn hotcue_numbers_and_future_fields_survive_roundtrip() {
        let mut p:Preparation=serde_json::from_value(serde_json::json!({"cue_in":0.,"cue_out":null,"bpm":null,"beat_offset":null,"manual_cues":[{"id":"first","label":"One","seconds":1.,"kind":"hotcue","number":2,"color":"red"},{"id":"second","label":"Two","seconds":2.,"kind":"hotcue"}]})).unwrap();
        assign_numbers(&mut p);
        assert_eq!(p.manual_cues[0].number, Some(2));
        assert_eq!(p.manual_cues[1].number, Some(1));
        assert_eq!(p.manual_cues[0].extensions["color"], "red");
        p.validate().unwrap();
        let p: Preparation = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert_eq!(p.manual_cues[0].extensions["color"], "red");
    }
    #[test]
    fn canonical_store_overrides_legacy_without_editing_it() {
        let d = tempfile::tempdir().unwrap();
        let audio = d.path().join("song.wav");
        fs::write(&audio, b"fixture").unwrap();
        let old = Preparation {
            bpm: Some(120.),
            ..Default::default()
        };
        let legacy = serde_json::to_vec(&old).unwrap();
        fs::write(sidecar(&audio), &legacy).unwrap();
        assert_eq!(load(&audio).unwrap().bpm, Some(120.));
        let new = Preparation {
            bpm: Some(121.),
            ..Default::default()
        };
        save(d.path(), Path::new("song.wav"), &new).unwrap();
        assert_eq!(load(&audio).unwrap().bpm, Some(121.));
        assert_eq!(fs::read(sidecar(&audio)).unwrap(), legacy);
        assert_eq!(crate::scan(d.path()).unwrap().tracks.len(), 1);
    }
    #[test]
    fn bad_cues_and_versions_rejected() {
        let mut p = Preparation {
            schema_version: Some(2),
            ..Default::default()
        };
        assert!(p.validate().is_err());
        p.schema_version = Some(1);
        p.manual_cues.push(Cue {
            id: "a".into(),
            label: "x".into(),
            seconds: f64::NAN,
            kind: "hotcue".into(),
            ..Default::default()
        });
        assert!(p.validate().is_err());
    }
}
