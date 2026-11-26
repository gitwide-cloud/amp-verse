//! Reused FFT scratch and immutable precomputed coefficients for native key analysis.
//! Same estimator/math as AMP's original implementation; no upstream engine port.
use super::{FFT, RATE};
use std::sync::OnceLock;
struct Plan {
    window: Vec<f64>,
    swaps: Vec<(usize, usize)>,
    stages: Vec<Vec<(f64, f64)>>,
}
fn plan() -> &'static Plan {
    static PLAN: OnceLock<Plan> = OnceLock::new();
    PLAN.get_or_init(|| {
        let window = (0..FFT)
            .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (FFT - 1) as f64).cos())
            .collect();
        let mut swaps = Vec::new();
        let mut j = 0;
        for i in 1..FFT {
            let mut bit = FFT >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j ^= bit;
            if i < j {
                swaps.push((i, j));
            }
        }
        let mut stages = Vec::new();
        let mut size = 2;
        while size <= FFT {
            stages.push(
                (0..size / 2)
                    .map(|k| {
                        let angle = -std::f64::consts::TAU * k as f64 / size as f64;
                        (angle.cos(), angle.sin())
                    })
                    .collect(),
            );
            size *= 2;
        }
        Plan {
            window,
            swaps,
            stages,
        }
    })
}
pub(super) struct Analyzer {
    re: Vec<f64>,
    im: Vec<f64>,
    power: Vec<f64>,
}
impl Analyzer {
    pub(super) fn new() -> Self {
        let _ = plan();
        Self {
            re: vec![0.; FFT],
            im: vec![0.; FFT],
            power: vec![0.; FFT / 2],
        }
    }
    pub(super) fn accumulate(&mut self, samples: &[f32], sum: &mut [f64; 12]) {
        let p = plan();
        self.re.fill(0.);
        self.im.fill(0.);
        for (i, x) in samples.iter().take(FFT).enumerate() {
            self.re[i] = *x as f64 * p.window[i];
        }
        for &(i, j) in &p.swaps {
            self.re.swap(i, j);
            self.im.swap(i, j);
        }
        let mut size = 2;
        for stage in &p.stages {
            for start in (0..FFT).step_by(size) {
                for (k, &(c, s)) in stage.iter().enumerate() {
                    let a = start + k;
                    let b = a + size / 2;
                    let (tr, ti) = (
                        self.re[b] * c - self.im[b] * s,
                        self.re[b] * s + self.im[b] * c,
                    );
                    self.re[b] = self.re[a] - tr;
                    self.im[b] = self.im[a] - ti;
                    self.re[a] += tr;
                    self.im[a] += ti;
                }
            }
            size *= 2;
        }
        for (i, value) in self.power.iter_mut().enumerate() {
            *value = self.re[i] * self.re[i] + self.im[i] * self.im[i];
        }
        let peak = self.power.iter().copied().fold(0., f64::max);
        if peak < 1e-8 {
            return;
        }
        for k in 1..FFT / 2 - 1 {
            let left = self.power[k - 1].max(1e-20).ln();
            let center = self.power[k].max(1e-20).ln();
            let right = self.power[k + 1].max(1e-20).ln();
            let denominator = left - 2. * center + right;
            let shift = if denominator.abs() > 1e-12 {
                (0.5 * (left - right) / denominator).clamp(-0.5, 0.5)
            } else {
                0.
            };
            let frequency = (k as f64 + shift) * RATE / FFT as f64;
            if !(65. ..=2500.).contains(&frequency)
                || self.power[k] < peak * 0.025
                || self.power[k] < self.power[k - 1]
                || self.power[k] < self.power[k + 1]
            {
                continue;
            }
            let note = 69. + 12. * (frequency / 440.).log2();
            if (note - note.round()).abs() > 0.35 {
                continue;
            }
            let pc = (note.round() as i32).rem_euclid(12) as usize;
            sum[pc] += self.power[k].sqrt() / frequency.sqrt();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn reference_chroma(samples: &[f32], sum: &mut [f64; 12]) {
        let mut re = vec![0.; FFT];
        let mut im = vec![0.; FFT];
        for (i, x) in samples.iter().take(FFT).enumerate() {
            re[i] = *x as f64
                * (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (FFT - 1) as f64).cos());
        }
        let mut j = 0;
        for i in 1..FFT {
            let mut bit = FFT >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j ^= bit;
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut size = 2;
        while size <= FFT {
            for start in (0..FFT).step_by(size) {
                for k in 0..size / 2 {
                    let angle = -std::f64::consts::TAU * k as f64 / size as f64;
                    let (c, s) = (angle.cos(), angle.sin());
                    let a = start + k;
                    let b = a + size / 2;
                    let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            size *= 2;
        }
        let power: Vec<_> = (0..FFT / 2)
            .map(|i| re[i] * re[i] + im[i] * im[i])
            .collect();
        let peak = power.iter().copied().fold(0., f64::max);
        if peak < 1e-8 {
            return;
        }
        for k in 1..FFT / 2 - 1 {
            // Quadratic interpolation of log power reduces low-note FFT-bin bias.
            let left = power[k - 1].max(1e-20).ln();
            let center = power[k].max(1e-20).ln();
            let right = power[k + 1].max(1e-20).ln();
            let denominator = left - 2. * center + right;
            let shift = if denominator.abs() > 1e-12 {
                (0.5 * (left - right) / denominator).clamp(-0.5, 0.5)
            } else {
                0.
            };
            let frequency = (k as f64 + shift) * RATE / FFT as f64;
            if !(65. ..=2500.).contains(&frequency)
                || power[k] < peak * 0.025
                || power[k] < power[k - 1]
                || power[k] < power[k + 1]
            {
                continue;
            }
            let note = 69. + 12. * (frequency / 440.).log2();
            if (note - note.round()).abs() > 0.35 {
                continue;
            }
            let pc = (note.round() as i32).rem_euclid(12) as usize;
            sum[pc] += power[k].sqrt() / frequency.sqrt();
        }
    }
    fn fixtures() -> Vec<Vec<f32>> {
        let mut seed = 0x12345678u32;
        let noise: Vec<_> = (0..FFT)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                ((seed >> 8) as f32 / 16777216. - 0.5) * 0.2
            })
            .collect();
        let chord: Vec<_> = (0..FFT)
            .map(|i| {
                [130.8128, 164.8138, 195.9977, 261.6256]
                    .iter()
                    .map(|hz| (std::f64::consts::TAU * hz * i as f64 / RATE).sin() as f32 * 0.1)
                    .sum()
            })
            .collect();
        let mut impulse = vec![0.; FFT];
        impulse[512] = 0.8;
        vec![
            vec![0.; FFT],
            chord.clone(),
            noise,
            impulse,
            chord[..FFT / 3].to_vec(),
            chord.iter().map(|x| x * 0.00001).collect(),
            chord.repeat(2),
        ]
    }
    #[test]
    fn reused_scratch_is_bit_identical_to_original_for_mixed_frame_sequences() {
        let mut analyzer = Analyzer::new();
        let mut actual = [0.; 12];
        let mut expected = [0.; 12];
        for samples in fixtures().iter().cycle().take(30) {
            analyzer.accumulate(samples, &mut actual);
            reference_chroma(samples, &mut expected);
            assert_eq!(actual.map(f64::to_bits), expected.map(f64::to_bits));
        }
    }
    #[test]
    fn independent_workers_do_not_share_mutable_spectral_state() {
        let frames = fixtures();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let frames = &frames;
                scope.spawn(move || {
                    let mut analyzer = Analyzer::new();
                    let mut actual = [0.; 12];
                    let mut expected = [0.; 12];
                    for samples in frames {
                        analyzer.accumulate(samples, &mut actual);
                        reference_chroma(samples, &mut expected);
                    }
                    assert_eq!(actual.map(f64::to_bits), expected.map(f64::to_bits));
                });
            }
        });
    }
    #[test]
    #[ignore = "bounded synthetic throughput benchmark; no timing assertion on shared CI"]
    fn benchmark_original_and_reused_scratch() {
        let frames = fixtures();
        let input = &frames[1];
        let rounds = 1000;
        let mut old = [0.; 12];
        let started = std::time::Instant::now();
        for _ in 0..rounds {
            reference_chroma(std::hint::black_box(input), &mut old);
        }
        let old_time = started.elapsed();
        let mut analyzer = Analyzer::new();
        let mut new = [0.; 12];
        let started = std::time::Instant::now();
        for _ in 0..rounds {
            analyzer.accumulate(std::hint::black_box(input), &mut new);
        }
        let new_time = started.elapsed();
        assert_eq!(new.map(f64::to_bits), old.map(f64::to_bits));
        eprintln!(
            "chroma benchmark: frames={rounds} original_us={} reused_us={} speedup={:.2}x",
            old_time.as_micros(),
            new_time.as_micros(),
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
    }
}
