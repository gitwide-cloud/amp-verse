//! Portable show rules and runtime energy plans; no playback or metadata mutation.
use crate::{Preparation, Track};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
pub fn song_id(track: &Track) -> String {
    format!(
        "song_{:x}",
        Sha256::digest(track.path.to_string_lossy().replace('\\', "/").as_bytes())
    )
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    Hype,
    #[default]
    Steady,
    Chill,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Fallback {
    Stop,
    #[default]
    RelaxRecent,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyPlan {
    pub target: f64,
    pub transitions: u32,
    pub hold: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistSource {
    pub name: String,
    pub songs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartPlaylist {
    #[serde(default)]
    pub companion_available: bool,
    pub id: String,
    pub name: String,
    /// Imported sources stay bound to their originating library until explicitly remapped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_library: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_root_hint: Option<PathBuf>,
    // Keep the conservative state in device-local startup caches; shared loads recompute it.
    #[serde(default)]
    pub source_unavailable: bool,
    /// Empty means all genres; selected genres are combined with OR.
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub filters: Vec<crate::smart_filters::Filter>,
    pub style: Style,
    #[serde(default)]
    pub folders: Vec<PathBuf>,
    #[serde(default)]
    pub playlist_sources: Vec<PlaylistSource>,
    #[serde(default)]
    pub restrict_folders: bool,
    #[serde(default)]
    pub allowed: Vec<String>,
    #[serde(default)]
    pub restrict_allowed: bool,
    #[serde(default)]
    pub excluded: Vec<String>,
    #[serde(default)]
    pub preferred: Vec<String>,
    #[serde(default)]
    pub recognition_song: Option<String>,
    #[serde(default)]
    pub target_energy: Option<f64>,
    #[serde(default = "default_recent")]
    pub recent_repeat: usize,
    #[serde(default)]
    pub fallback: Fallback,
    #[serde(default)]
    pub energy_progression: Option<EnergyPlan>,
    #[serde(default = "yes")]
    pub use_manual_cues: bool,
}
fn default_recent() -> usize {
    5
}
fn yes() -> bool {
    true
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default, rename = "_amp_sync", skip_serializing_if = "Option::is_none")]
    pub sync: Option<crate::coordination::Revision>,
    pub version: u32,
    pub playlists: Vec<SmartPlaylist>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            sync: None,
            version: 1,
            playlists: [("demo", "Demo", Style::Steady)]
                .into_iter()
                .map(|(id, name, style)| SmartPlaylist {
                    companion_available: false,
                    id: id.into(),
                    name: name.into(),
                    source_library: None,
                    source_root_hint: None,
                    source_unavailable: false,
                    genres: vec![],
                    filters: vec![],
                    style,
                    folders: vec![],
                    playlist_sources: vec![],
                    restrict_folders: false,
                    allowed: vec![],
                    restrict_allowed: false,
                    excluded: vec![],
                    preferred: vec![],
                    recognition_song: None,
                    target_energy: None,
                    recent_repeat: 5,
                    fallback: Fallback::RelaxRecent,
                    energy_progression: None,
                    use_manual_cues: true,
                })
                .collect(),
        }
    }
}
impl Settings {
    pub fn resolve_sources(&mut self, root: &Path) {
        if self.playlists.iter().all(|p| p.source_library.is_none()) {
            for p in &mut self.playlists {
                p.source_unavailable = false;
            }
            return;
        }
        let identity = crate::coordination::library_id(root).ok();
        for p in &mut self.playlists {
            p.source_unavailable = p
                .source_library
                .as_ref()
                .is_some_and(|id| Some(id) != identity.as_ref());
        }
    }
    fn normalize(&mut self) {
        if (1..=2).contains(&self.version) {
            self.version = if self.playlists.iter().any(|p| !p.filters.is_empty()) {
                2
            } else {
                1
            };
        }
        for p in &mut self.playlists {
            p.genres = p
                .genres
                .iter()
                .map(|g| crate::genres::canonical(g))
                .collect();
            let mut seen_genres = std::collections::HashSet::new();
            p.genres.retain(|g| seen_genres.insert(g.clone()));
            if !p.allowed.is_empty() {
                p.restrict_allowed = true;
            }
            for f in &mut p.folders {
                *f = PathBuf::from(f.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=2).contains(&self.version) || self.playlists.len() > 1000 {
            return Err("Unsupported show settings".into());
        }
        let mut ids = std::collections::HashSet::new();
        for p in &self.playlists {
            if p.id.is_empty()
                || !ids.insert(&p.id)
                || p.name.trim().is_empty()
                || p.target_energy
                    .is_some_and(|e| !e.is_finite() || !(1. ..=10.).contains(&e))
                || p.recent_repeat > 1000
                || p.filters.len() > 64
                || p.filters.iter().any(|f| f.validate().is_err())
                || p.genres.len() > 256
                || p.genres
                    .iter()
                    .any(|g| g.trim().is_empty() || g.len() > 256)
            {
                return Err("Invalid Smart Playlist rule".into());
            }
            if let Some(plan) = &p.energy_progression {
                Progression::new(5., plan.target, plan.transitions, plan.hold)?;
            }
            if p.folders.iter().any(|p| {
                p.is_absolute()
                    || p.components().any(|c| {
                        !matches!(
                            c,
                            std::path::Component::Normal(_) | std::path::Component::CurDir
                        )
                    })
            }) {
                return Err("Source folders must be relative to this library".into());
            }
        }
        Ok(())
    }
    pub fn load(root: &Path) -> Result<Self, String> {
        if let Some(bytes) = crate::catalog_db::read(&root.join("ampdj/smart-playlists.json"))? {
            let mut s: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            s.normalize();
            s.validate()?;
            s.resolve_sources(root);
            return Ok(s);
        }
        match std::fs::read(root.join("ampdj/smart-playlists.json")).or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                std::fs::read(root.join(".ampverse/show.json"))
            } else {
                Err(e)
            }
        }) {
            Ok(b) => {
                let mut value: serde_json::Value =
                    serde_json::from_slice(&b).map_err(|e| e.to_string())?;
                crate::coordination::adopt_legacy(&mut value)?;
                let mut s: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
                s.normalize();
                s.validate()?;
                s.resolve_sources(root);
                Ok(s)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn save(&self, root: &Path) -> Result<(), String> {
        let mut portable = self.clone();
        portable.normalize();
        portable.validate()?;
        let file = root.join("ampdj/smart-playlists.json");
        let proposed = serde_json::to_value(&portable).map_err(|e| e.to_string())?;
        let result = (|| {
            let id = crate::coordination::library_id(root)?;
            crate::coordination::save_document(&file, &proposed, &id)
        })();
        result.map_err(|e| {
            crate::coordination::retain_document(
                &file,
                &proposed,
                portable.sync.as_ref().map_or("", |r| r.library_id.as_str()),
                e,
            )
        })
    }
}
/// Small settings-only exchange file. Audio, analysis and shared revision tokens are excluded.
#[derive(Serialize, Deserialize)]
pub struct SmartPlaylistExport {
    format: String,
    version: u32,
    pub playlist: SmartPlaylist,
}
impl SmartPlaylistExport {
    pub const MAX_BYTES: u64 = 4 * 1024 * 1024;
    pub fn encode(playlist: &SmartPlaylist, root: &Path) -> Result<Vec<u8>, String> {
        let mut playlist = playlist.clone();
        if playlist.source_library.is_none() {
            playlist.source_library = Some(crate::coordination::library_id(root)?);
            playlist.source_root_hint = Some(root.to_path_buf());
        }
        let package = Self {
            format: "ampverse-smart-playlist".into(),
            version: if !playlist.filters.is_empty() {
                3
            } else if playlist.playlist_sources.is_empty() {
                1
            } else {
                2
            },
            playlist,
        };
        package.validate()?;
        let bytes = serde_json::to_vec_pretty(&package).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > Self::MAX_BYTES {
            return Err("Playlist export is too large".into());
        }
        Ok(bytes)
    }
    fn validate(&self) -> Result<(), String> {
        if self.format != "ampverse-smart-playlist" || !(1..=3).contains(&self.version) {
            return Err("Unsupported AMP Smart Playlist file".into());
        }
        let p = &self.playlist;
        // Check both separator spellings, regardless of the importing operating system.
        if p.folders.iter().any(|f| {
            let s = f.to_string_lossy().replace('\\', "/");
            s.starts_with('/') || s.contains(':') || s.split('/').any(|c| c == "..")
        }) {
            return Err("Export contains an unsafe source folder".into());
        }
        if p.source_library.as_ref().is_none_or(|s| s.is_empty()) {
            return Err("Export has no source library identity".into());
        }
        Settings {
            sync: None,
            version: 1,
            playlists: vec![p.clone()],
        }
        .validate()
    }
    pub fn decode(bytes: &[u8], root: &Path) -> Result<SmartPlaylist, String> {
        if bytes.len() as u64 > Self::MAX_BYTES {
            return Err("Playlist file is too large".into());
        }
        let package: Self = serde_json::from_slice(bytes)
            .map_err(|e| format!("Invalid Smart Playlist file: {e}"))?;
        package.validate()?;
        let mut settings = Settings {
            sync: None,
            version: 1,
            playlists: vec![package.playlist],
        };
        settings.normalize();
        settings.resolve_sources(root);
        let mut playlist = settings.playlists.remove(0);
        playlist.id = format!("imported-{}", uuid::Uuid::new_v4());
        Ok(playlist)
    }
}
pub fn energy(p: &Preparation) -> Option<f64> {
    crate::smart::energy(p).map(|e| 1. + e * 9.)
}
pub fn eligible(p: &SmartPlaylist, t: &Track) -> bool {
    if p.source_unavailable {
        return false;
    }
    let id = song_id(t);
    // Speech downloads stay out of broad music mixes. They can still be explicitly
    // selected as songs, a dedicated folder, or an imported playlist source.
    if t.preparation
        .extensions
        .get("media_kind")
        .and_then(|v| v.as_str())
        == Some("podcast")
        && !p.allowed.contains(&id)
        && !p
            .folders
            .iter()
            .any(|f| !f.as_os_str().is_empty() && crate::folder_contains(&t.path, f))
        && !p.playlist_sources.iter().any(|s| s.songs.contains(&id))
    {
        return false;
    }
    // Folder sources include all descendants. Older drafts used restrict_allowed as
    // an empty-source guard; do not require per-song Allow ticks after a folder is chosen.
    let allowed = p.allowed.contains(&id)
        || (p.allowed.is_empty() && (!p.restrict_allowed || !p.folders.is_empty()));
    p.filters.iter().all(|f| f.matches(t))
        && (p.genres.is_empty() || crate::genres::matches(&p.genres, &t.preparation))
        && !p.excluded.contains(&id)
        && ((allowed
            && ((!p.restrict_folders && p.folders.is_empty())
                || p.folders.iter().any(|f| crate::folder_contains(&t.path, f))))
            || p.playlist_sources
                .iter()
                .any(|source| source.songs.contains(&id)))
}
pub fn choose(
    p: &SmartPlaylist,
    tracks: &[Track],
    current: Option<usize>,
    recent: &[String],
    target: Option<f64>,
    alternate: usize,
) -> Option<usize> {
    choose_with_snapshot(p, tracks, current, recent, target, alternate, None)
}
pub fn choose_with_snapshot(
    p: &SmartPlaylist,
    tracks: &[Track],
    current: Option<usize>,
    recent: &[String],
    target: Option<f64>,
    alternate: usize,
    snapshot: Option<&Preparation>,
) -> Option<usize> {
    let ranked = ranked_with_snapshot(p, tracks, current, recent, target, snapshot);
    ranked
        .get(alternate.min(ranked.len().saturating_sub(1)))
        .copied()
}
pub fn ranked_with_snapshot(
    p: &SmartPlaylist,
    tracks: &[Track],
    current: Option<usize>,
    recent: &[String],
    target: Option<f64>,
    snapshot: Option<&Preparation>,
) -> Vec<usize> {
    let playing = snapshot.or_else(|| current.and_then(|i| tracks.get(i)).map(|t| &t.preparation));
    let mut candidates: Vec<_> = tracks
        .iter()
        .enumerate()
        .filter(|(i, t)| Some(*i) != current && eligible(p, t))
        .collect();
    let fresh: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|(_, t)| {
            !recent
                .iter()
                .rev()
                .take(p.recent_repeat)
                .any(|id| *id == song_id(t))
        })
        .collect();
    if !fresh.is_empty() {
        candidates = fresh;
    } else if p.fallback == Fallback::Stop {
        return Vec::new();
    }
    let wanted = target.or(p.target_energy).or(playing.and_then(energy));
    let score = |t: &Track| {
        let mut prepared = t.preparation.clone();
        if let Some(profile) = crate::energy_profile::read(&prepared) {
            let entry = if !crate::energy_profile::mixing_allowed(&prepared) {
                profile.at(prepared.cue_in)
            } else {
                profile
                    .entry(
                        wanted.map_or(0.5, |e| (e - 1.) / 9.),
                        crate::energy_profile::entry_bound(&prepared),
                        prepared.cue_out.unwrap_or(profile.duration),
                        8.,
                    )
                    .map(|s| s.energy)
            };
            if let Some(e) = entry {
                prepared
                    .extensions
                    .insert("energy".into(), serde_json::json!(e));
            }
        }
        let e = match (wanted, energy(&prepared)) {
            (Some(a), Some(b)) => (a - b).abs(),
            _ => 3.,
        };
        let tempo = playing
            .and_then(|p| p.bpm)
            .zip(t.preparation.bpm)
            .map_or(1., |(a, b)| {
                crate::transition_plan::tempo_distance(a, b) * 5.
            });
        e + tempo
            + playing.map_or(1., |p| {
                crate::smart::cost(p, &prepared, wanted.map(|e| (e - 1.) / 9.))
            })
            - if p.preferred.contains(&song_id(t)) {
                0.25
            } else {
                0.
            }
    };
    let mut scored: Vec<_> = candidates.iter().map(|(i, t)| (score(t), *i)).collect();
    scored.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, i)| i).collect()
}
#[derive(Debug, Clone, Serialize)]
pub struct Progression {
    pub start: f64,
    pub target: f64,
    pub total: u32,
    pub completed: u32,
    pub hold: bool,
}
impl Progression {
    pub fn new(start: f64, target: f64, total: u32, hold: bool) -> Result<Self, String> {
        if !start.is_finite()
            || !target.is_finite()
            || !(1. ..=10.).contains(&start)
            || !(1. ..=10.).contains(&target)
            || !(1..=100).contains(&total)
        {
            return Err("Energy must be 1–10 and progression 1–100 transitions".into());
        }
        Ok(Self {
            start,
            target,
            total,
            completed: 0,
            hold,
        })
    }
    pub fn next(&self) -> f64 {
        self.start
            + (self.target - self.start) * (self.completed + 1).min(self.total) as f64
                / self.total as f64
    }
    pub fn remaining(&self) -> u32 {
        self.total.saturating_sub(self.completed)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_uses_entry_section_energy_instead_of_only_song_average() {
        let make = |name: &str, average: f64, local: f64| {
            let mut p = Preparation {
                bpm: Some(120.),
                cue_out: Some(60.),
                ..Default::default()
            };
            p.extensions
                .insert("energy".into(), serde_json::json!(average));
            p.extensions.insert("energy_profile".into(),serde_json::json!({"version":1,"duration":60.,"step":2.,"curve":[local],"sections":[{"id":"one","name":"Main","start":0.,"end":60.,"energy":local}]}));
            Track {
                path: name.into(),
                title: name.into(),
                preparation: p,
            }
        };
        let tracks = vec![
            make("current.wav", 0.8, 0.8),
            make("matched-section.wav", 0.1, 0.8),
            make("wrong-section.wav", 0.8, 0.1),
        ];
        let profile = Settings::default().playlists.remove(0);
        assert_eq!(
            choose(&profile, &tracks, Some(0), &[], Some(8.2), 0),
            Some(1)
        );
    }
    #[test]
    fn allow_exclude_and_recent_fallback_are_enforced() {
        let tracks = vec![
            Track {
                path: "a.wav".into(),
                title: "A".into(),
                preparation: Default::default(),
            },
            Track {
                path: "b.wav".into(),
                title: "B".into(),
                preparation: Default::default(),
            },
        ];
        let mut p = Settings::default().playlists.remove(0);
        p.restrict_allowed = true;
        assert!(choose(&p, &tracks, None, &[], None, 0).is_none());
        p.allowed.push(song_id(&tracks[1]));
        assert_eq!(choose(&p, &tracks, None, &[], None, 0), Some(1));
        p.excluded.push(song_id(&tracks[1]));
        assert!(choose(&p, &tracks, None, &[], None, 0).is_none());
        p.excluded.clear();
        let recent = vec![song_id(&tracks[1])];
        p.fallback = Fallback::Stop;
        assert!(choose(&p, &tracks, None, &recent, None, 0).is_none());
        p.fallback = Fallback::RelaxRecent;
        assert_eq!(choose(&p, &tracks, None, &recent, None, 0), Some(1));
    }
    #[test]
    fn saved_folder_rules_use_portable_separators() {
        let d = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.playlists[0].folders = vec![PathBuf::from("Music\\Sets")];
        s.save(d.path()).unwrap();
        let text = std::fs::read_to_string(d.path().join("ampdj/smart-playlists.json")).unwrap();
        assert!(text.contains("Music/Sets"));
        Settings::load(d.path()).unwrap().validate().unwrap();
    }
    #[test]
    fn energy_changes_only_advance_with_completed_transitions() {
        let mut p = Progression::new(9., 3., 3, true).unwrap();
        assert_eq!(p.next(), 7.);
        p.completed += 1;
        assert_eq!(p.next(), 5.);
        p.completed += 1;
        assert_eq!(p.next(), 3.);
        assert_eq!(p.remaining(), 1);
    }
    #[test]
    fn defaults_are_editable_and_ids_survive_serialization() {
        let mut s = Settings::default();
        s.playlists[0].name = "Awards".into();
        s.validate().unwrap();
        let b = serde_json::to_vec(&s).unwrap();
        let v: Settings = serde_json::from_slice(&b).unwrap();
        assert_eq!(v.playlists[0].id, "demo");
    }
    #[test]
    fn performance_selection_uses_loaded_snapshot_after_shared_update() {
        let p = Settings::default().playlists.pop().unwrap();
        let track = |name: &str, bpm| Track {
            path: name.into(),
            title: name.into(),
            preparation: Preparation {
                bpm: Some(bpm),
                ..Default::default()
            },
        };
        let tracks = vec![
            track("playing.wav", 128.),
            track("old-match.wav", 120.),
            track("new-match.wav", 128.),
        ];
        let snapshot = Preparation {
            bpm: Some(120.),
            ..Default::default()
        };
        assert_eq!(choose(&p, &tracks, Some(0), &[], None, 0), Some(2));
        assert_eq!(
            choose_with_snapshot(&p, &tracks, Some(0), &[], None, 0, Some(&snapshot)),
            Some(1)
        );
    }
}

#[cfg(test)]
mod portable_playlist_tests {
    use super::*;
    #[test]
    fn roundtrip_all_rules_and_new_identity() {
        let root = tempfile::tempdir().unwrap();
        let mut p = Settings::default().playlists.remove(0);
        p.name = "Portable mix".into();
        p.style = Style::Hype;
        p.genres = vec!["Rock".into(), "R&B".into()];
        p.folders = vec!["Dance/House".into()];
        p.restrict_folders = true;
        p.allowed = vec!["song_allowed".into()];
        p.restrict_allowed = true;
        p.excluded = vec!["song_excluded".into()];
        p.preferred = vec!["song_preferred".into()];
        p.recognition_song = Some("song_recognition".into());
        p.target_energy = Some(8.);
        p.recent_repeat = 17;
        p.fallback = Fallback::Stop;
        p.use_manual_cues = false;
        p.energy_progression = Some(EnergyPlan {
            target: 9.,
            transitions: 6,
            hold: true,
        });
        let bytes = SmartPlaylistExport::encode(&p, root.path()).unwrap();
        let imported = SmartPlaylistExport::decode(&bytes, root.path()).unwrap();
        assert_ne!(p.id, imported.id);
        assert!(!imported.source_unavailable);
        let mut actual = serde_json::to_value(imported).unwrap();
        let mut expected = serde_json::to_value(p).unwrap();
        for key in ["id", "source_library", "source_root_hint"] {
            actual.as_object_mut().unwrap().remove(key);
            expected.as_object_mut().unwrap().remove(key);
        }
        assert_eq!(actual, expected);
        assert!(bytes.len() < 4096);
    }
    #[test]
    fn unavailable_import_retains_settings_and_blocks_unrelated_music() {
        let source = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let p = Settings::default().playlists.remove(0);
        let bytes = SmartPlaylistExport::encode(&p, source.path()).unwrap();
        let imported = SmartPlaylistExport::decode(&bytes, other.path()).unwrap();
        assert!(imported.source_unavailable);
        let cached: SmartPlaylist =
            serde_json::from_slice(&serde_json::to_vec(&imported).unwrap()).unwrap();
        assert!(cached.source_unavailable);
        assert_eq!(imported.name, p.name);
        let t = Track {
            path: "song.mp3".into(),
            title: "song".into(),
            preparation: Preparation::default(),
        };
        assert!(!eligible(&imported, &t));
        let mut settings = Settings {
            sync: None,
            version: 1,
            playlists: vec![imported],
        };
        settings.save(other.path()).unwrap();
        settings = Settings::load(other.path()).unwrap();
        assert!(settings.playlists[0].source_unavailable);
        settings.resolve_sources(source.path());
        assert!(eligible(&settings.playlists[0], &t));
    }
    #[test]
    fn rejects_unsupported_unsafe_and_oversized_packages() {
        let root = tempfile::tempdir().unwrap();
        let p = Settings::default().playlists.remove(0);
        let bytes = SmartPlaylistExport::encode(&p, root.path()).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        for field in ["../outside", "C:\\music", "/music", "..\\outside"] {
            let mut invalid = value.clone();
            invalid["playlist"]["folders"] = serde_json::json!([field]);
            assert!(
                SmartPlaylistExport::decode(&serde_json::to_vec(&invalid).unwrap(), root.path())
                    .is_err()
            );
        }
        let mut future = value;
        future["version"] = 99.into();
        assert!(
            SmartPlaylistExport::decode(&serde_json::to_vec(&future).unwrap(), root.path())
                .is_err()
        );
        assert!(SmartPlaylistExport::decode(b"not a playlist", root.path()).is_err());
        assert!(
            SmartPlaylistExport::decode(
                &vec![b' '; SmartPlaylistExport::MAX_BYTES as usize + 1],
                root.path()
            )
            .is_err()
        );
    }
    #[test]
    fn duplicate_imports_are_independent_and_share_identity_survives_relocation() {
        let source = tempfile::tempdir().unwrap();
        let relocated = tempfile::tempdir().unwrap();
        let p = Settings::default().playlists.remove(0);
        let bytes = SmartPlaylistExport::encode(&p, source.path()).unwrap();
        std::fs::create_dir_all(relocated.path().join("ampdj")).unwrap();
        std::fs::copy(
            source.path().join("ampdj/library.json"),
            relocated.path().join("ampdj/library.json"),
        )
        .unwrap();
        let a = SmartPlaylistExport::decode(&bytes, relocated.path()).unwrap();
        let b = SmartPlaylistExport::decode(&bytes, relocated.path()).unwrap();
        assert_ne!(a.id, b.id);
        assert!(!a.source_unavailable);
    }
}

/// Equal-power bus gains shared by AMP DJ and Smart Radio.
pub fn crossfade_weights(position: f32) -> [f32; 2] {
    let phase = position.clamp(0., 1.) * std::f32::consts::FRAC_PI_2;
    [phase.cos(), phase.sin()]
}
