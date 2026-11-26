//! Lazy separation with a timestamp-aligned dry path. No audio files are written.
use super::*;
use std::sync::mpsc;
type Infer = Arc<dyn Fn(Vec<f32>, Arc<AtomicBool>) -> Result<Parts, String> + Send + Sync>;

pub(super) fn infer(pcm: Vec<f32>, stop: Arc<AtomicBool>) -> Result<Parts, String> {
    crate::onnx_stem_worker::separate(pcm, stop, Duration::from_secs(60))
}
fn wanted(controls: &crate::mixer::Controls, deck: usize) -> bool {
    amp_core::stem_mix::changed(std::array::from_fn(|i| {
        f32::from_bits(controls.stem_knobs[deck][i].load(Relaxed))
    }))
}
struct Job {
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<Parts, String>>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Relaxed);
    }
}
struct Window<'a> {
    context: &'a Context,
    infer: Infer,
    job: Option<Job>,
    parts: Option<Parts>,
    attempted: bool,
    previous_tail: Option<Parts>,
    slot: Option<Arc<Admission>>,
}
impl Window<'_> {
    fn ready(&self, ready: bool) {
        let c = &self.context;
        if c.generation == 0
            || c.controls.deck_generation[c.deck].load(std::sync::atomic::Ordering::Acquire)
                == c.generation
        {
            c.controls.stem_processing_ready[c.deck].store(ready, Relaxed);
        }
    }
    fn update(&mut self, pcm: &[f32]) {
        let c = self.context;
        if c.stop.load(Relaxed) || !wanted(&c.controls, c.deck) {
            self.job = None; // Cancel in-flight inference without stopping the decoder.
            self.slot = None;
            self.attempted = false;
            self.ready(false);
            return;
        }
        if !self.attempted && self.parts.is_none() {
            self.attempted = true;
            if self.slot.is_none() {
                let policy = crate::stem_capability::current();
                if crate::stems::playlist_preparing() || !reserve(&ACTIVE, policy.limit) {
                    warn(
                        &c.controls,
                        c.deck,
                        c.generation,
                        format!("{} Original audio continues.", policy.error()),
                    );
                    return;
                }
                self.slot = Some(Arc::new(Admission));
            }
            let cancel = Arc::new(AtomicBool::new(false));
            let stop = cancel.clone();
            let (tx, result) = mpsc::channel();
            let infer = self.infer.clone();
            let pcm = pcm.to_vec();
            let slot = self.slot.clone();
            std::thread::spawn(move || {
                let _slot = slot; // Hold the slot until cancellation is acknowledged.
                let _ = tx.send(infer(pcm, stop));
            });
            self.job = Some(Job { cancel, result });
        }
        if let Some(job) = &self.job {
            match job.result.try_recv() {
                Ok(Ok(mut parts))
                    if parts
                        .iter()
                        .all(|p| p.len() == pcm.len() && p.iter().all(|v| v.is_finite())) =>
                {
                    if let Some(previous) = &self.previous_tail {
                        blend(previous, &mut parts);
                    }
                    self.parts = Some(parts);
                    self.job = None;
                }
                Ok(Err(error)) => {
                    warn(
                        &c.controls,
                        c.deck,
                        c.generation,
                        format!("Stems warming failed; original audio continues: {error}"),
                    );
                    self.job = None;
                }
                Ok(Ok(_)) | Err(mpsc::TryRecvError::Disconnected) => {
                    warn(
                        &c.controls,
                        c.deck,
                        c.generation,
                        "Invalid stem result; original audio continues".into(),
                    );
                    self.job = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        self.ready(self.parts.is_some());
    }
}

pub(super) fn stream(
    input: &mut impl Read,
    rate: u32,
    producer: &mut Producer<[f32; 8]>,
    context: Context,
    infer: Infer,
) -> Result<(), String> {
    let mut carry = Vec::new();
    let mut input_frame = 0u64;
    let mut output_frame = 0u64;
    let mut previous_tail = None;
    let mut slot = None;
    let mut mix = 0f32;
    let ramp = 1. / (rate as f32 * 0.05).max(1.);
    loop {
        if context.stop.load(Relaxed) || producer.is_abandoned() {
            return Ok(());
        }
        let first = carry.is_empty();
        let new = read_pcm(input, if first { WINDOW } else { WINDOW - OVERLAP })?;
        let last = new.len() / 2 < if first { WINDOW } else { WINDOW - OVERLAP };
        let mut pcm = std::mem::take(&mut carry);
        pcm.extend(new);
        if pcm.is_empty() {
            break;
        }
        let frames = pcm.len() / 2;
        let emit = if last { frames } else { frames - OVERLAP };
        if !last {
            carry = pcm[emit * 2..].to_vec();
        }
        let end = input_frame + emit as u64;
        let mut window = Window {
            context: &context,
            infer: infer.clone(),
            job: None,
            parts: None,
            attempted: false,
            previous_tail: previous_tail.take(),
            slot: slot.take(),
        };
        window.update(&pcm);
        while output_frame as f64 * RATE as f64 / (rate as f64) < end as f64 {
            if context.stop.load(Relaxed) || producer.is_abandoned() {
                return Ok(());
            }
            if output_frame.is_multiple_of(128) {
                window.update(&pcm);
            }
            // Bound neutral read-ahead so a first knob movement does not sit behind
            // seconds of queued dry audio. Active processing has a four-second runway.
            loop {
                let current = context.controls.deck_generation[context.deck]
                    .load(std::sync::atomic::Ordering::Acquire);
                let consumed = if current == context.generation || context.generation == 0 {
                    context.controls.position[context.deck].load(Relaxed)
                } else {
                    0
                };
                let lead = u64::from(rate)
                    * if wanted(&context.controls, context.deck) {
                        4
                    } else {
                        1
                    }
                    / if wanted(&context.controls, context.deck) {
                        1
                    } else {
                        2
                    };
                if output_frame <= consumed.saturating_add(lead) {
                    break;
                }
                if context.stop.load(Relaxed) || producer.is_abandoned() {
                    return Ok(());
                }
                window.update(&pcm);
                std::thread::sleep(Duration::from_millis(2));
            }
            let local =
                (output_frame as f64 * RATE as f64 / rate as f64 - input_frame as f64).max(0.);
            let a = (local.floor() as usize).min(frames - 1);
            let b = (a + 1).min(frames - 1);
            let fraction = local.fract() as f32;
            let active = window.parts.is_some() && wanted(&context.controls, context.deck);
            let target = if active { 1. } else { 0. };
            mix += (target - mix).clamp(-ramp, ramp);
            let sample = |pcm: &[f32], channel: usize| {
                pcm[a * 2 + channel] + (pcm[b * 2 + channel] - pcm[a * 2 + channel]) * fraction
            };
            let mut frame = [0.; 8];
            if let Some(parts) = &window.parts {
                for (channel, value) in frame[..6].iter_mut().enumerate() {
                    *value = sample(&parts[channel / 2], channel % 2) * mix;
                }
            }
            for (channel, value) in frame[6..].iter_mut().enumerate() {
                *value = sample(&pcm, channel) * (1. - mix);
            }
            loop {
                match producer.push(frame) {
                    Ok(()) => break,
                    Err(PushError::Full(value)) => {
                        frame = value;
                        if context.stop.load(Relaxed) || producer.is_abandoned() {
                            return Ok(());
                        }
                        window.update(&pcm);
                        std::thread::sleep(Duration::from_millis(2));
                    }
                }
            }
            output_frame += 1;
        }
        previous_tail = window
            .parts
            .as_ref()
            .filter(|_| !last)
            .map(|parts| std::array::from_fn(|i| parts[i][emit * 2..].to_vec()));
        slot = window.slot.take();
        input_frame = end;
        if last {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    fn pcm(seconds: usize) -> std::io::Cursor<Vec<u8>> {
        std::io::Cursor::new(
            (0..RATE * seconds * 2)
                .flat_map(|_| 0.5f32.to_le_bytes())
                .collect(),
        )
    }
    fn context(controls: Arc<crate::mixer::Controls>, stop: Arc<AtomicBool>) -> Context {
        controls.deck_generation[0].store(1, std::sync::atomic::Ordering::Release);
        Context {
            controls,
            stop,
            deck: 0,
            generation: 1,
            degraded_flag: Arc::new(AtomicBool::new(false)),
        }
    }
    #[test]
    fn neutral_lazy_stream_never_calls_separator_and_delivers_original_audio() {
        let controls = Arc::new(crate::mixer::Controls::default());
        for armed in &controls.on_demand_stems {
            armed.store(true, Relaxed);
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (mut producer, mut consumer) = rtrb::RingBuffer::new(RATE);
        let context = context(controls.clone(), stop);
        let task = std::thread::spawn(move || {
            stream(
                &mut pcm(1),
                RATE as u32,
                &mut producer,
                context,
                Arc::new(|_, _| panic!("neutral deck must not separate")),
            )
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut count = 0;
        while count < RATE && std::time::Instant::now() < deadline {
            if let Ok(frame) = consumer.pop() {
                assert_eq!(&frame[..6], &[0.; 6]);
                assert_eq!(&frame[6..], &[0.5; 2]);
                count += 1;
                controls.position[0].store(count as u64, Relaxed);
            } else {
                std::thread::yield_now();
            }
        }
        assert_eq!(count, RATE);
        assert!(task.join().unwrap().is_ok());
    }
    #[test]
    fn resetting_knobs_cancels_warming_inference_while_dry_audio_keeps_playing() {
        let controls = Arc::new(crate::mixer::Controls::default());
        controls.stem_knobs[0][0].store((-1f32).to_bits(), Relaxed);
        let stop = Arc::new(AtomicBool::new(false));
        let started = Arc::new(AtomicU64::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));
        let calls = started.clone();
        let finished = cancelled.clone();
        let infer: Infer = Arc::new(move |_, cancel| {
            calls.fetch_add(1, Relaxed);
            while !cancel.load(Relaxed) {
                std::thread::sleep(Duration::from_millis(1));
            }
            finished.store(true, Relaxed);
            Err("cancelled".into())
        });
        let (mut producer, mut consumer) = rtrb::RingBuffer::new(RATE * 5);
        let context = context(controls.clone(), stop.clone());
        let task = std::thread::spawn(move || {
            stream(&mut pcm(30), RATE as u32, &mut producer, context, infer)
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut count = 0;
        while (count < RATE / 4 || started.load(Relaxed) == 0)
            && std::time::Instant::now() < deadline
        {
            if let Ok(frame) = consumer.pop() {
                assert_eq!(&frame[..6], &[0.; 6]);
                assert_eq!(&frame[6..], &[0.5; 2]);
                count += 1;
                controls.position[0].store(count as u64, Relaxed);
            } else {
                std::thread::yield_now();
            }
        }
        controls.stem_knobs[0][0].store(0f32.to_bits(), Relaxed);
        while !cancelled.load(Relaxed) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        let acknowledged = cancelled.load(Relaxed);
        stop.store(true, Relaxed);
        assert!(task.join().unwrap().is_ok());
        assert!(
            acknowledged,
            "neutral reset must cancel the inference before stopping playback"
        );
        assert_eq!(started.load(Relaxed), 1);
    }
}

#[cfg(test)]
mod activation_tests {
    use super::*;
    #[test]
    fn moving_a_knob_activates_filters_without_stopping_original_playback() {
        let controls = Arc::new(crate::mixer::Controls::default());
        controls.deck_generation[0].store(1, std::sync::atomic::Ordering::Release);
        controls.on_demand_stems[0].store(true, Relaxed);
        let stop = Arc::new(AtomicBool::new(false));
        let context = Context {
            controls: controls.clone(),
            stop: stop.clone(),
            deck: 0,
            generation: 1,
            degraded_flag: Arc::new(AtomicBool::new(false)),
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let infer: Infer = Arc::new(move |pcm, _| {
            counted.fetch_add(1, Relaxed);
            Ok([
                pcm.iter().map(|v| v * 0.5).collect(),
                pcm.iter().map(|v| v * 0.3).collect(),
                pcm.iter().map(|v| v * 0.2).collect(),
            ])
        });
        let (mut producer, mut consumer) = rtrb::RingBuffer::new(RATE * 5);
        let task = std::thread::spawn(move || {
            let mut input = std::io::Cursor::new(
                (0..RATE * 30 * 2)
                    .flat_map(|_| 0.5f32.to_le_bytes())
                    .collect::<Vec<_>>(),
            );
            stream(&mut input, RATE as u32, &mut producer, context, infer)
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let mut frames = 0usize;
        let mut wet = 0;
        let mut reset = None;
        while std::time::Instant::now() < deadline {
            if let Ok(frame) = consumer.pop() {
                let muted = frames >= RATE / 4 && reset.is_none();
                let left = frame[0] * if muted { 0. } else { 1. } + frame[2] + frame[4] + frame[6];
                assert!(
                    (0.249..=0.501).contains(&left),
                    "No dropout while switching: {left}"
                );
                if reset.is_some() {
                    assert!(
                        (left - 0.5).abs() < 0.00001,
                        "Reset restores the original sum"
                    );
                }
                if muted && (left - 0.25).abs() < 0.00001 {
                    wet += 1;
                }
                frames += 1;
                controls.position[0].store(frames as u64, Relaxed);
                if frames == RATE / 4 {
                    assert_eq!(calls.load(Relaxed), 0, "Neutral playback does not separate");
                    controls.stem_knobs[0][0].store((-1f32).to_bits(), Relaxed);
                }
                if wet >= 256 && reset.is_none() {
                    controls.stem_knobs[0][0].store(0f32.to_bits(), Relaxed);
                    reset = Some(frames);
                }
                if reset.is_some_and(|at| frames > at + RATE / 4) {
                    break;
                }
            } else {
                std::thread::yield_now();
            }
        }
        stop.store(true, Relaxed);
        assert!(task.join().unwrap().is_ok());
        assert!(
            reset.is_some(),
            "A first knob movement must eventually produce filtered stems"
        );
        assert_eq!(calls.load(Relaxed), 1);
    }
}

#[cfg(test)]
mod live_acceptance {
    use super::*;
    #[test]
    #[ignore = "requires packaged real stem worker/model, ffmpeg, and AMP_STEM_TEST_SOURCE"]
    fn real_model_lazy_stems_start_filter_and_stop_at_neutral() {
        let source = std::path::PathBuf::from(
            std::env::var_os("AMP_STEM_TEST_SOURCE").expect("Set a read-only music fixture"),
        );
        assert!(crate::onnx_stem_worker::configured());
        let prep = amp_core::Preparation {
            cue_in: 1.,
            cue_out: Some(22.),
            ..Default::default()
        };
        let decoded = amp_core::command("ffmpeg")
            .args(["-v", "error"])
            .args(amp_core::cue_args(&prep).unwrap())
            .arg("-i")
            .arg(&source)
            .args(["-vn", "-ac", "2", "-ar", "44100", "-f", "f32le", "pipe:1"])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        let expected: Vec<f32> = decoded
            .stdout
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        assert!(expected.len() >= RATE * 18 * 2);
        let controls = Arc::new(crate::mixer::Controls::default());
        controls.deck_generation[0].store(1, std::sync::atomic::Ordering::Release);
        let stop = Arc::new(AtomicBool::new(false));
        let context = Context {
            controls: controls.clone(),
            stop: stop.clone(),
            deck: 0,
            generation: 1,
            degraded_flag: Arc::new(AtomicBool::new(false)),
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let actual: Infer = Arc::new(move |pcm, stop| {
            counted.fetch_add(1, Relaxed);
            infer(pcm, stop)
        });
        let (mut producer, mut consumer) = rtrb::RingBuffer::new(RATE * 5);
        let task = std::thread::spawn(move || {
            stream(
                &mut std::io::Cursor::new(decoded.stdout),
                RATE as u32,
                &mut producer,
                context,
                actual,
            )
        });
        let start = std::time::Instant::now();
        let mut count = 0usize;
        let mut wet = 0usize;
        let mut reset_calls = None;
        let mut worst_reset_error = 0f32;
        while count < RATE * 17 && start.elapsed() < Duration::from_secs(40) {
            if let Ok(frame) = consumer.pop() {
                assert!(frame.iter().all(|v| v.is_finite()));
                if count < RATE / 2 {
                    assert_eq!(frame[6], expected[count * 2]);
                    assert_eq!(frame[7], expected[count * 2 + 1]);
                }
                if (RATE..RATE * 14).contains(&count)
                    && frame[..6].iter().any(|v| v.abs() > 0.00001)
                {
                    wet += 1;
                }
                if count >= RATE * 14 {
                    for ch in 0..2 {
                        let restored = frame[ch] + frame[ch + 2] + frame[ch + 4] + frame[ch + 6];
                        worst_reset_error =
                            worst_reset_error.max((restored - expected[count * 2 + ch]).abs());
                    }
                }
                count += 1;
                controls.position[0].store(count as u64, Relaxed);
                if count == RATE / 2 {
                    assert_eq!(calls.load(Relaxed), 0);
                    controls.stem_knobs[0][0].store((-1f32).to_bits(), Relaxed);
                }
                if count == RATE * 14 {
                    controls.stem_knobs[0][0].store(0f32.to_bits(), Relaxed);
                    reset_calls = Some(calls.load(Relaxed));
                }
                if count.is_multiple_of(512) {
                    let due = Duration::from_secs_f64(count as f64 / RATE as f64);
                    if let Some(wait) = due.checked_sub(start.elapsed()) {
                        std::thread::sleep(wait);
                    }
                }
            } else {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        stop.store(true, Relaxed);
        assert!(task.join().unwrap().is_ok());
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "Original playback must not stall while separation warms"
        );
        assert_eq!(
            count,
            RATE * 17,
            "Playback must keep advancing while the real model warms"
        );
        assert!(
            wet > RATE / 4,
            "Real model must supply separated audio after a knob move"
        );
        assert!(reset_calls.is_some_and(|n| n > 0));
        assert_eq!(
            Some(calls.load(Relaxed)),
            reset_calls,
            "No additional inference after neutral reset"
        );
        assert!(
            worst_reset_error < 0.005,
            "Neutral restores original audio, error {worst_reset_error}"
        );
        println!(
            "Actual lazy stems: neutral bypass, real-model activation, neutral cancellation, 17sec uninterrupted source timeline"
        );
    }
}
