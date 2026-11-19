//! Pure session decisions. Inputs are validated snapshots, never persisted edits.
use crate::{TempoMatch, tail_fade, tempo_match};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionTier {
    T0PhraseLocked,
    T1BarLocked,
    T2BeatLocked,
    T3TempoOnly,
    T4UnsyncedCleanFade,
    T5EmergencyTailRescue,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FallbackReason {
    UnverifiedPhraseAnchor,
    UnverifiedBarAnchor,
    LowTempoConfidence,
    RhythmDriftDetected,
    UnknownRhythmStability,
    MissingBeatPhase,
    HalfDoublePulseAmbiguous,
    ExcessiveTempoDelta,
    MissingStemsDegradedToBassSwap,
    ShortOutgoingTail,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestedTiming {
    Phrase,
    Bar,
    Beat,
    Tempo,
    Immediate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Handoff {
    StemSwap,
    BassSwap,
    Fade,
}
#[derive(Clone, Copy, Debug)]
pub struct TrackTiming {
    pub bpm: Option<f64>,
    pub tempo_confidence: f64,
    pub reviewed: bool,
    pub beat_offset: Option<f64>,
    /// None is a legacy sidecar; Some(None) is explicitly unknown stability.
    pub rhythm_stable: Option<Option<bool>>,
    pub rhythm_windows: u64,
    pub phrase_anchor: Option<f64>,
    pub bar_anchor: Option<f64>,
    pub stems_loaded: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TransitionDecision {
    pub tier: TransitionTier,
    pub tempo: Option<TempoMatch>,
    pub effective_fade: f64,
    pub handoff: Handoff,
    pub fallback_reasons: Vec<FallbackReason>,
}
fn anchor(value: Option<f64>) -> bool {
    value.is_some_and(|v| v.is_finite() && v >= 0.)
}
fn tempo_known(track: TrackTiming) -> bool {
    track
        .bpm
        .is_some_and(|b| b.is_finite() && (40. ..=300.).contains(&b))
        && (track.reviewed || (track.tempo_confidence.is_finite() && track.tempo_confidence >= 0.5))
}
fn grid_known(track: TrackTiming) -> bool {
    tempo_known(track)
        && anchor(track.beat_offset)
        && (track.reviewed
            || match track.rhythm_stable {
                Some(Some(true)) => track.rhythm_windows >= 3,
                None => track.tempo_confidence >= 0.65,
                _ => false,
            })
}
/// Called outside the audio callback: the trace Vec belongs to planning/UI state.
#[allow(clippy::too_many_arguments)]
pub fn plan_transition(
    outgoing: TrackTiming,
    incoming: TrackTiming,
    requested: RequestedTiming,
    handoff: Handoff,
    out_rate: f64,
    max_stretch: f64,
    fade: f64,
    remaining: f64,
) -> TransitionDecision {
    use FallbackReason::*;
    use TransitionTier::*;
    let tracks = [outgoing, incoming];
    let mut reasons = Vec::new();
    let tempo_ok = tracks.iter().all(|t| tempo_known(*t));
    let drift = tracks
        .iter()
        .any(|t| !t.reviewed && t.rhythm_stable == Some(Some(false)));
    let unknown = tracks.iter().any(|t| {
        !t.reviewed
            && (t.rhythm_stable == Some(None)
                || (t.rhythm_stable.is_some() && t.rhythm_windows < 3))
    });
    if !tempo_ok {
        reasons.push(LowTempoConfidence);
    }
    if drift {
        reasons.push(RhythmDriftDetected);
    }
    if unknown {
        reasons.push(UnknownRhythmStability);
    }
    let tempo = if tempo_ok && !drift && !unknown {
        tempo_match(
            outgoing.bpm.unwrap(),
            out_rate,
            incoming.bpm.unwrap(),
            max_stretch,
        )
    } else {
        None
    };
    if tempo_ok && !drift && !unknown && tempo.is_none() {
        reasons.push(ExcessiveTempoDelta);
    }
    let grids = tracks.iter().all(|t| grid_known(*t));
    let mut tier = if requested == RequestedTiming::Immediate || tempo.is_none() {
        T4UnsyncedCleanFade
    } else if requested == RequestedTiming::Tempo || !grids {
        if !grids {
            reasons.push(MissingBeatPhase);
        }
        T3TempoOnly
    } else {
        T2BeatLocked
    };
    if tier == T2BeatLocked && matches!(requested, RequestedTiming::Phrase | RequestedTiming::Bar) {
        if tempo.unwrap().pulse_factor != 1. {
            reasons.push(HalfDoublePulseAmbiguous);
        } else {
            if requested == RequestedTiming::Phrase {
                if tracks.iter().all(|t| anchor(t.phrase_anchor)) {
                    tier = T0PhraseLocked;
                } else {
                    reasons.push(UnverifiedPhraseAnchor);
                }
            }
            if tier != T0PhraseLocked {
                if tracks.iter().all(|t| anchor(t.bar_anchor)) {
                    tier = T1BarLocked;
                } else {
                    reasons.push(UnverifiedBarAnchor);
                }
            }
        }
    }
    let effective_fade = tail_fade(fade, remaining);
    if remaining.is_finite() && effective_fade < tail_fade(fade, f64::NAN) {
        tier = T5EmergencyTailRescue;
        reasons.push(ShortOutgoingTail);
    }
    let timed = matches!(tier, T0PhraseLocked | T1BarLocked | T2BeatLocked);
    let handoff = match handoff {
        Handoff::StemSwap if !tracks.iter().all(|t| t.stems_loaded) => {
            if timed {
                reasons.push(MissingStemsDegradedToBassSwap);
                Handoff::BassSwap
            } else {
                Handoff::Fade
            }
        }
        Handoff::StemSwap | Handoff::BassSwap if !timed => Handoff::Fade,
        other => other,
    };
    TransitionDecision {
        tier,
        tempo,
        effective_fade,
        handoff,
        fallback_reasons: reasons,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn track() -> TrackTiming {
        TrackTiming {
            bpm: Some(124.),
            tempo_confidence: 0.94,
            reviewed: false,
            beat_offset: Some(0.1),
            rhythm_stable: Some(Some(true)),
            rhythm_windows: 8,
            phrase_anchor: Some(0.1),
            bar_anchor: Some(0.1),
            stems_loaded: true,
        }
    }
    fn plan(a: TrackTiming, b: TrackTiming, tail: f64) -> TransitionDecision {
        plan_transition(
            a,
            b,
            RequestedTiming::Phrase,
            Handoff::StemSwap,
            1.,
            0.16,
            4.,
            tail,
        )
    }
    #[test]
    fn f01_f02_phrase_and_unreviewed_anchor_ladder() {
        assert_eq!(
            plan(track(), track(), 30.).tier,
            TransitionTier::T0PhraseLocked
        );
        let mut b = track();
        b.phrase_anchor = None;
        b.bar_anchor = None;
        let p = plan(track(), b, 30.);
        assert_eq!(p.tier, TransitionTier::T2BeatLocked);
        assert!(
            p.fallback_reasons
                .contains(&FallbackReason::UnverifiedPhraseAnchor)
        );
        b.bar_anchor = Some(0.1);
        assert_eq!(plan(track(), b, 30.).tier, TransitionTier::T1BarLocked);
    }
    #[test]
    fn f03_half_double_is_beat_only() {
        let mut a = track();
        a.bpm = Some(140.);
        let mut b = track();
        b.bpm = Some(70.);
        let p = plan(a, b, 30.);
        assert_eq!(p.tier, TransitionTier::T2BeatLocked);
        assert_eq!(
            p.tempo.unwrap(),
            TempoMatch {
                rate: 1.,
                pulse_factor: 2.
            }
        );
    }
    #[test]
    fn f06_stems_degrade_through_bass_swap() {
        let mut b = track();
        b.stems_loaded = false;
        assert_eq!(plan(track(), b, 30.).handoff, Handoff::BassSwap);
        b.bpm = None;
        assert_eq!(plan(track(), b, 30.).handoff, Handoff::Fade);
    }
    #[test]
    fn f08_f11_uncertain_grids_never_lock() {
        for rhythm in [Some(false), None] {
            let mut b = track();
            b.rhythm_stable = Some(rhythm);
            assert_eq!(
                plan(track(), b, 30.).tier,
                TransitionTier::T4UnsyncedCleanFade
            );
        }
        let mut b = track();
        b.tempo_confidence = 0.42;
        assert_eq!(
            plan(track(), b, 30.).tier,
            TransitionTier::T4UnsyncedCleanFade
        );
        b = track();
        b.beat_offset = None;
        assert_eq!(plan(track(), b, 30.).tier, TransitionTier::T3TempoOnly);
    }
    #[test]
    fn f12_short_tail_does_not_change_requested_setting() {
        let p = plan(track(), track(), 2.);
        assert_eq!(p.tier, TransitionTier::T5EmergencyTailRescue);
        assert_eq!(p.effective_fade, 1.85);
        assert_eq!(plan(track(), track(), 30.).effective_fade, 4.);
    }
}
