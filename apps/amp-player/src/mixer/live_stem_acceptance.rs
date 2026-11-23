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
    seconds: u64,
) -> Source {
    let (mut producer, consumer) = RingBuffer::new(RATE as usize * 16);
    let stop = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));
    let degraded = Arc::new(AtomicBool::new(false));
    let source = Source {
        key_lock: None,
        ring: consumer,
        duration: Arc::new(AtomicU64::new(RATE as u64 * seconds)),
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
        cue_out: Some(seconds as f64),
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
    samples_with_levels(
        mixer,
        deck,
        if deck == 0 {
            [0., 1., 1.]
        } else {
            [1., 0., 0.]
        },
    )
}
fn samples_with_levels(mixer: &Mixer, deck: usize, gain: [f32; 3]) -> (f64, f64) {
    let source = mixer.voices[deck]
        .source
        .as_ref()
        .expect("deck source retained");
    let frame = source.history.back().copied().unwrap_or([0.; 6]);
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
        let source = stream_source(&path, deck, controls.clone(), &mut tasks, 45);
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

const SIX_LEVELS: [[f32; 3]; 6] = [
    [0., 1., 1.], // Vocals removed.
    [1., 0., 0.], // Vocals solo.
    [0., 0., 1.], // Beat solo.
    [0., 1., 0.], // Instruments solo.
    [0.35, 0.8, 0.55],
    [0.8, 0.25, 0.65],
];
fn assert_pool_limits() -> (usize, u32, usize, usize) {
    let status = crate::onnx_stem_worker::pool_status().expect("real model pool started");
    let (slots, cap_mib, live, peak) = status;
    assert_eq!(
        slots, 1,
        "qualified CUDA uses one persistent worker: {status:?}"
    );
    assert_eq!(cap_mib, 6144, "qualified CUDA OS process quota: {status:?}");
    assert!(
        slots as u64 * u64::from(cap_mib) <= 6144,
        "aggregate enforced quota exceeds six GiB: {status:?}"
    );
    assert!(
        live <= 1 && peak == 1,
        "observed unreaped model process count: {status:?}"
    );
    status
}
fn assert_decks_live(mixer: &Mixer, controls: &Controls, decks: &[usize]) {
    for &deck in decks {
        let source = mixer.voices[deck]
            .source
            .as_ref()
            .expect("active source retained");
        assert!(
            controls.play[deck].load(Relaxed) && !controls.finished[deck].load(Relaxed),
            "deck {deck} stopped"
        );
        assert!(!source.failed.load(Relaxed), "deck {deck} decoder failed");
        assert!(
            !source.stem_degraded.load(Relaxed),
            "deck {deck} dry fallback: {:?}",
            controls.stem_warnings.lock().unwrap()
        );
        assert!(
            controls.stems_loaded[deck].load(Relaxed),
            "deck {deck} lost separated audio"
        );
        assert_eq!(
            controls.underruns[deck].load(Relaxed),
            0,
            "deck {deck} underrun"
        );
    }
}
fn source_digest(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path).unwrap();
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    format!("{:x}", hash.finalize())
}

#[test]
#[ignore = "Requires explicit qualified CUDA GPU and AMP_STEM_TEST_SOURCE >=180sec; six real model decks at116% for90sec plus8sec survivors; no audio device/stem files"]
fn six_native_live_stem_decks_play_distinct_filters_and_cancel_independently() {
    assert_eq!(
        std::env::var("AMP_STEM_TEST_QUALIFIED_GPU").as_deref(),
        Ok("1"),
        "six-deck throughput qualification requires explicit GPU opt-in"
    );
    assert_eq!(
        std::env::var("AMP_ONNX_PROVIDER").as_deref(),
        Ok("cuda"),
        "CPU six-deck throughput is not qualified"
    );
    let path = std::path::PathBuf::from(
        std::env::var_os("AMP_STEM_TEST_SOURCE").expect("Set read-only source >=180sec"),
    );
    let before = std::fs::metadata(&path).unwrap();
    let digest = source_digest(&path);
    let parent = path.parent().unwrap();
    let files_before: std::collections::BTreeSet<_> = std::fs::read_dir(parent)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert!(crate::onnx_stem_worker::configured());
    let controls = Arc::new(Controls::default());
    set(&controls.master, 0.5);
    for deck in 0..6 {
        set(&controls.gain[deck], 0.35);
        set(&controls.rate[deck], 1.16);
        controls.on_demand_stems[deck].store(true, Relaxed);
    }
    let (tx, rx) = mpsc::sync_channel(64);
    let mut mixer = Mixer::new(RATE, rx, controls.clone());
    let mut tasks = LiveTasks {
        stops: Vec::new(),
        tasks: Vec::new(),
    };
    for deck in 0..6 {
        let source = stream_source(&path, deck, controls.clone(), &mut tasks, 180);
        tx.send(Command::Load(deck, source, false, 1)).unwrap();
    }
    mixer.process_commands();
    let prime_deadline = Instant::now() + Duration::from_secs(120);
    while (0..6)
        .any(|deck| mixer.voices[deck].source.as_ref().unwrap().ring.slots() < RATE as usize * 8)
    {
        assert!(
            Instant::now() < prime_deadline,
            "six native decks failed to prime"
        );
        for (deck, voice) in mixer.voices.iter().take(6).enumerate() {
            let source = voice.source.as_ref().unwrap();
            assert!(
                !source.failed.load(Relaxed),
                "deck {deck} priming decoder failure"
            );
            assert!(
                !source.stem_degraded.load(Relaxed),
                "deck {deck} priming fallback: {:?}",
                controls.stem_warnings.lock().unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let initial_pool = assert_pool_limits();
    tx.send(Command::Fader(0.5)).unwrap();
    for (deck, levels) in SIX_LEVELS.into_iter().enumerate() {
        tx.send(Command::StemLevels(deck, levels)).unwrap();
        tx.send(Command::Play(deck)).unwrap();
    }
    let start = Instant::now();
    let mut rendered = 0usize;
    let mut energy = [0f64; 6];
    let mut changed = [0f64; 6];
    let mut peaks = [0f32; 6];
    let mut mix_energy = 0f64;
    while rendered < RATE as usize * 90 {
        mixer.render(BLOCK, |_, frame| {
            assert!(frame.iter().all(|value| value.is_finite()));
            mix_energy += frame
                .iter()
                .map(|value| f64::from(*value).powi(2))
                .sum::<f64>();
        });
        rendered += BLOCK;
        assert_decks_live(&mixer, &controls, &[0, 1, 2, 3, 4, 5]);
        for deck in 0..6 {
            let (filtered, difference) = samples_with_levels(&mixer, deck, SIX_LEVELS[deck]);
            energy[deck] += filtered;
            changed[deck] += difference;
            peaks[deck] = peaks[deck].max(get(&controls.peak[deck]));
        }
        let due = start + Duration::from_secs_f64(rendered as f64 / f64::from(RATE));
        if let Some(delay) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(delay);
        }
    }
    for deck in 0..6 {
        assert!(
            controls.position[deck].load(Relaxed) >= (f64::from(RATE) * 90. * 1.16) as u64 - 2,
            "deck {deck} clock must advance >=104.4 source seconds at116%"
        );
        assert!(
            energy[deck] > 1e-8 && changed[deck] > 1e-8 && peaks[deck] > 1e-9,
            "deck {deck} must contribute real changed audio"
        );
        for stem in 0..3 {
            assert_eq!(
                get(&controls.stem_level[deck][stem]),
                SIX_LEVELS[deck][stem]
            );
            assert!(
                (mixer.voices[deck].stem_mix[stem] - SIX_LEVELS[deck][stem]).abs() < 1e-4,
                "actual mixer must apply deck {deck} stem {stem}"
            );
        }
    }
    assert!(mix_energy > 1e-6);
    for (deck, voice) in mixer.voices.iter().take(6).enumerate() {
        assert!(
            !voice.source.as_ref().unwrap().done.load(Relaxed),
            "deck {deck} must not reach decoder EOF before independent cancellation"
        );
    }
    let busy_pool = assert_pool_limits();
    let cancelled = 2usize;
    tasks.stops[cancelled].store(true, Relaxed);
    tx.send(Command::Stop(cancelled)).unwrap();
    let others = [0usize, 1, 3, 4, 5];
    let positions = others.map(|deck| controls.position[deck].load(Relaxed));
    let mut after_energy = 0f64;
    let mut after_changed = [0f64; 6];
    while rendered < RATE as usize * 98 {
        mixer.render(BLOCK, |_, frame| {
            assert!(frame.iter().all(|value| value.is_finite()));
            after_energy += frame
                .iter()
                .map(|value| f64::from(*value).powi(2))
                .sum::<f64>();
        });
        rendered += BLOCK;
        assert_decks_live(&mixer, &controls, &others);
        for &deck in &others {
            after_changed[deck] += samples_with_levels(&mixer, deck, SIX_LEVELS[deck]).1;
        }
        let due = start + Duration::from_secs_f64(rendered as f64 / f64::from(RATE));
        if let Some(delay) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(delay);
        }
    }
    assert!(!controls.play[cancelled].load(Relaxed));
    for (index, &deck) in others.iter().enumerate() {
        assert!(
            controls.position[deck].load(Relaxed)
                >= positions[index] + (f64::from(RATE) * 8. * 1.16) as u64 - 2,
            "cancelled deck must not affect deck {deck} clock"
        );
        assert!(
            after_changed[deck] > 1e-8,
            "deck {deck} filter must remain active after cancellation"
        );
    }
    assert!(after_energy > 1e-7);
    let final_pool = assert_pool_limits();
    assert!(
        controls.stem_warnings.lock().unwrap().is_empty(),
        "no dry fallback warnings permitted"
    );
    tasks.cancel_all();
    for deck in 0..6 {
        tx.send(Command::Stop(deck)).unwrap();
    }
    mixer.process_commands();
    assert!(
        tasks.wait_finished(),
        "all six decoders must acknowledge cancellation"
    );
    assert!(crate::onnx_stem_worker::shutdown());
    assert_eq!(
        crate::onnx_stem_worker::pool_status()
            .map(|status| status.2)
            .unwrap_or(0),
        0,
        "shutdown must reap every model process"
    );
    let after = std::fs::metadata(&path).unwrap();
    assert_eq!(before.len(), after.len());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    assert_eq!(
        source_digest(&path),
        digest,
        "original source bytes must remain unchanged"
    );
    let files_after: std::collections::BTreeSet<_> = std::fs::read_dir(parent)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(
        files_after, files_before,
        "no saved stem audio or other output files"
    );
    eprintln!(
        "Qualified CUDA six live decks:90sec at116% together+8sec five-only; filters={SIX_LEVELS:?}; energies={energy:?}; changes={changed:?}; peaks={peaks:?}; after_cancel_changes={after_changed:?}; pools={initial_pool:?}/{busy_pool:?}/{final_pool:?}; cap claims are enforced quotas, not measured RSS; underruns0; fallback0; source unchanged; saved stem audio files0"
    );
}

/// Deterministic DSP/control regression only. These are supplied separated PCM frames, not model output.
#[test]
fn six_separated_frame_controls_route_filters_and_cancel_without_model() {
    const FRAMES: usize = 6000;
    let controls = Arc::new(Controls::default());
    set(&controls.master, 1.);
    let (tx, rx) = mpsc::sync_channel(64);
    let mut mixer = Mixer::new(1000, rx, controls.clone());
    let parts: [[f32; 3]; 6] = std::array::from_fn(|deck| {
        [
            0.012 * (deck + 1) as f32,
            0.014 * (6 - deck) as f32,
            0.009 * (deck + 2) as f32,
        ]
    });
    for (deck, part) in parts.iter().enumerate() {
        let (mut producer, consumer) = RingBuffer::new(FRAMES + 1);
        for _ in 0..FRAMES {
            producer
                .push([
                    part[0],
                    part[0] * 0.5,
                    part[1],
                    part[1] * 0.5,
                    part[2],
                    part[2] * 0.5,
                ])
                .unwrap();
        }
        let source = Source {
            key_lock: None,
            ring: consumer,
            duration: Arc::new(AtomicU64::new(FRAMES as u64)),
            done: Arc::new(AtomicBool::new(true)),
            failed: Arc::new(AtomicBool::new(false)),
            history: VecDeque::with_capacity(FRAMES),
            history_start: 0,
            position: 0.,
            primed: true,
            separated: true,
            stem_degraded: Arc::new(AtomicBool::new(false)),
            stem_knobs: [0.; 3],
        };
        set(&controls.gain[deck], 0.5);
        tx.send(Command::Load(deck, source, false, 1)).unwrap();
        tx.send(Command::StemLevels(deck, SIX_LEVELS[deck]))
            .unwrap();
        tx.send(Command::Play(deck)).unwrap();
    }
    tx.send(Command::Fader(0.5)).unwrap();
    let contribution = |deck: usize, levels: [f32; 3]| {
        let value = (0..3)
            .map(|stem| parts[deck][stem] * levels[stem])
            .sum::<f32>();
        value
            * 0.5
            * if deck < 2 {
                std::f32::consts::FRAC_1_SQRT_2
            } else {
                1.
            }
            * std::f32::consts::FRAC_1_SQRT_2
    };
    let expected = (0..6)
        .map(|deck| contribution(deck, SIX_LEVELS[deck]))
        .sum::<f32>();
    let dry = (0..6).map(|deck| contribution(deck, [1.; 3])).sum::<f32>();
    let mut output = [0.; 2];
    mixer.render(4000, |_, frame| output = frame);
    assert!(
        (output[0] - expected).abs() < 1e-4,
        "actual six-deck master sum: {output:?} vs {expected}"
    );
    assert!((output[1] - expected * 0.5).abs() < 1e-4);
    assert!(
        (output[0] - dry).abs() > 1e-4,
        "six filters must change the actual master output"
    );
    for deck in 0..6 {
        assert_eq!(controls.position[deck].load(Relaxed), 4000);
        assert_eq!(controls.underruns[deck].load(Relaxed), 0);
        for (other, playing) in controls.play.iter().take(6).enumerate() {
            playing.store(other == deck, Relaxed);
        }
        mixer.render(100, |_, frame| output = frame);
        let filtered = contribution(deck, SIX_LEVELS[deck]);
        assert!(
            (output[0] - filtered).abs() < 1e-4,
            "deck {deck} actual filter/output routing"
        );
        assert!(
            (filtered - contribution(deck, [1.; 3])).abs() > 1e-5,
            "deck {deck} must have a distinct nonunity filter"
        );
        assert!(get(&controls.peak[deck]) > 1e-6);
    }
    for playing in controls.play.iter().take(6) {
        playing.store(true, Relaxed);
    }
    let before: [u64; 6] = std::array::from_fn(|deck| controls.position[deck].load(Relaxed));
    tx.send(Command::Stop(2)).unwrap();
    mixer.render(200, |_, frame| output = frame);
    assert!(!controls.play[2].load(Relaxed));
    assert!((output[0] - (expected - contribution(2, SIX_LEVELS[2]))).abs() < 1e-4);
    for deck in [0usize, 1, 3, 4, 5] {
        assert_eq!(controls.position[deck].load(Relaxed), before[deck] + 200);
        assert!(controls.play[deck].load(Relaxed) && controls.stems_loaded[deck].load(Relaxed));
        assert_eq!(controls.underruns[deck].load(Relaxed), 0);
    }
}
