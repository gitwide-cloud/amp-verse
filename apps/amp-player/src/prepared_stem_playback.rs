//! Verified gapless compressed parts enter the ordinary six-channel mixer ring.
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    time::Duration,
};
pub(crate) fn root() -> Option<PathBuf> {
    crate::theme::settings_path().and_then(|p| p.parent().map(|p| p.join("prepared-stems-v1")))
}
pub(crate) fn candidate(source: &Path) -> bool {
    root().is_some_and(|r| crate::prepared_stem_cache::candidate(&r, source).is_some())
}
struct ChildGuard {
    child: Arc<Mutex<crate::receiver_process::Decoder>>,
    finished: Arc<AtomicBool>,
    watcher: Option<std::thread::JoinHandle<()>>,
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.finished.store(true, Relaxed);
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some(w) = self.watcher.take() {
            let _ = w.join();
        }
    }
}
pub(crate) fn decode(
    source: &Path,
    prep: &amp_core::Preparation,
    rate: u32,
    producer: &mut rtrb::Producer<[f32; 8]>,
    context: crate::onnx_stem_stream::Context,
) -> Result<(), String> {
    decode_at(
        &root().unwrap_or_default(),
        source,
        prep,
        rate,
        producer,
        context,
    )
}
pub(crate) fn decode_at(
    cache_root: &Path,
    source: &Path,
    prep: &amp_core::Preparation,
    rate: u32,
    producer: &mut rtrb::Producer<[f32; 8]>,
    context: crate::onnx_stem_stream::Context,
) -> Result<(), String> {
    let prepared = crate::prepared_stem_cache::ready(cache_root, source, &context.stop)
        .and_then(|p| p.ok_or_else(|| "Prepared stems changed or are unavailable".to_string()));
    if context.stop.load(Relaxed) {
        return Ok(());
    }
    let mut command = amp_core::command("ffmpeg");
    command.args(["-v", "error"]);
    let channels = match prepared {
        Ok(prepared) => {
            for part in &prepared.manifest.parts {
                command.arg("-i").arg(prepared.directory.join(&part.name));
            }
            let start = (prep.cue_in * 44100.).round().max(0.) as u64;
            let end = prep
                .cue_out
                .map(|s| (s * 44100.).round().max(0.) as u64)
                .unwrap_or(prepared.manifest.frames)
                .min(prepared.manifest.frames);
            let mut graph = String::new();
            for (index, part) in prepared.manifest.parts.iter().enumerate() {
                graph.push_str(&format!("[{index}:a]aformat=sample_fmts=fltp:sample_rates=44100:channel_layouts=stereo,asettb=1/44100,asetpts=N/SR/TB,atrim=start_sample={start}:end_sample={end},asetpts=PTS-STARTPTS,volume={}[p{index}];",part.restore_gain));
            }
            graph.push_str(
                "[p0][p1][p2]amerge=inputs=3,pan=5.1|FL=c0|FR=c1|FC=c2|LFE=c3|BL=c4|BR=c5[out]",
            );
            command.args(["-filter_complex", &graph, "-map", "[out]"]);
            6
        }
        Err(_) => {
            context.degraded_flag.store(true, Relaxed);
            if let Ok(mut warnings) = context.controls.stem_warnings.lock() {
                warnings.retain(|(d, _, _)| *d != context.deck);
                warnings.push((context.deck,context.generation,"Prepared stems could not be verified; playing original audio. Prepare this playlist again to restore filters.".into()));
            }
            command
                .args(amp_core::cue_args(prep)?)
                .arg("-i")
                .arg(source);
            2
        }
    };
    command
        .args([
            "-vn",
            "-ac",
            &channels.to_string(),
            "-ar",
            &rate.to_string(),
            "-f",
            "f32le",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = crate::receiver_process::spawn(&mut command).map_err(|e| e.to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Prepared decoder output missing")?;
    let child = Arc::new(Mutex::new(child));
    let finished = Arc::new(AtomicBool::new(false));
    let watching = child.clone();
    let done = finished.clone();
    let stop = context.stop.clone();
    let watcher = std::thread::spawn(move || {
        while !done.load(Relaxed) {
            if stop.load(Relaxed) {
                if let Ok(mut child) = watching.lock() {
                    let _ = child.kill();
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    });
    let _guard = ChildGuard {
        child: child.clone(),
        finished,
        watcher: Some(watcher),
    };
    let mut input = std::io::BufReader::new(stdout);
    let frame_bytes = channels * 4;
    let mut bytes = vec![0u8; frame_bytes * 1024];
    loop {
        if context.stop.load(Relaxed) || producer.is_abandoned() {
            return Ok(());
        }
        let mut n = 0;
        while n < bytes.len() {
            let got = input.read(&mut bytes[n..]).map_err(|e| e.to_string())?;
            if got == 0 {
                break;
            }
            n += got;
        }
        if n == 0 {
            break;
        }
        if n % frame_bytes != 0 {
            return Err("Incomplete prepared PCM frame".into());
        }
        for frame in bytes[..n].chunks_exact(frame_bytes) {
            let mut value = [0.; 8];
            for (ch, sample) in frame.as_chunks::<4>().0.iter().enumerate() {
                value[ch] = f32::from_le_bytes(*sample);
            }
            if value.iter().any(|v| !v.is_finite()) {
                return Err("Invalid prepared PCM".into());
            }
            loop {
                match producer.push(value) {
                    Ok(()) => break,
                    Err(rtrb::PushError::Full(v)) => {
                        value = v;
                        if context.stop.load(Relaxed) || producer.is_abandoned() {
                            return Ok(());
                        }
                        std::thread::sleep(Duration::from_millis(2));
                    }
                }
            }
        }
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if context.stop.load(Relaxed) {
            return Ok(());
        }
        let status = child
            .lock()
            .map_err(|_| "Prepared decoder lock failed")?
            .try_wait()
            .map_err(|e| e.to_string())?;
        if let Some(status) = status {
            if !status.success() {
                return Err("Prepared audio decoder failed".into());
            }
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err("Prepared decoder exit timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    pub(crate) struct Fixture {
        pub _dir: tempfile::TempDir,
        pub root: PathBuf,
        pub source: PathBuf,
        pub entry: PathBuf,
    }
    fn sha(path: &Path) -> String {
        format!("{:x}", Sha256::digest(std::fs::read(path).unwrap()))
    }
    pub(crate) fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.wav");
        let root = dir.path().join("prepared-stems-v1");
        std::fs::create_dir(&root).unwrap();
        assert!(
            amp_core::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=997:sample_rate=44100:duration=3",
                    "-ac",
                    "2",
                    "-c:a",
                    "pcm_f32le"
                ])
                .arg(&source)
                .status()
                .unwrap()
                .success()
        );
        let source_hash = sha(&source);
        let entry = root.join(format!("{source_hash}-mp3256-v1"));
        std::fs::create_dir(&entry).unwrap();
        let parts = std::array::from_fn(|i| {
            let name = ["vocals.mp3", "instruments.mp3", "drums.mp3"][i];
            let file = entry.join(name);
            assert!(
                amp_core::command("ffmpeg")
                    .args(["-v", "error", "-i"])
                    .arg(&source)
                    .args(["-af", "volume=0.125", "-c:a", "libmp3lame", "-b:a", "256k"])
                    .arg(&file)
                    .status()
                    .unwrap()
                    .success()
            );
            crate::prepared_stem_cache::Part {
                name: name.into(),
                bytes: std::fs::metadata(&file).unwrap().len(),
                sha256: sha(&file),
                restore_gain: [2., 4., 2.][i],
            }
        });
        let manifest = crate::prepared_stem_cache::Manifest {
            version: 1,
            model_sha256: crate::prepared_stem_cache::MODEL.into(),
            source_sha256: source_hash,
            sample_rate: 44100,
            frames: 132300,
            codec: "mp3-gapless".into(),
            bitrate: 256000,
            parts,
        };
        std::fs::write(
            entry.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        Fixture {
            _dir: dir,
            source,
            root,
            entry,
        }
    }
    pub(crate) fn frames(fixture: &Fixture, prep: amp_core::Preparation) -> (Vec<[f32; 8]>, bool) {
        let (mut producer, mut consumer) = rtrb::RingBuffer::new(132301);
        let degraded = Arc::new(AtomicBool::new(false));
        let context = crate::onnx_stem_stream::Context {
            stop: Arc::new(AtomicBool::new(false)),
            controls: Arc::new(crate::mixer::Controls::default()),
            deck: 0,
            degraded_flag: degraded.clone(),
            generation: 1,
        };
        decode_at(
            &fixture.root,
            &fixture.source,
            &prep,
            44100,
            &mut producer,
            context,
        )
        .unwrap();
        let mut frames = Vec::new();
        while let Ok(frame) = consumer.pop() {
            frames.push(frame);
        }
        (frames, degraded.load(Relaxed))
    }
    #[test]
    fn actual_mp3_gain_gapless_trim_and_corruption_fallback_are_aligned() {
        let f = fixture();
        let before = sha(&f.source);
        let (full, degraded) = frames(&f, Default::default());
        assert!(!degraded);
        assert_eq!(full.len(), 132300);
        let (trimmed, degraded) = frames(
            &f,
            amp_core::Preparation {
                cue_in: 0.5,
                cue_out: Some(2.5),
                ..Default::default()
            },
        );
        assert!(!degraded);
        assert_eq!(trimmed.len(), 88200);
        let original = amp_core::command("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&f.source)
            .args(["-ac", "2", "-ar", "44100", "-f", "f32le", "pipe:1"])
            .output()
            .unwrap();
        assert!(original.status.success());
        let original: Vec<f32> = original
            .stdout
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        assert_eq!(original.len(), full.len() * 2);
        let mut error = 0.;
        let mut changed = 0.;
        for (i, frame) in full.iter().enumerate() {
            assert!((frame[2] - 2. * frame[0]).abs() < 1e-6);
            assert!((frame[4] - frame[0]).abs() < 1e-6);
            let dry = frame[0] + frame[2] + frame[4];
            let expected = original[i * 2] as f64;
            error += (dry as f64 - expected).powi(2);
            changed += (dry - frame[2]).powi(2) as f64;
        }
        assert!((error / full.len() as f64).sqrt() < 0.004);
        assert!(changed > 1.);
        for (a, b) in trimmed.iter().zip(&full[22050..110250]) {
            for (x, y) in a.iter().zip(b) {
                assert!((x - y).abs() < 1e-6);
            }
        }
        std::fs::write(f.entry.join("vocals.mp3"), b"corrupt").unwrap();
        let (dry, degraded) = frames(
            &f,
            amp_core::Preparation {
                cue_in: 0.5,
                cue_out: Some(2.5),
                ..Default::default()
            },
        );
        assert!(degraded);
        assert_eq!(dry.len(), 88200);
        assert!(dry.iter().all(|f| f[2..].iter().all(|v| *v == 0.)));
        assert_eq!(before, sha(&f.source));
    }
    #[test]
    fn canceling_one_prepared_decoder_keeps_another_independent() {
        let f = fixture();
        let mut tasks = Vec::new();
        let mut consumers = Vec::new();
        let mut stops = Vec::new();
        for deck in 0..2 {
            let (mut producer, consumer) = rtrb::RingBuffer::new(4096);
            consumers.push(consumer);
            let stop = Arc::new(AtomicBool::new(false));
            stops.push(stop.clone());
            let root = f.root.clone();
            let source = f.source.clone();
            tasks.push(std::thread::spawn(move || {
                decode_at(
                    &root,
                    &source,
                    &Default::default(),
                    44100,
                    &mut producer,
                    crate::onnx_stem_stream::Context {
                        stop,
                        controls: Arc::new(crate::mixer::Controls::default()),
                        deck,
                        degraded_flag: Arc::new(AtomicBool::new(false)),
                        generation: 1,
                    },
                )
            }));
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while consumers.iter().any(|c| c.slots() < 32) {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        stops[0].store(true, Relaxed);
        while !tasks[0].is_finished() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!tasks[1].is_finished());
        let mut count = 0;
        while count < 132300 {
            match consumers[1].pop() {
                Ok(frame) => {
                    assert!(frame.iter().all(|v| v.is_finite()));
                    count += 1;
                }
                Err(_) => {
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
        for task in tasks {
            task.join().unwrap().unwrap();
        }
        assert_eq!(count, 132300);
    }
}
