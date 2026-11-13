//! Session-only pair planning. Never edits shared cues, grids or tempo metadata.
use crate::Preparation;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoMatch {
    pub rate: f64,
    pub pulse_factor: f64,
}
/// Keep the fade inside the outgoing audio tail, leaving one UI tick of margin.
/// Unknown duration retains the user's setting; never modifies the saved preference.
pub fn tail_fade(requested: f32, remaining: Option<f32>) -> f32 {
    let requested = if requested.is_finite() {
        requested.clamp(0.1, 120.)
    } else {
        3.
    };
    remaining
        .filter(|r| r.is_finite())
        .map_or(requested, |r| requested.min((r - 0.15).max(0.1)))
}
/// Interpret half/double-time analysis without accelerating a song by 2x.
pub fn tempo_match(out_bpm: f64, out_rate: f64, in_bpm: f64, limit: f64) -> Option<TempoMatch> {
    if ![out_bpm, out_rate, in_bpm, limit]
        .iter()
        .all(|v| v.is_finite())
        || out_bpm <= 0.
        || in_bpm <= 0.
        || out_rate <= 0.
        || !(0. ..=0.2).contains(&limit)
    {
        return None;
    }
    for pulse_factor in [1., 0.5, 2.] {
        let rate = out_bpm * out_rate / (in_bpm * pulse_factor);
        if (1. - limit..=1. + limit).contains(&rate) {
            return Some(TempoMatch { rate, pulse_factor });
        }
    }
    None
}
pub fn tempo_distance(a: f64, b: f64) -> f64 {
    if !a.is_finite() || !b.is_finite() || a <= 0. || b <= 0. {
        return 1.;
    }
    [0.5, 1., 2.]
        .into_iter()
        .map(|factor| (a / (b * factor)).ln().abs())
        .fold(f64::INFINITY, f64::min)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Landing {
    pub entry: f64,
    pub landing: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildSwap {
    pub outgoing_start: f64,
    pub outgoing_drop: f64,
    pub seconds: f64,
}
/// Match two complete sixteen-beat buildups. Cues identify musical events; an
/// arbitrary loud section is not enough. Never seek the sounding deck or move cues.
pub fn build_swap(
    out: &Preparation,
    incoming: &Preparation,
    position: f64,
    entry: f64,
    drop: f64,
    rates: [f64; 2],
) -> Option<BuildSwap> {
    let [out_rate, in_rate] = rates;
    if ![position, entry, drop, out_rate, in_rate]
        .iter()
        .all(|v| v.is_finite())
        || out_rate <= 0.
        || in_rate <= 0.
        || !crate::mix_schedule::timing_reliable(out)
        || !crate::mix_schedule::timing_reliable(incoming)
    {
        return None;
    }
    let bpm_out = out.bpm?;
    let bpm_in = incoming.bpm?;
    if !incoming
        .manual_cues
        .iter()
        .chain(&incoming.cues)
        .any(|cue| {
            matches!(cue.kind.as_str(), "drop" | "chorus_start" | "chorus")
                && (cue.seconds - drop).abs() <= 0.01
        })
    {
        return None;
    }
    if (bpm_out * out_rate / (bpm_in * in_rate) - 1.).abs() > 0.0005 {
        return None;
    }
    let seconds = 16. * 60. / (bpm_out * out_rate);
    // Preload must already contain the whole incoming buildup, not be stretched
    // or silently sought after its decoder has been prepared.
    if ((drop - entry) / in_rate - seconds).abs() > 0.01
        || entry < 0.
        || incoming.cue_out.is_some_and(|end| drop >= end)
    {
        return None;
    }
    let phase = |p: &Preparation, at: f64| {
        ((at - p.beat_offset.unwrap()) * p.bpm.unwrap() / 60.).rem_euclid(1.)
    };
    let find = |cues: &[crate::preparation::Cue]| {
        cues.iter()
            .filter(|cue| {
                matches!(cue.kind.as_str(), "drop" | "chorus_start" | "chorus")
                    && cue.seconds.is_finite()
                    && out.cue_out.is_none_or(|end| cue.seconds < end)
            })
            .filter_map(|cue| {
                let start = cue.seconds - seconds * out_rate;
                let wait = (start - position) / out_rate;
                let error =
                    (phase(out, cue.seconds) - phase(incoming, drop) + 0.5).rem_euclid(1.) - 0.5;
                (wait >= 0.15 && wait <= seconds && error.abs() <= 0.02).then_some(BuildSwap {
                    outgoing_start: start,
                    outgoing_drop: cue.seconds,
                    seconds,
                })
            })
            .min_by(|a, b| a.outgoing_start.total_cmp(&b.outgoing_start))
    };
    find(&out.manual_cues).or_else(|| find(&out.cues))
}
/// For an automatically selected section/drop, start privately before the landing
/// so the new section arrives as the crossfade completes. Exact user fade time is retained.
pub fn landing(p: &Preparation, selected: f64, rate: f64, fade: f64) -> Option<Landing> {
    if !crate::mix_schedule::timing_reliable(p)
        || !crate::energy_profile::mixing_allowed(p)
        || p.manual_override
        || !p.manual_cues.is_empty()
        || ![selected, rate, fade].iter().all(|v| v.is_finite())
        || rate <= 0.
        || !(0.1..=120.).contains(&fade)
    {
        return None;
    }
    let end = p
        .cue_out
        .or_else(|| p.analysis.as_ref().map(|a| a.duration))?;
    let entry = selected - fade * rate;
    let first = crate::energy_profile::entry_bound(p);
    // Do not truncate pre-roll, run past the approved end, or fabricate a landing
    // when the source has no room for a complete introduction.
    if entry < first || selected >= end || selected < 0. || end - selected < 1. {
        return None;
    }
    Some(Landing {
        entry,
        landing: selected,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fade_finishes_before_the_outgoing_tail() {
        assert!((tail_fade(8., Some(2.)) - 1.85).abs() < 0.001);
        assert_eq!(tail_fade(3., Some(30.)), 3.);
        assert_eq!(tail_fade(3., None), 3.);
        assert_eq!(tail_fade(3., Some(f32::NAN)), 3.);
        assert_eq!(tail_fade(8., Some(0.1)), 0.1);
    }
    #[test]
    fn build_swap_lands_both_drops_and_preserves_cues() {
        let mut out = Preparation {
            bpm: Some(120.),
            beat_offset: Some(0.),
            cue_out: Some(120.),
            ..Default::default()
        };
        out.manual_cues.push(crate::preparation::Cue {
            seconds: 100.,
            kind: "drop".into(),
            ..Default::default()
        });
        let mut incoming = Preparation {
            bpm: Some(125.),
            beat_offset: Some(0.),
            cue_out: Some(100.),
            ..Default::default()
        };
        incoming.cues.push(crate::preparation::Cue {
            seconds: 38.4,
            kind: "drop".into(),
            ..Default::default()
        });
        let saved = out.clone();
        let rates = [1., 120. / 125.];
        let plan = build_swap(&out, &incoming, 90., 30.72, 38.4, rates).unwrap();
        assert_eq!(plan.outgoing_start, 92.);
        assert_eq!(plan.seconds, 8.);
        assert!((30.72 + plan.seconds * rates[1] - 38.4).abs() < 1e-9);
        assert_eq!(plan.outgoing_start + plan.seconds, plan.outgoing_drop);
        assert_eq!(out, saved);
        assert!(build_swap(&out, &incoming, 95., 30.72, 38.4, rates).is_none());
        assert!(build_swap(&out, &incoming, 20., 30.72, 38.4, rates).is_none());
        assert!(build_swap(&out, &incoming, 90., 35., 38.4, rates).is_none());
        assert!(build_swap(&out, &incoming, 90., 30.72, 38.4, [1., 1.]).is_none());
        out.manual_cues[0].kind = "energy-start".into();
        assert!(build_swap(&out, &incoming, 90., 30.72, 38.4, rates).is_none());
    }
    #[test]
    fn half_double_time_uses_small_pitch_changes_and_rejects_extreme_stretch() {
        assert_eq!(
            tempo_match(140., 1., 70., 0.16),
            Some(TempoMatch {
                rate: 1.,
                pulse_factor: 2.
            })
        );
        assert_eq!(
            tempo_match(70., 1., 140., 0.16),
            Some(TempoMatch {
                rate: 1.,
                pulse_factor: 0.5
            })
        );
        assert!((tempo_match(128., 1., 126., 0.16).unwrap().rate - 128. / 126.).abs() < 1e-10);
        assert!(tempo_match(140., 1., 100., 0.16).is_none());
        assert!(tempo_match(f64::NAN, 1., 120., 0.16).is_none());
        assert_eq!(tempo_distance(140., 70.), 0.);
        assert!(tempo_distance(140., 105.) > 0.2);
    }
    #[test]
    fn complete_preroll_lands_at_the_selected_drop_without_mutating_cues() {
        let mut p = Preparation {
            cue_in: 20.,
            cue_out: Some(90.),
            bpm: Some(120.),
            beat_offset: Some(0.),
            ..Default::default()
        };
        p.cues.push(crate::preparation::Cue {
            id: "auto-in".into(),
            seconds: 0.,
            ..Default::default()
        });
        let saved = p.clone();
        // Two three-second ramps plus sixteen beats at 120 BPM: full mix at the drop.
        let plan = landing(&p, 40., 1.02, 14.).unwrap();
        assert!(((40. - plan.entry) / 1.02 - 14.).abs() < 1e-9);
        assert_eq!(
            landing(&p, 20., 1., 3.),
            Some(Landing {
                entry: 17.,
                landing: 20.
            })
        );
        assert_eq!(
            landing(&p, 20., 1.1, 3.),
            Some(Landing {
                entry: 16.7,
                landing: 20.
            })
        );
        assert!(landing(&p, 2., 1., 3.).is_none());
        assert!(landing(&p, 89.5, 1., 3.).is_none());
        assert_eq!(p, saved);
        p.manual_override = true;
        assert!(landing(&p, 20., 1., 3.).is_none());
        p.manual_override = false;
        p.extensions
            .insert("rhythm_quality".into(), serde_json::json!({"stable":false}));
        assert!(landing(&p, 20., 1., 3.).is_none());
    }
}
