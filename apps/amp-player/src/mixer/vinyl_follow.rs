//! Event-paced source travel for a held record. No time stretching or second voice.
//! Independent implementation; no Mixxx source copied. Design comparison:
//! https://github.com/mixxxdj/mixxx/blob/main/src/engine/positionscratchcontroller.cpp
//! https://github.com/mixxxdj/mixxx/blob/main/src/widget/wspinnybase.cpp
#[derive(Default)]
pub(super) struct Follow {
    target: f64,
    elapsed: u32,
    remaining: u32,
}
impl Follow {
    pub(super) fn reset(&mut self, position: f64) {
        self.target = position;
        self.elapsed = 0;
        self.remaining = 0;
    }
    pub(super) fn rate(&mut self, target: f64, position: f64, sample_rate: u32) -> f32 {
        let nominal = (sample_rate / 60).max(1);
        let maximum = (sample_rate / 25).max(nominal);
        if target != self.target {
            // Traverse hand travel over its input interval instead of racing to
            // each mouse event at the maximum shuttle speed and then stopping.
            self.remaining = if self.elapsed == 0 || self.elapsed > maximum {
                nominal
            } else {
                self.elapsed.max(sample_rate / 250).max(1)
            };
            self.target = target;
            self.elapsed = 0;
        }
        self.elapsed = self.elapsed.saturating_add(1);
        let distance = self.target - position;
        if distance.abs() < 0.001 {
            self.remaining = 0;
            return 0.;
        }
        let rate = (distance / self.remaining.max(1) as f64).clamp(-4., 4.);
        self.remaining = self.remaining.saturating_sub(1);
        rate as f32
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hand_interval_is_traversed_smoothly_not_as_four_speed_bursts() {
        let mut follow = Follow::default();
        follow.reset(10000.);
        let mut position = 10000.;
        for _ in 0..800 {
            assert_eq!(follow.rate(position, position, 48000), 0.);
        }
        for _ in 0..800 {
            let rate = follow.rate(10800., position, 48000);
            assert!((rate - 1.).abs() < 0.0001);
            position += rate as f64;
        }
        assert!((position - 10800.).abs() < 0.001);
        for _ in 0..2400 {
            assert_eq!(follow.rate(10800., position, 48000), 0.);
        }
        for _ in 0..800 {
            let rate = follow.rate(10000., position, 48000);
            assert!((rate + 1.).abs() < 0.0001);
            position += rate as f64;
        }
        assert!((position - 10000.).abs() < 0.001);
    }
    #[test]
    fn six_independent_followers_hold_and_reverse_without_shared_state() {
        let mut follows: [Follow; 6] = std::array::from_fn(|_| Follow::default());
        let mut positions = [10000.; 6];
        for f in &mut follows {
            f.reset(10000.);
        }
        for n in 0..800 {
            for d in 0..6 {
                let target = 10000. + (d as f64 - 2.) * 160.;
                positions[d] += follows[d].rate(target, positions[d], 48000) as f64;
            }
            if n == 400 {
                follows[0].reset(positions[0]);
            }
        }
        for d in 1..6 {
            assert!((positions[d] - (10000. + (d as f64 - 2.) * 160.)).abs() < 0.002);
        }
        let held = positions[3];
        for _ in 0..4800 {
            assert_eq!(follows[3].rate(held, held, 48000), 0.);
        }
    }
}

#[cfg(test)]
mod codec_acceptance {
    use super::super::*;
    #[test]
    #[allow(clippy::needless_range_loop)] // Indexed per-deck control/source arrays.
    #[ignore = "requires AMP_VINYL_TEST_SOURCE real codec input and FFmpeg; no audio device or output files"]
    fn six_real_codec_decks_follow_short_hand_travel_without_shuttle_bursts() {
        let path = std::path::PathBuf::from(
            std::env::var_os("AMP_VINYL_TEST_SOURCE").expect("supply real music"),
        );
        let original = std::fs::read(&path).unwrap();
        let decoded = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(&path)
            .args([
                "-t", "4", "-f", "f32le", "-ac", "2", "-ar", "48000", "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        let pcm: Vec<f32> = decoded
            .stdout
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        assert!(pcm.len() >= 48000 * 4);
        assert!(pcm.iter().all(|x| x.is_finite()));
        assert!(pcm.iter().any(|x| x.abs() > 0.001));
        let c = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(16);
        let mut mixer = Mixer::new(48000, rx, c.clone());
        for d in 0..6 {
            let (mut producer, consumer) = RingBuffer::new(pcm.len() / 2 + 1);
            for pair in pcm.chunks_exact(2) {
                producer.push([pair[0], pair[1], 0., 0., 0., 0.]).unwrap();
            }
            let source = Source {
                key_lock: None,
                ring: consumer,
                duration: Arc::new(AtomicU64::new((pcm.len() / 2) as u64)),
                done: Arc::new(AtomicBool::new(true)),
                failed: Arc::new(AtomicBool::new(false)),
                history: VecDeque::with_capacity(48000 * 4),
                history_start: 0,
                position: 0.,
                primed: false,
                separated: false,
                stem_degraded: Arc::new(AtomicBool::new(false)),
                stem_knobs: [0.; 3],
            };
            tx.send(Command::Load(d, source, true, 1)).unwrap();
        }
        mixer.render(24000, |_, _| {});
        for d in 0..6 {
            let anchor = mixer.voices[d].source.as_ref().unwrap().position;
            c.vinyl_anchor[d].store(anchor.to_bits(), Relaxed);
            c.vinyl_target[d].store(anchor.to_bits(), Relaxed);
            c.vinyl_follow[d].store(true, Relaxed);
            c.scratching[d].store(true, Relaxed);
        }
        mixer.render(800, |_, _| {});
        let anchor: [f64; 6] =
            std::array::from_fn(|d| mixer.voices[d].source.as_ref().unwrap().position);
        for d in 0..6 {
            c.vinyl_target[d].store((anchor[d] + 800.).to_bits(), Relaxed);
        }
        mixer.render(400, |_, _| {});
        for d in 0..6 {
            assert!(
                (mixer.voices[d].source.as_ref().unwrap().position - anchor[d] - 400.).abs() < 1.,
                "deck {d} raced ahead of hand"
            );
        }
        mixer.render(400, |_, _| {});
        for d in 0..6 {
            c.vinyl_target[d].store(anchor[d].to_bits(), Relaxed);
        }
        mixer.render(800, |_, _| {});
        for d in 0..6 {
            assert!((mixer.voices[d].source.as_ref().unwrap().position - anchor[d]).abs() < 1.);
        }
        mixer.render(800, |_, _| {});
        c.scratching[0].store(false, Relaxed);
        c.vinyl_follow[0].store(false, Relaxed);
        mixer.render(800, |_, _| {});
        assert!((mixer.voices[0].source.as_ref().unwrap().position - anchor[0] - 800.).abs() < 1.);
        for d in 1..6 {
            assert!((mixer.voices[d].source.as_ref().unwrap().position - anchor[d]).abs() < 1.);
        }
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
}
