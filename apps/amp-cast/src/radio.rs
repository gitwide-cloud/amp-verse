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
        .zip(preparation.bpm)
        .and_then(|(out, next)| {
            let ratio = out / next;
            (ratio.is_finite() && (0.92..=1.08).contains(&ratio)).then_some(ratio)
        })
        .unwrap_or(1.);
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
        input.write_all(bytes).await.map_err(|e| e.to_string())?;
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
    let profile: Option<SmartPlaylist> = selection
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
    let mut current = decode(cast.clone(), first, profile.clone(), target, None).await?;
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
            let _ = sender.send(Bytes::copy_from_slice(&buffer[..n]));
        }
    });
    let output_guard = AbortTask(output_task);
    let mut clock = Clock {
        start: tokio::time::Instant::now(),
        frames: 0,
    };
    let mut consumed = 0;
    loop {
        {
            let mut station = cast.station.lock().await;
            if station.generation != owner {
                return Ok(());
            }
            station.now_playing = Some(cast.tracks[current.index].title.clone());
            station.current_index = Some(current.index);
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
        let bpm = current.preparation.bpm.map(|b| b * current.rate);
        let mut prefetch = AbortTask(tokio::spawn(decode(source, next_index, rules, target, bpm)));
        let fade_seconds = std::env::var("AMP_CAST_FADE_SECONDS")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(3.)
            .clamp(0.1, 30.);
        let fade = (fade_seconds * RATE as f64) as u64;
        let tail = fade.min(current.frames.saturating_sub(consumed) / 2);
        let mut before = current.frames.saturating_sub(consumed + tail);
        let incoming = &cast.tracks[next_index].preparation;
        let next_entry = entry(incoming, profile.as_ref(), target);
        if let Some(grid) = amp_core::mix_schedule::grid(
            &current.preparation,
            incoming,
            next_entry,
            0.,
            amp_core::mix_schedule::Timing::Beat,
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
