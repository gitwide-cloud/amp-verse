//! Staged playlist-only wire contract. No routes, publication, retries or deletion enabled.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const PROTOCOL: u32 = 1;
pub const MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Normal,
    Smart,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Create,
    Update,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub protocol: u32,
    pub operation_id: String,
    pub device_id: String,
    pub server_id: String,
    pub library_id: String,
    pub document_id: String,
    pub kind: Kind,
    pub document_version: u32,
    pub action: Action,
    pub base_revision: Option<Value>,
    pub document: Option<Value>,
}

/// Never a numeric catalog index, filesystem absolute path, or Android content URI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrackRef {
    pub library_id: String,
    pub track_key: String,
    pub relative_path: String,
}
impl TrackRef {
    /// Validates identity, not availability. Missing audio is intentionally representable.
    pub fn validate(&self, library: &str) -> Result<(), String> {
        if self.library_id != library {
            return Err("Track belongs to another library".into());
        }
        if self.relative_path.is_empty()
            || self.relative_path.len() > 4096
            || self.relative_path.contains(['\\', ':', '\0'])
            || self
                .relative_path
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
        {
            return Err("Expected portable root-relative track path".into());
        }
        let expected = format!("song_{:x}", Sha256::digest(self.relative_path.as_bytes()));
        if expected != self.track_key {
            return Err("Track key does not match relative path".into());
        }
        Ok(())
    }
}
fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
/// A legacy document ID is an exact filename stem, never a path.
pub fn safe_existing_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value != "."
        && value != ".."
        && !value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
}
impl Operation {
    /// Pure validation only. Delete remains unsupported until transactional tombstones exist.
    pub fn validate(&self, server: &str, library: &str) -> Result<(), String> {
        if self.protocol != PROTOCOL {
            return Err("Unsupported playlist sync protocol".into());
        }
        if self.server_id != server
            || self.library_id != library
            || server.is_empty()
            || library.is_empty()
        {
            return Err("Wrong canonical server or library".into());
        }
        if !token(&self.operation_id) || !token(&self.device_id) {
            return Err("Invalid operation/device ID".into());
        }
        if match self.kind {
            Kind::Normal => self.document_version != 1,
            Kind::Smart => !(1..=2).contains(&self.document_version),
        } {
            return Err("Unsupported document schema".into());
        }
        let prefix = match self.kind {
            Kind::Normal => "normal_",
            Kind::Smart => "smart_",
        };
        let id = self
            .document_id
            .strip_prefix(prefix)
            .filter(|id| {
                if self.action == Action::Create {
                    token(id)
                } else {
                    safe_existing_id(id)
                }
            })
            .ok_or("Invalid document ID")?;
        if self.action == Action::Update && self.base_revision.is_none() {
            return Err("Update requires a baseline".into());
        }
        if self.action == Action::Delete {
            return Err("Deletion unsupported until atomic tombstones are implemented".into());
        }
        if self.kind == Kind::Normal
            && (self.action == Action::Create) != self.base_revision.is_none()
        {
            return Err("Create requires no baseline; update requires a baseline".into());
        }
        if self.base_revision.as_ref().is_some_and(|r| !r.is_object()) {
            return Err("Expected revision object".into());
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > MAX_BYTES {
            return Err("Playlist operation too large".into());
        }
        let doc = self
            .document
            .as_ref()
            .and_then(Value::as_object)
            .ok_or("Expected complete playlist document")?;
        if doc.keys().any(|k| k.starts_with("_amp_")) {
            return Err("Storage-managed fields belong in the revision envelope".into());
        }
        if doc
            .get("name")
            .and_then(Value::as_str)
            .is_none_or(|n| n.trim().is_empty() || n.len() > 200)
        {
            return Err("Invalid playlist name".into());
        }
        if doc
            .get("companion_available")
            .is_some_and(|v| !v.is_boolean())
        {
            return Err("Invalid companion availability".into());
        }
        match self.kind {
            Kind::Normal => {
                if doc.get("version").and_then(Value::as_u64) != Some(1) {
                    return Err("Unsupported normal playlist version".into());
                }
                let tracks = doc
                    .get("tracks")
                    .and_then(Value::as_array)
                    .ok_or("Expected ordered track references")?;
                if tracks.len() > 20000 {
                    return Err("Too many playlist tracks".into());
                }
                for track in tracks {
                    serde_json::from_value::<TrackRef>(track.clone())
                        .map_err(|e| e.to_string())?
                        .validate(library)?;
                }
            }
            Kind::Smart => {
                // Validate through shared rules without reserializing: unknown compatible fields survive.
                let rules: crate::show::SmartPlaylist =
                    serde_json::from_value(Value::Object(doc.clone()))
                        .map_err(|e| e.to_string())?;
                if rules.id != id {
                    return Err("Smart rule ID differs from document ID".into());
                }
                if rules.source_library.as_ref().is_some_and(|v| v != library) {
                    return Err("Smart rules require explicit source remapping".into());
                }
                if rules.folders.iter().any(|p| {
                    p.to_string_lossy()
                        .contains([char::from(92), char::from(58)])
                }) {
                    return Err("Smart folders must use portable relative paths".into());
                }
                let mut settings = crate::show::Settings::default();
                settings.version = self.document_version;
                settings.playlists = vec![rules];
                settings.validate()?;
            }
        }
        Ok(())
    }
    /// Receipt lookup MUST compare this digest; an operation ID alone is insufficient.
    pub fn request_digest(&self) -> Result<String, String> {
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(self).map_err(|e| e.to_string())?)
        ))
    }
}

/// An ambiguous receipt never authorizes automatic retry or acknowledgment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Receipt {
    Applied {
        request_digest: String,
        canonical_document: Value,
        canonical_revision: Value,
    },
    Uncertain {
        request_digest: String,
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn track(path: &str) -> Value {
        json!({"library_id":"library","relative_path":path,"track_key":format!("song_{:x}",Sha256::digest(path.as_bytes()))})
    }
    fn operation() -> Operation {
        Operation {
            protocol: 1,
            operation_id: "op-1".into(),
            device_id: "phone-1".into(),
            server_id: "server".into(),
            library_id: "library".into(),
            document_id: "normal_existing".into(),
            kind: Kind::Normal,
            document_version: 1,
            action: Action::Create,
            base_revision: None,
            document: Some(
                json!({"version":1,"name":"Ordered","companion_available":true,"tracks":[track("missing/song.mp3"),track("another.flac"),track("missing/song.mp3")],"future_extension":{"preserve":true}}),
            ),
        }
    }
    #[test]
    fn preserves_duplicates_unresolved_references_and_compatible_fields() {
        let op = operation();
        let before = op.document.clone();
        op.validate("server", "library").unwrap();
        let round: Operation = serde_json::from_slice(&serde_json::to_vec(&op).unwrap()).unwrap();
        assert_eq!(round.document, before);
        assert_eq!(
            round.document.unwrap()["tracks"].as_array().unwrap().len(),
            3
        );
    }
    #[test]
    fn rejects_wrong_authority_and_nonportable_or_forged_refs() {
        let mut op = operation();
        assert!(op.validate("other", "library").is_err());
        assert!(op.validate("server", "other").is_err());
        for path in [
            "../song.mp3",
            "/song.mp3",
            "C:/song.mp3",
            "content://song",
            "a\\song.mp3",
            "a//song.mp3",
        ] {
            op.document.as_mut().unwrap()["tracks"] = json!([track(path)]);
            assert!(op.validate("server", "library").is_err(), "{path}");
        }
        op.document.as_mut().unwrap()["tracks"] =
            json!([{"library_id":"library","relative_path":"ok.mp3","track_key":"song_wrong"}]);
        assert!(op.validate("server", "library").is_err());
    }
    #[test]
    fn replay_digest_binds_order_payload_and_authority() {
        let op = operation();
        let digest = op.request_digest().unwrap();
        let mut changed = op.clone();
        changed.document.as_mut().unwrap()["tracks"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        assert_ne!(digest, changed.request_digest().unwrap());
        changed = op.clone();
        changed.server_id = "other".into();
        assert_ne!(digest, changed.request_digest().unwrap());
        let receipt = Receipt::Uncertain {
            request_digest: digest,
            reason: "Publication acknowledgment interrupted".into(),
        };
        assert!(matches!(
            serde_json::from_value::<Receipt>(serde_json::to_value(receipt).unwrap()).unwrap(),
            Receipt::Uncertain { .. }
        ));
    }
    #[test]
    fn deletes_and_missing_baselines_are_not_accidentally_enabled() {
        let mut op = operation();
        op.action = Action::Delete;
        assert!(op.validate("server", "library").is_err());
        op.action = Action::Update;
        assert!(op.validate("server", "library").is_err());
        op.base_revision = Some(json!({"revision":1}));
        op.validate("server", "library").unwrap();
    }
    #[test]
    fn preserves_complete_smart_definitions_and_extensions() {
        let mut op = operation();
        op.kind = Kind::Smart;
        op.document_id = "smart_demo".into();
        let mut rules =
            serde_json::to_value(&crate::show::Settings::default().playlists[0]).unwrap();
        rules["companion_available"] = json!(true);
        rules["future_compatible"] = json!({"keep":[1,2]});
        op.document = Some(rules.clone());
        op.validate("server", "library").unwrap();
        assert_eq!(op.document, Some(rules));
        op.document.as_mut().unwrap()["source_library"] = json!("unrelated");
        assert!(op.validate("server", "library").is_err());
    }
    #[test]
    fn rejects_nonplaylist_operations_and_unknown_envelope_fields() {
        let mut raw = serde_json::to_value(operation()).unwrap();
        raw["metadata"] = json!({});
        assert!(serde_json::from_value::<Operation>(raw).is_err());
        let mut op = operation();
        op.document_version = 999;
        assert!(op.validate("server", "library").is_err());
        op.document_version = 1;
        op.document_id = "preparation_track".into();
        assert!(op.validate("server", "library").is_err());
    }
}
