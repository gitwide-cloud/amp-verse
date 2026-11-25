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
/// Minimum complete musical passage before an automatic Hype handoff.
/// Only manual/reviewed section roles are semantic evidence. Legacy automatic
/// `drop` cues are loudness estimates and must not authorize a short vocal-only cut.
fn hype_passage_end(p: &Preparation, end: f64) -> f64 {
    let phrase = if crate::mix_schedule::timing_reliable(p) {
        p.bpm.map(|b| 64. * 60. / b)
    } else {
        None
    }
    .unwrap_or(32.)
    .max(16.);
    let reviewed = p.analysis.as_ref().is_some_and(|a| a.reviewed);
    let hook = p
        .manual_cues
        .iter()
        .chain(p.cues.iter().filter(|_| reviewed))
        .filter(|c| matches!(c.kind.as_str(), "hook" | "chorus" | "chorus_start" | "drop"))
        .filter(|c| {
            c.seconds.is_finite()
                && c.seconds >= 0.
                && c.seconds + phrase > p.cue_in
                && c.seconds < end
        })
        .min_by(|a, b| a.seconds.total_cmp(&b.seconds));
    if let Some(hook) = hook {
        (hook.seconds.max(p.cue_in) + phrase).min(end)
    } else {
        // Without semantic evidence retain most short songs and at least two
        // minutes of longer ones. This is a conservative duration fallback,
        // not a claim that a chorus has been detected.
        p.cue_in + ((end - p.cue_in) * 0.65).min(120.)
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
    let mut first = p.cue_in + minimum + reserve;
    if style == Style::Hype {
        // Scheduling can launch up to a phrase before the horizon. Reserve both
        // the complete fade and that phrase so neither cuts the protected passage.
        first = first.max(hype_passage_end(p, end) + reserve);
    }
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
            extensions: [("mix_phrase_anchor".into(), serde_json::json!(0.))].into(),
            manual_override: true,
            ..Default::default()
        };
        let saved = p.clone();
        let balanced = exit(&p, Style::Steady, Pace::Balanced, 300., 3.);
        let quick = exit(&p, Style::Hype, Pace::Quick, 300., 3.);
        assert!(quick >= 120. && balanced < 200.);
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
            extensions: [("mix_phrase_anchor".into(), serde_json::json!(0.))].into(),
            manual_override: true,
            ..Default::default()
        };
        let end = exit(&p, Style::Hype, Pace::Quick, 300., 8.);
        assert!(end >= 110. + 32. + 8. + 16.);
        assert!(end < 300.);
    }
    #[test]
    fn hype_does_not_fade_before_a_late_marked_hook_has_played() {
        let mut p = Preparation {
            bpm: Some(120.),
            beat_offset: Some(0.),
            manual_override: true,
            ..Default::default()
        };
        p.extensions
            .insert("mix_phrase_anchor".into(), serde_json::json!(0.));
        p.manual_cues.push(crate::preparation::Cue {
            id: "hook".into(),
            kind: "chorus_start".into(),
            seconds: 96.,
            ..Default::default()
        });
        let saved = p.clone();
        // Sixteen bars of the actual hook must finish before even the fade starts.
        assert!(exit(&p, Style::Hype, Pace::Quick, 240., 8.) - 8. >= 128.);
        assert_eq!(p, saved);
    }
    #[test]
    fn unreviewed_energy_peaks_do_not_justify_a_sixty_second_hype_cut() {
        let mut p = Preparation {
            cue_in: 15.963718820861676,
            cue_out: Some(217.5954648526077),
            bpm: Some(129.00085844822527),
            ..Default::default()
        };
        p.cues.push(crate::preparation::Cue {
            id: "auto-energy-1".into(),
            kind: "drop".into(),
            seconds: p.cue_in,
            ..Default::default()
        });
        assert!(exit(&p, Style::Hype, Pace::Quick, 219.61142857142855, 5.) - 5. >= p.cue_in + 120.);
    }
    #[test]
    fn hype_keeps_short_and_unanalysed_tracks_inside_the_audio() {
        for duration in [8., 25., 60., 179.12163265306123, 219.61142857142855] {
            let p = Preparation::default();
            let at = exit(&p, Style::Hype, Pace::Quick, duration, 8.);
            assert!(at <= duration && at >= duration.min((duration * 0.65 + 8.).min(128.)));
        }
    }
    #[test]
    fn reported_edm_decks_retain_a_substantial_passage_before_handoff() {
        let examples: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/reported-edm-passages.json"))
                .unwrap();
        for example in examples.as_array().unwrap() {
            let p: Preparation = serde_json::from_value(example["preparation"].clone()).unwrap();
            let original = p.clone();
            let duration = p.analysis.as_ref().unwrap().duration;
            for fade in [3., 5., 8., 15.] {
                let end = p.cue_out.unwrap_or(duration);
                let protected = p.cue_in + ((end - p.cue_in) * 0.65).min(120.);
                let horizon = exit(&p, Style::Hype, Pace::Quick, duration, fade);
                assert!(
                    horizon >= protected + fade || horizon == end,
                    "{}: {horizon}",
                    example["title"]
                );
                assert!(horizon <= end);
                println!(
                    "{}: entry={:.2}, Hype Quick exit={horizon:.2}, fade={fade:.1}",
                    example["title"], p.cue_in
                );
            }
            assert_eq!(p, original);
        }
    }
}
