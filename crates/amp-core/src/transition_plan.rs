//! Session-only pair planning. Never edits shared cues, grids or tempo metadata.
use crate::Preparation;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoMatch {
    pub rate: f64,
    pub pulse_factor: f64,
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
