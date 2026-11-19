//! Bounded master-output tap. No network or encoder work runs on the audio callback.
use std::{
    io::{Read, Write},
    process::{Child, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering::Relaxed},
    },
};
pub struct Broadcast {
    pub player_only: bool,
    level: Arc<AtomicU32>,
    muted: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Child>>,
    pub status: Arc<Mutex<String>>,
}
impl Drop for Broadcast {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}
impl Broadcast {
    pub fn levels(&self) -> (f32, bool) {
        (
            f32::from_bits(self.level.load(Relaxed)),
            self.muted.load(Relaxed),
        )
    }
    #[cfg(test)]
    pub fn start_source(
        audio: &crate::mixer::Engine,
        aux: &crate::mixer::Engine,
        address: String,
        password: String,
        player_only: bool,
    ) -> Result<Self, String> {
        Self::start_source_normalized(audio, aux, address, password, player_only, false)
    }
    pub fn start_source_normalized(
        audio: &crate::mixer::Engine,
        aux: &crate::mixer::Engine,
        address: String,
        password: String,
        player_only: bool,
        normalize: bool,
    ) -> Result<Self, String> {
        if audio.sample_rate != aux.sample_rate {
            return Err(
                "Sampler and DJ outputs need the same sample rate for live broadcast".into(),
            );
        }
        let main = audio.broadcast_source_tap(player_only)?;
        let samples = if player_only {
            None
        } else {
            Some(aux.broadcast_tap()?)
        };
        let child = Arc::new(Mutex::new(encoder(audio.sample_rate, normalize)?));
        let level = Arc::new(AtomicU32::new(1f32.to_bits()));
        let muted = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new("Connecting live output…".into()));
        let sender = Sender {
            main: Some(main),
            samples,
            sample_rate: audio.sample_rate,
            normalize,
            address,
            password,
            player_only,
            level: level.clone(),
            muted: muted.clone(),
            stop: stop.clone(),
            child: child.clone(),
            status: status.clone(),
            session: None,
        };
        std::thread::spawn(move || sender.run());
        Ok(Self {
            player_only,
            level,
            muted,
            stop,
            child,
            status,
        })
    }
    pub fn set_level(&self, level: f32, muted: bool) -> Result<(), String> {
        if !level.is_finite() || !(0. ..=1.).contains(&level) {
            return Err("Broadcast level must be 0–1".into());
        }
        self.level.store(level.to_bits(), Relaxed);
        self.muted.store(muted, Relaxed);
        Ok(())
    }
    pub fn running(&self) -> bool {
        !self.stop.load(Relaxed)
    }
}
fn encoder(sample_rate: u32, normalize: bool) -> Result<Child, String> {
    let mut command = amp_core::command("ffmpeg");
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-probesize",
            "32",
            "-analyzeduration",
            "0",
            "-f",
            "f32le",
            "-ar",
        ])
        .arg(sample_rate.to_string())
        .args(["-ac", "2", "-i", "pipe:0"]);
    if normalize {
        command.args(["-af", "loudnorm=I=-16:TP=-1.5:LRA=11"]);
    }
    command
        .args([
            "-c:a",
            "libmp3lame",
            "-b:a",
            "256k",
            "-reservoir",
            "0",
            "-f",
            "mp3",
            "-flush_packets",
            "1",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())
}
struct Sender {
    main: Option<rtrb::Consumer<[f32; 2]>>,
    samples: Option<rtrb::Consumer<[f32; 2]>>,
    sample_rate: u32,
    normalize: bool,
    address: String,
    password: String,
    player_only: bool,
    level: Arc<AtomicU32>,
    muted: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Child>>,
    status: Arc<Mutex<String>>,
    session: Option<String>,
}
impl Sender {
    fn run(mut self) {
        let result = self.run_loop();
        *self.status.lock().unwrap() = match result {
            Ok(()) => "Live broadcast stopped".into(),
            Err(error) => format!("Live broadcast stopped: {error}"),
        };
        self.stop.store(true, Relaxed);
        {
            let mut child = self.child.lock().unwrap();
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(session) = &self.session {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(2)))
                .proxy(None)
                .max_redirects(0)
                .build()
                .into();
            let _ = agent
                .post(format!("http://{}/api/live/stop", self.address))
                .header("x-amp-password", &self.password)
                .header("x-amp-live-session", session)
                .send_empty();
        }
    }
    fn connect(
        &self,
        agent: &ureq::Agent,
        deadline: std::time::Instant,
    ) -> Result<serde_json::Value, String> {
        loop {
            if self.stop.load(Relaxed) {
                return Err("Cancelled".into());
            }
            let mut request = agent
                .post(format!("http://{}/api/live/start", self.address))
                .header("x-amp-password", &self.password);
            if let Some(previous) = &self.session {
                request = request.header("x-amp-previous-session", previous);
            }
            let result = request.send_empty();
            match result {
                Ok(mut response) => {
                    return response.body_mut().read_json().map_err(|e| e.to_string());
                }
                Err(error) => {
                    let denied = matches!(&error,ureq::Error::StatusCode(code) if *code<500);
                    if self.session.is_none() || denied || std::time::Instant::now() >= deadline {
                        return Err(format!("Could not resume live owner: {error}"));
                    }
                    *self.status.lock().unwrap() =
                        "Network interrupted · reconnecting current output…".into();
                    if !wait_or_cancel(&self.stop, std::time::Duration::from_secs(4)) {
                        return Err("Cancelled".into());
                    }
                }
            }
        }
    }
    fn run_loop(&mut self) -> Result<(), String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(2)))
            .proxy(None)
            .max_redirects(0)
            .build()
            .into();
        let mut first = true;
        let mut last_audio = std::time::Instant::now();
        loop {
            if self.stop.load(Relaxed) {
                return Ok(());
            }
            if last_audio.elapsed() > std::time::Duration::from_secs(60) {
                return Err("Live audio could not reconnect within 60 seconds".into());
            }
            let response = self.connect(&agent, last_audio + std::time::Duration::from_secs(60))?;
            let token = response["session"]
                .as_str()
                .ok_or("Server does not support live broadcast")?
                .to_owned();
            self.session = Some(token.clone());
            let sequenced = response["upload_sequence"].as_bool().unwrap_or(false);
            let guarded = response["guarded_resume"].as_bool().unwrap_or(false);
            let mut main = self.main.take().ok_or("Master output tap unavailable")?;
            let mut samples = self.samples.take();
            // A resumed stream starts at current PCM, never at a backlog from a Wi-Fi roam.
            while main.pop().is_ok() {}
            if let Some(samples) = &mut samples {
                while samples.pop().is_ok() {}
            }
            let (mut input, mut output) = {
                let mut child = self.child.lock().unwrap();
                if !first {
                    *child = encoder(self.sample_rate, self.normalize)?;
                }
                first = false;
                (
                    child.stdin.take().ok_or("Encoder input unavailable")?,
                    child.stdout.take().ok_or("Encoder output unavailable")?,
                )
            };
            let attempt_stop = Arc::new(AtomicBool::new(false));
            let writer_stop = attempt_stop.clone();
            let stopped = self.stop.clone();
            let level = self.level.clone();
            let muted = self.muted.clone();
            let writer = std::thread::spawn(move || {
                let mut bytes = Vec::with_capacity(8192);
                while !writer_stop.load(Relaxed) && !stopped.load(Relaxed) {
                    bytes.clear();
                    for _ in 0..1024 {
                        let Ok(frame) = main.pop() else { break };
                        let aux = samples
                            .as_mut()
                            .and_then(|s| s.pop().ok())
                            .unwrap_or([0.; 2]);
                        for channel in 0..2 {
                            let gain = if muted.load(Relaxed) {
                                0.
                            } else {
                                f32::from_bits(level.load(Relaxed))
                            };
                            bytes.extend_from_slice(
                                &((frame[channel] + aux[channel]) * gain)
                                    .clamp(-0.98, 0.98)
                                    .to_le_bytes(),
                            );
                        }
                    }
                    if bytes.is_empty() {
                        std::thread::sleep(std::time::Duration::from_millis(2));
                    } else if input.write_all(&bytes).is_err() {
                        break;
                    }
                }
                (main, samples)
            });
            *self.status.lock().unwrap() = if self.player_only {
                "Broadcasting AMP Player output"
            } else {
                "Broadcasting live DJ mix · decks, mixer, stems and sampler"
            }
            .into();
            let outcome = (|| -> Result<(), String> {
                let mut bytes = [0; 8192];
                let mut sequence = response["next_sequence"].as_u64().unwrap_or(0);
                while !self.stop.load(Relaxed) {
                    let n = output.read(&mut bytes).map_err(|e| e.to_string())?;
                    if n == 0 {
                        return Err("Live encoder ended".into());
                    }
                    upload(
                        &agent,
                        &self.address,
                        &self.password,
                        &token,
                        sequenced.then_some(sequence),
                        &bytes[..n],
                        &self.stop,
                    )?;
                    last_audio = std::time::Instant::now();
                    sequence = sequence
                        .checked_add(1)
                        .ok_or("Live upload sequence exhausted")?;
                }
                Ok(())
            })();
            attempt_stop.store(true, Relaxed);
            {
                let mut child = self.child.lock().unwrap();
                let _ = child.kill();
                let _ = child.wait();
            }
            let (main, samples) = writer.join().map_err(|_| "Live audio writer stopped")?;
            self.main = Some(main);
            self.samples = samples;
            if self.stop.load(Relaxed) {
                return Ok(());
            }
            if !guarded {
                return outcome;
            }
            *self.status.lock().unwrap() =
                "Network interrupted · reconnecting current output…".into();
            if !wait_or_cancel(&self.stop, std::time::Duration::from_millis(250)) {
                return Ok(());
            }
        }
    }
}
fn wait_or_cancel(stop: &AtomicBool, duration: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + duration;
    while std::time::Instant::now() < deadline {
        if stop.load(Relaxed) {
            return false;
        }
        std::thread::sleep(
            deadline
                .saturating_duration_since(std::time::Instant::now())
                .min(std::time::Duration::from_millis(25)),
        );
    }
    !stop.load(Relaxed)
}
fn upload(
    agent: &ureq::Agent,
    address: &str,
    password: &str,
    token: &str,
    sequence: Option<u64>,
    bytes: &[u8],
    stopped: &AtomicBool,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut attempts = 0;
    loop {
        if stopped.load(Relaxed) {
            return Err("Live upload cancelled".into());
        }
        let mut request = agent
            .post(format!("http://{address}/api/live/audio"))
            .header("x-amp-password", password)
            .header("x-amp-live-session", token);
        if let Some(sequence) = sequence {
            request = request.header("x-amp-live-sequence", sequence.to_string());
        }
        let result = request
            .config()
            .timeout_global(Some(
                deadline
                    .saturating_duration_since(std::time::Instant::now())
                    .max(std::time::Duration::from_millis(1)),
            ))
            .build()
            .send(bytes);
        match result {
            Ok(_) => return Ok(()),
            Err(error) => {
                let retryable = !matches!(&error, ureq::Error::StatusCode(code) if *code < 500);
                attempts += 1;
                if sequence.is_none()
                    || !retryable
                    || attempts >= 3
                    || std::time::Instant::now() >= deadline
                {
                    return Err(format!("Live upload interrupted: {error}"));
                }
                std::thread::sleep(std::time::Duration::from_millis(50 * attempts));
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_backoff_stops_promptly() {
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let worker = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(60));
            signal.store(true, Relaxed);
        });
        let started = std::time::Instant::now();
        assert!(!wait_or_cancel(&stop, std::time::Duration::from_secs(4)));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        worker.join().unwrap();
    }
    #[test]
    #[ignore = "requires FFmpeg; synthetic PCM and injected five-second transport outage, no audio hardware"]
    fn supervised_sender_recovers_current_pcm_after_five_second_transport_outage() {
        use axum::response::IntoResponse;
        let root = tempfile::tempdir().unwrap();
        let server = amp_cast::Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let offline = Arc::new(AtomicBool::new(false));
        let failed = offline.clone();
        let attempts = Arc::new(Mutex::new(Vec::<bool>::new()));
        let requests = attempts.clone();
        let app = amp_cast::router(server.clone()).layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let failed = failed.clone();
                let requests = requests.clone();
                async move {
                    if request.uri().path() == "/api/live/start" {
                        requests
                            .lock()
                            .unwrap()
                            .push(request.headers().contains_key("x-amp-previous-session"));
                    }
                    if failed.load(Relaxed) && request.uri().path().starts_with("/api/live/") {
                        return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
                    }
                    next.run(request).await
                }
            },
        ));
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let address = listener.local_addr().unwrap().to_string();
        runtime.spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let (mut producer, main) = rtrb::RingBuffer::new(44100);
        let stop = Arc::new(AtomicBool::new(false));
        let feed_stop = stop.clone();
        let frequency = Arc::new(AtomicU32::new(440));
        let tone = frequency.clone();
        let feed = std::thread::spawn(move || {
            let mut phase = 0f32;
            while !feed_stop.load(Relaxed) {
                for _ in 0..1024 {
                    phase = (phase + std::f32::consts::TAU * tone.load(Relaxed) as f32 / 44100.)
                        % std::f32::consts::TAU;
                    let frame = phase.sin() * 0.1;
                    let _ = producer.push([frame, frame]);
                }
                std::thread::sleep(std::time::Duration::from_millis(23));
            }
        });
        let child = Arc::new(Mutex::new(encoder(44100, false).unwrap()));
        let status = Arc::new(Mutex::new(String::new()));
        let sender = Sender {
            main: Some(main),
            samples: None,
            sample_rate: 44100,
            normalize: false,
            address: address.clone(),
            password: String::new(),
            player_only: true,
            level: Arc::new(AtomicU32::new(1f32.to_bits())),
            muted: Arc::new(AtomicBool::new(false)),
            stop: stop.clone(),
            child: child.clone(),
            status: status.clone(),
            session: None,
        };
        let worker = std::thread::spawn(move || sender.run());
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(1)))
            .proxy(None)
            .build()
            .into();
        let manifest = |agent: &ureq::Agent| {
            agent
                .get(format!("http://{address}/api/audio/manifest"))
                .call()
                .ok()
                .and_then(|mut r| r.body_mut().read_json::<serde_json::Value>().ok())
        };
        let started = std::time::Instant::now();
        let first = loop {
            if let Some(value) = manifest(&agent) {
                break value;
            }
            assert!(
                started.elapsed() < std::time::Duration::from_secs(8),
                "{}",
                status.lock().unwrap()
            );
            std::thread::sleep(std::time::Duration::from_millis(30));
        };
        frequency.store(880, Relaxed);
        offline.store(true, Relaxed);
        std::thread::sleep(std::time::Duration::from_secs(5));
        offline.store(false, Relaxed);
        let resumed_at = std::time::Instant::now();
        let resumed = loop {
            if let Some(value) = manifest(&agent)
                && value["epoch"] != first["epoch"]
            {
                break value;
            }
            assert!(
                resumed_at.elapsed() < std::time::Duration::from_secs(10),
                "{}",
                status.lock().unwrap()
            );
            std::thread::sleep(std::time::Duration::from_millis(30));
        };
        let epoch = resumed["epoch"].as_str().unwrap();
        let sequence = resumed["start_sequence"].as_u64().unwrap();
        let encoded = agent
            .get(format!(
                "http://{address}/api/audio/chunks/{sequence}?epoch={epoch}"
            ))
            .call()
            .unwrap()
            .body_mut()
            .read_to_vec()
            .unwrap();
        let file = root.path().join("resumed.mp3");
        std::fs::write(&file, encoded).unwrap();
        let decoded = amp_core::command("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(file)
            .args(["-f", "s16le", "pipe:1"])
            .output()
            .unwrap();
        assert!(
            decoded.status.success(),
            "{}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        let samples: Vec<_> = decoded
            .stdout
            .as_chunks::<4>()
            .0
            .iter()
            .map(|frame| i16::from_le_bytes([frame[0], frame[1]]))
            .collect();
        assert!(samples.len() > 4096);
        // Ignore codec priming and measure narrow-band energy rather than counting
        // zero crossings in low-level MP3 quantization noise.
        let window = &samples[2048..samples.len().min(10240)];
        let power = |hz: f64| {
            let (real, imag) = window
                .iter()
                .enumerate()
                .fold((0., 0.), |(r, i), (n, sample)| {
                    let phase = std::f64::consts::TAU * hz * n as f64 / 44100.;
                    (
                        r + *sample as f64 * phase.cos(),
                        i + *sample as f64 * phase.sin(),
                    )
                });
            real * real + imag * imag
        };
        assert!(
            power(880.) > power(440.) * 8.,
            "resumed stale440Hz PCM instead of current880Hz"
        );
        let requests = attempts.lock().unwrap().clone();
        assert!(!requests[0]);
        assert!(requests[1..].iter().all(|guarded| *guarded));
        // A deliberate new owner must win; automatic recovery must not steal it back.
        let next: serde_json::Value = agent
            .post(format!("http://{address}/api/live/start"))
            .send_empty()
            .unwrap()
            .body_mut()
            .read_json()
            .unwrap();
        let next_token = next["session"].as_str().unwrap();
        let stopped_at = std::time::Instant::now();
        while !stop.load(Relaxed) {
            assert!(
                stopped_at.elapsed() < std::time::Duration::from_secs(5),
                "producer tried to reclaim a new owner"
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        worker.join().unwrap();
        feed.join().unwrap();
        assert_eq!(
            agent
                .post(format!("http://{address}/api/live/audio"))
                .header("x-amp-live-session", next_token)
                .send(b"new owner remains active")
                .unwrap()
                .status()
                .as_u16(),
            204
        );
        runtime.block_on(server.shutdown());
    }
    #[test]
    fn lost_ack_retries_identical_upload_only_when_server_supports_sequences() {
        use std::net::TcpListener;
        fn run(sequenced: bool) -> Vec<Vec<u8>> {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let worker = std::thread::spawn(move || {
                let mut requests = Vec::new();
                for attempt in 0..if sequenced { 2 } else { 1 } {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                        .unwrap();
                    let mut data = Vec::new();
                    let mut byte = [0];
                    while !data.ends_with(b"\r\n\r\n") {
                        stream.read_exact(&mut byte).unwrap();
                        data.push(byte[0]);
                    }
                    let header = String::from_utf8_lossy(&data);
                    let length = header
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap();
                    let mut body = vec![0; length];
                    stream.read_exact(&mut body).unwrap();
                    if sequenced {
                        assert!(
                            header
                                .to_ascii_lowercase()
                                .contains("x-amp-live-sequence: 7")
                        );
                    } else {
                        assert!(!header.to_ascii_lowercase().contains("x-amp-live-sequence:"));
                    }
                    requests.push(body);
                    // First response is lost after the server receives the complete immutable body.
                    if attempt > 0 {
                        stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                    }
                }
                requests
            });
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .proxy(None)
                .max_redirects(0)
                .build()
                .into();
            let result = upload(
                &agent,
                &address,
                "password",
                "session",
                sequenced.then_some(7),
                b"immutable audio",
                &AtomicBool::new(false),
            );
            assert_eq!(result.is_ok(), sequenced);
            worker.join().unwrap()
        }
        let retried = run(true);
        assert_eq!(retried, vec![b"immutable audio".to_vec(); 2]);
        assert_eq!(run(false), vec![b"immutable audio".to_vec()]);
    }
    #[test]
    #[ignore = "requires native audio and FFmpeg; output muted"]
    fn master_tap_encodes_and_streams_through_real_cast_server() {
        check_stream(false);
    }
    #[test]
    #[ignore = "requires native audio and FFmpeg; output muted"]
    fn normalized_master_tap_encodes_and_streams_through_real_cast_server() {
        check_stream(true);
    }
    fn check_stream(normalize: bool) {
        let root = tempfile::tempdir().unwrap();
        let library = amp_core::Library {
            tracks: vec![],
            tree: amp_core::Folder {
                name: String::new(),
                folders: vec![],
                tracks: vec![],
            },
        };
        let server = amp_cast::Cast::new(root.path().into(), library, None);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let app = amp_cast::router(server.clone());
        runtime.spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let main = crate::mixer::Engine::new().unwrap();
        main.set_master(0.);
        let aux = crate::mixer::Engine::new().unwrap();
        aux.set_master(0.);
        let broadcast = if normalize {
            Broadcast::start_source_normalized(
                &main,
                &aux,
                address.clone(),
                String::new(),
                false,
                true,
            )
            .unwrap()
        } else {
            Broadcast::start_source(&main, &aux, address.clone(), String::new(), false).unwrap()
        };
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(5)))
            .proxy(None)
            .build()
            .into();
        let start = std::time::Instant::now();
        loop {
            let s: serde_json::Value = agent
                .get(format!("http://{address}/api/status"))
                .call()
                .unwrap()
                .body_mut()
                .read_json()
                .unwrap();
            if s["running"] == true {
                break;
            }
            assert!(
                start.elapsed().as_secs() < 6,
                "{}",
                broadcast.status.lock().unwrap()
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let mut response = agent
            .get(format!("http://{address}/listen.mp3"))
            .call()
            .unwrap();
        let mut encoded = [0; 4096];
        response
            .body_mut()
            .as_reader()
            .read_exact(&mut encoded)
            .unwrap();
        let file = root.path().join("live.mp3");
        std::fs::write(&file, encoded).unwrap();
        let result = amp_core::command("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(file)
            .args(["-t", "0.05", "-f", "f32le", "pipe:1"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!result.stdout.is_empty());
        drop(broadcast);
        main.stop_broadcast_tap();
        aux.stop_broadcast_tap();
        runtime.block_on(server.shutdown());
    }
}
