//! Optional adaptive listener transport. It follows the existing owner; never starts playback.
use super::*;
use axum::extract::Path;
use std::sync::Mutex as StdMutex;
use tokio::io::AsyncWriteExt;

const PREFIX: &str = "/api/audio/hls";
const PROFILES: [&str; 3] = ["quality", "balanced", "resilient"];
#[derive(Default)]
struct StateData {
    epoch: Option<String>,
    directory: Option<Arc<tempfile::TempDir>>,
    worker: Option<JoinHandle<()>>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    error: Option<String>,
    last_request: Option<std::time::Instant>,
}
#[derive(Default)]
pub(super) struct Hls {
    state: StdMutex<StateData>,
}
impl Hls {
    pub async fn reset(&self) {
        let (worker, stop, directory) = {
            let mut state = self.state.lock().unwrap();
            state.epoch = None;
            state.error = None;
            state.last_request = None;
            (
                state.worker.take(),
                state.stop.take(),
                state.directory.take(),
            )
        };
        if let Some(stop) = stop {
            let _ = stop.send(());
        }
        if let Some(mut worker) = worker
            && tokio::time::timeout(std::time::Duration::from_secs(4), &mut worker)
                .await
                .is_err()
        {
            worker.abort();
            let _ = worker.await;
        }
        // The encoder is reaped before its TempDir can be deleted on Windows.
        drop(directory);
    }
    pub fn snapshot(&self) -> serde_json::Value {
        let state = self.state.lock().unwrap();
        let ready = state.directory.as_ref().is_some_and(|d| ready(d.path()))
            && state.worker.as_ref().is_some_and(|w| !w.is_finished());
        serde_json::json!({"ready":ready,"state":if ready {"ready"} else if state.epoch.is_some() {"preparing"} else {"idle"},"master_url":format!("{PREFIX}/master.m3u8"),"epoch":state.epoch,"codec":"aac","sample_rate":44100,"channels":2,"segment_target_ms":1000,"target_live_buffer_ms":3000,"profiles":[{"id":"quality","bitrate_bps":192000},{"id":"balanced","bitrate_bps":128000},{"id":"resilient","bitrate_bps":96000}],"error":state.error})
    }
}
fn ready(dir: &std::path::Path) -> bool {
    dir.join("master.m3u8").is_file()
        && PROFILES.iter().all(|p| {
            std::fs::read_to_string(dir.join(p).join("index.m3u8"))
                .is_ok_and(|s| s.contains("#EXTINF:"))
        })
}
pub fn routes() -> Router<Arc<Cast>> {
    Router::new()
        .route("/api/audio/hls/master.m3u8", get(master))
        .route("/api/audio/hls/{epoch}/{profile}/{file}", get(asset))
}
fn identity(cast: &Cast, headers: &HeaderMap) -> bool {
    [
        ("x-amp-server", &cast.server_id),
        ("x-amp-station", &cast.station_id),
    ]
    .iter()
    .all(|(key, expected)| {
        headers
            .get(*key)
            .is_none_or(|v| v.to_str().ok() == Some(expected.as_str()))
    })
}
fn pending() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [("retry-after", "1"), ("cache-control", "no-store")],
        "Adaptive audio is preparing",
    )
        .into_response()
}
async fn master(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !identity(&cast, &headers) {
        return StatusCode::CONFLICT.into_response();
    }
    let station = cast.station.lock().await;
    if !station.running {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let generation = station.generation;
    let mut state = cast.hls.state.lock().unwrap();
    state.last_request = Some(std::time::Instant::now());
    if state.worker.as_ref().is_none_or(JoinHandle::is_finished) {
        let directory = match tempfile::tempdir() {
            Ok(d) => Arc::new(d),
            Err(_) => return pending(),
        };
        for profile in PROFILES {
            if std::fs::create_dir(directory.path().join(profile)).is_err() {
                return pending();
            }
        }
        let epoch = amp_core::coordination::token();
        let receiver = cast.audio.subscribe();
        let weak = Arc::downgrade(&cast);
        let output = directory.clone();
        let owner = epoch.clone();
        state.epoch = Some(epoch);
        state.directory = Some(directory);
        state.error = None;
        let (stop, stopped) = tokio::sync::oneshot::channel();
        state.stop = Some(stop);
        state.worker = Some(tokio::spawn(async move {
            let result = encode(receiver, output.path(), weak.clone(), generation, stopped).await;
            if let Some(cast) = weak.upgrade() {
                let mut state = cast.hls.state.lock().unwrap();
                if state.epoch.as_deref() == Some(&owner) {
                    state.epoch = None;
                    state.directory = None;
                    state.error = result.err();
                }
            }
        }));
    }
    let Some(directory) = &state.directory else {
        return pending();
    };
    if !ready(directory.path()) {
        return pending();
    }
    let epoch = state.epoch.as_ref().unwrap();
    match std::fs::read_to_string(directory.path().join("master.m3u8")) {
        Ok(text) => {
            let text = text
                .lines()
                .map(|line| {
                    if line.starts_with('#') || line.is_empty() {
                        line.to_string()
                    } else {
                        format!("{PREFIX}/{epoch}/{line}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            (
                [
                    ("content-type", "application/vnd.apple.mpegurl"),
                    ("cache-control", "no-store"),
                ],
                text,
            )
                .into_response()
        }
        Err(_) => pending(),
    }
}
async fn asset(
    State(cast): State<Arc<Cast>>,
    Path((epoch, profile, file)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    if !identity(&cast, &headers) {
        return StatusCode::CONFLICT.into_response();
    }
    if !PROFILES.contains(&profile.as_str()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let playlist = file == "index.m3u8";
    let init = file == format!("init_{profile}.mp4");
    let segment = file
        .strip_prefix("segment-")
        .and_then(|v| v.strip_suffix(".m4s"))
        .is_some_and(|n| !n.is_empty() && n.len() <= 20 && n.bytes().all(|b| b.is_ascii_digit()));
    if !playlist && !init && !segment {
        return StatusCode::NOT_FOUND.into_response();
    }
    if !cast.station.lock().await.running {
        return StatusCode::GONE.into_response();
    }
    let directory = {
        let mut state = cast.hls.state.lock().unwrap();
        if state.epoch.as_deref() != Some(&epoch) {
            return StatusCode::GONE.into_response();
        }
        state.last_request = Some(std::time::Instant::now());
        state.directory.clone()
    };
    let Some(directory) = directory else {
        return StatusCode::GONE.into_response();
    };
    let loaded = async {
        let file = tokio::fs::File::open(directory.path().join(&profile).join(&file)).await?;
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes).await?;
        Ok::<_, std::io::Error>(bytes)
    }
    .await;
    // A stop/source change during the disk read must not serve bytes from its old epoch.
    if !cast.station.lock().await.running
        || cast.hls.state.lock().unwrap().epoch.as_deref() != Some(&epoch)
    {
        return StatusCode::GONE.into_response();
    }
    match loaded {
        Ok(bytes) if bytes.len() <= 1024 * 1024 => {
            let content = if playlist {
                "application/vnd.apple.mpegurl"
            } else {
                "audio/mp4"
            };
            (
                [("content-type", content), ("cache-control", "no-store")],
                bytes,
            )
                .into_response()
        }
        Ok(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
fn command(dir: &std::path::Path) -> std::process::Command {
    let mut command = amp_core::command("ffmpeg");
    command.current_dir(dir).args([
        "-nostdin",
        "-v",
        "error",
        "-fflags",
        "+nobuffer",
        "-probesize",
        "32768",
        "-analyzeduration",
        "0",
        "-f",
        "mp3",
        "-i",
        "pipe:0",
        "-filter_complex",
        "[0:a]aresample=44100,asplit=3[a0][a1][a2]",
        "-map",
        "[a0]",
        "-map",
        "[a1]",
        "-map",
        "[a2]",
        "-c:a",
        "aac",
        "-ac",
        "2",
        "-b:a:0",
        "192k",
        "-b:a:1",
        "128k",
        "-b:a:2",
        "96k",
        "-f",
        "hls",
        "-hls_time",
        "1",
        "-hls_list_size",
        "6",
        "-hls_delete_threshold",
        "2",
        "-hls_flags",
        "delete_segments+temp_file+independent_segments",
        "-hls_segment_type",
        "fmp4",
        "-hls_fmp4_init_filename",
        "init_%v.mp4",
        "-master_pl_name",
        "master.m3u8",
        "-var_stream_map",
        "a:0,name:quality a:1,name:balanced a:2,name:resilient",
        "-hls_segment_filename",
        "%v/segment-%d.m4s",
        "%v/index.m3u8",
    ]);
    command
}
async fn encode(
    mut receiver: broadcast::Receiver<Bytes>,
    directory: &std::path::Path,
    owner: std::sync::Weak<Cast>,
    generation: u64,
    mut stopped: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), String> {
    let mut child = tokio::process::Command::from(command(directory))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Adaptive encoder unavailable: {e}"))?;
    let mut input = child
        .stdin
        .take()
        .ok_or("Adaptive encoder input unavailable")?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or("Adaptive encoder diagnostics unavailable")?;
    // Drain diagnostics without unbounded retention or blocking the encoder.
    let mut diagnostics = tokio::spawn(async move {
        let mut tail = Vec::new();
        let mut buffer = [0; 1024];
        loop {
            match stderr.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    tail.extend_from_slice(&buffer[..n]);
                    if tail.len() > 8192 {
                        tail.drain(..tail.len() - 8192);
                    }
                }
            }
        }
        tail
    });
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    let result = loop {
        tokio::select! {
            _ = &mut stopped => break Ok(()),
            bytes=receiver.recv()=> match bytes {
                Ok(bytes)=> {
                    match tokio::time::timeout(std::time::Duration::from_secs(2),input.write_all(&bytes)).await {
                        Ok(Ok(()))=>{}, _=>break Err("Adaptive encoder fell behind; reconnect to live audio".into()),
                    }
                },
                Err(broadcast::error::RecvError::Lagged(_))=>break Err("Adaptive input lagged; reconnect to a new audio epoch".into()),
                Err(broadcast::error::RecvError::Closed)=>break Ok(()),
            },
            _=interval.tick()=> {
                let Some(cast)=owner.upgrade() else {break Ok(())};
                let station=cast.station.lock().await;
                if !station.running || station.generation!=generation {break Ok(())}
                if cast.hls.state.lock().unwrap().last_request.is_none_or(|t|t.elapsed()>std::time::Duration::from_secs(20)) {break Ok(())}
                if child.try_wait().map_err(|e| e.to_string())?.is_some() {break Err("Adaptive encoder stopped".into())}
            }
        }
    };
    drop(input);
    let _ = child.kill().await;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await;
    let tail =
        match tokio::time::timeout(std::time::Duration::from_millis(500), &mut diagnostics).await {
            Ok(Ok(tail)) => tail,
            _ => {
                diagnostics.abort();
                Vec::new()
            }
        };
    result.map_err(|message| {
        let detail: String = String::from_utf8_lossy(&tail)
            .trim()
            .chars()
            .take(2048)
            .collect();
        if detail.is_empty() {
            message
        } else {
            format!("{message}: {detail}")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;
    fn cast() -> (tempfile::TempDir, Arc<Cast>) {
        let root = tempfile::tempdir().unwrap();
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        (root, cast)
    }
    #[tokio::test]
    async fn stopped_sources_never_start_and_stale_epochs_never_serve_audio() {
        let (_root, cast) = cast();
        let app = router(cast.clone());
        let reply = app
            .clone()
            .oneshot(
                Request::get(format!("{PREFIX}/master.m3u8"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reply.status(), 503);
        assert!(cast.hls.state.lock().unwrap().worker.is_none());
        cast.station.lock().await.running = true;
        let dir = Arc::new(tempfile::tempdir().unwrap());
        std::fs::create_dir(dir.path().join("quality")).unwrap();
        std::fs::write(dir.path().join("quality/segment-0.m4s"), b"old audio").unwrap();
        {
            let mut s = cast.hls.state.lock().unwrap();
            s.epoch = Some("old".into());
            s.directory = Some(dir);
        }
        let request = || {
            Request::get(format!("{PREFIX}/old/quality/segment-0.m4s"))
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(app.clone().oneshot(request()).await.unwrap().status(), 200);
        cast.hls.reset().await;
        assert_eq!(app.clone().oneshot(request()).await.unwrap().status(), 410);
        let wrong = Request::get(format!("{PREFIX}/master.m3u8"))
            .header("x-amp-server", "different")
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.oneshot(wrong).await.unwrap().status(), 409);
        cast.shutdown().await;
    }
    #[test]
    #[ignore = "requires FFmpeg AAC/fMP4 support; generated audio only"]
    fn variants_decode_with_aligned_timestamps_and_bounded_rolling_files() {
        let dir = tempfile::tempdir().unwrap();
        for p in PROFILES {
            std::fs::create_dir(dir.path().join(p)).unwrap();
        }
        let input = dir.path().join("input.mp3");
        let generate = amp_core::command("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=24",
                "-ac",
                "2",
                "-c:a",
                "libmp3lame",
                "-b:a",
                "192k",
            ])
            .arg(&input)
            .output()
            .unwrap();
        assert!(
            generate.status.success(),
            "{}",
            String::from_utf8_lossy(&generate.stderr)
        );
        let output = command(dir.path())
            .stdin(std::fs::File::open(&input).unwrap())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(ready(dir.path()));
        let mut durations = None;
        let mut sample_count = None;
        let mut start_time = None;
        for profile in PROFILES {
            let playlist =
                std::fs::read_to_string(dir.path().join(profile).join("index.m3u8")).unwrap();
            let times: Vec<_> = playlist
                .lines()
                .filter(|l| l.starts_with("#EXTINF:"))
                .map(str::to_owned)
                .collect();
            assert!(!times.is_empty() && times.len() <= 6);
            if let Some(expected) = &durations {
                assert_eq!(&times, expected);
            } else {
                durations = Some(times);
            }
            let files: Vec<_> = std::fs::read_dir(dir.path().join(profile))
                .unwrap()
                .map(|x| x.unwrap().path())
                .collect();
            assert!(files.len() <= 11, "rolling output grew: {}", files.len());
            let segment = playlist
                .lines()
                .find(|l| l.starts_with("segment-"))
                .unwrap();
            let init = dir.path().join(profile).join(format!("init_{profile}.mp4"));
            assert!(init.is_file(), "init map missing for {profile}: {playlist}");
            let combined = dir.path().join(format!("{profile}.mp4"));
            let mut bytes = std::fs::read(init).unwrap();
            bytes.extend(std::fs::read(dir.path().join(profile).join(segment)).unwrap());
            std::fs::write(&combined, bytes).unwrap();
            let decode = amp_core::command("ffmpeg")
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(&combined)
                .args(["-f", "s16le", "pipe:1"])
                .output()
                .unwrap();
            assert!(
                decode.status.success(),
                "{profile}: {}",
                String::from_utf8_lossy(&decode.stderr)
            );
            assert!(!decode.stdout.is_empty());
            if let Some(expected) = sample_count {
                assert_eq!(decode.stdout.len(), expected);
            } else {
                sample_count = Some(decode.stdout.len());
            }
            let probe = amp_core::command("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-select_streams",
                    "a:0",
                    "-show_entries",
                    "stream=sample_rate,channels,start_time",
                    "-of",
                    "json",
                ])
                .arg(&combined)
                .output()
                .unwrap();
            assert!(probe.status.success());
            let value: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
            assert_eq!(value["streams"][0]["sample_rate"], "44100");
            assert_eq!(value["streams"][0]["channels"], 2);
            let start = value["streams"][0]["start_time"].clone();
            if let Some(expected) = &start_time {
                assert_eq!(&start, expected);
            } else {
                start_time = Some(start);
            }
        }
        let playlist = std::fs::read_to_string(dir.path().join("quality/index.m3u8")).unwrap();
        let segments: Vec<_> = playlist
            .lines()
            .filter(|line| line.starts_with("segment-"))
            .take(3)
            .collect();
        assert_eq!(segments.len(), 3);
        let mut sizes = Vec::new();
        for switched in [false, true] {
            let mut bytes = std::fs::read(dir.path().join("quality/init_quality.mp4")).unwrap();
            for (index, segment) in segments.iter().enumerate() {
                let profile = if switched { PROFILES[index] } else { "quality" };
                bytes.extend(std::fs::read(dir.path().join(profile).join(segment)).unwrap());
            }
            let path = dir.path().join(if switched {
                "switched.mp4"
            } else {
                "baseline.mp4"
            });
            std::fs::write(&path, bytes).unwrap();
            let decode = amp_core::command("ffmpeg")
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(&path)
                .args(["-f", "s16le", "pipe:1"])
                .output()
                .unwrap();
            assert!(
                decode.status.success(),
                "{}",
                String::from_utf8_lossy(&decode.stderr)
            );
            sizes.push(decode.stdout.len());
            // Ten-millisecond windows across profile boundaries retain real signal.
            for window in decode
                .stdout
                .chunks(441 * 2 * 2)
                .filter(|w| w.len() == 441 * 2 * 2)
            {
                let energy: u64 = window
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|s| i16::from_le_bytes(*s).unsigned_abs() as u64)
                    .sum();
                assert!(energy > 1000, "silent gap at adaptive profile boundary");
            }
        }
        assert_eq!(
            sizes[0], sizes[1],
            "adaptive transition changed the decoded timeline"
        );
    }
    #[tokio::test]
    #[ignore = "requires FFmpeg; generated mid-frame input and source interruption"]
    async fn live_join_recovers_from_midframe_then_invalidates_lagged_and_changed_sources() {
        let (_root, cast) = cast();
        cast.station.lock().await.running = true;
        let source = amp_core::command("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=9",
                "-ac",
                "2",
                "-c:a",
                "libmp3lame",
                "-b:a",
                "192k",
                "-f",
                "mp3",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(source.status.success());
        let app = router(cast.clone());
        let request = || {
            Request::get(format!("{PREFIX}/master.m3u8"))
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(app.clone().oneshot(request()).await.unwrap().status(), 503);
        let old_epoch = cast.hls.state.lock().unwrap().epoch.clone().unwrap();
        // Real live subscriptions can begin inside an MP3 frame, not at a file header.
        for bytes in source.stdout[177..].chunks(4096) {
            cast.audio.send(Bytes::copy_from_slice(bytes)).unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        let mut ready = false;
        for _ in 0..100 {
            if cast.hls.snapshot()["ready"] == true {
                ready = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }
        assert!(ready, "{}", cast.hls.snapshot());
        let output_directory = cast.hls.state.lock().unwrap().directory.clone().unwrap();
        let playlist =
            std::fs::read_to_string(output_directory.path().join("balanced/index.m3u8")).unwrap();
        let segment = playlist
            .lines()
            .find(|line| line.starts_with("segment-"))
            .unwrap();
        let mut encoded =
            std::fs::read(output_directory.path().join("balanced/init_balanced.mp4")).unwrap();
        encoded
            .extend(std::fs::read(output_directory.path().join("balanced").join(segment)).unwrap());
        let sample = output_directory.path().join("midframe-decoding.mp4");
        std::fs::write(&sample, encoded).unwrap();
        let decoded = amp_core::command("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(&sample)
            .args(["-f", "s16le", "pipe:1"])
            .output()
            .unwrap();
        assert!(
            decoded.status.success(),
            "{}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        assert!(
            decoded.stdout.len() > 44100 * 2,
            "mid-frame join failed to produce playable audio"
        );
        drop(output_directory);
        let master = app.clone().oneshot(request()).await.unwrap();
        assert_eq!(master.status(), 200);
        let master = axum::body::to_bytes(master.into_body(), 16384)
            .await
            .unwrap();
        assert!(
            String::from_utf8_lossy(&master)
                .contains(&format!("{PREFIX}/{old_epoch}/quality/index.m3u8"))
        );
        // Saturate without yielding: the adaptive worker must invalidate rather than skip bytes.
        for _ in 0..300 {
            cast.audio.send(Bytes::from_static(b"lag")).unwrap();
        }
        for _ in 0..100 {
            if cast.hls.state.lock().unwrap().epoch.is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(cast.hls.state.lock().unwrap().epoch.is_none());
        let old = Request::get(format!("{PREFIX}/{old_epoch}/quality/index.m3u8"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.clone().oneshot(old).await.unwrap().status(), 410);
        assert_eq!(app.oneshot(request()).await.unwrap().status(), 503);
        let new_epoch = cast.hls.state.lock().unwrap().epoch.clone().unwrap();
        assert_ne!(old_epoch, new_epoch);
        cast.station.lock().await.generation += 1;
        for _ in 0..100 {
            if cast.hls.state.lock().unwrap().epoch.is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(cast.hls.state.lock().unwrap().epoch.is_none());
        cast.shutdown().await;
    }
    #[tokio::test]
    #[ignore = "requires FFmpeg; verifies shared lazy worker and idle cleanup"]
    async fn listeners_share_one_worker_and_idle_status_polling_cannot_keep_it_alive() {
        let (_root, cast) = cast();
        cast.station.lock().await.running = true;
        let app = router(cast.clone());
        let request = || {
            Request::get(format!("{PREFIX}/master.m3u8"))
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(app.clone().oneshot(request()).await.unwrap().status(), 503);
        let epoch = cast.hls.state.lock().unwrap().epoch.clone().unwrap();
        let path = cast
            .hls
            .state
            .lock()
            .unwrap()
            .directory
            .as_ref()
            .unwrap()
            .path()
            .to_path_buf();
        assert_eq!(app.oneshot(request()).await.unwrap().status(), 503);
        assert_eq!(cast.hls.state.lock().unwrap().epoch.as_ref(), Some(&epoch));
        assert!(cast.station.lock().await.running);
        cast.hls.state.lock().unwrap().last_request =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(21));
        for _ in 0..100 {
            let _ = cast.hls.snapshot(); // Status reads never count as an active listener.
            if cast.hls.state.lock().unwrap().epoch.is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(cast.hls.state.lock().unwrap().epoch.is_none());
        assert!(cast.station.lock().await.running);
        for _ in 0..50 {
            if !path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(!path.exists());
        cast.shutdown().await;
    }
    #[tokio::test]
    #[ignore = "requires FFmpeg; generated source lifecycle only"]
    async fn worker_cancellation_discards_epoch_and_temporary_files() {
        let (_root, cast) = cast();
        cast.station.lock().await.running = true;
        let app = router(cast.clone());
        let reply = app
            .oneshot(
                Request::get(format!("{PREFIX}/master.m3u8"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reply.status(), 503);
        let path = cast
            .hls
            .state
            .lock()
            .unwrap()
            .directory
            .as_ref()
            .unwrap()
            .path()
            .to_path_buf();
        cast.shutdown().await;
        for _ in 0..50 {
            if !path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(!path.exists());
        assert!(cast.hls.state.lock().unwrap().epoch.is_none());
    }
}
