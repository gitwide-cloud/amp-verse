//! Read-only source decoding into bounded, overlapping in-memory stem windows.
use rtrb::{Producer, PushError};
use std::{
    io::Read,
    path::Path,
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    },
    time::Duration,
};
const RATE: usize = 44100;
const WINDOW: usize = RATE * 11;
const OVERLAP: usize = RATE / 2;
type Parts = [Vec<f32>; 3];
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
fn reserve(active: &AtomicUsize, limit: usize) -> bool {
    let n = active.load(Relaxed);
    n < limit && active.compare_exchange(n, n + 1, Relaxed, Relaxed).is_ok()
}
struct Admission;
impl Admission {
    fn acquire(stop: &AtomicBool) -> Option<Self> {
        let limit = crate::stem_capability::current().limit;
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !stop.load(Relaxed) && std::time::Instant::now() < deadline {
            if reserve(&ACTIVE, limit) {
                return Some(Self);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Relaxed);
    }
}
fn warn(controls: &crate::mixer::Controls, deck: usize, generation: u64, message: String) {
    if let Ok(mut messages) = controls.stem_warnings.lock() {
        messages.retain(|(d, _, _)| *d != deck);
        messages.push((deck, generation, message));
    }
}

struct DecoderGuard {
    child: Arc<Mutex<crate::receiver_process::Decoder>>,
    finished: Arc<AtomicBool>,
    watcher: Option<std::thread::JoinHandle<()>>,
}
impl Drop for DecoderGuard {
    fn drop(&mut self) {
        self.finished.store(true, Relaxed);
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(w) = self.watcher.take() {
            let _ = w.join();
        }
    }
}
fn read_pcm(reader: &mut impl Read, frames: usize) -> Result<Vec<f32>, String> {
    if frames > WINDOW {
        return Err("Stem input window exceeds budget".into());
    }
    let mut bytes = vec![0u8; frames * 8];
    let mut n = 0;
    while n < bytes.len() {
        let count = reader.read(&mut bytes[n..]).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        n += count;
    }
    if n % 8 != 0 {
        return Err("Incomplete stereo stem input".into());
    }
    let samples: Vec<_> = bytes[..n]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| f32::from_le_bytes(*v))
        .collect();
    if samples.iter().any(|v| !v.is_finite()) {
        return Err("Invalid stem input PCM".into());
    }
    Ok(samples)
}
fn blend(previous: &Parts, current: &mut Parts) {
    let samples = previous[0].len().min(current[0].len());
    let frames = samples / 2;
    for part in 0..3 {
        for frame in 0..frames {
            let a = if frames > 1 {
                frame as f32 / (frames - 1) as f32
            } else {
                1.
            };
            for ch in 0..2 {
                let i = frame * 2 + ch;
                current[part][i] = previous[part][i] * (1. - a) + current[part][i] * a;
            }
        }
    }
}
struct Output {
    input_frame: u64,
    output_frame: u64,
    rate: u32,
}
impl Output {
    fn inference_budget(
        &self,
        controls: &crate::mixer::Controls,
        deck: usize,
        generation: u64,
        first: bool,
    ) -> Duration {
        if first
            || !controls.play[deck].load(Relaxed)
            || controls.deck_generation[deck].load(Relaxed) != generation
        {
            return Duration::from_secs(20);
        }
        let speed = controls.effective_rate(deck).abs();
        let speed = if speed.is_finite() {
            f64::from(speed.max(0.01))
        } else {
            1.
        };
        let emitted = self.input_frame as f64 / RATE as f64;
        let consumed = controls.position[deck].load(Relaxed) as f64 / self.rate.max(1) as f64;
        // Allow time to publish the retained original before the playback ring empties.
        Duration::from_secs_f64(((emitted - consumed) / speed - 0.25).clamp(0.1, 20.))
    }
    fn emit(
        &mut self,
        parts: &Parts,
        frames: usize,
        producer: &mut Producer<[f32; 6]>,
        stop: &AtomicBool,
    ) -> Result<(), String> {
        let end = self.input_frame + frames as u64;
        while self.output_frame as f64 * RATE as f64 / (self.rate as f64) < end as f64 {
            if stop.load(Relaxed) || producer.is_abandoned() {
                return Err("Stem stream canceled".into());
            }
            let local = (self.output_frame as f64 * RATE as f64 / (self.rate as f64)
                - self.input_frame as f64)
                .max(0.);
            let a = (local.floor() as usize).min(frames.saturating_sub(1));
            let b = (a + 1).min(frames.saturating_sub(1));
            let fraction = local.fract() as f32;
            let mut value = std::array::from_fn(|ch| {
                let part = ch / 2;
                let channel = ch % 2;
                parts[part][a * 2 + channel]
                    + (parts[part][b * 2 + channel] - parts[part][a * 2 + channel]) * fraction
            });
            loop {
                match producer.push(value) {
                    Ok(()) => break,
                    Err(PushError::Full(v)) => {
                        value = v;
                        if stop.load(Relaxed) || producer.is_abandoned() {
                            return Err("Stem stream canceled".into());
                        }
                        std::thread::sleep(Duration::from_millis(2));
                    }
                }
            }
            self.output_frame += 1;
        }
        self.input_frame = end;
        Ok(())
    }
}
/// Called only on the decoder thread after explicit paused-deck activation.
/// Worker failure switches this track to the unchanged original signal.
pub(crate) struct Context {
    pub stop: Arc<AtomicBool>,
    pub controls: Arc<crate::mixer::Controls>,
    pub deck: usize,
    pub degraded_flag: Arc<AtomicBool>,
    pub generation: u64,
}
pub(crate) fn decode(
    path: &Path,
    prep: &amp_core::Preparation,
    rate: u32,
    producer: &mut Producer<[f32; 6]>,
    context: Context,
) -> Result<(), String> {
    let Context {
        stop,
        controls,
        deck,
        degraded_flag,
        generation,
    } = context;
    let mut command = amp_core::command("ffmpeg");
    command
        .args(["-v", "error"])
        .args(amp_core::cue_args(prep)?)
        .arg("-i")
        .arg(path)
        .args(["-vn", "-ac", "2", "-ar", "44100", "-f", "f32le", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = crate::receiver_process::spawn(&mut command).map_err(|e| e.to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Stem source decoder output missing")?;
    let child = Arc::new(Mutex::new(child));
    let finished = Arc::new(AtomicBool::new(false));
    let watching = child.clone();
    let done = finished.clone();
    let canceled = stop.clone();
    let watcher = std::thread::spawn(move || {
        while !done.load(Relaxed) {
            if canceled.load(Relaxed) {
                if let Ok(mut c) = watching.lock() {
                    let _ = c.kill();
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    });
    let _guard = DecoderGuard {
        child: child.clone(),
        finished,
        watcher: Some(watcher),
    };
    let mut input = std::io::BufReader::new(stdout);
    let mut carry = Vec::new();
    let mut tail: Option<Parts> = None;
    let mut output = Output {
        input_frame: 0,
        output_frame: 0,
        rate,
    };
    let _admission = Admission::acquire(&stop);
    let mut degraded = _admission.is_none();
    if degraded {
        degraded_flag.store(true, Relaxed);
        warn(
            &controls,
            deck,
            generation,
            crate::stem_capability::current().error(),
        );
    }
    loop {
        if stop.load(Relaxed) || producer.is_abandoned() {
            return Ok(());
        }
        let first = carry.is_empty();
        let new = read_pcm(&mut input, if first { WINDOW } else { WINDOW - OVERLAP })?;
        if new.is_empty() {
            if let Some(t) = tail.take() {
                output.emit(&t, t[0].len() / 2, producer, &stop)?;
            }
            break;
        }
        let mut pcm = std::mem::take(&mut carry);
        pcm.extend(new);
        let frames = pcm.len() / 2;
        let full = frames == WINDOW;
        if full {
            carry = pcm[(frames - OVERLAP) * 2..].to_vec();
        }
        // Keep original available until a complete validated result is returned.
        let dry = pcm.clone();
        let mut parts = if degraded {
            [pcm, vec![0.; frames * 2], vec![0.; frames * 2]]
        } else {
            let budget = output.inference_budget(&controls, deck, generation, first);
            match crate::onnx_stem_worker::separate(pcm, stop.clone(), budget) {
                Ok(parts) => parts,
                Err(error) => {
                    if stop.load(Relaxed) {
                        return Ok(());
                    }
                    degraded = true;
                    degraded_flag.store(true, Relaxed);
                    warn(
                        &controls,
                        deck,
                        generation,
                        format!("Stems unavailable; playing original audio: {error}"),
                    );
                    [dry, vec![0.; frames * 2], vec![0.; frames * 2]]
                }
            }
        };
        if let Some(previous) = tail.take() {
            blend(&previous, &mut parts);
        }
        let emit = if full { frames - OVERLAP } else { frames };
        output.emit(&parts, emit, producer, &stop)?;
        if full {
            tail = Some(std::array::from_fn(|p| parts[p][emit * 2..].to_vec()));
        } else {
            break;
        }
    }
    let status = child
        .lock()
        .map_err(|_| "Stem decoder lock unavailable")?
        .wait()
        .map_err(|e| e.to_string())?;
    if !status.success() && !stop.load(Relaxed) {
        return Err("Original audio decoder failed".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlap_preserves_original_mix_and_endpoints() {
        let old = [vec![0.2; 8], vec![0.3; 8], vec![0.5; 8]];
        let mut new = [vec![0.5; 8], vec![0.1; 8], vec![0.4; 8]];
        blend(&old, &mut new);
        for i in 0..8 {
            assert!((new.iter().map(|p| p[i]).sum::<f32>() - 1.).abs() < 1e-6);
        }
        assert_eq!(new[0][0], 0.2);
        assert_eq!(new[0][7], 0.5);
    }
    #[test]
    fn rejects_truncated_or_nonfinite_source_without_model() {
        assert!(read_pcm(&mut &b"123"[..], 2).is_err());
        let bytes = f32::NAN.to_le_bytes().repeat(2);
        assert!(read_pcm(&mut bytes.as_slice(), 1).is_err());
    }
    #[test]
    fn device_resampling_is_contiguous_across_windows() {
        let (mut p, mut c) = rtrb::RingBuffer::new(100);
        let stop = AtomicBool::new(false);
        let parts = [vec![0.2; 20], vec![0.3; 20], vec![0.5; 20]];
        let mut out = Output {
            input_frame: 0,
            output_frame: 0,
            rate: 48000,
        };
        out.emit(&parts, 10, &mut p, &stop).unwrap();
        out.emit(&parts, 10, &mut p, &stop).unwrap();
        let mut count = 0;
        while let Ok(v) = c.pop() {
            count += 1;
            assert!((v[0] + v[2] + v[4] - 1.).abs() < 1e-6);
        }
        assert_eq!(count, 22);
        assert_eq!(out.input_frame, 20);
    }
    #[test]
    fn inference_deadline_follows_buffer_tempo_and_source_generation() {
        let controls = crate::mixer::Controls::default();
        controls.play[0].store(true, Relaxed);
        controls.deck_generation[0].store(7, Relaxed);
        controls.position[0].store(48000 * 2, Relaxed);
        let out = Output {
            input_frame: (RATE * 10) as u64,
            output_frame: 0,
            rate: 48000,
        };
        assert_eq!(
            out.inference_budget(&controls, 0, 7, false),
            Duration::from_secs_f64(7.75)
        );
        controls.rate[0].store(2f32.to_bits(), Relaxed);
        assert_eq!(
            out.inference_budget(&controls, 0, 7, false),
            Duration::from_secs_f64(3.75)
        );
        assert_eq!(
            out.inference_budget(&controls, 0, 8, false),
            Duration::from_secs(20)
        );
        assert_eq!(
            out.inference_budget(&controls, 0, 7, true),
            Duration::from_secs(20)
        );
        controls.position[0].store(48000 * 11, Relaxed);
        assert_eq!(
            out.inference_budget(&controls, 0, 7, false),
            Duration::from_millis(100)
        );
        controls.play[0].store(false, Relaxed);
        assert_eq!(
            out.inference_budget(&controls, 0, 7, false),
            Duration::from_secs(20)
        );
    }
}

#[cfg(test)]
mod live_acceptance {
    use super::*;
    #[test]
    #[ignore = "Requires installed live model/runtime and AMP_STEM_TEST_SOURCE; no audio output files"]
    fn real_model_source_pipeline_preserves_mix_across_windows() {
        let source = std::path::PathBuf::from(
            std::env::var_os("AMP_STEM_TEST_SOURCE").expect("Set a read-only music source"),
        );
        assert!(source.is_file());
        assert!(crate::onnx_stem_worker::configured());
        let prep = amp_core::Preparation {
            cue_in: 30.,
            cue_out: Some(53.),
            ..Default::default()
        };
        let original = amp_core::command("ffmpeg")
            .args(["-v", "error"])
            .args(amp_core::cue_args(&prep).unwrap())
            .arg("-i")
            .arg(&source)
            .args(["-vn", "-ac", "2", "-ar", "44100", "-f", "f32le", "pipe:1"])
            .output()
            .unwrap();
        assert!(original.status.success());
        let expected: Vec<f32> = original
            .stdout
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        let (mut producer, mut consumer) = rtrb::RingBuffer::new(32768);
        let stop = Arc::new(AtomicBool::new(false));
        let controls = Arc::new(crate::mixer::Controls::default());
        let degraded = Arc::new(AtomicBool::new(false));
        let context = Context {
            stop,
            controls: controls.clone(),
            deck: 0,
            degraded_flag: degraded.clone(),
            generation: 1,
        };
        let task =
            std::thread::spawn(move || decode(&source, &prep, 44100, &mut producer, context));
        let mut frames = 0usize;
        let mut maximum_error = 0f32;
        let mut changed_energy = 0f64;
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        loop {
            match consumer.pop() {
                Ok(frame) => {
                    assert!(frame.iter().all(|v| v.is_finite()));
                    for ch in 0..2 {
                        let original = expected[frames * 2 + ch];
                        maximum_error = maximum_error
                            .max((frame[ch] + frame[2 + ch] + frame[4 + ch] - original).abs());
                        changed_energy += (frame[ch] as f64).powi(2);
                    }
                    frames += 1;
                }
                Err(_) if consumer.is_abandoned() => break,
                Err(_) => {
                    assert!(std::time::Instant::now() < deadline, "live pipeline hung");
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
        task.join().unwrap().unwrap();
        assert!(!degraded.load(Relaxed), "real model fell back to dry audio");
        assert!(controls.stem_warnings.lock().unwrap().is_empty());
        assert_eq!(
            frames * 2,
            expected.len(),
            "window joins must neither repeat nor drop source samples"
        );
        assert!(
            maximum_error < 0.00001,
            "original reconstruction error {maximum_error}"
        );
        assert!(
            changed_energy > 0.01,
            "model must produce a nonzero vocal part"
        );
        eprintln!(
            "Real live pipeline: {frames} frames, maximum original-mix error {maximum_error}; audio files written: 0"
        );
        assert!(
            crate::onnx_stem_worker::shutdown(),
            "worker must acknowledge app shutdown"
        );
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    #[test]
    fn stream_reservation_obeys_every_tier_without_revoking_existing_slots() {
        for limit in [2, 4, 6] {
            let active = AtomicUsize::new(0);
            for _ in 0..limit {
                assert!(reserve(&active, limit));
            }
            assert!(!reserve(&active, limit));
            assert!(!reserve(&active, 2));
            assert_eq!(active.load(Relaxed), limit);
            active.fetch_sub(1, Relaxed);
            assert!(reserve(&active, limit));
        }
    }
}
