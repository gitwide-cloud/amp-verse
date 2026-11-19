//! Stereo-linked lookahead bus protection. All storage is allocated at construction.
//! A 4x windowed-sinc detector plus a -1.5 dBFS guard leaves reconstruction headroom; acceptance must
//! additionally measure rendered audio with an oversampled true-peak meter.
pub struct LookaheadLimiter {
    history: [[f32; 2]; 16],
    history_at: usize,
    interpolation: [[f32; 16]; 4],
    delay: Vec<[f32; 2]>,
    peaks: Vec<(u64, f32)>,
    head: usize,
    count: usize,
    frame: u64,
    gain: f32,
    release: f32,
}
impl LookaheadLimiter {
    pub fn new(sample_rate: u32) -> Self {
        let length = ((sample_rate as usize * 5) / 1000).max(1);
        let interpolation = std::array::from_fn(|phase| {
            let mut coefficients = std::array::from_fn(|k| {
                let x = k as f64 - (7. + phase as f64 / 4.);
                let sinc = if x.abs() < 1e-12 {
                    1.
                } else {
                    (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
                };
                (sinc * (0.5 + 0.5 * (std::f64::consts::PI * x / 8.).cos())) as f32
            });
            let sum: f32 = coefficients.iter().sum();
            for value in &mut coefficients {
                *value /= sum;
            }
            coefficients
        });
        Self {
            history: [[0.; 2]; 16],
            history_at: 0,
            interpolation,
            delay: vec![[0.; 2]; length],
            peaks: vec![(0, 0.); length + 2],
            head: 0,
            count: 0,
            frame: 0,
            gain: 1.,
            release: 1. - (-1. / (sample_rate.max(1) as f32 * 0.060)).exp(),
        }
    }
    pub fn latency(&self) -> usize {
        self.delay.len()
    }
    /// No locks, allocation, I/O, or per-channel gain differences.
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let input = input.map(|v| if v.is_finite() { v } else { 0. });
        let mut peak = input[0].abs().max(input[1].abs());
        self.history[self.history_at] = input;
        self.history_at = (self.history_at + 1) % 16;
        for coefficients in &self.interpolation {
            let mut reconstructed = [0f32; 2];
            for (k, coefficient) in coefficients.iter().enumerate() {
                let frame = self.history[(self.history_at + k) % 16];
                reconstructed[0] += coefficient * frame[0];
                reconstructed[1] += coefficient * frame[1];
            }
            peak = peak.max(reconstructed[0].abs()).max(reconstructed[1].abs());
        }
        let oldest = self.frame.saturating_sub(self.delay.len() as u64);
        while self.count > 0 && self.peaks[self.head].0 < oldest {
            self.head = (self.head + 1) % self.peaks.len();
            self.count -= 1;
        }
        while self.count > 0 {
            let back = (self.head + self.count - 1) % self.peaks.len();
            if self.peaks[back].1 > peak {
                break;
            }
            self.count -= 1;
        }
        let at = (self.head + self.count) % self.peaks.len();
        self.peaks[at] = (self.frame, peak);
        self.count += 1;
        let target = (0.84139514 / self.peaks[self.head].1.max(1e-20)).min(1.);
        self.gain = (self.gain + (1. - self.gain) * self.release).min(target);
        let slot = self.frame as usize % self.delay.len();
        let output = self.delay[slot].map(|v| v * self.gain);
        self.delay[slot] = input;
        self.frame += 1;
        output
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hot_signal_is_linked_and_never_clipped() {
        let mut limiter = LookaheadLimiter::new(44100);
        let latency = limiter.latency();
        for i in 0..44100 {
            let x = 4. * (i as f32 * 0.12).sin();
            let out = limiter.process([x, x * 0.5]);
            assert!(out[0].abs() <= 0.8414);
            assert_eq!(out[1], out[0] * 0.5);
            if i < latency {
                assert_eq!(out, [0.; 2]);
            }
        }
    }
    #[test]
    fn invalid_input_does_not_poison_bus() {
        let mut limiter = LookaheadLimiter::new(1000);
        limiter.process([f32::NAN, f32::INFINITY]);
        for _ in 0..100 {
            assert!(limiter.process([0.1; 2]).iter().all(|v| v.is_finite()));
        }
    }
}
