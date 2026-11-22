//! Shared advisory metadata. Absence and ambiguous tags never prove clean audio.
use crate::Preparation;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Classification {
    Clean,
    Explicit,
    #[default]
    Unknown,
}
impl Classification {
    pub fn label(self) -> &'static str {
        match self {
            Self::Clean => "Clean",
            Self::Explicit => "Explicit",
            Self::Unknown => "Unknown",
        }
    }
}
pub fn classification(p: &Preparation) -> Classification {
    // A manual Unknown is an intentional override, not permission to infer from tags.
    if let Some(v) = p.extensions.get("content_advisory") {
        return serde_json::from_value(v.clone()).unwrap_or_default();
    }
    let Some(tags) = p
        .extensions
        .get("embedded_tags")
        .and_then(|v| v.get("ParentalAdvisory"))
        .and_then(|v| v.as_array())
    else {
        return Classification::Unknown;
    };
    let mut found = None;
    for value in tags {
        let current = match value.as_str().map(str::trim) {
            Some("2") | Some("Clean") | Some("clean") => Classification::Clean,
            Some("1") | Some("4") | Some("Explicit") | Some("explicit") => Classification::Explicit,
            _ => Classification::Unknown,
        };
        if current == Classification::Unknown || found.is_some_and(|v| v != current) {
            return Classification::Unknown;
        }
        found = Some(current);
    }
    found.unwrap_or_default()
}
pub fn set_manual(p: &mut Preparation, value: Classification) {
    p.extensions
        .insert("content_advisory".into(), serde_json::json!(value));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refresh_failure_never_reuses_stale_clean_tag_or_removes_manual_choice() {
        let mut p = Preparation::default();
        p.extensions.insert(
            "embedded_tags".into(),
            serde_json::json!({"ParentalAdvisory":["2"]}),
        );
        crate::genres::read_embedded(std::path::Path::new("/missing/test-advisory.flac"), &mut p);
        assert_eq!(classification(&p), Classification::Unknown);
        set_manual(&mut p, Classification::Explicit);
        crate::genres::read_embedded(std::path::Path::new("/missing/test-advisory.flac"), &mut p);
        assert_eq!(classification(&p), Classification::Explicit);
    }
    #[test]
    fn tags_are_conservative_and_manual_survives_roundtrip() {
        let mut p = Preparation::default();
        assert_eq!(classification(&p), Classification::Unknown);
        for (tags, expected) in [
            (serde_json::json!(["2"]), Classification::Clean),
            (serde_json::json!(["1"]), Classification::Explicit),
            (serde_json::json!(["0"]), Classification::Unknown),
            (serde_json::json!(["2", "1"]), Classification::Unknown),
        ] {
            p.extensions.insert(
                "embedded_tags".into(),
                serde_json::json!({"ParentalAdvisory":tags,"TrackTitle":["Clean radio edit"]}),
            );
            assert_eq!(classification(&p), expected);
        }
        set_manual(&mut p, Classification::Unknown);
        p.extensions.insert(
            "embedded_tags".into(),
            serde_json::json!({"ParentalAdvisory":["2"]}),
        );
        let saved: Preparation = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert_eq!(classification(&saved), Classification::Unknown);
    }
}

#[cfg(test)]
mod rule_tests {
    use super::*;
    use crate::{
        playlist_sync_contract::{Action, Kind, Operation},
        show,
        smart_filters::{Field, Filter},
    };
    #[test]
    fn clean_rules_roundtrip_and_require_version_three_on_sync() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = show::Settings::default();
        settings.playlists[0]
            .filters
            .push(Filter::new(Field::Clean));
        assert!(settings.validate().is_err()); // Never label new rules as legacy-compatible.
        settings.save(root.path()).unwrap();
        let saved = show::Settings::load(root.path()).unwrap();
        assert_eq!(saved.version, 3);
        let rules = &saved.playlists[0];
        for classification in [
            Classification::Clean,
            Classification::Explicit,
            Classification::Unknown,
        ] {
            let mut prep = Preparation::default();
            set_manual(&mut prep, classification);
            let track = crate::Track {
                path: "song.mp3".into(),
                title: "Clean edit".into(),
                preparation: prep,
            };
            assert_eq!(
                show::eligible(rules, &track),
                classification == Classification::Clean
            );
        }
        let mut op = Operation {
            protocol: 1,
            operation_id: "test-op".into(),
            device_id: "device".into(),
            server_id: "server".into(),
            library_id: "library".into(),
            document_id: format!("smart_{}", rules.id),
            kind: Kind::Smart,
            document_version: 3,
            action: Action::Create,
            base_revision: None,
            document: Some(serde_json::to_value(rules).unwrap()),
        };
        op.validate("server", "library").unwrap();
        let decoded: Operation = serde_json::from_slice(&serde_json::to_vec(&op).unwrap()).unwrap();
        assert_eq!(decoded.document, op.document);
        op.document_version = 2;
        assert!(op.validate("server", "library").is_err());
        // Legacy rules still parse and validate unchanged.
        show::Settings::default().validate().unwrap();
    }
}

#[cfg(test)]
mod import_tests {
    use super::*;
    #[test]
    fn flac_advisory_import_is_read_only_and_manual_override_wins() {
        // Minimal FLAC metadata fixture: STREAMINFO and a Vorbis ITUNESADVISORY tag.
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Clean title.flac");
        let mut bytes = b"fLaC".to_vec();
        bytes.extend([0, 0, 0, 34]);
        let mut info = [0u8; 34];
        info[0..2].copy_from_slice(&4096u16.to_be_bytes());
        info[2..4].copy_from_slice(&4096u16.to_be_bytes());
        let format = (44100u64 << 44) | (15u64 << 36) | 44100;
        info[10..18].copy_from_slice(&format.to_be_bytes());
        bytes.extend(info);
        let tag = b"ITUNESADVISORY=2";
        let mut comments = Vec::new();
        comments.extend(0u32.to_le_bytes());
        comments.extend(1u32.to_le_bytes());
        comments.extend((tag.len() as u32).to_le_bytes());
        comments.extend(tag);
        bytes.push(0x84);
        bytes.extend((comments.len() as u32).to_be_bytes()[1..].iter());
        bytes.extend(comments);
        std::fs::write(&path, &bytes).unwrap();
        let mut prep = Preparation::default();
        crate::genres::read_embedded(&path, &mut prep);
        assert_eq!(classification(&prep), Classification::Clean);
        set_manual(&mut prep, Classification::Explicit);
        crate::genres::read_embedded(&path, &mut prep);
        assert_eq!(classification(&prep), Classification::Explicit);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}
