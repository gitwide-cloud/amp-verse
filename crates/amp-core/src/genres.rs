//! Genre tags are read without decoding audio or changing the source file.
use crate::Preparation;
use lofty::{
    file::{AudioFile, TaggedFileExt},
    tag::ItemKey,
};

pub fn canonical(value: &str) -> String {
    let value = value.trim();
    match value.to_lowercase().as_str() {
        "r&b" | "rnb" | "r & b" | "rhythm and blues" | "rhythm & blues" => "R&B".into(),
        "rock" => "Rock".into(),
        _ => value.to_lowercase(),
    }
}

pub fn values(preparation: &Preparation) -> Vec<String> {
    let value = preparation
        .extensions
        .get("genre")
        .or_else(|| preparation.extensions.get("embedded_genres"));
    let mut genres = Vec::new();
    let mut add = |text: &str| {
        for part in text.split([';', '/', '\0']) {
            let genre = canonical(part);
            if !genre.is_empty() && !genres.contains(&genre) {
                genres.push(genre);
            }
        }
    };
    match value {
        Some(serde_json::Value::String(text)) => add(text),
        Some(serde_json::Value::Array(items)) => {
            for item in items {
                if let Some(text) = item.as_str() {
                    add(text);
                }
            }
        }
        _ => {}
    }
    genres
}

pub fn matches(selected: &[String], preparation: &Preparation) -> bool {
    let tags = values(preparation);
    selected
        .iter()
        .any(|genre| tags.contains(&canonical(genre)))
}

pub(crate) fn read_embedded(path: &std::path::Path, preparation: &mut Preparation) {
    // Derived tags never replace a user-saved genre. Refresh them on each background
    // scan so edits made in another tag editor become visible without reanalysis.
    preparation.extensions.remove("embedded_genres");
    let Ok(probe) = lofty::probe::Probe::open(path) else {
        return;
    };
    let options = lofty::config::ParseOptions::new()
        .read_properties(true)
        .read_cover_art(false);
    let Ok(file) = probe.options(options).read() else {
        return;
    };
    let mut tags = std::collections::BTreeMap::<String, Vec<String>>::new();
    for tag in file.tags() {
        for item in tag.items() {
            let value = match item.value() {
                lofty::tag::ItemValue::Text(text) | lofty::tag::ItemValue::Locator(text) => {
                    text.clone()
                }
                lofty::tag::ItemValue::Binary(bytes) => {
                    format!("[{} bytes of binary metadata]", bytes.len())
                }
            };
            tags.entry(format!("{:?}", item.key()))
                .or_default()
                .push(value);
        }
    }
    preparation
        .extensions
        .insert("embedded_tags".into(), serde_json::json!(tags));
    let properties = file.properties();
    preparation.extensions.insert(
        "audio_properties".into(),
        serde_json::json!({
            "duration": properties.duration().as_secs_f64(), "bitrate": properties.audio_bitrate(),
            "sample_rate": properties.sample_rate(), "channels": properties.channels(),
        }),
    );
    let genres: Vec<_> = file
        .tags()
        .iter()
        .flat_map(|tag| tag.get_strings(ItemKey::Genre))
        .map(str::to_owned)
        .collect();
    if !genres.is_empty() {
        preparation
            .extensions
            .insert("embedded_genres".into(), serde_json::json!(genres));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scanner_reads_embedded_tags_without_analysis_or_audio_changes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("tagged.wav");
        let mut body = b"WAVEfmt ".to_vec();
        body.extend(16u32.to_le_bytes());
        body.extend(1u16.to_le_bytes()); // PCM, mono, 8000 Hz
        body.extend(1u16.to_le_bytes());
        body.extend(8000u32.to_le_bytes());
        body.extend(16000u32.to_le_bytes());
        body.extend(2u16.to_le_bytes());
        body.extend(16u16.to_le_bytes());
        body.extend(b"LIST");
        body.extend(18u32.to_le_bytes());
        body.extend(b"INFOIGNR");
        body.extend(5u32.to_le_bytes());
        body.extend(b"Rock\0\0");
        body.extend(b"data");
        body.extend(16u32.to_le_bytes());
        body.extend([0; 16]);
        let mut wav = b"RIFF".to_vec();
        wav.extend((body.len() as u32).to_le_bytes());
        wav.extend(body);
        std::fs::write(&path, &wav).unwrap();
        let library = crate::scan(root.path()).unwrap();
        assert_eq!(values(&library.tracks[0].preparation), vec!["Rock"]);
        let prep = &library.tracks[0].preparation;
        assert_eq!(prep.extensions["audio_properties"]["sample_rate"], 8000);
        assert_eq!(prep.extensions["audio_properties"]["channels"], 1);
        assert_eq!(prep.extensions["embedded_tags"]["Genre"][0], "Rock");
        let mut manual = Preparation::default();
        manual
            .extensions
            .insert("genre".into(), serde_json::json!("Jazz"));
        read_embedded(&path, &mut manual);
        assert_eq!(values(&manual), vec!["jazz"]);
        assert!(manual.extensions.contains_key("audio_properties"));

        assert_eq!(std::fs::read(&path).unwrap(), wav);
        let mut settings = crate::show::Settings::default();
        settings.playlists[0].genres = vec!["Rock".into()];
        settings.save(root.path()).unwrap();
        let loaded = crate::show::Settings::load(root.path()).unwrap();
        assert!(crate::show::eligible(
            &loaded.playlists[0],
            &library.tracks[0]
        ));
        let mut legacy = serde_json::to_value(&loaded).unwrap();
        legacy["playlists"][0]
            .as_object_mut()
            .unwrap()
            .remove("genres");
        let old: crate::show::Settings = serde_json::from_value(legacy).unwrap();
        assert!(old.playlists[0].genres.is_empty());
    }
    #[test]
    fn aliases_multivalue_and_manual_precedence() {
        let mut p = Preparation::default();
        p.extensions.insert(
            "embedded_genres".into(),
            serde_json::json!(["ROCK; Rhythm & Blues", "Soul"]),
        );
        assert!(matches(&["rnb".into()], &p));
        assert!(matches(&["rock".into()], &p));
        assert!(!matches(&["pop".into()], &p));
        p.extensions
            .insert("genre".into(), serde_json::json!("Jazz"));
        assert!(!matches(&["Rock".into()], &p));
        assert!(matches(&["JAZZ".into()], &p));
    }
    #[test]
    fn filter_survives_serialization_and_never_relaxes_with_recent_fallback() {
        let mut settings = crate::show::Settings::default();
        settings.playlists[0].genres = vec!["Rock".into(), "R&B".into()];
        let saved = serde_json::to_vec(&settings).unwrap();
        let loaded: crate::show::Settings = serde_json::from_slice(&saved).unwrap();
        let mut tracks = Vec::new();
        for (name, genre) in [
            ("rock", "Rock"),
            ("rnb", "R&B"),
            ("pop", "Pop"),
            ("unknown", ""),
        ] {
            let mut preparation = Preparation::default();
            preparation
                .extensions
                .insert("genre".into(), serde_json::json!(genre));
            tracks.push(crate::Track {
                path: format!("{name}.mp3").into(),
                title: name.into(),
                preparation,
            });
        }
        let p = &loaded.playlists[0];
        let recent: Vec<_> = tracks.iter().map(crate::show::song_id).collect();
        let ranked = crate::show::ranked_with_snapshot(p, &tracks, None, &recent, None, None);
        assert_eq!(ranked, vec![0, 1]);
        let mut p = p.clone();
        p.excluded.push(crate::show::song_id(&tracks[0]));
        assert!(!crate::show::eligible(&p, &tracks[0]));
        p.folders = vec!["other".into()];
        assert!(!crate::show::eligible(&p, &tracks[1]));
        p.folders.clear();
        p.genres.clear();
        assert!(crate::show::eligible(&p, &tracks[3]));
    }
}
