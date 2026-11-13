//! Device-independent DJ decisions. No codecs, analysis, filesystem or audio devices.
#[derive(Clone, Copy, Debug, PartialEq)]
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
    for pulse_factor in [1., 0.5, 2.] {
        let value = a * rate / (b * pulse_factor);
        if (1. - limit..=1. + limit).contains(&value) {
            return Some(TempoMatch {
                rate: value,
                pulse_factor,
            });
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
    let key = if (1..=24).contains(&ak) && (1..=24).contains(&bk) {
        let an = (ak - 1) % 12;
        let bn = (bk - 1) % 12;
        let same = (ak - 1) / 12 == (bk - 1) / 12;
        let d = (an - bn).abs();
        let d = d.min(12 - d);
        if d == 0 {
            if same { 0. } else { 0.25 }
        } else if d == 1 && same {
            0.4
        } else {
            2. + f64::from(d) * 0.2
        }
    } else {
        1.5
    };
    let known = |e: f64| e.is_finite() && (0. ..=1.).contains(&e);
    tempo
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
