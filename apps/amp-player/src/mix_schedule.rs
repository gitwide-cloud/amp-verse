use amp_core::Preparation;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Timing {
    Immediate,
    #[default]
    Beat,
    Bar,
}
impl Timing {
    pub fn label(self) -> &'static str {
        match self {
            Self::Immediate => "Immediately",
            Self::Beat => "Next beat",
            Self::Bar => "Next bar",
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
    let (anchor, in_anchor, multiple) = if timing == Timing::Bar
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
}
