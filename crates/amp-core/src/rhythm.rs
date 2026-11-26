//! Constant-tempo onset consensus. Independent implementation; see ANALYSIS_RESEARCH.md.
pub(super) fn snap(seconds: f64, bpm: f64, offset: f64) -> f64 {
    let period = 60. / bpm;
    (offset + ((seconds - offset) / period).round() * period).max(0.)
}

pub(super) fn estimate(energy: &[f64], hz: f64) -> (Option<f64>, Option<f64>, f64) {
    if energy.len() < 500 {
        return (None, None, 0.);
    }
    let onset: Vec<_> = energy.windows(2).map(|w| (w[1] - w[0]).max(0.)).collect();
    let peak = onset.iter().copied().fold(0., f64::max);
    if peak < 1e-4 {
        return (None, None, 0.);
    }
    // Cap isolated transients so an opening click cannot dominate the whole grid.
    let mut strengths: Vec<_> = onset.iter().copied().filter(|v| *v > peak * 0.05).collect();
    strengths.sort_by(f64::total_cmp);
    let cap = strengths[strengths.len() / 2].max(peak * 0.1);
    let peaks: Vec<_> = (1..onset.len() - 1)
        .filter(|&i| onset[i] > cap * 0.2 && onset[i] >= onset[i - 1] && onset[i] > onset[i + 1])
        .map(|i| ((i + 1) as f64, onset[i].min(cap)))
        .collect();
    if peaks.len() < 6 {
        return (None, None, 0.);
    }
    let mut best = (0., 0., 0.);
    // Sub-frame tempo search; vote over the entire track, not the first transient.
    for step in 600..=1800 {
        let bpm = step as f64 / 10.;
        let period = hz * 60. / bpm;
        let mut bins = [0.; 64];
        for &(at, weight) in &peaks {
            let phase = (at / period).fract() * 64.;
            let bin = phase.round() as usize % 64;
            bins[bin] += weight;
        }
        for i in 0..64 {
            let score = (bins[(i + 63) % 64] + bins[i] + bins[(i + 1) % 64])
                * (1. - (bpm - 120.).abs() / 600.);
            if score > best.0 {
                best = (score, period, i as f64 * period / 64.);
            }
        }
    }
    let (_, period, phase) = best;
    // Robustly fit beat number -> onset time to remove quantization drift.
    let matched: Vec<_> = peaks
        .iter()
        .filter_map(|&(at, weight)| {
            let beat = ((at - phase) / period).round();
            ((at - phase - beat * period).abs() <= period * 0.08).then_some((beat, at, weight))
        })
        .collect();
    let weight: f64 = matched.iter().map(|v| v.2).sum();
    let total: f64 = peaks.iter().map(|v| v.1).sum();
    let confidence = weight / total;
    if matched.len() < 6 || confidence < 0.35 {
        return (None, None, confidence);
    }
    let mx = matched.iter().map(|v| v.0 * v.2).sum::<f64>() / weight;
    let my = matched.iter().map(|v| v.1 * v.2).sum::<f64>() / weight;
    let denominator = matched
        .iter()
        .map(|v| (v.0 - mx).powi(2) * v.2)
        .sum::<f64>();
    if denominator <= 0. {
        return (None, None, 0.);
    }
    let fitted = matched
        .iter()
        .map(|v| (v.0 - mx) * (v.1 - my) * v.2)
        .sum::<f64>()
        / denominator;
    if (fitted - period).abs() > period * 0.02 {
        return (None, None, 0.);
    }
    let offset = (my - fitted * mx).rem_euclid(fitted) / hz;
    (Some(hz * 60. / fitted), Some(offset), confidence)
}

/// Compare local onset phase across the recording. This is a drift diagnostic,
/// not a downbeat/section classifier. Ambiguous/sparse windows return unknown.
pub(super) fn stability(energy: &[f64], hz: f64, bpm: f64, offset: f64) -> serde_json::Value {
    let period = 60. / bpm;
    let width = (hz * period * 16.).round().max(1.) as usize;
    let mut phases = Vec::new();
    for (window, samples) in energy.chunks(width).enumerate() {
        if samples.len() < width / 2 {
            continue;
        }
        let mut x = 0.;
        let mut y = 0.;
        let mut total = 0.;
        let mut hits = 0;
        let peak = samples.iter().copied().fold(0., f64::max);
        for (i, pair) in samples.windows(2).enumerate() {
            let weight = (pair[1] - pair[0]).max(0.).min(peak * 0.5);
            if weight <= peak * 0.02 {
                continue;
            }
            let seconds = (window * width + i + 1) as f64 / hz;
            let angle = (seconds - offset) / period * std::f64::consts::TAU;
            x += angle.cos() * weight;
            y += angle.sin() * weight;
            total += weight;
            hits += 1;
        }
        if hits >= 6 && total > 1e-6 && x.hypot(y) / total >= 0.25 {
            phases.push(y.atan2(x));
        }
    }
    let mean = phases
        .iter()
        .map(|p| p.sin())
        .sum::<f64>()
        .atan2(phases.iter().map(|p| p.cos()).sum::<f64>());
    let drift = phases
        .iter()
        .map(|p| {
            ((p - mean + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
                - std::f64::consts::PI)
                .abs()
                / std::f64::consts::TAU
        })
        .fold(0., f64::max);
    serde_json::json!({"version":1,"windows":phases.len(),"max_phase_deviation_beats":drift,
        "stable":if phases.len()>=3 {Some(drift<=0.12)}else{None::<bool>},"downbeat_verified":false})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stability_distinguishes_steady_grid_from_sustained_drift_without_claiming_downbeats() {
        let mut steady = vec![0.; 12000];
        let mut drift = steady.clone();
        for i in 0..240 {
            let at = 20 + i * 50;
            if at < steady.len() {
                steady[at] = 1.;
            }
            let shifted = (20. + i as f64 * 50.8).round() as usize;
            if shifted < drift.len() {
                drift[shifted] = 1.;
            }
        }
        assert_eq!(stability(&steady, 100., 120., 0.2)["stable"], true);
        assert_eq!(stability(&drift, 100., 120., 0.2)["stable"], false);
        assert!(stability(&vec![0.; 12000], 100., 120., 0.)["stable"].is_null());
        assert_eq!(
            stability(&steady, 100., 120., 0.2)["downbeat_verified"],
            false
        );
    }
    #[test]
    fn fractional_tempo_and_phase_ignore_opening_click() {
        let hz = 11025. / 110.;
        for bpm in [87.3, 123.7, 156.2] {
            let mut e = vec![0.; (hz * 120.) as usize];
            e[2] = 5.;
            for beat in 0..(120. * bpm / 60.) as usize {
                let i = ((0.23 + beat as f64 * 60. / bpm) * hz).round() as usize;
                e[i] = 1.;
            }
            let (found, offset, confidence) = estimate(&e, hz);
            assert!((found.unwrap() - bpm).abs() < 0.05, "{bpm}: {found:?}");
            assert!((offset.unwrap() - 0.23).abs() < 0.02, "{offset:?}");
            assert!(confidence > 0.8);
        }
    }
    #[test]
    fn silence_and_single_hit_have_no_grid() {
        let mut e = vec![0.; 1000];
        assert_eq!(estimate(&e, 100.).0, None);
        e[123] = 1.;
        assert_eq!(estimate(&e, 100.).0, None);
        assert!((snap(20.13, 120., 0.2) - 20.2).abs() < 1e-8);
    }
}
