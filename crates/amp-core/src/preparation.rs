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
    let root = crate::storage::root_for(path);
    let relative = path.strip_prefix(&root).unwrap_or(path);
    let mut name = relative.as_os_str().to_os_string();
    name.push(".json");
    root.join("ampdj/tracks").join(name)
}
fn legacy(path: &Path) -> std::path::PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".json");
    path.parent()
        .unwrap_or(Path::new("."))
        .join(".ampverse/tracks")
        .join(name)
}
fn record(path: &Path) -> std::io::Result<Vec<u8>> {
    if let Some(bytes) = crate::catalog_db::read(&portable(path)).map_err(std::io::Error::other)? {
        return Ok(bytes);
    }
    let mut error = None;
    for candidate in [portable(path), legacy(path), sidecar(path)] {
        match fs::read(candidate) {
            Ok(bytes) => return Ok(bytes),
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    || e.kind() == std::io::ErrorKind::NotADirectory =>
            {
                error = Some(e)
            }
            Err(e) => return Err(e),
        }
    }
    Err(error.unwrap())
}
pub fn revision(path: &Path) -> Option<Vec<u8>> {
    record(path).ok()
}
pub fn load(path: &Path) -> Result<Preparation, String> {
    load_with_revision(path).map(|(p, _)| p)
}
pub fn load_with_revision(path: &Path) -> Result<(Preparation, Option<Vec<u8>>), String> {
    match record(path) {
        Ok(bytes) => {
            let mut p: Preparation = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            assign_numbers(&mut p);
            p.validate()?;
            Ok((p, Some(bytes)))
        }
        Err(e)
            if e.kind() == std::io::ErrorKind::NotFound
                || e.kind() == std::io::ErrorKind::NotADirectory =>
        {
            Ok((Preparation::default(), None))
        }
        Err(e) => Err(e.to_string()),
    }
}
pub fn save(root: &Path, relative: &Path, p: &Preparation) -> Result<(), String> {
    save_record(root, relative, p, None, None)
}
pub fn save_if_unchanged(
    root: &Path,
    relative: &Path,
    p: &Preparation,
    expected: Option<&[u8]>,
) -> Result<(), String> {
    save_record(root, relative, p, Some(expected), None)
}
pub fn publish_claimed(
    root: &Path,
    relative: &Path,
    p: &Preparation,
    expected: Option<&[u8]>,
    lease: &crate::coordination::Lease,
) -> Result<(), String> {
    save_record(root, relative, p, Some(expected), Some(lease))
}
fn save_record(
    root: &Path,
    relative: &Path,
    p: &Preparation,
    expected: Option<Option<&[u8]>>,
    lease: Option<&crate::coordination::Lease>,
) -> Result<(), String> {
    crate::storage::enroll(root)?;
    let _migration = crate::coordination::Lock::acquire(&root.join("ampdj/catalog-migration"))?;
    p.validate()?;
    let audio = resolve(root, relative).map_err(|e| e.to_string())?;
    if let Some(expected) = &p.source_sha256
        && fingerprint(&audio)? != *expected
    {
        return Err("Audio changed since preparation; analyze it again".into());
    }
    crate::storage::register(&root.canonicalize().map_err(|e| e.to_string())?);
    let target = portable(&audio);
    let metadata_folder = root.join("ampdj");
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
        .starts_with(root.canonicalize().map_err(|e| e.to_string())?)
    {
        return Err("Portable metadata must stay inside its library".into());
    }

    let device = if let Some(lease) = lease {
        lease.device.clone()
    } else {
        crate::coordination::device_id()?
    };
    let library = crate::coordination::library_id(root)?;
    let _lock = crate::coordination::Lock::acquire(&target)?;
    if let Some(lease) = lease {
        lease.verify_record_locked(&target, &library)?;
    }
    let (current, current_bytes) = load_with_revision(&audio)?;
    let current_json = serde_json::to_value(&current).map_err(|e| e.to_string())?;
    let mut proposed = serde_json::to_value(p).map_err(|e| e.to_string())?;
    if expected.is_none()
        && current_bytes.is_some()
        && proposed.get("_amp_sync").is_none()
        && proposed != current_json
    {
        return Err(
            "Legacy metadata needs a read snapshot before editing; reload and use a guarded save"
                .into(),
        );
    }
    let mut base = if let Some(expected) = expected {
        let mut base: Preparation = if let Some(bytes) = expected {
            serde_json::from_slice(bytes).map_err(|e| e.to_string())?
        } else {
            Preparation::default()
        };
        assign_numbers(&mut base);
        serde_json::to_value(base).map_err(|e| e.to_string())?
    } else {
        if crate::coordination::revision(&proposed).revision
            != crate::coordination::revision(&current_json).revision
        {
            return Err("Metadata revision changed; reload before saving".into());
        }
        current_json.clone()
    };
    // Stage protected automatic deltas against CURRENT values: an energy-only
    // worker must not accidentally propose undoing a cue edited on another PC.
    let automatic_fields = [
        "_amp_sync",
        "source_sha256",
        "schema_version",
        "analysis_origin",
        "analysis_source_stat",
        "analysis_engine_checked",
        "analysis_recommendation",
        "analysis_proposals",
        "waveform",
        "waveform_overview",
    ];
    let keys: std::collections::BTreeSet<_> = base
        .as_object()
        .unwrap()
        .keys()
        .chain(proposed.as_object().unwrap().keys())
        .cloned()
        .collect();
    let changes_protected = keys
        .iter()
        .any(|key| !automatic_fields.contains(&key.as_str()) && proposed.get(key) != base.get(key));
    if let Some(lease) = lease
        && (current.manual_override
            || !current.manual_cues.is_empty()
            || current.analysis.as_ref().is_some_and(|a| a.reviewed))
        && changes_protected
    {
        let original = proposed.clone();
        let mut candidate = current_json.clone();
        for key in &keys {
            if original.get(key) != base.get(key) {
                if let Some(value) = original.get(key) {
                    candidate
                        .as_object_mut()
                        .unwrap()
                        .insert(key.clone(), value.clone());
                } else {
                    candidate.as_object_mut().unwrap().remove(key);
                }
            }
        }
        for key in ["_amp_sync", "analysis_recommendation", "analysis_proposals"] {
            candidate.as_object_mut().unwrap().remove(key);
        }
        proposed = current_json.clone();
        // Nonconflicting derived assets/provenance may coexist with an approved cue.
        for key in automatic_fields.iter().filter(|&&k| {
            !matches!(
                k,
                "_amp_sync" | "analysis_recommendation" | "analysis_proposals"
            )
        }) {
            if original.get(*key) != base.get(*key)
                && (current_json.get(*key) == base.get(*key)
                    || current_json.get(*key) == original.get(*key))
                && let Some(value) = original.get(*key)
            {
                proposed
                    .as_object_mut()
                    .unwrap()
                    .insert((*key).into(), value.clone());
            }
        }

        let mut proposals = current_json
            .get("analysis_proposals")
            .and_then(serde_json::Value::as_object)
            .cloned()
            .unwrap_or_default();
        let (key, record) = lease.proposal_record(&candidate);
        proposals.insert(key, record);
        proposed.as_object_mut().unwrap().insert(
            "analysis_proposals".into(),
            serde_json::Value::Object(proposals),
        );
        if lease.primary_analysis() || current_json.get("analysis_recommendation").is_none() {
            proposed
                .as_object_mut()
                .unwrap()
                .insert("analysis_recommendation".into(), candidate);
        }
        base = current_json.clone();
    }
    let merged = crate::coordination::merge(&base, &current_json, &proposed, &device, &library)?;
    let checked: Preparation = serde_json::from_value(merged).map_err(|e| e.to_string())?;
    checked.validate()?;
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
        if crate::catalog_db::write_guarded(
            &target,
            &serde_json::to_vec(&checked).map_err(|e| e.to_string())?,
            || {
                if let Some(lease) = lease {
                    lease.verify_record_locked(&target, &library)
                } else {
                    Ok(())
                }
            },
        )? {
            if let Some(lease) = lease {
                lease.finish_locked()?;
            }
            return Ok(());
        }
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        f.write_all(&serde_json::to_vec_pretty(&checked).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        if let Some(lease) = lease {
            lease.verify_record_locked(&target, &library)?;
        }
        fs::rename(&temp, &target).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        fs::File::open(target.parent().unwrap())
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        if let Some(lease) = lease {
            lease.finish_locked()?;
        }
        Ok(())
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
        let mut loaded = load(&d.path().join("song.wav")).unwrap();
        let sync = loaded.extensions.remove("_amp_sync").unwrap();
        assert_eq!(loaded, p);
        p.extensions.insert("_amp_sync".into(), sync);
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
        let mut corrected = load(&d.path().join("song.wav")).unwrap();
        corrected.cue_in = 2.;
        corrected.manual_override = true;
        save(d.path(), Path::new("song.wav"), &corrected).unwrap();
        let mut obsolete = first.clone();
        obsolete.cue_in = 1.;
        assert!(
            save_if_unchanged(d.path(), Path::new("song.wav"), &obsolete, Some(&before)).is_err()
        );
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
        let before = revision(&audio);
        save_if_unchanged(d.path(), Path::new("song.wav"), &new, before.as_deref()).unwrap();
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
