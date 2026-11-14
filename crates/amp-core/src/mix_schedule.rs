use crate::Preparation;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Timing {
    Immediate,
    #[default]
    Beat,
    Bar,
    Phrase8,
    Phrase16,
    Phrase32,
}
impl Timing {
    pub fn phrase_beats(self) -> Option<f64> {
        match self {
            Self::Phrase8 => Some(8.),
            Self::Phrase16 => Some(16.),
            Self::Phrase32 => Some(32.),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Immediate => "Immediately",
            Self::Beat => "Next beat",
            Self::Bar => "Next bar",
            Self::Phrase8 => "8-beat phrase",
            Self::Phrase16 => "16-beat phrase",
            Self::Phrase32 => "32-beat phrase",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Grid {
    pub period: f64,
    pub anchor: f64,
}
impl Grid {
    pub fn next(self, position: f64) -> f64 {
        amp_dj_policy::next_boundary(self.anchor, self.period, position)
    }
}
/// Commit the last phase-matched launch that finishes before the chosen exit.
/// All positions use outgoing decoder source time; duration uses wall seconds.
/// A late caller gets no schedule rather than waiting another whole phrase.
pub fn launch_before_exit(grid: Grid, now: f64, exit: f64, rate: f64, seconds: f64) -> Option<f64> {
    if ![grid.period, grid.anchor, now, exit, rate, seconds]
        .iter()
        .all(|v| v.is_finite())
        || grid.period <= 0.
        || rate <= 0.
        || seconds <= 0.
    {
        return None;
    }
    let latest = exit - (seconds + 0.15) * rate;
    let at = grid.anchor + ((latest - grid.anchor) / grid.period).floor() * grid.period;
    (at >= now + 0.02 * rate).then_some(at)
}
/// Reviewed phrase starts override counted estimates; no semantic chorus/verse recognition is implied.
pub fn phrase_anchor(p: &Preparation) -> Option<f64> {
    p.extensions
        .get("mix_phrase_anchor")
        .and_then(|v| v.as_f64())
        .filter(|v| v.is_finite() && *v >= 0.)
        .or_else(|| {
            p.extensions
                .get("mix_bar_anchor")
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite() && *v >= 0.)
        })
        .or(p.beat_offset)
}
/// Hand-approved grids retain priority. Automatic constant grids must not drive
/// quantized transitions when analysis reports low confidence or audible drift.
pub fn timing_reliable(p: &Preparation) -> bool {
    p.bpm
        .is_some_and(|b| b.is_finite() && (40. ..=300.).contains(&b))
        && p.beat_offset.is_some_and(|o| o.is_finite() && o >= 0.)
        && (p.manual_override
            || p.analysis.as_ref().is_some_and(|a| a.reviewed)
            || (p
                .analysis
                .as_ref()
                .is_none_or(|a| a.tempo_confidence.is_finite() && a.tempo_confidence >= 0.5)
                && p.extensions
                    .get("rhythm_quality")
                    .and_then(|v| v.get("stable"))
                    .and_then(|v| v.as_bool())
                    != Some(false)))
}
/// Source-time grid. Bar anchors are explicit user corrections, never inferred from beat phase.
pub fn grid(
    out: &Preparation,
    incoming: &Preparation,
    entry: f64,
    origin: f64,
    timing: Timing,
) -> Option<Grid> {
    if timing == Timing::Immediate || !timing_reliable(out) || !timing_reliable(incoming) {
        return None;
    }
    let beat = 60.
        / out
            .bpm
            .filter(|b| b.is_finite() && (40. ..=300.).contains(b))?;
    let in_beat = 60.
        / incoming
            .bpm
            .filter(|b| b.is_finite() && (40. ..=300.).contains(b))?;
    let bar = |p: &Preparation| {
        p.extensions
            .get("mix_bar_anchor")
            .and_then(|v| v.as_f64())
            .filter(|v| v.is_finite() && *v >= 0.)
    };
    let (anchor, in_anchor, multiple) = if let Some(beats) = timing.phrase_beats() {
        (phrase_anchor(out)?, phrase_anchor(incoming)?, beats)
    } else if timing == Timing::Bar
        && let Some((a, b)) = bar(out).zip(bar(incoming))
    {
        (a, b, 4.)
    } else {
        (out.beat_offset?, incoming.beat_offset?, 1.)
    };
    if !anchor.is_finite() || !in_anchor.is_finite() {
        return None;
    }
    let phase = ((entry - in_anchor) / (in_beat * multiple)).rem_euclid(1.);
    Some(Grid {
        period: beat * multiple,
        anchor: anchor - origin + phase * beat * multiple,
    })
}
/// Account for actual playback rates and half/double-time pulse interpretations.
/// Ambiguous half-time pairs may align beats, but do not claim matching musical bars/phrases.
#[allow(clippy::too_many_arguments)]
pub fn grid_at_rates(
    out: &Preparation,
    incoming: &Preparation,
    entry: f64,
    origin: f64,
    timing: Timing,
    out_rate: f64,
    in_rate: f64,
) -> Option<Grid> {
    if ![out_rate, in_rate].iter().all(|r| r.is_finite() && *r > 0.) {
        return None;
    }
    let ratio = out.bpm? * out_rate / (incoming.bpm? * in_rate);
    let factor = [1., 0.5, 2.]
        .into_iter()
        .find(|factor| (ratio / factor - 1.).abs() <= 0.0005)?;
    if factor != 1. && timing.phrase_beats().is_some() {
        return None;
    }
    let mut pulse = incoming.clone();
    pulse.bpm = pulse.bpm.map(|b| b * factor);
    grid(
        out,
        &pulse,
        entry,
        origin,
        if factor != 1. && timing == Timing::Bar {
            Timing::Beat
        } else {
            timing
        },
    )
}
/// When musical phrases are ambiguous (half/double time), retain beat alignment.
#[allow(clippy::too_many_arguments)]
pub fn safe_grid_at_rates(
    out: &Preparation,
    incoming: &Preparation,
    entry: f64,
    origin: f64,
    timing: Timing,
    out_rate: f64,
    in_rate: f64,
) -> Option<Grid> {
    grid_at_rates(out, incoming, entry, origin, timing, out_rate, in_rate).or_else(|| {
        timing.phrase_beats().and_then(|_| {
            grid_at_rates(
                out,
                incoming,
                entry,
                origin,
                Timing::Beat,
                out_rate,
                in_rate,
            )
        })
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn committed_launch_finishes_before_exit_without_changing_fade_length() {
        let grid = Grid {
            period: 8.,
            anchor: 5.,
        };
        assert_eq!(launch_before_exit(grid, 20., 32., 1., 3.), Some(21.));
        assert_eq!(launch_before_exit(grid, 21.01, 32., 1., 3.), None);
        let grid = Grid {
            period: 0.5,
            anchor: 0.13,
        };
        for rate in [0.84, 1., 1.16] {
            let at = launch_before_exit(grid, 20., 30., rate, 3.).unwrap();
            assert!(at + 3.15 * rate <= 30. + 1e-9);
            assert!(at + grid.period + 3.15 * rate > 30.);
            assert!(((at - grid.anchor) / grid.period).fract().abs() < 1e-9);
        }
        assert!(launch_before_exit(grid, 29.9, 30., 1., 3.).is_none());
        assert!(launch_before_exit(grid, f64::NAN, 30., 1., 3.).is_none());
    }
    #[test]
    fn matched_grids_keep_phase_for_a_thousand_beats_and_reject_drift() {
        let a = Preparation {
            bpm: Some(128.),
            beat_offset: Some(0.13),
            ..Default::default()
        };
        let b = Preparation {
            bpm: Some(125.),
            beat_offset: Some(0.27),
            ..Default::default()
        };
        let rate = 128. / 125.;
        let entry = 10.321;
        let origin = 0.4;
        let g = grid_at_rates(&a, &b, entry, origin, Timing::Phrase16, 1., rate).unwrap();
        let launch = g.next(3.);
        for n in 0..1000 {
            let time = n as f64 * 60. / 128.;
            let phase_out = (launch + origin + time - 0.13) / (60. / 128.);
            let phase_in = (entry + time * rate - 0.27) / (60. / 125.);
            let error = (phase_out - phase_in + 0.5).rem_euclid(1.) - 0.5;
            assert!(error.abs() < 1e-9);
        }
        assert!(grid_at_rates(&a, &b, entry, origin, Timing::Beat, 1., rate * 1.001).is_none());
    }
    #[test]
    fn preroll_and_exact_fade_land_on_reviewed_phrase_boundary() {
        let a = Preparation {
            bpm: Some(120.),
            beat_offset: Some(0.),
            ..Default::default()
        };
        let mut b = a.clone();
        b.extensions
            .insert("mix_phrase_anchor".into(), serde_json::json!(20.));
        let grid = grid_at_rates(&a, &b, 17., 0., Timing::Phrase16, 1., 1.).unwrap();
        let start = grid.next(0.1);
        assert!((start - 5.).abs() < 1e-9);
        assert!(((start + 3.) / grid.period).fract().abs() < 1e-9);
        let mut half = b.clone();
        half.bpm = Some(60.);
        assert!(grid_at_rates(&a, &half, 17., 0., Timing::Beat, 1., 1.).is_some());
        assert!(grid_at_rates(&a, &half, 17., 0., Timing::Phrase16, 1., 1.).is_none());
        assert!(grid_at_rates(&a, &b, 17., 0., Timing::Beat, 1., 1.1).is_none());
    }
    #[test]
    fn drifting_automatic_grids_fall_back_until_reviewed() {
        let mut p = Preparation {
            bpm: Some(120.),
            beat_offset: Some(0.),
            ..Default::default()
        };
        p.extensions
            .insert("rhythm_quality".into(), serde_json::json!({"stable":false}));
        assert!(grid(&p, &p, 0., 0., Timing::Phrase16).is_none());
        p.manual_override = true;
        assert!(grid(&p, &p, 0., 0., Timing::Phrase16).is_some());
    }
    #[test]
    fn phase_matches_entry_and_bars_require_reviewed_anchors() {
        let mut p = Preparation {
            bpm: Some(120.),
            beat_offset: Some(0.1),
            ..Default::default()
        };
        let g = grid(&p, &p, 1.35, 1., Timing::Beat).unwrap();
        assert!((g.next(2.) - 2.35).abs() < 1e-8);
        assert_eq!(grid(&p, &p, 0.1, 0., Timing::Bar).unwrap().period, 0.5);
        p.extensions
            .insert("mix_bar_anchor".into(), serde_json::json!(0.1));
        let g = grid(&p, &p, 0.1, 0., Timing::Bar).unwrap();
        assert!((g.next(0.2) - 2.1).abs() < 1e-8);
        p.beat_offset = None;
        p.bpm = None;
        assert!(grid(&p, &p, 0., 0., Timing::Beat).is_none());
    }
    #[test]
    fn reviewed_phrase_anchors_match_phase_and_preserve_requested_entry() {
        let mut a = Preparation {
            bpm: Some(120.),
            beat_offset: Some(0.1),
            ..Default::default()
        };
        a.extensions
            .insert("mix_phrase_anchor".into(), serde_json::json!(2.1));
        let mut b = a.clone();
        b.extensions
            .insert("mix_phrase_anchor".into(), serde_json::json!(4.1));
        let g = grid(&a, &b, 4.1, 0., Timing::Phrase16).unwrap();
        assert_eq!(g.period, 8.);
        assert!((g.next(3.) - 10.1).abs() < 1e-8);
        let g = grid(&a, &b, 8.1, 0., Timing::Phrase16).unwrap();
        assert!((g.next(3.) - 6.1).abs() < 1e-8);
        assert_eq!(grid(&a, &b, 4.1, 0., Timing::Phrase32).unwrap().period, 16.);
    }
}
