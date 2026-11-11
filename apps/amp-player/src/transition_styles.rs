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
        }
    }
}
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub style: Style,
    pub every: u32,
    pub techniques: [bool; 6],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            style: Style::Variety,
            every: 4,
            techniques: [true; 6],
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
        let enabled: Vec<_> = all
            .into_iter()
            .zip(self.techniques)
            .filter_map(|(s, yes)| yes.then_some(s))
            .collect();
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
        .filter(|s| (2..=12).contains(&s.every))
        .unwrap_or_default()
}
pub fn save(settings: Settings) -> Result<(), String> {
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
