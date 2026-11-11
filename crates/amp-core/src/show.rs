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
pub struct SmartPlaylist {
    pub id: String,
    pub name: String,
    pub style: Style,
    #[serde(default)]
    pub folders: Vec<PathBuf>,
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
                    id: id.into(),
                    name: name.into(),
                    style,
                    folders: vec![],
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
    fn normalize(&mut self) {
        for p in &mut self.playlists {
            if !p.allowed.is_empty() {
                p.restrict_allowed = true;
            }
            for f in &mut p.folders {
                *f = PathBuf::from(f.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.playlists.len() > 1000 {
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
pub fn energy(p: &Preparation) -> Option<f64> {
    crate::smart::energy(p).map(|e| 1. + e * 9.)
}
pub fn eligible(p: &SmartPlaylist, t: &Track) -> bool {
    let id = song_id(t);
    !p.excluded.contains(&id)
        && ((!p.restrict_allowed && p.allowed.is_empty()) || p.allowed.contains(&id))
        && ((!p.restrict_folders && p.folders.is_empty())
            || p.folders.iter().any(|f| t.path.starts_with(f)))
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
        return None;
    }
    let wanted = target.or(p.target_energy).or(playing.and_then(energy));
    candidates.sort_by(|(a, x), (b, y)| {
        let score = |t: &Track| {
            let e = match (wanted, energy(&t.preparation)) {
                (Some(a), Some(b)) => (a - b).abs(),
                _ => 3.,
            };
            let tempo = playing
                .and_then(|p| p.bpm)
                .zip(t.preparation.bpm)
                .map_or(1., |(a, b)| (a / b).ln().abs() * 5.);
            e + tempo
                + playing.map_or(1., |p| {
                    crate::smart::cost(p, &t.preparation, wanted.map(|e| (e - 1.) / 9.))
                })
                - if p.preferred.contains(&song_id(t)) {
                    0.25
                } else {
                    0.
                }
        };
        score(x).total_cmp(&score(y)).then(a.cmp(b))
    });
    candidates
        .get(alternate.min(candidates.len().saturating_sub(1)))
        .map(|(i, _)| *i)
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
