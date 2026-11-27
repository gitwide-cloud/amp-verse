//! Conservative, portable music-content labels; never calendar/date or artist inference.
use super::Field;
use crate::Preparation;

const HOLIDAYS: &[(&str, &[&str])] = &[
    ("Christmas", &["christmas", "xmas", "noel", "navidad"]),
    ("Halloween", &["halloween"]),
    ("Thanksgiving", &["thanksgiving"]),
    ("New Year", &["new year", "new years"]),
    ("Valentine’s Day", &["valentine", "valentines"]),
    ("Easter", &["easter"]),
    ("Hanukkah", &["hanukkah", "chanukah", "hanukah"]),
    ("Kwanzaa", &["kwanzaa"]),
    ("Diwali", &["diwali", "deepavali"]),
    ("Ramadan", &["ramadan", "ramazan"]),
    ("Eid", &["eid"]),
    (
        "St Patrick’s Day",
        &["st patricks day", "saint patricks day"],
    ),
    (
        "Independence Day",
        &["independence day", "fourth of july", "4th of july"],
    ),
];
const HOLIDAY_NAMES: &[&str] = &[
    "Christmas",
    "Halloween",
    "Thanksgiving",
    "New Year",
    "Valentine’s Day",
    "Easter",
    "Hanukkah",
    "Kwanzaa",
    "Diwali",
    "Ramadan",
    "Eid",
    "St Patrick’s Day",
    "Independence Day",
];
const SEASON_NAMES: &[&str] = &["Spring", "Summer", "Autumn", "Winter"];
pub fn presets(field: Field) -> &'static [&'static str] {
    match field {
        Field::Holiday => HOLIDAY_NAMES,
        Field::Season => SEASON_NAMES,
        _ => &[],
    }
}
fn normalized(s: &str) -> String {
    s.to_lowercase()
        .replace(['\'', '’'], "")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
fn phrase(value: &str, term: &str) -> bool {
    value.match_indices(term).any(|(start, _)| {
        (start == 0 || value.as_bytes()[start - 1] == b' ')
            && (start + term.len() == value.len() || value.as_bytes()[start + term.len()] == b' ')
    })
}
fn add(out: &mut Vec<String>, value: &str) {
    if !out.iter().any(|s| s == value) {
        out.push(value.to_owned());
    }
}
fn season(value: &str) -> Option<&'static str> {
    match value {
        "spring" => Some("Spring"),
        "summer" => Some("Summer"),
        "autumn" | "fall" => Some("Autumn"),
        "winter" => Some("Winter"),
        _ => None,
    }
}
pub(super) fn values(field: Field, p: &Preparation) -> Vec<String> {
    let key = if field == Field::Holiday {
        "holiday"
    } else {
        "season"
    };
    let tag_key = if field == Field::Holiday {
        "Holiday"
    } else {
        "Season"
    };
    let mut explicit = super::strings(p.extensions.get(key));
    explicit.extend(super::strings(
        p.extensions
            .get("embedded_tags")
            .and_then(|t| t.get(tag_key)),
    ));
    let mut out = Vec::new();
    for raw in explicit {
        for value in raw.split([';', '/', '\0']) {
            let value = normalized(value);
            if value.is_empty() {
                continue;
            }
            if field == Field::Season {
                if let Some(name) = season(&value) {
                    add(&mut out, name);
                }
            } else {
                for (name, aliases) in HOLIDAYS {
                    if aliases.iter().any(|alias| phrase(&value, alias)) {
                        add(&mut out, name);
                    }
                }
            }
        }
    }
    if !out.is_empty() {
        return out;
    }
    // Genre and album labels are useful for existing libraries without new tags.
    let mut labels = crate::genres::values(p);
    labels.extend(super::texts(Field::Album, p));
    if field == Field::Holiday {
        labels.extend(super::strings(
            p.extensions
                .get("title")
                .or_else(|| p.extensions.get("embedded_tags")?.get("TrackTitle")),
        ));
    }
    for raw in labels {
        let value = normalized(&raw);
        if field == Field::Holiday {
            for (name, aliases) in HOLIDAYS {
                if aliases.iter().any(|alias| phrase(&value, alias)) {
                    add(&mut out, name);
                }
            }
        } else {
            for name in SEASON_NAMES {
                let term = name.to_lowercase();
                if value == term
                    || [
                        "music",
                        "hits",
                        "songs",
                        "collection",
                        "playlist",
                        "anthems",
                        "vibes",
                    ]
                    .iter()
                    .any(|marker| phrase(&value, &term) && phrase(&value, marker))
                {
                    add(&mut out, name);
                }
            }
            if value == "fall" || phrase(&value, "fall music") || phrase(&value, "fall hits") {
                add(&mut out, "Autumn");
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Track, smart_filters::Filter};
    fn track(data: serde_json::Value) -> Track {
        let mut p = Preparation::default();
        p.extensions = serde_json::from_value(data).unwrap();
        Track {
            path: "song.mp3".into(),
            title: "song".into(),
            preparation: p,
        }
    }
    #[test]
    fn holiday_and_season_rules_use_labels_without_calendar_or_artist_guessing() {
        let t = track(
            serde_json::json!({"embedded_genres":["Christmas"],"embedded_tags":{"AlbumTitle":["Summer Hits"],"TrackArtist":["Winter"]}}),
        );
        assert_eq!(values(Field::Holiday, &t.preparation), vec!["Christmas"]);
        assert_eq!(values(Field::Season, &t.preparation), vec!["Summer"]);
        assert!(
            values(
                Field::Holiday,
                &track(serde_json::json!({"title":"Christmasland","artist":"Christmas"}))
                    .preparation
            )
            .is_empty()
        );
        assert!(
            values(
                Field::Season,
                &track(serde_json::json!({"title":"Winter Wonderland","album":"Fall Out Boy"}))
                    .preparation
            )
            .is_empty()
        );
        assert!(
            values(
                Field::Season,
                &track(serde_json::json!({"genre":"Christmas"})).preparation
            )
            .is_empty()
        );
    }
    #[test]
    fn aliases_multiple_choices_and_round_trip_preserve_eligibility() {
        let t = track(
            serde_json::json!({"holiday":["Xmas","New Year's Eve"],"season":"Fall","album":"Summer Hits"}),
        );
        assert_eq!(
            values(Field::Holiday, &t.preparation),
            vec!["Christmas", "New Year"]
        );
        assert_eq!(values(Field::Season, &t.preparation), vec!["Autumn"]);
        let mut settings = crate::show::Settings::default();
        settings.playlists[0].filters = vec![
            Filter::Text {
                field: Field::Holiday,
                terms: "Halloween; Christmas".into(),
                exact: true,
            },
            Filter::Text {
                field: Field::Season,
                terms: "Autumn".into(),
                exact: true,
            },
        ];
        let dir = tempfile::tempdir().unwrap();
        settings.save(dir.path()).unwrap();
        let loaded = crate::show::Settings::load(dir.path()).unwrap();
        assert!(crate::show::eligible(&loaded.playlists[0], &t));
        let bytes =
            crate::show::SmartPlaylistExport::encode(&loaded.playlists[0], dir.path()).unwrap();
        let synced = crate::show::SmartPlaylistExport::decode(&bytes, dir.path()).unwrap();
        assert!(crate::show::eligible(&synced, &t));
        assert!(!crate::show::eligible(
            &synced,
            &track(serde_json::json!({}))
        ));
    }
    #[test]
    fn custom_file_tags_are_imported_without_writing_audio() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("music.flac");
        let mut bytes = b"fLaC".to_vec();
        bytes.extend([0, 0, 0, 34]);
        let mut info = [0u8; 34];
        info[0..2].copy_from_slice(&4096u16.to_be_bytes());
        info[2..4].copy_from_slice(&4096u16.to_be_bytes());
        info[10..18].copy_from_slice(&((44100u64 << 44) | (15u64 << 36) | 44100).to_be_bytes());
        bytes.extend(info);
        let mut comments = Vec::new();
        comments.extend(0u32.to_le_bytes());
        comments.extend(2u32.to_le_bytes());
        for tag in ["HOLIDAY=Xmas", "SEASON=Fall"] {
            comments.extend((tag.len() as u32).to_le_bytes());
            comments.extend(tag.as_bytes());
        }
        bytes.push(0x84);
        bytes.extend((comments.len() as u32).to_be_bytes()[1..].iter());
        bytes.extend(comments);
        std::fs::write(&path, &bytes).unwrap();
        let mut p = Preparation::default();
        crate::genres::read_embedded(&path, &mut p);
        assert_eq!(values(Field::Holiday, &p), vec!["Christmas"]);
        assert_eq!(values(Field::Season, &p), vec!["Autumn"]);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}
