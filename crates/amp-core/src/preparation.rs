//! Portable, versioned preparation beside the original audio file.
use crate::{Preparation, resolve, sidecar};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Cue {
    pub id: String,
    pub label: String,
    pub seconds: f64,
    pub kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalysisInfo {
    pub engine: String,
    pub duration: f64,
    pub tempo_confidence: f64,
    pub key_confidence: f64,
    pub reviewed: bool,
}
pub fn validate(p: &Preparation) -> Result<(), String> {
    if p.schema_version.is_some_and(|v| v != 1) {
        return Err("Unsupported preparation schema".into());
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
pub fn load(path: &Path) -> Result<Preparation, String> {
    match fs::read(sidecar(path)) {
        Ok(bytes) => {
            let p: Preparation = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            p.validate()?;
            Ok(p)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Preparation::default()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn save(root: &Path, relative: &Path, p: &Preparation) -> Result<(), String> {
    p.validate()?;
    let audio = resolve(root, relative).map_err(|e| e.to_string())?;
    if let Some(expected) = &p.source_sha256
        && fingerprint(&audio)? != *expected
    {
        return Err("Audio changed since preparation; analyze it again".into());
    }
    let target = sidecar(&audio);
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
        });
        save(d.path(), Path::new("song.wav"), &p).unwrap();
        assert_eq!(load(&d.path().join("song.wav")).unwrap(), p);
        save(d.path(), Path::new("song.wav"), &p).unwrap();
        fs::write(d.path().join("song.wav"), b"changed").unwrap();
        assert!(save(d.path(), Path::new("song.wav"), &p).is_err());
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
        });
        assert!(p.validate().is_err());
    }
}
