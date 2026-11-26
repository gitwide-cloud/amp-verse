//! Conservative content categories. Never infer speech from duration or move files.
use std::path::Path;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Music,
    Audiobook,
    Speech,
    Podcast,
}
impl Kind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Music => "music",
            Self::Audiobook => "audiobook",
            Self::Speech => "speech",
            Self::Podcast => "podcast",
        }
    }
}
fn named(value: &str) -> Option<Kind> {
    let text: String = value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    match text.as_str() {
        "music" => Some(Kind::Music),
        "audiobook" | "audiobooks" => Some(Kind::Audiobook),
        "speech" | "speeches" | "spokenword" | "lecture" | "lectures" => Some(Kind::Speech),
        "podcast" | "podcasts" => Some(Kind::Podcast),
        _ => None,
    }
}
pub fn kind(path: &Path, p: &crate::Preparation) -> Kind {
    if let Some(kind) = p
        .extensions
        .get("media_kind")
        .and_then(|v| v.as_str())
        .and_then(named)
    {
        return kind;
    }
    for genre in crate::genres::values(p) {
        if let Some(k) = named(&genre)
            && k != Kind::Music
        {
            return k;
        }
    }
    for part in path.parent().unwrap_or(Path::new("")).components() {
        let folder = part.as_os_str().to_string_lossy().to_lowercase();
        if matches!(
            folder.as_str(),
            "audiobooks" | "audio books" | "spoken word" | "speeches" | "lectures" | "podcasts"
        ) && let Some(k) = named(&folder)
        {
            return k;
        }
    }
    p.extensions
        .get("media_kind_detected")
        .and_then(|v| v.as_str())
        .and_then(named)
        .unwrap_or(Kind::Music)
}
/// Inspect tags once for a new/changed unknown item; never decode its audio.
pub fn inspect_audio(path: &Path, p: &mut crate::Preparation) -> Kind {
    if !p.extensions.contains_key("media_kind") && kind(path, p) == Kind::Music {
        crate::genres::read_embedded(path, p);
    }
    kind(path, p)
}
pub fn is_music(path: &Path, p: &crate::Preparation) -> bool {
    kind(path, p) == Kind::Music
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn broad_smart_folders_exclude_speech_but_explicit_song_selection_can_include_it() {
        let mut settings = crate::show::Settings::default();
        let mut playlist = settings.playlists.remove(0);
        playlist.folders = vec!["mixed".into()];
        let mut preparation = crate::Preparation::default();
        preparation
            .extensions
            .insert("media_kind".into(), serde_json::json!("speech"));
        let track = crate::Track {
            path: "mixed/Talk.mp3".into(),
            title: "Talk".into(),
            preparation,
        };
        assert!(!crate::show::eligible(&playlist, &track));
        playlist.allowed.push(crate::show::song_id(&track));
        assert!(crate::show::eligible(&playlist, &track));
    }
    #[test]
    fn explicit_categories_override_folders_and_duration_is_not_a_classifier() {
        let mut p = crate::Preparation {
            cue_out: Some(7200.),
            ..Default::default()
        };
        assert_eq!(kind(Path::new("Long concert.mp3"), &p), Kind::Music);
        assert_eq!(
            kind(Path::new("Audiobooks/Chapter 1.mp3"), &p),
            Kind::Audiobook
        );
        p.extensions
            .insert("media_kind".into(), serde_json::json!("music"));
        assert_eq!(kind(Path::new("Speeches/Track.mp3"), &p), Kind::Music);
        p.extensions.remove("media_kind");
        p.extensions
            .insert("genre".into(), serde_json::json!("Spoken Word"));
        assert_eq!(kind(Path::new("Mixed files/Track.mp3"), &p), Kind::Speech);
    }
}
