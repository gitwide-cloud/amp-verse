//! Deterministic preparation-based ordering shared by desktop and server consumers.
use crate::Preparation;
pub fn energy(p: &Preparation) -> Option<f64> {
    p.extensions
        .get("energy")
        .and_then(|v| v.as_f64())
        .filter(|v| v.is_finite() && (0. ..=1.).contains(v))
}
fn harmonic(s: &str) -> Option<(i32, u8)> {
    let s = s.trim().to_ascii_uppercase();
    let mode = *s.as_bytes().last()?;
    if mode != b'A' && mode != b'B' {
        return None;
    }
    let n = s[..s.len() - 1].parse::<i32>().ok()?;
    (1..=12).contains(&n).then_some((n, mode))
}
fn cost(a: &Preparation, b: &Preparation, target: Option<f64>) -> f64 {
    let tempo = match (a.bpm, b.bpm) {
        (Some(a), Some(b)) if a > 0. && b > 0. => ((a / b).ln().abs() * 12.).min(4.),
        _ => 2.,
    };
    let key = match (
        a.harmonic_key.as_deref().and_then(harmonic),
        b.harmonic_key.as_deref().and_then(harmonic),
    ) {
        (Some((a, m)), Some((b, n))) => {
            let d = (a - b).abs();
            let d = d.min(12 - d);
            if d == 0 {
                if m == n { 0. } else { 0.25 }
            } else if d == 1 && m == n {
                0.4
            } else {
                2. + d as f64 * 0.2
            }
        }
        _ => 1.5,
    };
    let continuity = match (energy(a), energy(b)) {
        (Some(a), Some(b)) => (a - b).abs() * 4.,
        _ => 1.,
    };
    let drift = match (target, energy(b)) {
        (Some(a), Some(b)) => (a - b).abs() * 6.,
        _ => 1.,
    };
    tempo + key + continuity + drift
}
/// Keeps the first song as the anchor. Returns an exact permutation, including repeats.
/// Greedy ordering is a recommendation; it is not beat synchronization or a guarantee of a mix.
pub fn order(songs: &[&Preparation]) -> Vec<usize> {
    if songs.is_empty() {
        return vec![];
    }
    let target = energy(songs[0]);
    let mut remaining: Vec<_> = (1..songs.len()).collect();
    let mut result = vec![0];
    while !remaining.is_empty() {
        let previous = songs[*result.last().unwrap()];
        let best = (0..remaining.len())
            .min_by(|&a, &b| {
                cost(previous, songs[remaining[a]], target).total_cmp(&cost(
                    previous,
                    songs[remaining[b]],
                    target,
                ))
            })
            .unwrap();
        result.push(remaining.remove(best));
    }
    result
}
/// Expands seed songs with prepared neighbors, preserving the first seed as anchor.
/// Unknown candidates are excluded rather than assigned fictional measurements.
pub fn curate(songs: &[&Preparation], seeds: &[usize], limit: usize) -> Result<Vec<usize>, String> {
    let &anchor = seeds
        .first()
        .ok_or("Add a seed song to the playlist first")?;
    if seeds.iter().any(|&i| i >= songs.len()) {
        return Err("Seed is outside the library".into());
    }
    let a = songs[anchor];
    let target =
        energy(a).ok_or("Analyze or set the first song's energy before finding similar songs")?;
    let bpm = a.bpm.ok_or("Analyze or set the first song's BPM")?;
    if a.harmonic_key.as_deref().and_then(harmonic).is_none() {
        return Err("Set a valid harmonic code (1A–12B) for the first song".into());
    }
    let mut chosen = Vec::new();
    for &seed in seeds {
        if !chosen.contains(&seed) {
            chosen.push(seed);
        }
    }
    while chosen.len() < limit.min(1000) {
        let previous = songs[*chosen.last().unwrap()];
        let candidate = (0..songs.len())
            .filter(|i| !chosen.contains(i))
            .filter(|&i| {
                let p = songs[i];
                let compatible = match (
                    previous.harmonic_key.as_deref().and_then(harmonic),
                    p.harmonic_key.as_deref().and_then(harmonic),
                ) {
                    (Some((a, m)), Some((b, n))) => {
                        let d = (a - b).abs();
                        let d = d.min(12 - d);
                        d == 0 || (d == 1 && m == n)
                    }
                    _ => false,
                };
                compatible
                    && energy(p).is_some_and(|e| (e - target).abs() <= 0.2)
                    && p.bpm.is_some_and(|b| (b / bpm - 1.).abs() <= 0.06)
                    && p.harmonic_key.as_deref().and_then(harmonic).is_some()
            })
            .min_by(|&i, &j| {
                cost(previous, songs[i], Some(target)).total_cmp(&cost(
                    previous,
                    songs[j],
                    Some(target),
                ))
            });
        let Some(i) = candidate else {
            break;
        };
        // Reject large harmonic jumps even when the rest of the library is incompatible.
        if cost(previous, songs[i], Some(target)) > 3. {
            break;
        }
        chosen.push(i);
    }
    let refs: Vec<_> = chosen.iter().map(|&i| songs[i]).collect();
    Ok(order(&refs).iter().map(|&i| chosen[i]).collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn song(bpm: f64, key: &str, e: f64) -> Preparation {
        let mut p = Preparation {
            bpm: Some(bpm),
            harmonic_key: Some(key.into()),
            ..Default::default()
        };
        p.extensions.insert("energy".into(), serde_json::json!(e));
        p
    }
    #[test]
    fn compatible_neighbors_win_and_anchor_energy_is_kept() {
        let a = song(124., "8A", 0.5);
        let bad = song(155., "3B", 0.9);
        let good = song(125., "9A", 0.52);
        let close = song(126., "9B", 0.54);
        assert_eq!(order(&[&a, &bad, &close, &good]), vec![0, 3, 2, 1]);
    }
    #[test]
    fn expansion_excludes_unknown_and_high_energy_candidates() {
        let a = song(124., "8A", 0.5);
        let b = song(125., "9A", 0.52);
        let high = song(124., "8A", 0.95);
        let unknown = Preparation::default();
        assert_eq!(
            curate(&[&a, &high, &unknown, &b], &[0], 20).unwrap(),
            vec![0, 3]
        );
        assert!(curate(&[&unknown], &[0], 20).is_err());
        assert!(curate(&[&a], &[9], 20).is_err());
    }
    #[test]
    fn unknowns_and_repeats_remain_stable_without_loss() {
        let p = Preparation::default();
        assert_eq!(order(&[]), Vec::<usize>::new());
        assert_eq!(order(&[&p, &p, &p]), vec![0, 1, 2]);
        assert_eq!(harmonic("12b"), Some((12, b'B')));
        assert_eq!(harmonic("13A"), None);
        assert_eq!(
            cost(&song(120., "12A", 0.5), &song(120., "1A", 0.5), Some(0.5)),
            0.4
        );
    }
}
