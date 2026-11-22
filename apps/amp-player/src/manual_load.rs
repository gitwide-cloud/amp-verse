//! Runtime-only cue policy for explicit manual loads. Auto DJ keeps its own planned ranges.
pub(super) fn full_track(prep: &mut amp_core::Preparation) {
    if !prep.manual_override {
        prep.cue_in = 0.;
        prep.cue_out = None;
    }
}
pub(super) fn seek(prep: &mut amp_core::Preparation, cue: f64) -> Result<(), String> {
    if !cue.is_finite() || cue < 0. {
        return Err("Choose a valid cue position".into());
    }
    full_track(prep);
    let end = prep.cue_out.or(prep.analysis.as_ref().map(|a| a.duration));
    prep.cue_in = end.map_or(cue, |end| cue.min((end - 0.02).max(0.)));
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn automatic(start: f64, end: f64) -> amp_core::Preparation {
        amp_core::Preparation {
            cue_in: start,
            cue_out: Some(end),
            ..Default::default()
        }
    }
    #[test]
    fn reported_analysis_tail_ranges_do_not_crop_manual_decks_or_modify_metadata() {
        for (start, end) in [(132.0202687, 146.8362812), (195.395918, 211.858503)] {
            let saved = automatic(start, end);
            let before = saved.clone();
            let mut runtime = saved.clone();
            full_track(&mut runtime);
            assert_eq!(runtime.cue_in, 0.);
            assert_eq!(runtime.cue_out, None);
            assert_eq!(saved, before);
        }
    }
    #[test]
    fn hot_cues_do_not_turn_an_automatic_range_into_a_manual_crop() {
        let mut prep = automatic(195., 212.);
        prep.manual_cues.push(amp_core::preparation::Cue {
            id: "hot".into(),
            label: "Intro".into(),
            seconds: 12.,
            kind: "hot".into(),
            number: Some(1),
            extensions: Default::default(),
        });
        prep.cues = prep.manual_cues.clone();
        prep.cues[0].kind = "phrase".into();
        prep.cues[0].label = "Analysis phrase guidance".into();
        let cues = prep.manual_cues.clone();
        let guidance = prep.cues.clone();
        full_track(&mut prep);
        assert_eq!(prep.cue_in, 0.);
        assert_eq!(prep.cue_out, None);
        assert_eq!(prep.manual_cues, cues);
        assert_eq!(prep.cues, guidance);
    }
    #[test]
    fn explicit_manual_range_remains_approved() {
        let mut prep = automatic(12., 80.);
        prep.manual_override = true;
        let saved = prep.clone();
        full_track(&mut prep);
        assert_eq!(prep, saved);
    }
    #[test]
    fn explicit_seek_is_applied_after_automatic_crop_is_removed() {
        let mut prep = automatic(195., 212.);
        seek(&mut prep, 30.).unwrap();
        assert_eq!(prep.cue_in, 30.);
        assert_eq!(prep.cue_out, None);
    }
    #[test]
    fn explicit_seek_respects_manual_end_and_rejects_invalid_position_without_mutation() {
        let mut prep = automatic(12., 80.);
        prep.manual_override = true;
        seek(&mut prep, 90.).unwrap();
        assert_eq!(prep.cue_in, 79.98);
        assert_eq!(prep.cue_out, Some(80.));
        let before = prep.clone();
        assert!(seek(&mut prep, f64::NAN).is_err());
        assert_eq!(prep, before);
    }
}
