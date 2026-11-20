//! Explicit local input capture. Child ownership and all waits stay off the UI thread.
use eframe::egui;
use std::{
    ffi::OsString,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

const STOP_GRACE: Duration = Duration::from_secs(2);
const STDERR_LIMIT: usize = 16 * 1024;

enum Event {
    Started,
    Finished(Result<PathBuf, String>),
}
struct Session {
    stop: mpsc::Sender<()>,
    events: mpsc::Receiver<Event>,
    process: Arc<Mutex<Option<Child>>>,
}

pub(super) struct Recorder {
    source: String,
    session: Option<Session>,
    started: Option<Instant>,
    stopping: bool,
    status: String,
    completed: Option<PathBuf>,
}
impl Default for Recorder {
    fn default() -> Self {
        Self {
            source: if cfg!(target_os = "linux") {
                "default".into()
            } else {
                String::new()
            },
            session: None,
            started: None,
            stopping: false,
            status: "Idle".into(),
            completed: None,
        }
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        // No join, write to a process pipe, or child wait on an egui/Drop path.
        self.stop();
    }
}
impl Recorder {
    pub(super) fn take_completed(&mut self) -> Option<PathBuf> {
        self.completed.take()
    }
    pub(super) fn stop(&mut self) {
        if let Some(session) = &self.session {
            let _ = session.stop.send(());
            self.stopping = true;
            self.status = "Stopping capture and saving WAV…".into();
        }
    }
    // Called only during module/app disposal, never from a normal Stop frame.
    pub(super) fn shutdown(&mut self) {
        self.stop();
        let Some(session) = self.session.take() else {
            return;
        };
        let deadline = Instant::now() + STOP_GRACE + Duration::from_secs(1);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match session.events.recv_timeout(remaining) {
                Ok(Event::Started) => continue,
                Ok(Event::Finished(result)) => {
                    if let Err(error) = result {
                        eprintln!("AMP Lab recorder: {error}");
                    }
                    break;
                }
                Err(_) => {
                    // Final best-effort kill through the actual owned Child handle, not a
                    // potentially reused PID. The UI never waits for the child itself.
                    if let Ok(mut process) = session.process.try_lock()
                        && let Some(child) = process.as_mut()
                    {
                        let _ = child.kill();
                    }
                    eprintln!("AMP Lab recorder shutdown timed out; capture termination requested");
                    break;
                }
            }
        }
        self.started = None;
        self.stopping = false;
    }
    fn start(&mut self) {
        if self.session.is_some() {
            return;
        }
        if self.source.trim().is_empty() {
            self.status = "Enter an audio capture device/source first".into();
            return;
        }
        let input = match device_input(self.source.trim()) {
            Ok(input) => input,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let Some(output) = rfd::FileDialog::new()
            .add_filter("WAV", &["wav"])
            .set_file_name("amp-lab-recording.wav")
            .save_file()
        else {
            return;
        };
        self.session = Some(capture(output, input));
        self.stopping = false;
        self.status = "Opening capture source…".into();
    }
    pub(super) fn poll(&mut self, ctx: &egui::Context) {
        let mut finished = None;
        if let Some(session) = &self.session {
            loop {
                match session.events.try_recv() {
                    Ok(Event::Started) => {
                        self.started = Some(Instant::now());
                        if !self.stopping {
                            self.status = "Recording (24-bit WAV, 48 kHz)".into();
                        }
                    }
                    Ok(Event::Finished(result)) => {
                        finished = Some(result);
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = Some(Err("Capture worker stopped unexpectedly".into()));
                        break;
                    }
                }
            }
            // Keep polling even when the recorder disclosure or waveform tab is closed.
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if let Some(result) = finished {
            self.session = None;
            self.started = None;
            self.stopping = false;
            match result {
                Ok(path) => {
                    self.status = format!("Recorded {}", path.display());
                    self.completed = Some(path);
                }
                Err(error) => self.status = error,
            }
        }
    }
    pub(super) fn show(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Record audio (authorized sources)").default_open(false).show(ui, |ui| {
            ui.label("Capture an explicitly selected local input, then edit in AMP Lab. Record audio you have permission to capture.");
            #[cfg(target_os = "windows")]
            ui.label("Windows: enter a DirectShow audio device name, such as Stereo Mix if your driver provides it. Use FFmpeg -list_devices true -f dshow -i dummy to list devices. Universal WASAPI loopback is not implemented.");
            #[cfg(target_os = "linux")]
            ui.label("Linux: enter a PulseAudio/PipeWire source. For desktop output use a monitor from 'pactl list short sources'; 'default' normally records your default input.");
            #[cfg(not(any(target_os = "windows", target_os = "linux")))]
            ui.label("Capture is supported on Windows and Linux only.");
            ui.horizontal_wrapped(|ui| {
                ui.label("Source:");
                ui.add_enabled(self.session.is_none(), egui::TextEdit::singleline(&mut self.source).desired_width(260.));
                if self.session.is_none() {
                    if ui.button("● Record to WAV...").clicked() { self.start(); }
                } else if ui.add_enabled(!self.stopping, egui::Button::new("■ Stop and open in editor")).clicked() {
                    self.stop();
                }
            });
            if let Some(start) = self.started { ui.strong(format!("REC  {:02}:{:02}", start.elapsed().as_secs()/60, start.elapsed().as_secs()%60)); }
            ui.label(&self.status);
            ui.weak("Saved losslessly first. Leaving AMP Lab stops capture. Existing trim, fades, normalization, preview and 256 kbps MP3 export apply afterward.");
        });
    }
}

fn device_input(source: &str) -> Result<Vec<OsString>, String> {
    #[cfg(target_os = "windows")]
    {
        Ok(vec![
            "-f".into(),
            "dshow".into(),
            "-i".into(),
            format!("audio={source}").into(),
        ])
    }
    #[cfg(target_os = "linux")]
    {
        Ok(vec![
            "-f".into(),
            "pulse".into(),
            "-i".into(),
            source.into(),
        ])
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = source;
        Err("Recording is supported on Windows and Linux only".into())
    }
}

fn capture(output: PathBuf, input: Vec<OsString>) -> Session {
    let (stop, requests) = mpsc::channel();
    let (events_tx, events) = mpsc::channel();
    let process = Arc::new(Mutex::new(None));
    let worker_process = process.clone();
    std::thread::spawn(move || {
        let result = record(&output, &input, requests, &events_tx, &worker_process);
        let _ = events_tx.send(Event::Finished(result));
    });
    Session {
        stop,
        events,
        process,
    }
}

fn terminate(process: &Mutex<Option<Child>>) {
    // Only a capture worker calls wait; shutdown can still return at its own deadline.
    if let Some(mut child) = process.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn diagnostics(stderr: &Mutex<Vec<u8>>) -> String {
    String::from_utf8_lossy(&stderr.lock().unwrap())
        .trim()
        .to_owned()
}

fn record(
    output: &Path,
    input: &[OsString],
    requests: mpsc::Receiver<()>,
    events: &mpsc::Sender<Event>,
    process: &Mutex<Option<Child>>,
) -> Result<PathBuf, String> {
    if output.exists() {
        return Err("Choose a new filename: recordings never overwrite existing audio".into());
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let staging = tempfile::Builder::new()
        .prefix(".amp-recording-")
        .suffix(".wav")
        .tempfile_in(parent)
        .map_err(|e| format!("Cannot create recording: {e}"))?;
    let mut command = amp_core::command("ffmpeg");
    // -y applies only to our private, exclusively created staging file, never to output.
    command
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(input)
        .args([
            "-vn",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-c:a",
            "pcm_s24le",
            "-f",
            "wav",
        ])
        .arg(staging.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not start FFmpeg: {e}"))?;
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let (stderr_done_tx, stderr_done) = mpsc::channel();
    if let Some(mut reader) = child.stderr.take() {
        let captured = stderr.clone();
        std::thread::spawn(move || {
            let mut bytes = [0u8; 2048];
            while let Ok(count) = reader.read(&mut bytes) {
                if count == 0 {
                    break;
                }
                let mut tail = captured.lock().unwrap();
                tail.extend_from_slice(&bytes[..count]);
                let excess = tail.len().saturating_sub(STDERR_LIMIT);
                if excess > 0 {
                    tail.drain(..excess);
                }
            }
            let _ = stderr_done_tx.send(());
        });
    }
    *process.lock().unwrap() = Some(child);
    let _ = events.send(Event::Started);
    let mut stopping = None;
    let status = loop {
        let status = process.lock().unwrap().as_mut().unwrap().try_wait();
        match status {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                terminate(process);
                return Err(format!(
                    "Capture status failed: {error}. {}",
                    diagnostics(&stderr)
                ));
            }
        }
        if stopping.is_none() && !matches!(requests.try_recv(), Err(mpsc::TryRecvError::Empty)) {
            let stdin = process.lock().unwrap().as_mut().unwrap().stdin.take();
            if let Some(mut stdin) = stdin {
                let _ = stdin.write_all(b"q\n");
            }
            stopping = Some(Instant::now());
        }
        if stopping.is_some_and(|time| time.elapsed() >= STOP_GRACE) {
            terminate(process);
            return Err(format!(
                "Capture did not stop within two seconds and was terminated. {}",
                diagnostics(&stderr)
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    process.lock().unwrap().take();
    let _ = stderr_done.recv_timeout(Duration::from_millis(200));
    if !status.success() {
        return Err(format!(
            "Capture failed ({status}). Check source/device access. {}",
            diagnostics(&stderr)
        ));
    }
    if !wav_has_audio(staging.path()).unwrap_or(false) {
        return Err(format!(
            "Capture produced no usable audio. {}",
            diagnostics(&stderr)
        ));
    }
    match staging.persist_noclobber(output) {
        Ok(_) => Ok(output.to_path_buf()),
        Err(error) => {
            let reason = error.error.to_string();
            // An output created during capture must survive; keep captured audio for recovery.
            let recovered = error.file.keep().map(|(_, p)| p).map_err(|e| {
                format!("Recording could not be saved: {reason}; recovery failed: {e}")
            })?;
            Err(format!(
                "Recording destination was not replaced: {reason}. Captured WAV retained at {}",
                recovered.display()
            ))
        }
    }
}
// Inspect only bounded chunk headers, never load a potentially hours-long capture.
// A valid WAV header can exceed 100 bytes even when it contains zero audio frames.
fn wav_has_audio(path: &Path) -> std::io::Result<bool> {
    let mut file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    let mut header = [0u8; 12];
    if length < 12 {
        return Ok(false);
    }
    file.read_exact(&mut header)?;
    if &header[..4] != b"RIFF" || &header[8..] != b"WAVE" {
        return Ok(false);
    }
    let declared = u64::from(u32::from_le_bytes(header[4..8].try_into().unwrap())) + 8;
    if declared < 12 || declared > length {
        return Ok(false);
    }
    let length = declared;
    let mut offset = 12u64;
    let mut block_align = None;
    for _ in 0..128 {
        if offset.checked_add(8).is_none_or(|end| end > length) {
            return Ok(false);
        }
        file.seek(SeekFrom::Start(offset))?;
        let mut chunk = [0u8; 8];
        file.read_exact(&mut chunk)?;
        let size = u32::from_le_bytes(chunk[4..].try_into().unwrap()) as u64;
        let start = offset + 8;
        let Some(end) = start.checked_add(size).filter(|end| *end <= length) else {
            return Ok(false);
        };
        if &chunk[..4] == b"fmt " {
            if size < 16 {
                return Ok(false);
            }
            let mut format = [0u8; 16];
            file.read_exact(&mut format)?;
            let encoding = u16::from_le_bytes(format[..2].try_into().unwrap());
            let channels = u16::from_le_bytes(format[2..4].try_into().unwrap());
            let rate = u32::from_le_bytes(format[4..8].try_into().unwrap());
            let align = u16::from_le_bytes(format[12..14].try_into().unwrap());
            let bits = u16::from_le_bytes(format[14..16].try_into().unwrap());
            if !matches!(encoding, 1 | 0xfffe) || (channels, rate, bits, align) != (2, 48000, 24, 6)
            {
                return Ok(false);
            }
            block_align = Some(u64::from(align));
        } else if &chunk[..4] == b"data" {
            return Ok(block_align.is_some_and(|align| size >= align && size.is_multiple_of(align)));
        }
        offset = end + size % 2;
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn generated() -> Vec<OsString> {
        [
            "-re",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
        ]
        .into_iter()
        .map(Into::into)
        .collect()
    }
    fn wait_for_audio(directory: &Path) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if std::fs::read_dir(directory)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".amp-recording-")
                        && entry.metadata().is_ok_and(|m| m.len() > 16384)
                })
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("Generated FFmpeg source did not write capture audio within five seconds");
    }
    fn started(session: &Session) {
        match session
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("capture start event")
        {
            Event::Started => {}
            Event::Finished(result) => panic!("FFmpeg fixture could not start: {result:?}"),
        }
    }
    fn finished(session: &Session) -> Result<PathBuf, String> {
        loop {
            if let Event::Finished(result) = session
                .events
                .recv_timeout(Duration::from_secs(8))
                .expect("bounded capture shutdown")
            {
                return result;
            }
        }
    }
    fn wav_format(path: &Path) -> (u16, u32, u16, u32) {
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        let mut offset = 12;
        let mut format = None;
        let mut data = 0;
        while offset + 8 <= bytes.len() {
            let size =
                u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let chunk = offset + 8;
            if &bytes[offset..offset + 4] == b"fmt " {
                format = Some((
                    u16::from_le_bytes(bytes[chunk + 2..chunk + 4].try_into().unwrap()),
                    u32::from_le_bytes(bytes[chunk + 4..chunk + 8].try_into().unwrap()),
                    u16::from_le_bytes(bytes[chunk + 14..chunk + 16].try_into().unwrap()),
                ));
            }
            if &bytes[offset..offset + 4] == b"data" {
                data = size as u32;
                assert!(chunk + size <= bytes.len());
                break;
            }
            offset = chunk + size + size % 2;
        }
        let (channels, rate, bits) = format.expect("WAV format chunk");
        (channels, rate, bits, data)
    }
    #[test]
    fn recorder_generated_audio_stops_to_lossless_wav_without_input_hardware() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("recording.wav");
        let session = capture(output.clone(), generated());
        started(&session);
        wait_for_audio(directory.path());
        let before = Instant::now();
        session.stop.send(()).unwrap();
        assert_eq!(finished(&session).unwrap(), output);
        assert!(before.elapsed() < Duration::from_secs(5));
        let (channels, rate, bits, data) = wav_format(&output);
        assert_eq!((channels, rate, bits), (2, 48000, 24));
        assert!(data > 4800);
        let result = amp_core::command("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&output)
            .args(["-f", "null", "-"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    #[test]
    fn recorder_never_replaces_existing_or_racing_output() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("existing.wav");
        std::fs::write(&output, b"owner audio").unwrap();
        let session = capture(output.clone(), generated());
        assert!(finished(&session).unwrap_err().contains("never overwrite"));
        assert_eq!(std::fs::read(&output).unwrap(), b"owner audio");
        let racing = directory.path().join("racing.wav");
        let session = capture(racing.clone(), generated());
        started(&session);
        wait_for_audio(directory.path());
        std::fs::write(&racing, b"created during recording").unwrap();
        session.stop.send(()).unwrap();
        let error = finished(&session).unwrap_err();
        assert!(error.contains("retained at"), "{error}");
        assert_eq!(std::fs::read(&racing).unwrap(), b"created during recording");
        let recovery = std::fs::read_dir(directory.path())
            .unwrap()
            .filter_map(Result::ok)
            .find(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".amp-recording-")
            })
            .unwrap();
        assert!(wav_format(&recovery.path()).3 > 0);
    }
    #[test]
    fn recorder_reports_ffmpeg_diagnostics_and_drop_does_not_wait() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("failed.wav");
        let invalid = ["-f", "lavfi", "-i", "amp_invalid_capture_filter"]
            .into_iter()
            .map(Into::into)
            .collect();
        let session = capture(output.clone(), invalid);
        let error = finished(&session).unwrap_err();
        assert!(
            error.contains("amp_invalid_capture_filter"),
            "stderr detail missing: {error}"
        );
        assert!(!output.exists());
        let output = directory.path().join("closed.wav");
        let session = capture(output.clone(), generated());
        started(&session);
        wait_for_audio(directory.path());
        let mut recorder = Recorder::default();
        assert!(recorder.session.replace(session).is_none());
        // Detach only the event receiver so completion remains observable after UI disposal.
        let events = recorder.session.take().unwrap();
        recorder.session = Some(Session {
            stop: events.stop,
            events: mpsc::channel().1,
            process: events.process,
        });
        let before = Instant::now();
        drop(recorder);
        assert!(before.elapsed() < Duration::from_millis(500));
        assert!(matches!(
            events.events.recv_timeout(Duration::from_secs(8)).unwrap(),
            Event::Finished(Ok(_))
        ));
        assert!(wav_format(&output).3 > 0);
    }
    #[test]
    fn recorder_shutdown_finishes_worker_before_app_disposal_returns() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("exit.wav");
        let session = capture(output.clone(), generated());
        started(&session);
        wait_for_audio(directory.path());
        let process = session.process.clone();
        let mut recorder = Recorder::default();
        assert!(recorder.session.replace(session).is_none());
        let before = Instant::now();
        recorder.shutdown();
        assert!(before.elapsed() < Duration::from_secs(4));
        assert!(
            process.lock().unwrap().is_none(),
            "capture child must be reaped before exit acknowledgment"
        );
        assert!(wav_format(&output).3 > 0);
    }
    #[test]
    fn recorder_rejects_header_only_and_truncated_audio() {
        let directory = tempfile::tempdir().unwrap();
        let fixture = directory.path().join("header-only.wav");
        let mut bytes = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&48000u32.to_le_bytes());
        bytes.extend_from_slice(&288000u32.to_le_bytes());
        bytes.extend_from_slice(&6u16.to_le_bytes());
        bytes.extend_from_slice(&24u16.to_le_bytes());
        bytes.extend_from_slice(b"JUNK");
        bytes.extend_from_slice(&64u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 64]);
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let riff_size = (bytes.len() - 8) as u32;
        bytes[4..8].copy_from_slice(&riff_size.to_le_bytes());
        assert!(bytes.len() > 100);
        std::fs::write(&fixture, &bytes).unwrap();
        assert!(
            !wav_has_audio(&fixture).unwrap(),
            "Header size is not audio payload"
        );
        let data_length = bytes.len() - 4;
        bytes[data_length..].copy_from_slice(&6u32.to_le_bytes());
        std::fs::write(&fixture, &bytes).unwrap();
        assert!(
            !wav_has_audio(&fixture).unwrap(),
            "Declared bytes must actually exist"
        );
        bytes.extend_from_slice(&[0; 6]);
        let riff_size = (bytes.len() - 8) as u32;
        bytes[4..8].copy_from_slice(&riff_size.to_le_bytes());
        std::fs::write(&fixture, &bytes).unwrap();
        assert!(
            wav_has_audio(&fixture).unwrap(),
            "A complete silent audio frame is valid capture data"
        );
        let output = directory.path().join("empty-capture.wav");
        let input = [
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-t",
            "0",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        let session = capture(output.clone(), input);
        let error = finished(&session).unwrap_err();
        assert!(error.contains("no usable audio"), "{error}");
        assert!(
            !output.exists(),
            "Header-only capture must not be published"
        );
    }
}
