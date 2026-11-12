//! Runtime-only song-length policy. Energy boundaries are not semantic chorus detection.
use crate::{Preparation, show::Style};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Pace {
    Quick,
    #[default]
    Balanced,
    Full,
}
impl Pace {
    pub fn label(self) -> &'static str {
        match self {
            Self::Quick => "Quick mixes",
            Self::Balanced => "Balanced",
            Self::Full => "Full track",
        }
    }
}
/// Latest handoff horizon in source seconds. Cue/phrase data and source preparation are never changed.
pub fn exit(p: &Preparation, style: Style, pace: Pace, duration: f64, fade: f64) -> f64 {
    let end = p.cue_out.unwrap_or(duration).min(duration);
    if !end.is_finite() || end <= p.cue_in {
        return duration;
    }
    if pace == Pace::Full || (p.manual_override && p.cue_out.is_some()) {
        return end;
    }
    if let Some(cue) = p
        .manual_cues
        .iter()
        .filter(|c| {
            matches!(c.kind.as_str(), "mix_out" | "chorus_end" | "drop_end")
                && c.seconds > p.cue_in
                && c.seconds <= end
        })
        .min_by(|a, b| a.seconds.total_cmp(&b.seconds))
    {
        return cue.seconds;
    }
    let phrase = p
        .bpm
        .filter(|b| b.is_finite() && (40. ..=250.).contains(b))
        .map(|b| 32. * 60. / b);
    let reserve = fade.max(0.) + phrase.unwrap_or(4.);
    let minimum = if pace == Pace::Quick { 32. } else { 45. };
    let first = p.cue_in + minimum + reserve;
    if first + 8. >= end {
        return end;
    }
    let budget = match (pace, style) {
        (Pace::Quick, Style::Hype) => 60.,
        (Pace::Quick, _) => 90.,
        (_, Style::Hype) => 100.,
        (_, Style::Steady) => 130.,
        (_, Style::Chill) => 180.,
    };
    let desired = (p.cue_in + budget).max(first).min(end - 8.);
    let last = (desired + phrase.unwrap_or(12.)).min(end - 4.);
    let mut candidates = Vec::new();
    // Approved section ends can be useful without inventing verse/chorus labels.
    candidates.extend(
        p.manual_cues
            .iter()
            .filter(|c| matches!(c.kind.as_str(), "section_end" | "transition"))
            .map(|c| (0u8, c.seconds)),
    );
    if crate::energy_profile::mixing_allowed(p)
        && let Some(profile) = crate::energy_profile::read(p)
    {
        candidates.extend(profile.sections.iter().map(|s| {
            let end = if let (Some(period), Some(anchor)) =
                (phrase, crate::mix_schedule::phrase_anchor(p))
            {
                anchor + ((s.end - anchor) / period).ceil() * period
            } else {
                s.end
            };
            (1u8, end)
        }));
    }
    if let (Some(period), Some(anchor)) = (phrase, crate::mix_schedule::phrase_anchor(p)) {
        let boundary = anchor + ((desired - anchor) / period).ceil() * period;
        candidates.push((2u8, boundary));
        candidates.push((2u8, boundary - period));
    }
    candidates
        .into_iter()
        .filter(|(_, v)| v.is_finite() && *v >= first && *v <= last)
        .min_by(|(priority_a, a), (priority_b, b)| {
            priority_a
                .cmp(priority_b)
                .then_with(|| (a - desired).abs().total_cmp(&(b - desired).abs()))
        })
        .map_or(desired, |(_, value)| value)
        .clamp(first, end)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn balanced_and_quick_leave_long_songs_early_without_changing_preparation() {
        let p = Preparation {
            bpm: Some(120.),
            beat_offset: Some(0.),
            ..Default::default()
        };
        let saved = p.clone();
        let balanced = exit(&p, Style::Steady, Pace::Balanced, 300., 3.);
        let quick = exit(&p, Style::Hype, Pace::Quick, 300., 3.);
        assert!(quick < balanced && balanced < 200.);
        assert_eq!(balanced % 16., 0.);
        assert_eq!(p, saved);
        assert_eq!(exit(&p, Style::Hype, Pace::Full, 300., 3.), 300.);
    }
    #[test]
    fn manual_mix_out_and_short_tracks_are_respected() {
        let mut p = Preparation {
            cue_out: Some(240.),
            manual_override: true,
            ..Default::default()
        };
        assert_eq!(exit(&p, Style::Hype, Pace::Quick, 300., 8.), 240.);
        p.manual_override = false;
        p.manual_cues.push(crate::preparation::Cue {
            id: "out".into(),
            seconds: 180.,
            kind: "mix_out".into(),
            ..Default::default()
        });
        assert_eq!(exit(&p, Style::Hype, Pace::Quick, 300., 8.), 180.);
        assert_eq!(
            exit(&Preparation::default(), Style::Hype, Pace::Quick, 25., 8.),
            25.
        );
    }
    #[test]
    fn advanced_entry_keeps_a_minimum_listening_window() {
        let p = Preparation {
            cue_in: 110.,
            bpm: Some(120.),
            beat_offset: Some(0.),
            ..Default::default()
        };
        let end = exit(&p, Style::Hype, Pace::Quick, 300., 8.);
        assert!(end >= 110. + 32. + 8. + 16.);
        assert!(end < 250.);
    }
}
