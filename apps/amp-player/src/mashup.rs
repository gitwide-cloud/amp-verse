//! Phrase mashup policy and envelopes; operates on loaded snapshots, never saved stem defaults.
use amp_core::Preparation;
/// Conservative pairing: same/relative key, close effective tuning, and phase-capable tempo grids.
pub fn compatible(out: &Preparation, incoming: &Preparation, out_rate: f64, in_rate: f64) -> bool {
    let (Some(a), Some(b)) = (out.bpm, incoming.bpm) else {
        return false;
    };
    if !a.is_finite() || !b.is_finite() || a <= 0. || b <= 0. {
        return false;
    }
    if !out_rate.is_finite() || !in_rate.is_finite() || out_rate <= 0. || in_rate <= 0. {
        return false;
    }
    if (a * out_rate / (b * in_rate) - 1.).abs() > 0.005
        || (12. * (out_rate / in_rate).log2()).abs() > 0.2
    {
        return false;
    }
    let key = |p: &Preparation| {
        p.harmonic_key.as_ref().and_then(|k| {
            let n = k.get(..k.len().checked_sub(1)?)?.parse::<u8>().ok()?;
            let mode = k.chars().last()?;
            (n > 0 && n <= 12 && matches!(mode, 'A' | 'B')).then_some(n)
        })
    };
    key(out).zip(key(incoming)).is_some_and(|(a, b)| a == b)
}
/// During hold: outgoing vocals + incoming instruments/drums, with just one vocal source.
/// Returns bus levels and per-deck stem masks, independent of saved/live stem gains.
pub fn envelope(elapsed: u64, fade: u64, hold: u64, incoming: usize) -> ([f32; 2], [[f32; 3]; 2]) {
    let outgoing = 1 - incoming;
    let mut weights = [1.; 2];
    let mut masks = [[1.; 3]; 2];
    if elapsed < fade {
        let p = elapsed as f32 / fade.max(1) as f32;
        weights[incoming] = p;
        masks[outgoing] = [1., 1. - p, 1. - p];
        masks[incoming] = [0., 1., 1.];
    } else if elapsed < fade + hold {
        masks[outgoing] = [1., 0., 0.];
        masks[incoming] = [0., 1., 1.];
    } else {
        let p = ((elapsed - fade - hold) as f32 / fade.max(1) as f32).min(1.);
        weights[outgoing] = 1. - p;
        masks[outgoing] = [1., 0., 0.];
        masks[incoming] = [p, 1., 1.];
    }
    (weights, masks)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hold_has_one_vocal_and_restores_the_incoming_mix() {
        let (w, m) = envelope(150, 100, 200, 1);
        assert_eq!(w, [1., 1.]);
        assert_eq!(m, [[1., 0., 0.], [0., 1., 1.]]);
        let (w, m) = envelope(400, 100, 200, 1);
        assert_eq!(w, [0., 1.]);
        assert_eq!(m[1], [1.; 3]);
        for t in 0..400 {
            let (_, m) = envelope(t, 100, 200, 0);
            assert!(m.iter().flatten().all(|v| (0. ..=1.).contains(v)));
        }
    }
    #[test]
    fn unknown_keys_or_varispeed_detuning_do_not_qualify() {
        let a = Preparation {
            bpm: Some(120.),
            harmonic_key: Some("8A".into()),
            ..Default::default()
        };
        let mut b = a.clone();
        b.harmonic_key = Some("8B".into());
        assert!(compatible(&a, &b, 1., 1.));
        b.bpm = Some(118.);
        assert!(!compatible(&a, &b, 1., 120. / 118.));
        b.bpm = Some(120.);
        b.harmonic_key = None;
        assert!(!compatible(&a, &b, 1., 1.));
    }
}
