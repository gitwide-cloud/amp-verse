//! A bounded HTTP reader feeds pipe-only FFplay builds. Never give a URL to FFplay.
//! The supervisor owns the child; the pump owns only HTTP + stdin, so killing the
//! child can always unblock a stalled pipe write. No UI thread waits on I/O.
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const READ_TIMEOUT: Duration = Duration::from_secs(2);
const STALL_TIMEOUT: Duration = Duration::from_secs(5);
const CHUNK: usize = 8192;
const DIAGNOSTIC_LIMIT: usize = 2048;

#[derive(Clone, Default)]
pub(crate) struct Status {
    pub message: String,
    pub finished: bool,
}
pub(crate) struct Receiver {
    cancel: Arc<AtomicBool>,
    state: Arc<Mutex<Status>>,
    worker: Option<JoinHandle<()>>,
}
impl Receiver {
    pub fn start(url: &str, gate: Arc<crate::output_admission::Gate>) -> Result<Self, String> {
        Self::with_owned_decoder(url, gate, || decoder_command(amp_core::command("ffplay")))
    }
    #[cfg(test)]
    fn with_decoder<F>(url: &str, decoder: F) -> Result<Self, String>
    where
        F: Fn() -> Command + Send + 'static,
    {
        Self::with_owned_decoder(url, Arc::default(), decoder)
    }
    fn with_owned_decoder<F>(
        url: &str,
        gate: Arc<crate::output_admission::Gate>,
        decoder: F,
    ) -> Result<Self, String>
    where
        F: Fn() -> Command + Send + 'static,
    {
        let parsed = url::Url::parse(url).map_err(|_| "Invalid station stream address")?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
        {
            return Err(
                "Station streams require an HTTP(S) address without embedded credentials.".into(),
            );
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(Status {
            message: "Connecting to station…".into(),
            finished: false,
        }));
        let worker_cancel = cancel.clone();
        let worker_state = state.clone();
        let address = parsed.to_string();
        let worker = thread::Builder::new()
            .name("amp-station-receiver".into())
            .spawn(move || {
                match gate.acquire(&worker_cancel) {
                    Ok(_lease) => supervise(&address, &worker_cancel, &worker_state, decoder),
                    Err(error) => message(&worker_state, error),
                }
                let mut status = worker_state.lock().unwrap();
                status.finished = true;
                if worker_cancel.load(Ordering::Acquire) {
                    status.message = "Listening stopped.".into();
                }
            })
            .map_err(|e| format!("Could not start station receiver: {e}"))?;
        Ok(Self {
            cancel,
            state,
            worker: Some(worker),
        })
    }
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn status(&mut self) -> Status {
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished)
            && let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            *self.state.lock().unwrap() = Status {
                message: "Station receiver stopped unexpectedly.".into(),
                finished: true,
            };
        }
        self.state.lock().unwrap().clone()
    }
}
impl Drop for Receiver {
    fn drop(&mut self) {
        self.stop();
        // The finite supervisor retains ownership and reaps the decoder. Do not
        // block a window/UI drop on DNS, an HTTP read, or a stalled pipe.
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished)
            && let Some(worker) = self.worker.take()
        {
            let _ = worker.join();
        }
    }
}
fn decoder_command(mut command: Command) -> Command {
    command.args([
        "-nodisp",
        "-autoexit",
        "-loglevel",
        "error",
        "-protocol_whitelist",
        "file,pipe",
        "-probesize",
        "32768",
        "-analyzeduration",
        "0",
        "-i",
        "pipe:0",
    ]);
    command
}
fn message(state: &Mutex<Status>, text: impl Into<String>) {
    state.lock().unwrap().message = text.into();
}
fn canceled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Acquire)
}
fn delay(cancel: &AtomicBool, duration: Duration) {
    let start = Instant::now();
    while !canceled(cancel) && start.elapsed() < duration {
        thread::sleep(Duration::from_millis(20));
    }
}
fn supervise<F: Fn() -> Command>(
    url: &str,
    cancel: &Arc<AtomicBool>,
    state: &Arc<Mutex<Status>>,
    decoder: F,
) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .proxy(None)
        .max_redirects(0)
        .timeout_resolve(Some(Duration::from_secs(2)))
        .timeout_connect(Some(Duration::from_secs(2)))
        .timeout_recv_response(Some(Duration::from_secs(2)))
        .timeout_send_request(Some(Duration::from_secs(2)))
        .timeout_per_write(Some(Duration::from_secs(2)))
        .timeout_per_read(Some(READ_TIMEOUT))
        .build()
        .into();
    let mut failures = 0;
    while !canceled(cancel) {
        if failures > 0 {
            message(state, format!("Reconnecting station ({failures}/5)…"));
            delay(cancel, Duration::from_millis(250 * failures));
            if canceled(cancel) {
                break;
            }
        }
        let response = match agent.get(url).call() {
            Ok(response) => response,
            Err(error) => {
                // Ureq's OS resolver helper is not cancellable. Never create a
                // second unresolved lookup after its bounded caller timed out.
                if matches!(error, ureq::Error::Timeout(ureq::Timeout::Resolve)) {
                    message(
                        state,
                        "Station name lookup timed out. Reconnect using the verified LAN address.",
                    );
                    break;
                }
                failures += 1;
                if failures >= 5 {
                    message(state, format!("Station connection failed: {error}"));
                    break;
                }
                continue;
            }
        };
        if canceled(cancel) {
            break;
        }
        let start = Instant::now();
        let mut command = decoder();
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = match crate::receiver_process::spawn(&mut command) {
            Ok(child) => child,
            Err(error) => {
                message(state, format!("Could not start station decoder: {error}"));
                break;
            }
        };
        let Some(mut input) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            message(state, "Station decoder has no input pipe.");
            break;
        };
        let diagnostic = Arc::new(Mutex::new(Vec::<u8>::new()));
        let diagnostic_copy = diagnostic.clone();
        let stderr = child.stderr.take();
        let errors = thread::spawn(move || {
            if let Some(mut stderr) = stderr {
                let mut bytes = [0u8; 512];
                while let Ok(count) = stderr.read(&mut bytes) {
                    if count == 0 {
                        break;
                    }
                    let mut tail = diagnostic_copy.lock().unwrap();
                    tail.extend_from_slice(&bytes[..count]);
                    let excess = tail.len().saturating_sub(DIAGNOSTIC_LIMIT);
                    tail.drain(..excess);
                }
            }
        });
        let progress = Arc::new(AtomicU64::new(0));
        let pump_progress = progress.clone();
        let pump_cancel = cancel.clone();
        let pump_error = Arc::new(Mutex::new(None::<String>));
        let pump_failure = pump_error.clone();
        let pump = thread::spawn(move || {
            let mut reader = response.into_body().into_reader();
            let mut chunk = [0u8; CHUNK];
            while !canceled(&pump_cancel) {
                match reader.read(&mut chunk) {
                    Ok(0) => {
                        *pump_failure.lock().unwrap() = Some("Station stream ended.".into());
                        break;
                    }
                    Ok(count) => {
                        if let Err(error) = input.write_all(&chunk[..count]) {
                            *pump_failure.lock().unwrap() =
                                Some(format!("Station decoder input failed: {error}"));
                            break;
                        }
                        pump_progress
                            .store(start.elapsed().as_millis() as u64 + 1, Ordering::Release);
                    }
                    Err(error) => {
                        *pump_failure.lock().unwrap() =
                            Some(format!("Station stream read failed: {error}"));
                        break;
                    }
                }
            }
        });
        message(state, "Listening to station.");
        let mut reason = None;
        while !canceled(cancel) {
            match child.try_wait() {
                Ok(Some(exit)) => {
                    reason = Some(format!("Station decoder exited ({exit})."));
                    break;
                }
                Err(error) => {
                    reason = Some(format!("Cannot inspect station decoder: {error}"));
                    break;
                }
                Ok(None) => {}
            }
            if pump.is_finished() {
                break;
            }
            let elapsed = start.elapsed().as_millis() as u64;
            if elapsed.saturating_sub(progress.load(Ordering::Acquire))
                > STALL_TIMEOUT.as_millis() as u64
            {
                reason = Some("Station stream stalled; restarting decoder.".into());
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        // Independent ownership is essential: kill unblocks write_all even if
        // the decoder stopped reading; per-read timeout bounds the HTTP reader.
        let _ = child.kill();
        let _ = child.wait();
        let _ = pump.join();
        let _ = errors.join();
        if canceled(cancel) {
            break;
        }
        let reason = reason
            .or_else(|| pump_error.lock().unwrap().clone())
            .unwrap_or_else(|| "Station receiver stopped.".into());
        let stderr = String::from_utf8_lossy(&diagnostic.lock().unwrap())
            .trim()
            .to_owned();
        let detail = if stderr.is_empty() {
            reason
        } else {
            format!("{reason} {stderr}")
        };
        if start.elapsed() > Duration::from_secs(15) {
            failures = 0;
        }
        failures += 1;
        if failures >= 5 {
            message(state, detail);
            break;
        }
        message(state, detail);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    fn fixture(bytes: Vec<u8>, idle: bool) -> (String, Arc<AtomicBool>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/listen.mp3", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let worker = thread::spawn(move || {
            while !canceled(&done) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket
                            .set_write_timeout(Some(Duration::from_millis(100)))
                            .unwrap();
                        socket
                            .set_read_timeout(Some(Duration::from_millis(100)))
                            .unwrap();
                        let mut request = [0; 2048];
                        let _ = socket.read(&mut request);
                        let _=socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nConnection: close\r\n\r\n");
                        if idle {
                            while !canceled(&done) {
                                thread::sleep(Duration::from_millis(10));
                            }
                        } else {
                            for chunk in bytes.chunks(CHUNK) {
                                if canceled(&done) || socket.write_all(chunk).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
        });
        (url, stop, worker)
    }
    fn child_command(mode: &'static str) -> Command {
        let mut c = Command::new(std::env::current_exe().unwrap());
        c.args([
            "--exact",
            "station_receiver::tests::decoder_fixture_child",
            "--ignored",
            "--nocapture",
        ])
        .env("AMP_RECEIVER_TEST_CHILD", mode);
        c
    }
    fn await_finished(receiver: &mut Receiver, timeout: Duration) -> Status {
        let start = Instant::now();
        loop {
            let state = receiver.status();
            if state.finished {
                return state;
            }
            assert!(
                start.elapsed() < timeout,
                "Receiver failed to reap within {timeout:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    #[ignore = "Owned subprocess fixture invoked only by lifecycle tests"]
    fn decoder_fixture_child() {
        match std::env::var("AMP_RECEIVER_TEST_CHILD").as_deref() {
            Ok("stall") => thread::sleep(Duration::from_secs(30)),
            Ok("fail") => {
                eprintln!("fixture decoder failure");
                std::process::exit(17);
            }
            _ => panic!("Only the owning fixture may invoke this child"),
        }
    }
    #[test]
    fn rejects_embedded_credentials_and_non_http_without_spawning() {
        for url in [
            "file:///secret.mp3",
            "http://user:password@localhost/a",
            "http://localhost/a#fragment",
        ] {
            assert!(Receiver::with_decoder(url, || panic!("Must not spawn")).is_err());
        }
    }
    #[test]
    fn idle_and_stalled_pipe_cancellation_reaps_without_ui_wait() {
        for (bytes, idle) in [(Vec::new(), true), (vec![0u8; 2 * 1024 * 1024], false)] {
            let (url, stop, server) = fixture(bytes, idle);
            let mut receiver = Receiver::with_decoder(&url, || child_command("stall")).unwrap();
            thread::sleep(Duration::from_millis(200));
            let start = Instant::now();
            receiver.stop();
            assert!(start.elapsed() < Duration::from_millis(50));
            await_finished(&mut receiver, Duration::from_secs(3));
            assert!(receiver.worker.is_none());
            stop.store(true, Ordering::Release);
            server.join().unwrap();
        }
    }

    #[test]
    fn window_drop_cancels_without_blocking_and_worker_reaps() {
        let (url, stop, server) = fixture(Vec::new(), true);
        let receiver = Receiver::with_decoder(&url, || child_command("stall")).unwrap();
        let state = receiver.state.clone();
        thread::sleep(Duration::from_millis(100));
        let start = Instant::now();
        drop(receiver);
        assert!(start.elapsed() < Duration::from_millis(50));
        while !state.lock().unwrap().finished {
            assert!(start.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(10));
        }
        stop.store(true, Ordering::Release);
        server.join().unwrap();
    }
    #[test]
    fn dead_decoder_is_reaped_and_reports_bounded_error() {
        let (url, stop, server) = fixture(vec![0u8; 2 * 1024 * 1024], false);
        let mut receiver = Receiver::with_decoder(&url, || child_command("fail")).unwrap();
        let state = await_finished(&mut receiver, Duration::from_secs(8));
        assert!(
            state.message.contains("fixture decoder failure"),
            "{}",
            state.message
        );
        assert!(state.message.len() < DIAGNOSTIC_LIMIT + 200);
        stop.store(true, Ordering::Release);
        server.join().unwrap();
    }

    #[test]
    fn dropped_stream_restarts_decoder_and_cancel_reaps_replacement() {
        let (url, stop, server) = fixture(vec![0u8; 2 * 1024 * 1024], false);
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let spawned = attempts.clone();
        let mut receiver = Receiver::with_decoder(&url, move || {
            let attempt = spawned.fetch_add(1, Ordering::SeqCst);
            // Only the original decoder fails. Keep its replacement alive so
            // this test verifies cancellation/reaping, not exhaustion of five
            // deliberately failing children racing a separate counter read.
            child_command(if attempt == 0 { "fail" } else { "stall" })
        })
        .unwrap();
        let started = Instant::now();
        loop {
            // Read the counter first: Listening must then belong to the
            // replacement, after its spawn and pump setup have completed.
            let count = attempts.load(Ordering::SeqCst);
            let status = receiver.status();
            assert!(
                !status.finished,
                "Receiver ended before replacement was listening: attempts={count}, status={}",
                status.message
            );
            if count >= 2 && status.message == "Listening to station." {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(4),
                "Replacement did not start: attempts={count}, status={}",
                status.message
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            2,
            "Exactly one live replacement must be established before cancellation"
        );
        receiver.stop();
        let finished = await_finished(&mut receiver, Duration::from_secs(3));
        assert_eq!(finished.message, "Listening stopped.");
        assert!(
            receiver.worker.is_none(),
            "Cancellation must join the reaper after the replacement was reaped"
        );
        stop.store(true, Ordering::Release);
        server.join().unwrap();
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            2,
            "Cancellation must not launch a further decoder"
        );
    }

    #[test]
    #[ignore = "Real FFmpeg/FFplay codec test with SDL dummy audio; hosted CI supplies exact shipped Windows executable"]
    fn network_disabled_ffplay_decodes_http_via_pipe() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("tone.mp3");
        let result = amp_core::command("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-c:a",
                "libmp3lame",
                "-y",
            ])
            .arg(&path)
            .status()
            .unwrap();
        assert!(result.success());
        let data = std::fs::read(path).unwrap();
        let expected = data.len();
        let (url, stop, server) = fixture(data, false);
        let executable = std::env::var_os("AMP_RECEIVER_FFPLAY")
            .expect("Supply tested FFplay executable explicitly");
        let mut command = Command::new(executable);
        command.env("SDL_AUDIODRIVER", "dummy");
        let mut child = decoder_command(command)
            .args(["-loglevel", "info"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        let mut response = ureq::get(&url).call().unwrap();
        let copied = std::io::copy(
            &mut response.body_mut().as_reader(),
            &mut child.stdin.take().unwrap(),
        )
        .unwrap();
        assert_eq!(copied, expected as u64);
        let deadline = Instant::now() + Duration::from_secs(8);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                stop.store(true, Ordering::Release);
                server.join().unwrap();
                panic!("Pipe decoder did not complete within eight seconds");
            }
            thread::sleep(Duration::from_millis(20));
        }
        let output = child.wait_with_output().unwrap();
        stop.store(true, Ordering::Release);
        server.join().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stderr}");
        assert!(stderr.contains("Audio: mp3"), "{stderr}");
        assert!(
            start.elapsed() >= Duration::from_millis(1200),
            "Must actually clock decoded audio through SDL dummy"
        );
    }
}
