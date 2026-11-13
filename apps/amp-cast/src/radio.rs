//! Headless prepared two-track radio. One PCM timeline and one encoder feed every listener.
use super::*;
use amp_core::{
    Preparation,
    show::{self, SmartPlaylist, Style},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const RATE: u64 = 44100;
struct Decoded {
    file: tokio::fs::File,
    _temp: tempfile::TempPath,
    frames: u64,
    index: usize,
    preparation: Preparation,
    rate: f64,
}
struct AbortTask<T>(tokio::task::JoinHandle<T>);
impl<T> Drop for AbortTask<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}
fn fade_seconds() -> f64 {
    std::env::var("AMP_CAST_FADE_SECONDS")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(3.)
        .clamp(0.1, 30.)
}
fn entry(p: &Preparation, profile: Option<&SmartPlaylist>, target: Option<f64>) -> f64 {
    if profile.is_some_and(|r| r.style == Style::Hype && r.use_manual_cues)
        && let Some(c) = p.manual_cues.iter().find(|c| {
            let name = c.label.to_lowercase();
            (name.contains("drop") || name.contains("build"))
                && p.cue_out.is_none_or(|out| c.seconds < out)
        })
    {
        return c.seconds;
    }
    if amp_core::energy_profile::mixing_allowed(p)
        && let Some(activity) = amp_core::energy_profile::read(p)
        && let Some(section) = activity.entry(
            target.map_or(0.5, |e| (e - 1.) / 9.),
            amp_core::energy_profile::entry_bound(p),
            p.cue_out.unwrap_or(activity.duration),
            8.,
        )
    {
        return section.start.max(p.cue_in);
    }
    p.cue_in
}
async fn decode(
    cast: Arc<Cast>,
    index: usize,
    profile: Option<SmartPlaylist>,
    target: Option<f64>,
    out_bpm: Option<f64>,
) -> Result<Decoded, String> {
    let track = cast.tracks.get(index).ok_or("Track unavailable")?;
    let path = amp_core::resolve(&cast.root, &track.path).map_err(|e| e.to_string())?;
    let mut preparation = amp_core::preparation::load(&path)?;
    preparation.cue_in = entry(&preparation, profile.as_ref(), target);
    let rate = out_bpm
        .filter(|_| amp_core::mix_schedule::timing_reliable(&preparation))
        .zip(preparation.bpm)
        .and_then(|(out, next)| {
            amp_core::transition_plan::tempo_match(out, 1., next, 0.08).map(|m| m.rate)
        })
        .unwrap_or(1.);
    if profile.is_some()
        && out_bpm.is_some()
        && let Some(plan) = amp_core::transition_plan::landing(
            &preparation,
            preparation.cue_in,
            rate,
            fade_seconds(),
        )
    {
        preparation.cue_in = plan.entry;
    }
    let temp = tempfile::NamedTempFile::new()
        .map_err(|e| e.to_string())?
        .into_temp_path();
    let mut command = amp_core::command("ffmpeg");
    // A bounded temporary PCM cache keeps preparation off the streaming loop and memory bounded.
    let duration = preparation
        .cue_out
        .map(|out| out - preparation.cue_in)
        .unwrap_or(7200.)
        .clamp(0.02, 7200.);
    command
        .args(["-nostdin", "-v", "error", "-y", "-ss"])
        .arg(preparation.cue_in.to_string())
        .arg("-t")
        .arg(duration.to_string())
        .arg("-i")
        .arg(&path)
        .args(["-vn", "-ac", "2", "-ar", "44100", "-af"])
        .arg(format!("atempo={rate}"))
        .args(["-f", "f32le"])
        .arg(temp.as_os_str());
    let result = tokio::process::Command::from(command)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("Audio preparation unavailable: {e}"))?;
    if !result.status.success() {
        return Err(format!(
            "Cannot prepare {}: {}",
            track.title,
            String::from_utf8_lossy(&result.stderr)
                .chars()
                .take(300)
                .collect::<String>()
        ));
    }
    let file = tokio::fs::File::open(&temp)
        .await
        .map_err(|e| e.to_string())?;
    let frames = file.metadata().await.map_err(|e| e.to_string())?.len() / 8;
    if frames == 0 {
        return Err(format!("No audio in {}", track.title));
    }
    Ok(Decoded {
        file,
        _temp: temp,
        frames,
        index,
        preparation,
        rate,
    })
}
struct Clock {
    start: tokio::time::Instant,
    frames: u64,
}
impl Clock {
    async fn write(
        &mut self,
        input: &mut tokio::process::ChildStdin,
        bytes: &[u8],
    ) -> Result<(), String> {
        self.frames += bytes.len() as u64 / 8;
        tokio::time::timeout(std::time::Duration::from_secs(15), input.write_all(bytes))
            .await
            .map_err(|_| "Station encoder stopped accepting audio".to_string())?
            .map_err(|e| e.to_string())?;
        tokio::time::sleep_until(
            self.start + std::time::Duration::from_secs_f64(self.frames as f64 / RATE as f64),
        )
        .await;
        Ok(())
    }
}
async fn emit(
    decoded: &mut Decoded,
    frames: u64,
    input: &mut tokio::process::ChildStdin,
    clock: &mut Clock,
) -> Result<(), String> {
    let mut remaining = frames;
    let mut buffer = [0u8; 8192];
    while remaining > 0 {
        let n = (remaining.min(1024) * 8) as usize;
        decoded
            .file
            .read_exact(&mut buffer[..n])
            .await
            .map_err(|e| e.to_string())?;
        clock.write(input, &buffer[..n]).await?;
        remaining -= n as u64 / 8;
    }
    Ok(())
}
fn pick(
    cast: &Cast,
    profile: Option<&SmartPlaylist>,
    indices: &[usize],
    cursor: &mut usize,
    current: Option<usize>,
    recent: &[String],
    target: Option<f64>,
) -> Result<usize, String> {
    if let Some(p) = profile {
        show::choose(p, &cast.tracks, current, recent, target, 0)
            .or_else(|| {
                current.filter(|i| {
                    p.fallback == show::Fallback::RelaxRecent && show::eligible(p, &cast.tracks[*i])
                })
            })
            .ok_or_else(|| "No eligible songs remain in Smart Playlist".into())
    } else {
        let index = *indices
            .get(*cursor % indices.len().max(1))
            .ok_or("Playlist is empty")?;
        *cursor += 1;
        Ok(index)
    }
}
/// A corrupt/missing candidate must not take an otherwise healthy station down.
/// Exclusions are local to this preparation attempt, never saved to library rules.
#[allow(clippy::too_many_arguments)]
async fn prepare_next(
    cast: Arc<Cast>,
    first: usize,
    mut profile: Option<SmartPlaylist>,
    indices: Vec<usize>,
    current: Option<usize>,
    recent: Vec<String>,
    target: Option<f64>,
    bpm: Option<f64>,
) -> Result<Decoded, String> {
    let mut index = first;
    let mut failed = std::collections::HashSet::new();
    let mut cursor = indices.iter().position(|i| *i == first).unwrap_or(0) + 1;
    let mut errors = Vec::new();
    for _ in 0..cast.tracks.len().min(16) {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            decode(cast.clone(), index, profile.clone(), target, bpm),
        )
        .await;
        match result {
            Ok(Ok(decoded)) => return Ok(decoded),
            Ok(Err(error)) => errors.push(error),
            Err(_) => errors.push(format!(
                "Audio preparation timed out: {}",
                cast.tracks[index].title
            )),
        }
        failed.insert(index);
        if let Some(rules) = &mut profile {
            rules.excluded.push(show::song_id(&cast.tracks[index]));
        }
        let start = indices.iter().position(|i| *i == index).unwrap_or(0) + 1;
        let candidates: Vec<_> = indices
            .iter()
            .cycle()
            .skip(start)
            .take(indices.len())
            .copied()
            .filter(|i| !failed.contains(i))
            .collect();
        if candidates.is_empty() {
            break;
        }
        if profile.is_none() {
            cursor = 0;
        }
        match pick(
            &cast,
            profile.as_ref(),
            &candidates,
            &mut cursor,
            current,
            &recent,
            target,
        ) {
            Ok(next) if !failed.contains(&next) => index = next,
            _ => break,
        }
    }
    Err(format!(
        "No playable next song after {} attempts: {}",
        errors.len(),
        errors.last().map_or("Playlist empty", String::as_str)
    ))
}
pub(super) async fn broadcast(cast: Arc<Cast>, owner: u64) -> Result<(), String> {
    let selection = cast.selection.lock().unwrap().clone();
    let indices: Vec<usize> = selection
        .as_ref()
        .and_then(|v| v["tracks"].as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_u64().map(|v| v as usize))
                .collect()
        })
        .unwrap_or_else(|| (0..cast.tracks.len()).collect());
    let mut profile: Option<SmartPlaylist> = selection
        .as_ref()
        .and_then(|v| serde_json::from_value(v["rules"].clone()).ok());
    let mut cursor = 0;
    let mut recent = Vec::new();
    let mut target = profile.as_ref().and_then(|p| p.target_energy);
    let first = pick(
        &cast,
        profile.as_ref(),
        &indices,
        &mut cursor,
        None,
        &recent,
        target,
    )?;
    let mut current = prepare_next(
        cast.clone(),
        first,
        profile.clone(),
        indices.clone(),
        None,
        recent.clone(),
        target,
        None,
    )
    .await?;
    if profile.is_none() {
        cursor = indices
            .iter()
            .position(|i| *i == current.index)
            .unwrap_or(0)
            + 1;
    }
    let mut progression = profile
        .as_ref()
        .and_then(|p| p.energy_progression.as_ref())
        .map(|plan| {
            show::Progression::new(
                show::energy(&current.preparation).unwrap_or(5.),
                plan.target,
                plan.transitions,
                plan.hold,
            )
        })
        .transpose()?;
    let mut command = amp_core::command("ffmpeg");
    command.args([
        "-nostdin",
        "-v",
        "error",
        "-f",
        "f32le",
        "-ar",
        "44100",
        "-ac",
        "2",
        "-i",
        "pipe:0",
        "-c:a",
        "libmp3lame",
        "-b:a",
        "192k",
        "-write_xing",
        "0",
        "-id3v2_version",
        "0",
        "-flush_packets",
        "1",
        "-f",
        "mp3",
        "pipe:1",
    ]);
    let mut encoder = tokio::process::Command::from(command)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut input = encoder.stdin.take().ok_or("Encoder input unavailable")?;
    let mut output = encoder.stdout.take().ok_or("Encoder output unavailable")?;
    let sender = cast.audio.clone();
    let stream_cast = cast.clone();
    let output_task = tokio::spawn(async move {
        let mut buffer = [0u8; 4096];
        while let Ok(n) = output.read(&mut buffer).await {
            if n == 0 {
                break;
            }
            let mut station = stream_cast.station.lock().await;
            if station.generation != owner || !station.running {
                break;
            }
            station.audio_ready = true;
            stream_cast.recovery.push(&buffer[..n]);
            let _ = sender.send(Bytes::copy_from_slice(&buffer[..n]));
        }
    });
    let output_guard = AbortTask(output_task);
    let mut clock = Clock {
        start: tokio::time::Instant::now(),
        frames: 0,
    };
    let mut consumed = 0;
    let mut mix_revision = 0;
    loop {
        {
            let mut station = cast.station.lock().await;
            if station.generation != owner {
                return Ok(());
            }
            station.now_playing = Some(cast.tracks[current.index].title.clone());
            station.current_index = Some(current.index);
        }
        let requested = cast.mix.lock().unwrap().clone();
        if requested.revision != mix_revision {
            if profile.is_none() {
                let mut rules = show::Settings::default().playlists.remove(0);
                rules.allowed = indices
                    .iter()
                    .map(|i| show::song_id(&cast.tracks[*i]))
                    .collect();
                rules.restrict_allowed = true;
                rules.fallback = show::Fallback::RelaxRecent;
                profile = Some(rules);
            }
            if let Some(rules) = &mut profile {
                if let Some(style) = requested.style {
                    rules.style = style;
                }
                rules.target_energy = requested.target;
                target = requested.target;
            }
            progression = requested
                .transitions
                .map(|n| {
                    show::Progression::new(
                        show::energy(&current.preparation).unwrap_or(5.),
                        requested.target.unwrap_or(5.),
                        n,
                        requested.hold,
                    )
                })
                .transpose()?;
            mix_revision = requested.revision;
            cast.station.lock().await.mix_revision = mix_revision;
        }
        recent.push(show::song_id(&cast.tracks[current.index]));
        if recent.len() > 100 {
            recent.remove(0);
        }
        if let Some(plan) = &progression {
            target = Some(plan.next());
        }
        let next_index = match pick(
            &cast,
            profile.as_ref(),
            &indices,
            &mut cursor,
            Some(current.index),
            &recent,
            target,
        ) {
            Ok(index) => index,
            Err(error) => {
                let remaining = current.frames.saturating_sub(consumed);
                emit(&mut current, remaining, &mut input, &mut clock).await?;
                return Err(error);
            }
        };
        let source = cast.clone();
        let rules = profile.clone();
        let bpm = current
            .preparation
            .bpm
            .filter(|_| amp_core::mix_schedule::timing_reliable(&current.preparation))
            .map(|b| b * current.rate);
        let mut prefetch = AbortTask(tokio::spawn(prepare_next(
            source,
            next_index,
            rules,
            indices.clone(),
            Some(current.index),
            recent.clone(),
            target,
            bpm,
        )));
        let fade_seconds = fade_seconds();
        let fade = (fade_seconds * RATE as f64) as u64;
        let tail = fade.min(current.frames.saturating_sub(consumed) / 2);
        let mut before = current.frames.saturating_sub(consumed + tail);
        let incoming = &cast.tracks[next_index].preparation;
        let mut next_entry = entry(incoming, profile.as_ref(), target);
        let in_rate = bpm
            .zip(incoming.bpm)
            .filter(|_| amp_core::mix_schedule::timing_reliable(incoming))
            .and_then(|(out, next)| amp_core::transition_plan::tempo_match(out, 1., next, 0.08))
            .map_or(1., |m| m.rate);
        if profile.is_some()
            && let Some(plan) =
                amp_core::transition_plan::landing(incoming, next_entry, in_rate, fade_seconds)
        {
            next_entry = plan.entry;
        }
        if let Some(grid) = amp_core::mix_schedule::grid_at_rates(
            &current.preparation,
            incoming,
            next_entry,
            0.,
            amp_core::mix_schedule::Timing::Beat,
            current.rate,
            in_rate,
        ) {
            let desired = current.preparation.cue_in
                + (consumed + before) as f64 / RATE as f64 * current.rate;
            let mut aligned = grid.next(desired);
            if aligned > desired + 0.001 {
                aligned -= grid.period;
            }
            let frame = ((aligned - current.preparation.cue_in) / current.rate * RATE as f64)
                .max(0.) as u64;
            if frame > consumed + RATE {
                before = frame - consumed;
            }
        }
        emit(&mut current, before, &mut input, &mut clock).await?;
        let mut next = match (&mut prefetch.0).await.map_err(|e| e.to_string())? {
            Ok(next) => next,
            Err(error) => {
                emit(&mut current, tail, &mut input, &mut clock).await?;
                return Err(error);
            }
        };
        if profile.is_none() {
            cursor = indices.iter().position(|i| *i == next.index).unwrap_or(0) + 1;
        }
        let overlap = tail.min(next.frames / 2);
        let extra = tail - overlap;
        emit(&mut current, extra, &mut input, &mut clock).await?;
        let mut remaining = overlap;
        let mut offset = 0;
        let mut a = [0u8; 8192];
        let mut b = [0u8; 8192];
        while remaining > 0 {
            let frames = remaining.min(1024);
            let n = (frames * 8) as usize;
            current
                .file
                .read_exact(&mut a[..n])
                .await
                .map_err(|e| e.to_string())?;
            next.file
                .read_exact(&mut b[..n])
                .await
                .map_err(|e| e.to_string())?;
            for frame in 0..frames as usize {
                let weights =
                    show::crossfade_weights((offset + frame as u64) as f32 / overlap.max(1) as f32);
                for channel in 0..2 {
                    let i = frame * 8 + channel * 4;
                    let x = f32::from_le_bytes(a[i..i + 4].try_into().unwrap());
                    let y = f32::from_le_bytes(b[i..i + 4].try_into().unwrap());
                    let mixed = (x * weights[0] + y * weights[1]).clamp(-1., 1.);
                    a[i..i + 4].copy_from_slice(&mixed.to_le_bytes());
                }
            }
            clock.write(&mut input, &a[..n]).await?;
            offset += frames;
            remaining -= frames;
        }
        if let Some(plan) = &mut progression {
            plan.completed += 1;
            if plan.remaining() == 0 && !plan.hold {
                progression = None;
                target = profile.as_ref().and_then(|p| p.target_energy);
            }
        }
        current = next;
        consumed = overlap;
        if encoder.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("Station encoder stopped".into());
        }
        let _ = &output_guard;
    }
}

#[cfg(test)]
mod reliability_tests {
    use super::*;
    use tower::ServiceExt;
    #[tokio::test]
    #[ignore = "requires FFmpeg; real encoded short-session test, no receiver hardware"]
    async fn repeated_handoffs_survive_bad_files_and_shutdown_cleanly() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("broken.wav"), b"broken").unwrap();
        for (name, hz) in [("a.wav", 440), ("b.wav", 660)] {
            assert!(
                amp_core::command("ffmpeg")
                    .args(["-v", "error", "-f", "lavfi", "-i"])
                    .arg(format!("sine=frequency={hz}:duration=0.4"))
                    .arg(root.path().join(name))
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        let mut receiver = cast.audio.subscribe();
        let response = router(cast.clone())
            .oneshot(
                axum::http::Request::post("/api/start")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let mut last = None;
        let mut changes = 0;
        let mut bytes = Vec::new();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
        while tokio::time::Instant::now() < deadline {
            if let Ok(Ok(chunk)) =
                tokio::time::timeout(std::time::Duration::from_millis(30), receiver.recv()).await
            {
                bytes.extend_from_slice(&chunk);
            }
            let station = cast.station.lock().await;
            assert!(station.running, "{:?}", station.error);
            if station.current_index.is_some() && station.current_index != last {
                changes += 1;
                last = station.current_index;
            }
        }
        cast.shutdown().await;
        assert!(!cast.station.lock().await.running);
        assert!(changes >= 4, "Only {changes} handoffs");
        let mp3 = root.path().join("captured.mp3");
        std::fs::write(&mp3, &bytes).unwrap();
        let decoded = amp_core::command("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(mp3)
            .args(["-f", "f32le", "-ac", "1", "pipe:1"])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        assert!(decoded.stdout.len() > 44100 * 4);
    }
    #[tokio::test]
    #[ignore = "requires FFmpeg; isolated generated audio, no hardware playback"]
    async fn skips_unreadable_candidates_without_rewriting_metadata() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("broken.wav"), b"not audio").unwrap();
        let generated = amp_core::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.25",
            ])
            .arg(root.path().join("good.wav"))
            .status()
            .unwrap();
        assert!(generated.success());
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        let bad = cast
            .tracks
            .iter()
            .position(|t| t.path == std::path::Path::new("broken.wav"))
            .unwrap();
        let good = cast
            .tracks
            .iter()
            .position(|t| t.path == std::path::Path::new("good.wav"))
            .unwrap();
        let before = amp_core::preparation::load(&root.path().join("good.wav")).unwrap();
        let decoded = prepare_next(
            cast.clone(),
            bad,
            None,
            vec![bad, good],
            None,
            vec![],
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(decoded.index, good);
        assert!(decoded.frames > 0);
        assert_eq!(
            amp_core::preparation::load(&root.path().join("good.wav")).unwrap(),
            before
        );
        let failure = prepare_next(cast, bad, None, vec![bad], None, vec![], None, None)
            .await
            .err()
            .unwrap();
        assert!(failure.contains("No playable next song"));
    }
}
