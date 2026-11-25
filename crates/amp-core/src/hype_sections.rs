//! Hype section estimates from sustained relative activity, not semantic chorus recognition.
//! Pure planning over existing preparation; no audio, model, or catalog writes.
use crate::Preparation;

fn activity(p: &Preparation) -> Option<crate::energy_profile::Profile> {
    if p.manual_override
        || !p.manual_cues.is_empty()
        || p.analysis.as_ref().is_some_and(|a| a.reviewed)
        || !crate::energy_profile::mixing_allowed(p)
    {
        return None;
    }
    let profile = crate::energy_profile::read(p)?;
    if p.analysis.as_ref().is_some_and(|a| {
        !a.duration.is_finite() || (a.duration - profile.duration).abs() > profile.step
    }) {
        return None;
    }
    (profile.curve.len() >= 12).then_some(profile)
}
fn mean(curve: &[f64], first: usize, count: usize) -> Option<f64> {
    let values = curve.get(first..first.checked_add(count)?)?;
    Some(values.iter().sum::<f64>() / count as f64)
}
fn beats(p: &Preparation, count: f64, fallback: f64) -> f64 {
    if crate::mix_schedule::timing_reliable(p) {
        p.bpm.map_or(fallback, |b| count * 60. / b)
    } else {
        fallback
    }
}
/// Explicitly marked drops/hooks take priority over build-up labels.
pub fn marked_entry(p: &Preparation) -> Option<f64> {
    let end = p
        .cue_out
        .or_else(|| p.analysis.as_ref().map(|a| a.duration))
        .unwrap_or(f64::INFINITY);
    p.manual_cues
        .iter()
        .filter(|c| {
            let label = c.label.to_lowercase();
            let exit_marker = matches!(
                c.kind.as_str(),
                "drop_end" | "chorus_end" | "mix_out" | "section_end"
            ) || label
                .split_whitespace()
                .any(|word| matches!(word, "end" | "out" | "outro"));
            (!exit_marker
                && (matches!(c.kind.as_str(), "drop" | "hook" | "chorus" | "chorus_start")
                    || label.contains("drop")
                    || label.contains("chorus")
                    || label.contains("hook")))
                && c.seconds.is_finite()
                && c.seconds >= 0.
                && c.seconds < end
        })
        .min_by(|a, b| a.seconds.total_cmp(&b.seconds))
        .map(|c| c.seconds)
}
/// A substantial rise into a sustained strong section. Unlike ordinary safe-entry
/// policy, Hype may intentionally choose a drop after the first minute, but never
/// a loud outro with insufficient music left. A lone transient is not a drop.
pub fn entry(p: &Preparation) -> Option<f64> {
    let profile = activity(p)?;
    let first = crate::energy_profile::entry_bound(p);
    let end = p.cue_out.unwrap_or(profile.duration).min(profile.duration);
    let mut sorted = profile.curve.clone();
    sorted.sort_by(f64::total_cmp);
    let low = sorted[sorted.len() / 5];
    let high = sorted[sorted.len() * 4 / 5];
    if high - low < 0.08 {
        return None;
    }
    let threshold = low + (high - low) * 0.65;
    let before = (4. / profile.step).ceil().max(1.) as usize;
    let hold = (8. / profile.step).ceil().max(2.) as usize;
    let minimum = beats(p, 32., 16.).max(12.);
    let mut candidates = Vec::new();
    for i in before..profile.curve.len().saturating_sub(hold) {
        let at = i as f64 * profile.step;
        if at < first || at + minimum + 8. > end {
            continue;
        }
        let old = mean(&profile.curve, i - before, before)?;
        let next = mean(&profile.curve, i, hold)?;
        let strong = profile.curve[i..i + hold]
            .iter()
            .filter(|v| **v >= threshold)
            .count();
        if next >= threshold && next - old >= 0.06 && strong * 4 >= hold * 3 {
            candidates.push((at, 0.7 * next + 0.3 * (next - old)));
        }
    }
    let best = candidates
        .iter()
        .map(|(_, score)| *score)
        .max_by(f64::total_cmp)?;
    let at = candidates
        .into_iter()
        .find(|(_, score)| *score >= best * 0.9)?
        .0;
    // Keep a nearby saved landmark's sub-bin precision, but only after its
    // surrounding activity has independently passed the sustained-rise check.
    Some(
        p.cues
            .iter()
            .filter(|c| {
                c.kind == "drop"
                    && (c.seconds - at).abs() <= 2. * profile.step
                    && c.seconds >= first
                    && c.seconds + minimum + 8. <= end
            })
            .min_by(|a, b| (a.seconds - at).abs().total_cmp(&(b.seconds - at).abs()))
            .map_or(at, |c| c.seconds),
    )
}
/// First sustained energy fall after the selected drop has played. Short fills
/// are ignored. The handoff horizon is the lull onset; the outgoing transition
/// must leave a complete energetic passage before its own lead-in starts.
pub fn exit(p: &Preparation, transition: f64) -> Option<f64> {
    if !transition.is_finite() || transition < 0. {
        return None;
    }
    let minimum = beats(p, 32., 16.).max(12.);
    let bound = p
        .cue_out
        .or_else(|| p.analysis.as_ref().map(|a| a.duration))
        .unwrap_or(f64::INFINITY);
    let reviewed = p.analysis.as_ref().is_some_and(|a| a.reviewed);
    if let Some(mark) = p
        .manual_cues
        .iter()
        .chain(p.cues.iter().filter(|_| reviewed))
        .filter(|c| {
            matches!(c.kind.as_str(), "breakdown" | "lull")
                && c.seconds.is_finite()
                && c.seconds >= p.cue_in + minimum + transition
                && c.seconds < bound
        })
        .min_by(|a, b| a.seconds.total_cmp(&b.seconds))
    {
        return Some(mark.seconds);
    }
    let profile = activity(p)?;
    let end = p.cue_out.unwrap_or(profile.duration).min(profile.duration);
    let first = p.cue_in;
    let window = (8. / profile.step).ceil().max(2.) as usize;
    let start = (first / profile.step).ceil() as usize;
    let mut peak: f64 = 0.;
    for i in start..profile.curve.len().saturating_sub(window) {
        let next = mean(&profile.curve, i, window)?;
        let at = i as f64 * profile.step;
        if at + 2. > end {
            break;
        }
        if at >= first + minimum + transition && peak >= 0.35 && peak - next >= 0.10 {
            let weak = profile.curve[i..i + window]
                .iter()
                .filter(|v| **v <= peak - 0.08)
                .count();
            if weak * 4 >= window * 3 {
                return Some(at);
            }
        }
        peak = peak.max(next);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn song(curve: Vec<f64>) -> Preparation {
        let duration = curve.len() as f64 * 2.;
        let profile = crate::energy_profile::Profile {
            version: 1,
            duration,
            step: 2.,
            curve,
            sections: vec![],
        };
        Preparation {
            extensions: [(
                crate::energy_profile::FIELD.into(),
                serde_json::to_value(profile).unwrap(),
            )]
            .into(),
            ..Default::default()
        }
    }
    #[test]
    fn strong_drop_then_sustained_lull_drives_the_passage_not_a_timer() {
        let mut p = song(
            [
                vec![0.2; 45],
                vec![0.9; 24],
                vec![0.25; 12],
                vec![0.95; 30],
                vec![0.2; 12],
            ]
            .concat(),
        );
        let saved = p.clone();
        let at = entry(&p).unwrap();
        assert!((88. ..=92.).contains(&at), "{at}");
        p.cue_in = at;
        let end = exit(&p, 5.).unwrap();
        assert!((134. ..=140.).contains(&end), "{end}");
        p.cue_in = saved.cue_in;
        assert_eq!(p, saved);
    }
    #[test]
    fn loud_spike_and_short_fill_are_not_a_drop_or_lull() {
        let mut p = song(vec![0.5; 90]);
        p.extensions.get_mut(crate::energy_profile::FIELD).unwrap()["curve"][20] =
            serde_json::json!(1.);
        assert_eq!(entry(&p), None);
        let p = song([vec![0.9; 20], vec![0.2; 1], vec![0.9; 30]].concat());
        assert_eq!(exit(&p, 5.), None);
    }
    #[test]
    fn no_long_transition_can_consume_the_entire_strong_passage() {
        let mut p = song([vec![0.2; 10], vec![0.9; 20], vec![0.2; 10]].concat());
        p.cue_in = entry(&p).unwrap();
        assert!(exit(&p, 5.).is_some());
        assert_eq!(exit(&p, 60.), None);
        p.manual_override = true;
        assert_eq!(entry(&p), None);
        assert_eq!(exit(&p, 5.), None);
    }
    #[test]
    fn late_loud_outro_is_not_a_drop_entry() {
        let p = song([vec![0.2; 60], vec![0.95; 5]].concat());
        assert_eq!(entry(&p), None);
    }
    #[test]
    fn typed_drop_is_preferred_to_a_build_label() {
        let p = Preparation {
            manual_cues: vec![
                crate::preparation::Cue {
                    id: "build".into(),
                    label: "Build".into(),
                    kind: "build".into(),
                    seconds: 24.,
                    ..Default::default()
                },
                crate::preparation::Cue {
                    id: "drop".into(),
                    kind: "drop".into(),
                    seconds: 48.,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(marked_entry(&p), Some(48.));
    }
    #[test]
    fn reported_current_edm_pair_uses_structure_when_activity_supports_it() {
        let examples: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/current-edm-passages.json"))
                .unwrap();
        for example in examples.as_array().unwrap() {
            let mut p: Preparation =
                serde_json::from_value(example["preparation"].clone()).unwrap();
            let source = p.clone();
            let at = entry(&p).expect("reported pair has a sustained rise");
            assert!(at > 20. && at < 100., "{}: {at}", example["title"]);
            p.cue_in = at;
            let lull = exit(&p, 5.);
            if example["title"].as_str().unwrap().starts_with("Calabria") {
                assert!(lull.is_some_and(|t| (58. ..=70.).contains(&t)), "{lull:?}");
            }
            println!("{}: drop={at:.2} lull={lull:?}", example["title"]);
            p.cue_in = source.cue_in;
            assert_eq!(p, source);
        }
    }
    #[test]
    fn marked_lull_remains_authoritative_without_a_loudness_curve() {
        let mut p = Preparation {
            cue_in: 48.,
            ..Default::default()
        };
        p.manual_cues.push(crate::preparation::Cue {
            id: "lull".into(),
            kind: "breakdown".into(),
            seconds: 96.,
            ..Default::default()
        });
        assert_eq!(exit(&p, 5.), Some(96.));
        assert_eq!(exit(&p, 60.), None);
    }
    #[test]
    fn a_drop_end_marker_cannot_become_an_incoming_drop() {
        let p = Preparation {
            manual_cues: vec![crate::preparation::Cue {
                id: "out".into(),
                label: "Drop end".into(),
                kind: "drop_end".into(),
                seconds: 180.,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(marked_entry(&p), None);
    }
    #[test]
    fn stale_activity_duration_cannot_supply_a_drop_or_lull() {
        let mut p = song([vec![0.2; 20], vec![0.9; 20], vec![0.2; 20]].concat());
        p.analysis = Some(crate::preparation::AnalysisInfo {
            engine: "fixture".into(),
            duration: 20.,
            tempo_confidence: 0.,
            key_confidence: 0.,
            reviewed: false,
        });
        assert_eq!(entry(&p), None);
        assert_eq!(exit(&p, 5.), None);
    }
}
