//! Portable AND filters over actual preparation and embedded metadata. Missing required values never match.
use crate::{Preparation, Track};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Field {
    Bpm,
    TempoAlternative,
    Key,
    Camelot,
    Energy,
    Duration,
    TempoConfidence,
    KeyConfidence,
    Analyzed,
    Reviewed,
    BeatGrid,
    ManualEdits,
    CueCount,
    ManualCueCount,
    CueKind,
    CueLabel,
    SectionCount,
    SectionName,
    SectionEnergy,
    EnergyProfile,
    Engine,
    BeatOffset,
    CueIn,
    CueOut,
    StemCount,
    Artist,
    Album,
    Year,
    Rating,
    Bitrate,
    SampleRate,
    Channels,
}
#[derive(Clone, Copy)]
pub enum Kind {
    Number(f64, f64),
    Text,
    Flag,
}
impl Field {
    pub const ALL: &'static [Self] = &[
        Self::Bpm,
        Self::TempoAlternative,
        Self::Key,
        Self::Camelot,
        Self::Energy,
        Self::Duration,
        Self::TempoConfidence,
        Self::KeyConfidence,
        Self::Analyzed,
        Self::Reviewed,
        Self::BeatGrid,
        Self::ManualEdits,
        Self::CueCount,
        Self::ManualCueCount,
        Self::CueKind,
        Self::CueLabel,
        Self::SectionCount,
        Self::SectionName,
        Self::SectionEnergy,
        Self::EnergyProfile,
        Self::Engine,
        Self::BeatOffset,
        Self::CueIn,
        Self::CueOut,
        Self::StemCount,
        Self::Artist,
        Self::Album,
        Self::Year,
        Self::Rating,
        Self::Bitrate,
        Self::SampleRate,
        Self::Channels,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Bpm => "BPM",
            Self::TempoAlternative => "Alternative BPM",
            Self::Key => "Musical key",
            Self::Camelot => "Camelot key",
            Self::Energy => "Energy (1–10)",
            Self::Duration => "Duration (seconds)",
            Self::TempoConfidence => "BPM confidence (%)",
            Self::KeyConfidence => "Key confidence (%)",
            Self::Analyzed => "Has analysis",
            Self::Reviewed => "Analysis reviewed",
            Self::BeatGrid => "Has beatgrid",
            Self::ManualEdits => "Has manual edits",
            Self::CueCount => "Cue count",
            Self::ManualCueCount => "Manual cue count",
            Self::CueKind => "Cue type",
            Self::CueLabel => "Cue label",
            Self::SectionCount => "Section count",
            Self::SectionName => "Section label",
            Self::SectionEnergy => "Any section energy (1–10)",
            Self::EnergyProfile => "Has energy profile",
            Self::Engine => "Analysis engine",
            Self::BeatOffset => "Beat offset (seconds)",
            Self::CueIn => "Cue-in (seconds)",
            Self::CueOut => "Cue-out (seconds)",
            Self::StemCount => "Recorded stem count",
            Self::Artist => "Artist",
            Self::Album => "Album",
            Self::Year => "Year tag",
            Self::Rating => "Rating",
            Self::Bitrate => "Bitrate (kbps)",
            Self::SampleRate => "Sample rate (Hz)",
            Self::Channels => "Audio channels",
        }
    }
    pub fn kind(self) -> Kind {
        match self {
            Self::Key
            | Self::Camelot
            | Self::CueKind
            | Self::CueLabel
            | Self::SectionName
            | Self::Engine
            | Self::Artist
            | Self::Album
            | Self::Year => Kind::Text,
            Self::Analyzed
            | Self::Reviewed
            | Self::BeatGrid
            | Self::ManualEdits
            | Self::EnergyProfile => Kind::Flag,
            Self::Bpm | Self::TempoAlternative => Kind::Number(0., 400.),
            Self::Energy | Self::SectionEnergy => Kind::Number(1., 10.),
            Self::TempoConfidence | Self::KeyConfidence => Kind::Number(0., 100.),
            Self::Rating => Kind::Number(0., 5.),
            Self::CueCount | Self::ManualCueCount => Kind::Number(0., 256.),
            Self::SectionCount => Kind::Number(0., 4096.),
            Self::StemCount | Self::Channels => Kind::Number(0., 32.),
            Self::Bitrate => Kind::Number(0., 10000.),
            Self::SampleRate => Kind::Number(0., 384000.),
            _ => Kind::Number(0., 21600.),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Filter {
    Number {
        field: Field,
        min: f64,
        max: f64,
    },
    Text {
        field: Field,
        terms: String,
        exact: bool,
    },
    Flag {
        field: Field,
        value: bool,
    },
}
impl Filter {
    pub fn new(field: Field) -> Self {
        match field.kind() {
            Kind::Number(min, max) => Self::Number { field, min, max },
            Kind::Text => Self::Text {
                field,
                terms: String::new(),
                exact: true,
            },
            Kind::Flag => Self::Flag { field, value: true },
        }
    }
    pub fn field(&self) -> Field {
        match self {
            Self::Number { field, .. } | Self::Text { field, .. } | Self::Flag { field, .. } => {
                *field
            }
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        let valid = match self {
            Self::Number { field, min, max } => {
                matches!(field.kind(),Kind::Number(lo,hi) if min.is_finite()&&max.is_finite()&&*min>=lo&&*max<=hi&&min<=max)
            }
            Self::Text { field, terms, .. } => {
                matches!(field.kind(), Kind::Text) && terms.len() <= 4096
            }
            Self::Flag { field, .. } => matches!(field.kind(), Kind::Flag),
        };
        if valid {
            Ok(())
        } else {
            Err(format!("Invalid filter: {}", self.field().label()))
        }
    }
    pub fn matches(&self, t: &Track) -> bool {
        if self.validate().is_err() {
            return false;
        }
        let p = &t.preparation;
        match self {
            Self::Number { field, min, max } => numbers(*field, p)
                .iter()
                .any(|v| v.is_finite() && v >= min && v <= max),
            Self::Flag { field, value } => flag(*field, p) == *value,
            Self::Text {
                field,
                terms,
                exact,
            } => {
                let values = texts(*field, p);
                let terms: Vec<_> = terms
                    .split(';')
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
                values.iter().any(|v| {
                    !v.trim().is_empty()
                        && (terms.is_empty()
                            || terms.iter().any(|term| {
                                if *exact {
                                    v.trim().to_lowercase() == *term
                                } else {
                                    v.to_lowercase().contains(term)
                                }
                            }))
                })
            }
        }
    }
}
fn ext_num(p: &Preparation, key: &str) -> Option<f64> {
    p.extensions.get(key)?.as_f64()
}
fn sections(p: &Preparation) -> Option<&Vec<serde_json::Value>> {
    p.extensions
        .get("energy_profile")?
        .get("sections")?
        .as_array()
}
fn numbers(field: Field, p: &Preparation) -> Vec<f64> {
    let analysis = p.analysis.as_ref();
    let properties = |key| p.extensions.get("audio_properties")?.get(key)?.as_f64();
    if field == Field::TempoAlternative {
        return p
            .extensions
            .get("tempo_alternatives")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
            .unwrap_or_default();
    }
    if field == Field::SectionEnergy {
        return sections(p)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.get("energy")?.as_f64().map(|e| 1. + e * 9.))
                    .collect()
            })
            .unwrap_or_default();
    }
    let value = match field {
        Field::Bpm => p.bpm,
        Field::Energy => crate::show::energy(p),
        Field::Duration => analysis
            .map(|a| a.duration)
            .or_else(|| properties("duration"))
            .or(p.cue_out),
        Field::TempoConfidence => analysis.map(|a| a.tempo_confidence * 100.),
        Field::KeyConfidence => analysis.map(|a| a.key_confidence * 100.),
        Field::CueCount => Some((p.cues.len() + p.manual_cues.len()) as f64),
        Field::ManualCueCount => Some(p.manual_cues.len() as f64),
        Field::SectionCount => sections(p).map(|a| a.len() as f64),
        Field::BeatOffset => p.beat_offset,
        Field::CueIn => Some(p.cue_in),
        Field::CueOut => p.cue_out,
        Field::StemCount => Some(p.stems.len() as f64),
        Field::Rating => ext_num(p, "rating"),
        Field::Bitrate => properties("bitrate"),
        Field::SampleRate => properties("sample_rate"),
        Field::Channels => properties("channels"),
        _ => None,
    };
    value.into_iter().collect()
}
fn flag(field: Field, p: &Preparation) -> bool {
    match field {
        Field::Analyzed => p.analysis.is_some(),
        Field::Reviewed => p.analysis.as_ref().is_some_and(|a| a.reviewed),
        Field::BeatGrid => p.bpm.is_some() && p.beat_offset.is_some(),
        Field::ManualEdits => p.manual_override || !p.manual_cues.is_empty(),
        Field::EnergyProfile => p.extensions.contains_key("energy_profile"),
        _ => false,
    }
}
fn strings(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Number(n)) => vec![n.to_string()],
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => vec![],
    }
}
pub fn texts(field: Field, p: &Preparation) -> Vec<String> {
    match field {
        Field::Key => p.key.iter().cloned().collect(),
        Field::Camelot => p.harmonic_key.iter().cloned().collect(),
        Field::Engine => p.analysis.iter().map(|a| a.engine.clone()).collect(),
        Field::CueKind => p
            .cues
            .iter()
            .chain(&p.manual_cues)
            .map(|c| c.kind.clone())
            .collect(),
        Field::CueLabel => p
            .cues
            .iter()
            .chain(&p.manual_cues)
            .map(|c| c.label.clone())
            .collect(),
        Field::SectionName => sections(p)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.get("name")?.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        Field::Artist | Field::Album | Field::Year => {
            let (manual, tag) = match field {
                Field::Artist => ("artist", "TrackArtist"),
                Field::Album => ("album", "AlbumTitle"),
                _ => ("year", "RecordingDate"),
            };
            strings(
                p.extensions
                    .get(manual)
                    .or_else(|| p.extensions.get("embedded_tags")?.get(tag)),
            )
        }
        _ => vec![],
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filtered_exports_require_new_reader_and_old_rules_still_load() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = crate::show::Settings::default();
        settings.playlists[0].filters = vec![Filter::new(Field::Analyzed)];
        let bytes =
            crate::show::SmartPlaylistExport::encode(&settings.playlists[0], root.path()).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["version"],
            3
        );
        assert_eq!(
            crate::show::SmartPlaylistExport::decode(&bytes, root.path())
                .unwrap()
                .filters,
            settings.playlists[0].filters
        );
        let mut legacy = serde_json::to_value(crate::show::Settings::default()).unwrap();
        legacy["playlists"][0]
            .as_object_mut()
            .unwrap()
            .remove("filters");
        let old: crate::show::Settings = serde_json::from_value(legacy).unwrap();
        assert!(old.playlists[0].filters.is_empty());
        settings.save(root.path()).unwrap();
        assert_eq!(crate::show::Settings::load(root.path()).unwrap().version, 2);
    }
    #[test]
    fn ranges_text_flags_and_missing_data_are_predictable() {
        let mut t = Track {
            path: "x.mp3".into(),
            title: "x".into(),
            preparation: Preparation {
                bpm: Some(100.),
                harmonic_key: Some("8A".into()),
                ..Default::default()
            },
        };
        t.preparation
            .extensions
            .insert("energy".into(), serde_json::json!(0.7));
        let filters = [
            Filter::Number {
                field: Field::Bpm,
                min: 90.,
                max: 110.,
            },
            Filter::Text {
                field: Field::Camelot,
                terms: "7A;8a".into(),
                exact: true,
            },
            Filter::Number {
                field: Field::Energy,
                min: 6.,
                max: 8.,
            },
        ];
        assert!(filters.iter().all(|f| f.matches(&t)));
        assert!(!Filter::new(Field::TempoConfidence).matches(&t));
        assert!(
            Filter::Flag {
                field: Field::Analyzed,
                value: false
            }
            .matches(&t)
        );
        assert!(
            Filter::Number {
                field: Field::Bpm,
                min: 120.,
                max: 80.
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn filters_survive_playlist_save_and_apply_to_sources_and_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = crate::show::Settings::default();
        settings.playlists[0].filters = vec![Filter::Number {
            field: Field::Bpm,
            min: 90.,
            max: 110.,
        }];
        settings.save(dir.path()).unwrap();
        let loaded = crate::show::Settings::load(dir.path()).unwrap();
        assert_eq!(loaded.playlists[0].filters, settings.playlists[0].filters);
        let tracks = vec![
            Track {
                path: "good.mp3".into(),
                title: "good".into(),
                preparation: Preparation {
                    bpm: Some(100.),
                    ..Default::default()
                },
            },
            Track {
                path: "bad.mp3".into(),
                title: "bad".into(),
                preparation: Preparation {
                    bpm: Some(130.),
                    ..Default::default()
                },
            },
        ];
        let p = &loaded.playlists[0];
        assert!(crate::show::eligible(p, &tracks[0]));
        assert!(!crate::show::eligible(p, &tracks[1]));
        assert_ne!(
            crate::show::choose(
                p,
                &tracks,
                None,
                &[crate::show::song_id(&tracks[0])],
                None,
                0
            ),
            Some(1)
        );
    }
}
