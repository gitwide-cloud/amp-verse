//! Sustained relative-energy cue estimates. Not semantic section recognition.
use super::{BLOCK, RATE};
/// Sustained relative energy falls, kept separate from high-energy mix-in cues.
pub(super) fn breakdowns(energy: &[f64], first: usize, last: usize) -> Vec<f64> {
    let bin = (RATE / BLOCK as f64).round() as usize;
    let bins: Vec<f64> = energy
        .chunks(bin)
        .map(|c| c.iter().sum::<f64>() / c.len() as f64)
        .collect();
    let max = bins.iter().copied().fold(0., f64::max);
    let mut result = Vec::new();
    for i in 4..bins.len().saturating_sub(4) {
        let before = bins[i - 4..i].iter().sum::<f64>() / 4.;
        let after = bins[i..i + 4].iter().sum::<f64>() / 4.;
        let at = i * bin;
        if at < first || at + 4 * bin > last || before < max * 0.65 || after > before * 0.45 {
            continue;
        }
        let seconds = at as f64 * BLOCK as f64 / RATE;
        if result
            .last()
            .is_none_or(|previous| seconds - previous >= 12.)
        {
            result.push(seconds);
        }
        if result.len() == 3 {
            break;
        }
    }
    result
}
pub(super) fn candidates(energy: &[f64], first: usize, last: usize) -> Vec<(f64, &'static str)> {
    let bin = (RATE / BLOCK as f64 / 2.).round() as usize;
    let bins: Vec<f64> = energy
        .chunks(bin)
        .map(|chunk| (chunk.iter().map(|v| v * v).sum::<f64>() / chunk.len() as f64).sqrt())
        .collect();
    if bins.len() < 4 {
        return Vec::new();
    }
    // A brief loud section still matters in a long track. Compare against the
    // strongest sustained two-second window, rather than a whole-song percentile.
    let reference = bins
        .windows(4)
        .map(|w| w.iter().sum::<f64>() / 4.)
        .fold(0., f64::max);
    let high = reference * 0.72;
    if high < 0.002 {
        return Vec::new();
    }
    let mean = |a: usize, b: usize| bins[a..b].iter().sum::<f64>() / (b - a) as f64;
    let mut cues = Vec::new();
    for i in 0..=bins.len() - 4 {
        let at = i * bin;
        if at + 4 * bin > last + 1 || at + bin < first {
            continue;
        }
        let after = mean(i, i + 4);
        if after < high || bins[i..i + 4].iter().filter(|v| **v >= high * 0.85).count() < 3 {
            continue;
        }
        let opening = at <= first + 3 * bin;
        let drop = i >= 4 && after >= mean(i.saturating_sub(8), i).max(0.0001) * 1.65;
        if !opening && !drop {
            continue;
        }
        // Refine the half-second envelope to the first sustained crossing, so an
        // isolated click ahead of a drop is not selected as the mix-in transient.
        let start = at.saturating_sub(bin).max(first);
        let end = (at + bin).min(last + 1);
        let hold = 20usize;
        let crossing = (start..end)
            .find(|&j| {
                j + hold <= energy.len()
                    && energy[j..j + hold]
                        .iter()
                        .filter(|v| **v >= high * 0.7)
                        .count()
                        >= hold * 3 / 4
            })
            .unwrap_or(at.max(first));
        let seconds = crossing as f64 * BLOCK as f64 / RATE;
        if cues
            .last()
            .is_some_and(|(previous, _)| seconds - previous < 8.)
        {
            continue;
        }
        cues.push((
            seconds,
            if opening {
                "High-energy beginning"
            } else {
                "High-energy drop"
            },
        ));
        if cues.len() == 16 {
            break;
        }
    }
    cues
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sustained_breakdown_not_a_single_quiet_frame() {
        let mut e = vec![0.8; 6000];
        e[500] = 0.;
        e[2000..3000].fill(0.1);
        let points = breakdowns(&e, 0, 5999);
        assert_eq!(points.len(), 1);
        assert!((points[0] - 20.).abs() < 1.1);
    }
    #[test]
    fn sustained_drop_beats_quiet_intro_and_single_click() {
        let mut e = vec![0.05; 6000];
        e[300] = 1.;
        e[2000..4000].fill(0.8);
        let c = candidates(&e, 0, 5999);
        assert_eq!(c[0].1, "High-energy drop");
        assert!((c[0].0 - 20.).abs() < 0.3, "{c:?}");
    }
    #[test]
    fn strong_opening_and_later_drop_are_both_retained() {
        let mut e = vec![0.05; 6000];
        e[..1000].fill(0.8);
        e[3000..5000].fill(0.8);
        let c = candidates(&e, 0, 5999);
        assert_eq!(c[0].1, "High-energy beginning");
        assert!(c[0].0 < 0.1);
        assert!(
            c.iter()
                .any(|(s, label)| *s > 29. && *label == "High-energy drop")
        );
    }
    #[test]
    fn isolated_spike_and_short_or_silent_audio_do_not_make_drop() {
        let mut e = vec![0.01; 3000];
        e[800] = 1.;
        let c = candidates(&e, 0, 2999);
        assert!(c.iter().all(|(_, label)| *label != "High-energy drop"));
        assert!(candidates(&[0.; 3000], 0, 2999).is_empty());
        assert!(candidates(&[0.8; 30], 0, 29).is_empty());
    }
}
