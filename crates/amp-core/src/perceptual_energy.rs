//! Rhythm activity measured on a -14 LUFS normalized envelope.
//! This is a perceptual proxy, not semantic genre or listener arousal prediction.
pub fn from_rms(rms: &[f64], frames_per_second: f64, integrated_lufs: f64) -> Option<f64> {
    if rms.len() < 2
        || !frames_per_second.is_finite()
        || frames_per_second <= 0.
        || !integrated_lufs.is_finite()
        || rms.iter().any(|v| !v.is_finite() || *v < 0.)
    {
        return None;
    }
    let gain = 10f64.powf((-14. - integrated_lufs) / 20.);
    let peak = rms
        .windows(2)
        .map(|w| ((w[1] - w[0]) * gain).max(0.))
        .fold(0., f64::max);
    let threshold = 0.04f64.max(peak * 0.25);
    let refractory = (0.12 * frames_per_second).ceil() as usize;
    let mut last = None;
    let mut hits = 0;
    let mut sum = 0.;
    for (i, w) in rms.windows(2).enumerate() {
        let flux = ((w[1] - w[0]) * gain).max(0.);
        sum += flux;
        if flux >= threshold && last.is_none_or(|last| i - last >= refractory) {
            hits += 1;
            last = Some(i);
        }
    }
    let density = hits as f64 / (rms.len() as f64 / frames_per_second);
    Some(
        (0.55 * (density / 3.5).min(1.) + 0.45 * (sum / (rms.len() - 1) as f64 / 0.06).min(1.))
            .clamp(0., 1.),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn f07_mastering_gain_does_not_change_rhythmic_energy() {
        let dance: Vec<_> = (0..1600)
            .map(|i| if i % 50 < 4 { 0.25 } else { 0.02 })
            .collect();
        let pad = vec![0.7; 1600];
        let e = from_rms(&dance, 100., -15.8).unwrap();
        assert!(e > from_rms(&pad, 100., -3.6).unwrap());
        let louder: Vec<_> = dance.iter().map(|x| x * 2.).collect();
        assert!((e - from_rms(&louder, 100., -15.8 + 20. * 2f64.log10()).unwrap()).abs() < 1e-12);
    }
}
