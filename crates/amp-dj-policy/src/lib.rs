//! Device-independent DJ decisions. No codecs, analysis, filesystem or audio devices.
mod transition;
pub use transition::*;
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TempoMatch {
    pub rate: f64,
    pub pulse_factor: f64,
}
pub fn tempo_match(a: f64, rate: f64, b: f64, limit: f64) -> Option<TempoMatch> {
    if ![a, rate, b, limit].iter().all(|v| v.is_finite())
        || a <= 0.
        || b <= 0.
        || rate <= 0.
        || !(0. ..=0.2).contains(&limit)
    {
        return None;
    }
    [1., 0.5, 2.]
        .into_iter()
        .filter_map(|pulse_factor| {
            let value = a * rate / (b * pulse_factor);
            (1. - limit..=1. + limit)
                .contains(&value)
                .then_some(TempoMatch {
                    rate: value,
                    pulse_factor,
                })
        })
        .min_by(|a, b| {
            (a.rate - 1.)
                .abs()
                .total_cmp(&(b.rate - 1.).abs())
                .then_with(|| (a.pulse_factor != 1.).cmp(&(b.pulse_factor != 1.)))
        })
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
pub fn next_boundary(anchor: f64, period: f64, position: f64) -> f64 {
    if ![anchor, period, position].iter().all(|v| v.is_finite()) || period <= 0. {
        return position;
    }
    anchor + ((position - anchor) / period).ceil() * period
}
pub fn tail_fade(requested: f64, remaining: f64) -> f64 {
    let requested = if requested.is_finite() {
        requested.clamp(0.1, 120.)
    } else {
        3.
    };
    if remaining.is_finite() {
        requested.min((remaining - 0.15).max(0.1))
    } else {
        requested
    }
}
pub fn harmonic(s: &str) -> Option<(i32, u8)> {
    let s = s.trim().to_ascii_uppercase();
    let mode = *s.as_bytes().last()?;
    if mode != b'A' && mode != b'B' {
        return None;
    }
    let n = s[..s.len() - 1].parse::<i32>().ok()?;
    (1..=12).contains(&n).then_some((n, mode))
}
pub fn pair_cost(a: f64, b: f64, ae: f64, be: f64, target: f64, ak: i32, bk: i32) -> f64 {
    let tempo = if a.is_finite() && b.is_finite() && a > 0. && b > 0. {
        (tempo_distance(a, b) * 12.).min(4.)
    } else {
        2.
    };
    let key = harmonic_cost(ak, bk, 1., 1., 1., 1., true, true);
    let groove =
        tempo_match(a, 1., b, 0.2).map_or(0., |m| if m.pulse_factor != 1. { 0.35 } else { 0. });
    let known = |e: f64| e.is_finite() && (0. ..=1.).contains(&e);
    tempo
        + groove
        + key
        + if known(ae) && known(be) {
            (ae - be).abs() * 4.
        } else {
            1.
        }
        + if known(target) && known(be) {
            (target - be).abs() * 6.
        } else {
            1.
        }
}
/// Effective Camelot key after varispeed; Key Lock leaves the analyzed key intact.
pub fn sounding_key(key: i32, rate: f64, locked: bool) -> i32 {
    if !(1..=24).contains(&key) || !rate.is_finite() || rate <= 0. {
        return 0;
    }
    let semitones = if locked {
        0
    } else {
        (12. * rate.log2()).round().clamp(-120., 120.) as i32
    };
    ((key - 1) % 12 + 7 * semitones).rem_euclid(12) + 1 + 12 * ((key - 1) / 12)
}
#[allow(clippy::too_many_arguments)]
pub fn harmonic_cost(
    a: i32,
    b: i32,
    ac: f64,
    bc: f64,
    ar: f64,
    br: f64,
    al: bool,
    bl: bool,
) -> f64 {
    let (a, b) = (sounding_key(a, ar, al), sounding_key(b, br, bl));
    if a == 0 || b == 0 {
        return 0.9;
    }
    let same = (a - 1) / 12 == (b - 1) / 12;
    let delta = ((a - 1) % 12 - (b - 1) % 12).abs();
    let d = delta.min(12 - delta);
    let raw = match (d, same) {
        (0, true) => 0.,
        (0, false) => 0.25,
        (1, true) => 0.35,
        (1, false) => 0.65,
        (2, true) => 0.95,
        _ => (1.4 + 0.25 * d as f64).min(2.9),
    };
    let confidence = if ac.is_finite() && bc.is_finite() {
        ac.min(bc).clamp(0., 1.)
    } else {
        0.
    };
    0.9 * (1. - confidence) + raw * confidence
}
#[allow(clippy::too_many_arguments)]
pub fn pair_cost_with_context(
    a: f64,
    b: f64,
    ae: f64,
    be: f64,
    target: f64,
    ak: i32,
    bk: i32,
    ac: f64,
    bc: f64,
    ar: f64,
    br: f64,
    al: bool,
    bl: bool,
) -> f64 {
    pair_cost(a * ar, b, ae, be, target, ak, bk) - harmonic_cost(ak, bk, 1., 1., 1., 1., true, true)
        + harmonic_cost(ak, bk, ac, bc, ar, br, al, bl)
}

pub fn key_code(s: &str) -> i32 {
    harmonic(s).map_or(0, |(n, m)| n + if m == b'B' { 12 } else { 0 })
}
// JNI primitive-only entry points: no Java pointers are dereferenced or retained.
#[cfg(target_os = "android")]
mod android {
    use super::*;
    use std::ffi::c_void;
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_cloud_gitwide_ampverse_SharedDjLogic_version(
        _: *mut c_void,
        _: *mut c_void,
    ) -> i32 {
        1
    }
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_cloud_gitwide_ampverse_SharedDjLogic_tempoRate(
        _: *mut c_void,
        _: *mut c_void,
        a: f64,
        r: f64,
        b: f64,
        l: f64,
    ) -> f64 {
        tempo_match(a, r, b, l).map_or(1., |m| m.rate)
    }
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_cloud_gitwide_ampverse_SharedDjLogic_distance(
        _: *mut c_void,
        _: *mut c_void,
        a: f64,
        b: f64,
    ) -> f64 {
        tempo_distance(a, b)
    }
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_cloud_gitwide_ampverse_SharedDjLogic_boundary(
        _: *mut c_void,
        _: *mut c_void,
        a: f64,
        p: f64,
        t: f64,
    ) -> f64 {
        next_boundary(a, p, t)
    }
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_cloud_gitwide_ampverse_SharedDjLogic_fade(
        _: *mut c_void,
        _: *mut c_void,
        s: f64,
        r: f64,
    ) -> f64 {
        tail_fade(s, r)
    }
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_cloud_gitwide_ampverse_SharedDjLogic_pairCost(
        _: *mut c_void,
        _: *mut c_void,
        a: f64,
        b: f64,
        ae: f64,
        be: f64,
        t: f64,
        ak: i32,
        bk: i32,
    ) -> f64 {
        pair_cost(a, b, ae, be, t, ak, bk)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_data_is_not_a_fake_match() {
        assert_eq!(tempo_match(0., 1., 128., 0.08), None);
        assert_eq!(tempo_match(f64::NAN, 1., 128., 0.08), None);
        assert_eq!(tempo_match(130., 1., 100., 0.08), None);
    }
    #[test]
    fn tempo_and_grid() {
        assert_eq!(tempo_match(128., 1., 64., 0.08).unwrap().rate, 1.);
        assert!((tempo_match(130., 1., 125., 0.08).unwrap().rate - 1.04).abs() < 1e-10);
        assert_eq!(next_boundary(0., 4., 5.), 8.);
        assert_eq!(tail_fade(3., 2.), 1.85);
    }
    #[test]
    fn prepared_neighbors_rank_first() {
        assert!(
            pair_cost(128., 128., 0.7, 0.7, 0.7, 8, 9) < pair_cost(128., 100., 0.7, 0.2, 0.7, 8, 3)
        );
        assert_eq!(key_code("8B"), 20);
        assert_eq!(key_code("unknown"), 0);
    }
}

/// Multiplicative session masks, in outgoing/incoming order. The drum exchange
/// begins on a beat boundary and lasts at most one beat. Vocals have a 7% gap.
pub fn stem_handoff(elapsed: u64, total: u64, beat: u64) -> [[f32; 3]; 2] {
    let p = elapsed as f32 / total.max(1) as f32;
    let ramp = |a: f32, b: f32| ((p - a) / (b - a)).clamp(0., 1.);
    let beat = beat.max(1).min(total.max(1));
    let start = ((total / 2) / beat) * beat;
    let drums = (elapsed.saturating_sub(start) as f32 / beat as f32).clamp(0., 1.);
    [
        [1. - ramp(0., 0.35), 1. - ramp(0.5, 1.), 1. - drums],
        [ramp(0.42, 0.70), ramp(0.45, 0.85), drums],
    ]
}
#[cfg(test)]
mod envelope_tests {
    #[test]
    fn f05_f06_vocals_exclusive_and_drum_swap_one_beat() {
        let mut silent = 0;
        for i in 0..4000 {
            let m = super::stem_handoff(i, 4000, 484);
            assert_eq!(m[0][0] * m[1][0], 0.);
            if m[0][0] == 0. && m[1][0] == 0. {
                silent += 1;
            }
            assert_eq!(m[1][0] * 0., 0.);
        }
        assert!(silent >= 200);
        let start = (2000 / 484) * 484;
        assert_eq!(super::stem_handoff(start, 4000, 484)[1][2], 0.);
        assert_eq!(super::stem_handoff(start + 484, 4000, 484)[0][2], 0.);
    }
}

#[cfg(test)]
mod harmonic_tests {
    use super::*;
    #[test]
    fn pair_cost_ranks_camelot_neighbors_and_invalid_keys() {
        // Isolate harmony by holding tempo and all energy inputs equal.
        let cost = |key| pair_cost(128., 128., 0.7, 0.7, 0.7, key_code("8A"), key);
        for (key, expected) in [
            ("8A", 0.),
            ("8B", 0.25),
            ("9A", 0.35),
            ("9B", 0.65),
            ("unknown", 0.9),
            ("10A", 0.95),
            ("2B", 2.9),
        ] {
            assert!((cost(key_code(key)) - expected).abs() < 1e-10, "{key}");
        }
        for invalid in [-1, 0, 25] {
            assert!((cost(invalid) - 0.9).abs() < 1e-10, "{invalid}");
        }
    }

    #[test]
    fn f04_confidence_and_sounding_key_are_soft_costs() {
        assert_eq!(harmonic_cost(8, 9, 1., 1., 1., 1., true, true), 0.35);
        assert_eq!(sounding_key(8, 1.06, false), 3);
        assert!(harmonic_cost(8, 8, 1., 1., 1.06, 1., false, true) >= 1.8);
        assert_eq!(harmonic_cost(8, 8, 1., 1., 1.06, 1., true, true), 0.);
        assert!(
            harmonic_cost(8, 3, 0.08, 1., 1., 1., true, true)
                < harmonic_cost(8, 3, 1., 1., 1., 1., true, true)
        );
    }
    #[test]
    fn f03_half_double_adds_soft_groove_cost() {
        let direct = pair_cost(140., 140., 0.5, 0.5, 0.5, 8, 8);
        assert!((pair_cost(140., 70., 0.5, 0.5, 0.5, 8, 8) - direct - 0.35).abs() < 1e-10);
    }
}
