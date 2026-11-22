//! Ignored, actual-model acceptance: no audio device and no saved stem outputs.
use super::*;
use std::time::Instant;
const RATE: u32 = 44100;
const BLOCK: usize = 441;

struct LiveTasks {
    stops: Vec<Arc<AtomicBool>>,
    tasks: Vec<std::thread::JoinHandle<Result<(), String>>>,
}
impl LiveTasks {
    fn cancel_all(&self) {
        for stop in &self.stops {
            stop.store(true, Relaxed);
        }
    }
    fn wait_finished(&self) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.tasks.iter().any(|task| !task.is_finished()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        self.tasks.iter().all(|task| task.is_finished())
    }
}
impl Drop for LiveTasks {
    fn drop(&mut self) {
        self.cancel_all();
        let _ = crate::onnx_stem_worker::shutdown();
        let _ = self.wait_finished();
        for task in self.tasks.drain(..) {
            if task.is_finished() {
                let _ = task.join();
            }
        }
    }
}
fn stream_source(
    path: &Path,
    deck: usize,
    controls: Arc<Controls>,
    tasks: &mut LiveTasks,
) -> Source {
    let (mut producer, consumer) = RingBuffer::new(RATE as usize * 16);
    let stop = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));
    let degraded = Arc::new(AtomicBool::new(false));
    let source = Source {
        key_lock: None,
        ring: consumer,
        duration: Arc::new(AtomicU64::new(RATE as u64 * 45)),
        done: done.clone(),
        failed: failed.clone(),
        history: VecDeque::with_capacity(RATE as usize * 4),
        history_start: 0,
        position: 0.,
        primed: false,
        separated: true,
        stem_degraded: degraded.clone(),
        stem_knobs: [0.; 3],
    };
    tasks.stops.push(stop.clone());
    let path = path.to_path_buf();
    let prep = amp_core::Preparation {
        cue_in: 0.,
        cue_out: Some(45.),
        ..Default::default()
    };
    let context = crate::onnx_stem_stream::Context {
        stop,
        controls,
        deck,
        degraded_flag: degraded,
        generation: 1,
    };
    tasks.tasks.push(std::thread::spawn(move || {
        let result = crate::onnx_stem_stream::decode(&path, &prep, RATE, &mut producer, context);
        if result.is_err() {
            failed.store(true, Relaxed);
        }
        done.store(true, Relaxed);
        result
    }));
    source
}
fn samples(mixer: &Mixer, deck: usize) -> (f64, f64) {
    let source = mixer.voices[deck]
        .source
        .as_ref()
        .expect("deck source retained");
    let frame = source.history.back().copied().unwrap_or([0.; 6]);
    let gain = if deck == 0 {
        [0., 1., 1.]
    } else {
        [1., 0., 0.]
    };
    let dry = stereo_frame(frame, true, [1.; 3], false);
    let filtered = stereo_frame(frame, true, gain, false);
    let energy = filtered.iter().map(|v| (*v as f64).powi(2)).sum();
    let change = dry
        .iter()
        .zip(filtered)
        .map(|(a, b)| (*a as f64 - b as f64).powi(2))
        .sum();
    (energy, change)
}

#[test]
#[ignore = "Requires native live module and AMP_STEM_TEST_SOURCE >=45sec; paced real two-deck model test, no audio device/files"]
fn two_native_live_stem_decks_play_filters_and_cancel_independently() {
    let path = std::path::PathBuf::from(
        std::env::var_os("AMP_STEM_TEST_SOURCE").expect("Set read-only source >=45sec"),
    );
    let before = std::fs::metadata(&path).unwrap();
    assert!(crate::onnx_stem_worker::configured());
    let controls = Arc::new(Controls::default());
    let (tx, rx) = mpsc::sync_channel(32);
    let mut mixer = Mixer::new(RATE, rx, controls.clone());
    let mut tasks = LiveTasks {
        stops: Vec::new(),
        tasks: Vec::new(),
    };
    for deck in [0, 1] {
        let source = stream_source(&path, deck, controls.clone(), &mut tasks);
        tx.send(Command::Load(deck, source, false, 1)).unwrap();
    }
    mixer.process_commands();
    // Both bounded rings get a real first estimate before their clocks start together.
    let prime_deadline = Instant::now() + Duration::from_secs(45);
    while (0..2)
        .any(|deck| mixer.voices[deck].source.as_ref().unwrap().ring.slots() < RATE as usize * 8)
    {
        assert!(
            Instant::now() < prime_deadline,
            "two native decks failed to prime"
        );
        for deck in [0, 1] {
            let source = mixer.voices[deck].source.as_ref().unwrap();
            assert!(
                !source.stem_degraded.load(Relaxed),
                "priming degraded deck {deck}: {:?}",
                controls.stem_warnings.lock().unwrap()
            );
            assert!(
                !source.failed.load(Relaxed),
                "source decoder failed during priming"
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    tx.send(Command::Fader(0.5)).unwrap();
    tx.send(Command::StemLevels(0, [0., 1., 1.])).unwrap(); // A: remove real vocals.
    tx.send(Command::StemLevels(1, [1., 0., 0.])).unwrap(); // B: actual vocal solo.
    tx.send(Command::Play(0)).unwrap();
    tx.send(Command::Play(1)).unwrap();
    let start = Instant::now();
    let mut rendered = 0usize;
    let mut energy = [0f64; 2];
    let mut changed = [0f64; 2];
    let mut mix_energy = 0f64;
    let mut peak_seen = [0f32; 2];
    while rendered < (RATE as usize * 30) {
        mixer.render(BLOCK, |_, frame| {
            assert!(frame.iter().all(|v| v.is_finite()));
            mix_energy += frame.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
        });
        rendered += BLOCK;
        for deck in [0, 1] {
            assert!(
                controls.play[deck].load(Relaxed),
                "deck {deck} stopped at {}sec",
                rendered as f64 / RATE as f64
            );
            assert!(!controls.finished[deck].load(Relaxed));
            assert!(
                !mixer.voices[deck]
                    .source
                    .as_ref()
                    .unwrap()
                    .stem_degraded
                    .load(Relaxed),
                "deck {deck} fell back: {:?}",
                controls.stem_warnings.lock().unwrap()
            );
            let (filtered, difference) = samples(&mixer, deck);
            energy[deck] += filtered;
            changed[deck] += difference;
            peak_seen[deck] = peak_seen[deck].max(get(&controls.peak[deck]));
        }
        let due = start + Duration::from_secs_f64(rendered as f64 / RATE as f64);
        if let Some(delay) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(delay);
        }
    }
    for deck in [0, 1] {
        assert!(
            controls.position[deck].load(Relaxed) >= RATE as u64 * 30 - 2,
            "both deck clocks must advance30sec"
        );
        assert_eq!(
            controls.underruns[deck].load(Relaxed),
            0,
            "deck {deck} underrun across joins"
        );
        assert!(
            energy[deck] > 1e-8 && changed[deck] > 1e-8 && peak_seen[deck] > 1e-9,
            "deck {deck} must contribute actual filtered audio"
        );
    }
    assert!(mix_energy > 1e-6);
    assert_eq!(get(&controls.stem_level[0][0]), 0.);
    assert_eq!(get(&controls.stem_level[1][1]), 0.);
    assert_eq!(get(&controls.stem_level[1][2]), 0.);
    // Match Engine stop ownership: cancel A's decoder token, then remove only A's voice.
    tasks.stops[0].store(true, Relaxed);
    tx.send(Command::Stop(0)).unwrap();
    let b_before = controls.position[1].load(Relaxed);
    let mut remaining_energy = 0f64;
    while rendered < RATE as usize * 36 {
        mixer.render(BLOCK, |_, frame| {
            assert!(frame.iter().all(|v| v.is_finite()));
            remaining_energy += frame.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
        });
        rendered += BLOCK;
        assert!(
            controls.play[1].load(Relaxed),
            "canceling A must not stop B"
        );
        assert!(
            !mixer.voices[1]
                .source
                .as_ref()
                .unwrap()
                .stem_degraded
                .load(Relaxed),
            "B must retain live filtering after A cancellation"
        );
        let due = start + Duration::from_secs_f64(rendered as f64 / RATE as f64);
        if let Some(delay) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(delay);
        }
    }
    assert!(!controls.play[0].load(Relaxed));
    assert!(controls.position[1].load(Relaxed) >= b_before + RATE as u64 * 6 - 2);
    assert_eq!(controls.underruns[1].load(Relaxed), 0);
    assert!(
        remaining_energy > 1e-7,
        "B vocal-solo output must continue after A cancellation"
    );
    assert!(controls.stems_loaded[1].load(Relaxed));
    assert_eq!(get(&controls.stem_level[1][0]), 1.);
    assert_eq!(get(&controls.stem_level[1][1]), 0.);
    assert_eq!(get(&controls.stem_level[1][2]), 0.);
    tasks.cancel_all();
    tx.send(Command::Stop(1)).unwrap();
    mixer.process_commands();
    assert!(
        tasks.wait_finished(),
        "both decoder threads must acknowledge cancellation"
    );
    assert!(crate::onnx_stem_worker::shutdown());
    let after = std::fs::metadata(&path).unwrap();
    assert_eq!(before.len(), after.len());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    eprintln!(
        "Actual concurrent live decks:30sec together+6sec B-only; energies={energy:?}, filter_differences={changed:?}, deck_peaks={peak_seen:?}, B_after_cancel_energy={remaining_energy}; underruns0; source unchanged; saved stem audio files0"
    );
}
