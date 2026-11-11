use amp_core::Preparation;
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
        self.anchor + ((position - self.anchor) / self.period).ceil() * self.period
    }
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
/// Source-time grid. Bar anchors are explicit user corrections, never inferred from beat phase.
pub fn grid(
    out: &Preparation,
    incoming: &Preparation,
    entry: f64,
    origin: f64,
    timing: Timing,
) -> Option<Grid> {
    if timing == Timing::Immediate {
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
#[cfg(test)]
mod tests {
    use super::*;
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
