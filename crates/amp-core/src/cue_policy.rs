//! Pure safety policy for automatically derived cue defaults. No audio or catalog I/O.
use crate::Preparation;

/// Latest safe automatic default entry. Later landmarks are optional DJ choices.
pub fn automatic_entry_limit(audio_start: f64, audio_end: f64) -> f64 {
    audio_start + ((audio_end - audio_start).max(0.) * 0.4).min(60.)
}
/// In-memory safety repair for old automatic records that defaulted to an outro.
/// No audio/catalog writes, identity changes, or optimistic-revision rewrites.
pub fn repair_legacy_entry(p: &mut Preparation) -> bool {
    if p.manual_override
        || !p.manual_cues.is_empty()
        || p.analysis.as_ref().is_none_or(|a| a.reviewed)
    {
        return false;
    }
    let Some(start) = p
        .cues
        .iter()
        .find(|c| c.id == "auto-in" && c.kind == "boundary")
        .map(|c| c.seconds)
    else {
        return false;
    };
    let duration = p.analysis.as_ref().unwrap().duration;
    let end = p.cue_out.unwrap_or(duration).min(duration);
    if !duration.is_finite()
        || !start.is_finite()
        || start < 0.
        || !end.is_finite()
        || end <= start
        || !p.cue_in.is_finite()
        || p.cue_in <= automatic_entry_limit(start, end)
    {
        return false;
    }
    let previous = p.cue_in;
    p.cue_in = start;
    for cue in &mut p.cues {
        if cue.id == "auto-mix-in" && cue.kind == "mix-in" {
            cue.seconds = start;
            cue.label = "Mix in · early audio fallback (legacy repair)".into();
        }
    }
    p.extensions.insert(
        "auto_cue_entry_repair".into(),
        serde_json::json!({
            "policy":"early-entry-v2","previous_cue_in":previous,
            "basis":"saved_audio_start_boundary","persisted":false
        }),
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preparation::{AnalysisInfo, Cue};
    fn legacy() -> Preparation {
        let mut p = Preparation {
            cue_in: 132.,
            cue_out: Some(146.8),
            analysis: Some(AnalysisInfo {
                engine: "fixture".into(),
                duration: 149.7,
                tempo_confidence: 0.,
                key_confidence: 0.,
                reviewed: false,
            }),
            ..Default::default()
        };
        p.cues = vec![
            Cue {
                id: "auto-in".into(),
                kind: "boundary".into(),
                seconds: 1.686,
                ..Default::default()
            },
            Cue {
                id: "auto-mix-in".into(),
                kind: "mix-in".into(),
                seconds: 132.,
                ..Default::default()
            },
            Cue {
                id: "late-drop".into(),
                kind: "drop".into(),
                seconds: 132.,
                ..Default::default()
            },
        ];
        p.extensions
            .insert("_amp_sync".into(), serde_json::json!({"revision":123}));
        p
    }
    #[test]
    fn bounds_scale_to_short_tracks_and_cap_long_intros() {
        assert_eq!(automatic_entry_limit(0., 5.), 2.);
        assert_eq!(automatic_entry_limit(2., 152.), 62.);
        assert_eq!(automatic_entry_limit(2., 602.), 62.);
    }
    #[test]
    fn derived_repair_keeps_late_landmarks_and_revision_and_is_idempotent() {
        let mut p = legacy();
        let original = p.clone();
        assert!(repair_legacy_entry(&mut p));
        assert_eq!(p.cue_in, 1.686);
        assert_eq!(p.cues[1].seconds, 1.686);
        assert_eq!(p.cues[2], original.cues[2]);
        assert_eq!(p.extensions["_amp_sync"], original.extensions["_amp_sync"]);
        assert_eq!(p.analysis, original.analysis);
        assert_eq!(p.source_sha256, original.source_sha256);
        assert!(!repair_legacy_entry(&mut p));
    }
    #[test]
    fn manual_reviewed_or_unidentified_boundaries_never_change() {
        for mode in 0..5 {
            let mut p = legacy();
            match mode {
                0 => p.manual_override = true,
                1 => p.analysis.as_mut().unwrap().reviewed = true,
                2 => p.manual_cues.push(Cue {
                    id: "mine".into(),
                    seconds: 132.,
                    ..Default::default()
                }),
                3 => {
                    p.cues.remove(0);
                }
                _ => {
                    p.analysis = None;
                }
            };
            let before = p.clone();
            assert!(!repair_legacy_entry(&mut p));
            assert_eq!(p.cue_in, before.cue_in);
            assert_eq!(p.cues, before.cues);
            assert_eq!(p.manual_cues, before.manual_cues);
            assert_eq!(p.extensions, before.extensions);
        }
    }
}
