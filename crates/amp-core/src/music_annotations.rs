//! Declared track categories, not an acoustic AI detector. Keep them out of genres.
use crate::Preparation;

pub const FIELD: &str = "track_annotations";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    AiGenerated,
    Acappella,
    Instrumental,
    Clip,
}
impl Category {
    pub const ALL: &'static [Self] = &[
        Self::AiGenerated,
        Self::Acappella,
        Self::Instrumental,
        Self::Clip,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::AiGenerated => "ai_generated",
            Self::Acappella => "acappella",
            Self::Instrumental => "instrumental",
            Self::Clip => "clip",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::AiGenerated => "AI-generated",
            Self::Acappella => "A cappella",
            Self::Instrumental => "Instrumental",
            Self::Clip => "Clip",
        }
    }
}
/// Explicit choices override declared genre labels. Never inspect filenames,
/// infer from duration, or claim that an unmarked recording is human-made.
pub fn marked(p: &Preparation, category: Category) -> bool {
    if let Some(value) = p
        .extensions
        .get(FIELD)
        .and_then(|v| v.get(category.id()))
        .and_then(|v| v.as_bool())
    {
        return value;
    }
    if let Some(value) = embedded_choice(p, category) {
        return value;
    }
    crate::genres::values(p).iter().any(|genre| {
        let normalized: String = genre
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect();
        match category {
            Category::AiGenerated => matches!(normalized.as_str(), "aigenerated" | "aimusic"),
            Category::Acappella => matches!(normalized.as_str(), "acappella" | "acapella"),
            Category::Instrumental => normalized == "instrumental",
            Category::Clip => normalized == "clip",
        }
    })
}
fn embedded_choice(p: &Preparation, category: Category) -> Option<bool> {
    let values = p
        .extensions
        .get("embedded_tags")?
        .get("AMPVerseCategories")?
        .as_array()?;
    if values.len() != 1 {
        return None;
    }
    let text = values[0].as_str().filter(|text| text.len() <= 4096)?;
    let map = serde_json::from_str::<std::collections::BTreeMap<String, bool>>(text).ok()?;
    if map.len() > 32 || map.keys().any(|key| key.len() > 64) {
        return None;
    }
    map.get(category.id()).copied()
}
pub fn set(p: &mut Preparation, category: Category, marked: bool) {
    let value = p
        .extensions
        .entry(FIELD.into())
        .or_insert_with(|| serde_json::json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert(category.id().into(), serde_json::json!(marked));
    }
}
pub fn labels(p: &Preparation) -> Vec<String> {
    Category::ALL
        .iter()
        .filter(|&&c| marked(p, c))
        .map(|c| c.label().into())
        .collect()
}
/// These items still receive local audio analysis, but should not be matched
/// automatically against recordings in a web catalog. Manual search is allowed.
pub fn automatic_lookup_allowed(p: &Preparation) -> bool {
    !marked(p, Category::AiGenerated) && !marked(p, Category::Clip)
}
/// A derivative has its own timeline: never copy source cues, analysis or hashes.
pub fn for_clip(source: &Preparation) -> Preparation {
    let mut result = Preparation::default();
    set(&mut result, Category::Clip, true);
    if marked(source, Category::AiGenerated) {
        set(&mut result, Category::AiGenerated, true);
    }
    result
}
pub fn validate(p: &Preparation) -> Result<(), String> {
    if let Some(value) = p.extensions.get(FIELD) {
        let object = value
            .as_object()
            .ok_or("Track categories must be an object")?;
        if object.len() > 32 || object.iter().any(|(k, v)| k.len() > 64 || !v.is_boolean()) {
            return Err("Track categories must contain bounded yes/no choices".into());
        }
    }
    if p.extensions
        .get("track_notes")
        .is_some_and(|v| v.as_str().is_none_or(|s| s.len() > 4096))
    {
        return Err("Track notes must be text up to 4096 bytes".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_choices_override_tags_and_multiple_categories_survive_roundtrip() {
        let mut p = Preparation::default();
        p.extensions
            .insert("genre".into(), serde_json::json!("Instrumental"));
        assert!(marked(&p, Category::Instrumental));
        set(&mut p, Category::Instrumental, false);
        set(&mut p, Category::AiGenerated, true);
        set(&mut p, Category::Clip, true);
        p.extensions
            .insert("track_notes".into(), serde_json::json!("Event intro"));
        let saved: Preparation = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert!(!marked(&saved, Category::Instrumental));
        assert_eq!(labels(&saved), ["AI-generated", "Clip"]);
        assert!(!automatic_lookup_allowed(&saved));
        saved.validate().unwrap();
    }
    #[test]
    fn unknown_does_not_mean_human_and_instrumentals_can_still_be_looked_up() {
        let mut p = Preparation::default();
        assert!(labels(&p).is_empty());
        assert!(automatic_lookup_allowed(&p));
        set(&mut p, Category::Instrumental, true);
        assert!(automatic_lookup_allowed(&p));
        set(&mut p, Category::Acappella, true);
        assert_eq!(labels(&p), ["A cappella", "Instrumental"]);
    }
    #[test]
    fn malformed_annotations_are_rejected_without_touching_other_metadata() {
        let mut p = Preparation::default();
        p.extensions
            .insert(FIELD.into(), serde_json::json!(["clip"]));
        assert!(validate(&p).is_err());
        p.extensions
            .insert(FIELD.into(), serde_json::json!({"clip":"yes"}));
        assert!(validate(&p).is_err());
        p.extensions
            .insert(FIELD.into(), serde_json::json!({"future_note":true}));
        set(&mut p, Category::Clip, true);
        assert_eq!(p.extensions[FIELD]["future_note"], true);
        assert!(validate(&p).is_ok());
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    use crate::smart_filters::{Field, Filter};
    #[test]
    fn shared_smart_filters_select_marked_categories_and_can_exclude_them() {
        let mut track = crate::Track {
            path: "AI generated is just a title.mp3".into(),
            title: "Instrumental?".into(),
            preparation: Preparation::default(),
        };
        for (field, category) in [
            (Field::AiGenerated, Category::AiGenerated),
            (Field::Acappella, Category::Acappella),
            (Field::Instrumental, Category::Instrumental),
            (Field::Clip, Category::Clip),
        ] {
            let yes = Filter::Flag { field, value: true };
            let no = Filter::Flag {
                field,
                value: false,
            };
            assert!(!yes.matches(&track));
            assert!(no.matches(&track));
            set(&mut track.preparation, category, true);
            assert!(yes.matches(&track));
            assert!(!no.matches(&track));
            let portable: Filter =
                serde_json::from_slice(&serde_json::to_vec(&yes).unwrap()).unwrap();
            assert!(portable.matches(&track));
        }
    }
}

#[cfg(test)]
mod compatibility_tests {
    use crate::{
        show::{Settings, SmartPlaylistExport},
        smart_filters::{Field, Filter},
    };
    #[test]
    fn category_rules_are_version_fenced_for_settings_exports_and_sync() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = Settings::default();
        settings.playlists[0].filters.push(Filter::Flag {
            field: Field::Clip,
            value: true,
        });
        assert_eq!(settings.required_version(), 4);
        assert!(settings.validate().is_err());
        settings.version = 4;
        settings.validate().unwrap();
        use crate::playlist_sync_contract::{Action, Kind, Operation};
        let mut operation = Operation {
            protocol: 1,
            operation_id: "op-category".into(),
            device_id: "phone".into(),
            server_id: "server".into(),
            library_id: "library".into(),
            document_id: "smart_demo".into(),
            kind: Kind::Smart,
            document_version: 4,
            action: Action::Create,
            base_revision: None,
            document: Some(serde_json::to_value(&settings.playlists[0]).unwrap()),
        };
        operation.validate("server", "library").unwrap();
        operation.document_version = 3;
        assert!(operation.validate("server", "library").is_err());
        let bytes = SmartPlaylistExport::encode(&settings.playlists[0], root.path()).unwrap();
        let mut doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(doc["version"], 5);
        SmartPlaylistExport::decode(&bytes, root.path()).unwrap();
        doc["version"] = 4.into();
        assert!(
            SmartPlaylistExport::decode(&serde_json::to_vec(&doc).unwrap(), root.path()).is_err()
        );
    }
}

#[cfg(test)]
mod shared_cases {
    #[test]
    fn rust_category_matching_agrees_with_shared_client_vectors() {
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/track_categories.json")).unwrap();
        for case in value["cases"].as_array().unwrap() {
            let prep: crate::Preparation =
                serde_json::from_value(case["preparation"].clone()).unwrap();
            let actual: Vec<_> = super::Category::ALL
                .iter()
                .map(|&c| super::marked(&prep, c))
                .collect();
            assert_eq!(
                serde_json::json!(actual),
                case["expected"],
                "{:?}",
                case["preparation"]
            );
        }
    }
}
