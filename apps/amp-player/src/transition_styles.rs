//! Device-local performance choices, independent of protected playlist/cue metadata.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Style {
    #[default]
    Variety,
    Fade,
    Scratch,
    Stutter,
    Brake,
    Filter,
    BassSwap,
    Beatmatch,
    LoopBuild,
    StemSwap,
    Mashup,
}
impl Style {
    pub fn label(self) -> &'static str {
        match self {
            Self::Variety => "Variety",
            Self::Fade => "Clean fade",
            Self::Scratch => "Scratch",
            Self::Stutter => "Stutter",
            Self::Brake => "Tape brake",
            Self::Filter => "Filter sweep",
            Self::BassSwap => "Bass swap",
            Self::Beatmatch => "Beatmatch",
            Self::LoopBuild => "Loop buildup",
            Self::StemSwap => "Stem handoff",
            Self::Mashup => "Vocal / instrumental mashup",
        }
    }
}
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub pace: amp_core::dj_phrasing::Pace,
    pub timing: crate::mix_schedule::Timing,
    pub style: Style,
    pub every: u32,
    pub techniques: [bool; 6],
    pub loop_build: bool,
    pub stem_swap: bool,
    pub mashup: bool,
    pub mashup_beats: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            pace: Default::default(),
            timing: Default::default(),
            style: Style::Variety,
            every: 4,
            techniques: [true; 6],
            loop_build: false,
            stem_swap: false,
            mashup: false,
            mashup_beats: 16,
        }
    }
}
impl Settings {
    pub fn pick(self, completed: u32) -> Option<Style> {
        if !self.enabled
            || !(2..=12).contains(&self.every)
            || !completed.wrapping_add(1).is_multiple_of(self.every)
        {
            return None;
        }
        let all = [
            Style::Scratch,
            Style::Stutter,
            Style::Brake,
            Style::Filter,
            Style::BassSwap,
            Style::Beatmatch,
        ];
        let mut enabled: Vec<_> = all
            .into_iter()
            .zip(self.techniques)
            .filter_map(|(s, yes)| yes.then_some(s))
            .collect();
        if self.loop_build {
            enabled.push(Style::LoopBuild);
        }
        if self.stem_swap {
            enabled.push(Style::StemSwap);
        }
        if self.mashup {
            enabled.push(Style::Mashup);
        }
        match self.style {
            Style::Fade => None,
            Style::Variety if !enabled.is_empty() => {
                Some(enabled[(completed / self.every) as usize % enabled.len()])
            }
            Style::Variety => None,
            style => enabled.contains(&style).then_some(style),
        }
    }
}
pub fn load() -> Settings {
    if cfg!(test) {
        return Settings::default();
    }
    crate::theme::settings_path()
        .and_then(|p| std::fs::read(p.with_file_name("auto-transitions.json")).ok())
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
        .filter(|s| (2..=12).contains(&s.every) && [8, 16, 32].contains(&s.mashup_beats))
        .unwrap_or_default()
}
pub fn save(settings: Settings) -> Result<(), String> {
    if ![8, 16, 32].contains(&settings.mashup_beats) {
        return Err("Mashup length must be 8, 16 or 32 beats".into());
    }
    if !(2..=12).contains(&settings.every) {
        return Err("Effect frequency must be 2–12 mixes".into());
    }
    let path = crate::theme::settings_path()
        .ok_or("Settings folder unavailable")?
        .with_file_name("auto-transitions.json");
    amp_core::coordination::directory(path.parent().ok_or("Settings folder unavailable")?)?;
    amp_core::coordination::atomic_write(&path, &settings)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pace_is_remembered_and_older_settings_default_to_balanced() {
        let settings = Settings {
            pace: amp_core::dj_phrasing::Pace::Quick,
            ..Default::default()
        };
        let bytes = serde_json::to_vec(&settings).unwrap();
        assert_eq!(
            serde_json::from_slice::<Settings>(&bytes).unwrap().pace,
            amp_core::dj_phrasing::Pace::Quick
        );
        let mut value = serde_json::to_value(settings).unwrap();
        value.as_object_mut().unwrap().remove("pace");
        assert_eq!(
            serde_json::from_value::<Settings>(value).unwrap().pace,
            amp_core::dj_phrasing::Pace::Balanced
        );
    }
    #[test]
    fn mashups_are_opt_in_and_use_saved_frequency_and_hold() {
        let s = Settings {
            style: Style::Mashup,
            mashup: true,
            every: 2,
            mashup_beats: 8,
            ..Default::default()
        };
        assert_eq!(s.pick(0), None);
        assert_eq!(s.pick(1), Some(Style::Mashup));
        let restored: Settings = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(restored.mashup_beats, 8);
        assert_eq!(restored.pick(1), Some(Style::Mashup));
        assert_eq!(Settings { mashup: false, ..s }.pick(1), None);
    }
    #[test]
    fn new_techniques_are_opt_in_and_old_settings_remain_readable() {
        let old: Settings = serde_json::from_str(r#"{"enabled":true,"every":2,"style":"Variety","techniques":[false,false,false,false,false,false]}"#).unwrap();
        assert!(!old.loop_build && !old.stem_swap);
        assert_eq!(old.pick(1), None);
        let updated = Settings {
            loop_build: true,
            stem_swap: true,
            ..old
        };
        assert_eq!(updated.pick(1), Some(Style::LoopBuild));
        assert_eq!(updated.pick(3), Some(Style::StemSwap));
        assert_eq!(
            serde_json::from_str::<Settings>(&serde_json::to_string(&updated).unwrap()).unwrap(),
            updated
        );
    }
    #[test]
    fn variety_is_occasional_and_rotates_without_changing_fade_duration() {
        let s = Settings::default();
        assert!((0..3).all(|i| s.pick(i).is_none()));
        assert_eq!(s.pick(3), Some(Style::Scratch));
        assert_eq!(s.pick(7), Some(Style::Stutter));
        assert_eq!(s.pick(11), Some(Style::Brake));
        assert_eq!(s.pick(15), Some(Style::Filter));
        assert_eq!(s.pick(19), Some(Style::BassSwap));
        assert_eq!(s.pick(23), Some(Style::Beatmatch));
        assert_eq!(
            Settings {
                techniques: [false; 6],
                ..s
            }
            .pick(3),
            None
        );
        assert!(
            Settings {
                enabled: false,
                ..s
            }
            .pick(3)
            .is_none()
        );
        assert!(
            Settings {
                style: Style::Fade,
                ..s
            }
            .pick(3)
            .is_none()
        );
        assert_eq!(
            Settings {
                style: Style::Stutter,
                every: 2,
                ..s
            }
            .pick(1),
            Some(Style::Stutter)
        );
    }
}
