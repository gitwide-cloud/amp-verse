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
        .arg(format!(
            "atempo={rate},volume={}dB",
            amp_core::loudness::auto_gain_db(&preparation)
        ))
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
    limiter: amp_core::limiter::LookaheadLimiter,
    start: tokio::time::Instant,
    frames: u64,
}
fn limit_pcm(limiter: &mut amp_core::limiter::LookaheadLimiter, bytes: &mut [u8]) {
    let (frames, _) = bytes.as_chunks_mut::<8>();
    for frame in frames {
        let input = [
            f32::from_le_bytes(frame[..4].try_into().unwrap()),
            f32::from_le_bytes(frame[4..].try_into().unwrap()),
        ];
        let output = limiter.process(input);
        frame[..4].copy_from_slice(&output[0].to_le_bytes());
        frame[4..].copy_from_slice(&output[1].to_le_bytes());
    }
}
impl Clock {
    async fn write(
        &mut self,
        input: &mut tokio::process::ChildStdin,
        bytes: &mut [u8],
    ) -> Result<(), String> {
        limit_pcm(&mut self.limiter, bytes);
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
        clock.write(input, &mut buffer[..n]).await?;
        remaining -= n as u64 / 8;
    }
    Ok(())
}
#[derive(Clone)]
struct SmartSession {
    cycle: std::sync::Arc<std::sync::Mutex<amp_core::smart_cycle::SmartCycle>>,
    library: String,
}
impl SmartSession {
    fn new(cast: &Cast) -> Self {
        Self {
            cycle: Default::default(),
            library: cast
                .library_id
                .clone()
                .unwrap_or_else(|| cast.root.to_string_lossy().into_owned()),
        }
    }
    fn pick(
        &self,
        cast: &Cast,
        p: &SmartPlaylist,
        current: Option<usize>,
        target: Option<f64>,
    ) -> Result<usize, String> {
        self.cycle
            .lock()
            .unwrap()
            .ranked(&self.library, p, &cast.tracks, current, target, None)
            .first()
            .copied()
            .ok_or_else(|| "No playable eligible songs remain in Smart Playlist".into())
    }
    fn played(&self, cast: &Cast, p: &SmartPlaylist, index: usize) {
        self.cycle
            .lock()
            .unwrap()
            .played(&self.library, &p.id, &cast.tracks[index]);
    }
    fn failed(&self, cast: &Cast, p: &SmartPlaylist, index: usize) {
        self.cycle
            .lock()
            .unwrap()
            .failed(&self.library, &p.id, &cast.tracks[index]);
    }
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
    cycle: Option<SmartSession>,
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
        if let (Some(cycle), Some(rules)) = (&cycle, &profile) {
            cycle.failed(&cast, rules, index);
        }
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
        let next = if let (Some(cycle), Some(rules)) = (&cycle, &profile) {
            cycle.pick(&cast, rules, current, target)
        } else {
            pick(
                &cast,
                profile.as_ref(),
                &candidates,
                &mut cursor,
                current,
                &recent,
                target,
            )
        };
        match next {
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
pub(super) async fn validate_selection(
    cast: Arc<Cast>,
    selection: &mut serde_json::Value,
) -> Result<(), String> {
    let indices: Vec<usize> = selection["tracks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_u64().map(|v| v as usize))
        .collect();
    let mut profile: Option<SmartPlaylist> =
        serde_json::from_value(selection["rules"].clone()).ok();
    let finite = selection["finite"] == true;
    let mut cursor = 0;
    let unavailable = selection["unavailable"].as_u64().unwrap_or(0);
    for (occurrence, &ordered) in indices.iter().take(32).enumerate() {
        let index = if finite {
            ordered
        } else {
            pick(
                &cast,
                profile.as_ref(),
                &indices,
                &mut cursor,
                None,
                &[],
                profile.as_ref().and_then(|p| p.target_energy),
            )?
        };
        let result = probe_track(&cast, index).await;
        if result
            .as_ref()
            .is_err_and(|error| error == "Audio decoder unavailable")
        {
            return Err("Audio decoder unavailable".into());
        }
        if result.is_ok() {
            if finite {
                selection["start_occurrence"] = serde_json::json!(occurrence);
                selection["failed_occurrences"] =
                    serde_json::json!(unavailable + occurrence as u64);
            }
            return Ok(());
        }
        if let Some(profile) = &mut profile {
            profile.excluded.push(show::song_id(&cast.tracks[index]));
        }
    }
    Err("No playable songs found in the first 32 playlist occurrences".into())
}
async fn probe_track(cast: &Cast, index: usize) -> Result<(), String> {
    let track = cast.tracks.get(index).ok_or("Song unavailable")?;
    let path = amp_core::resolve(&cast.root, &track.path).map_err(|_| "Song unavailable")?;
    let mut command = amp_core::command("ffmpeg");
    command
        .args(["-nostdin", "-v", "fatal", "-i"])
        .arg(path)
        .args([
            "-t", "0.05", "-vn", "-ac", "2", "-ar", "44100", "-f", "f32le", "pipe:1",
        ]);
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        tokio::process::Command::from(command)
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| "Audio check timed out")?
    .map_err(|_| "Audio decoder unavailable")?;
    if !output.status.success() || output.stdout.is_empty() {
        return Err("No playable audio".into());
    }
    Ok(())
}
fn finite_next(cast: &Cast, occurrence: usize) -> Option<usize> {
    cast.selection
        .lock()
        .unwrap()
        .as_ref()?
        .get("tracks")?
        .as_array()?
        .get(occurrence)?
        .as_u64()
        .map(|i| i as usize)
}
fn finish_or_next(cast: &Cast, occurrence: usize) -> Option<usize> {
    let mut selection = cast.selection.lock().unwrap();
    let selected = selection.as_mut()?;
    let next = selected["tracks"]
        .as_array()?
        .get(occurrence)
        .and_then(|v| v.as_u64())
        .map(|i| i as usize);
    if next.is_none() {
        selected["queue_closed"] = serde_json::json!(true);
    }
    next
}
async fn prepare_finite(
    cast: Arc<Cast>,
    mut occurrence: usize,
    owner: u64,
) -> Result<Option<(Decoded, usize)>, String> {
    for _ in 0..32 {
        let Some(index) = finish_or_next(&cast, occurrence) else {
            return Ok(None);
        };
        match tokio::time::timeout(
            std::time::Duration::from_secs(120),
            decode(cast.clone(), index, None, None, None),
        )
        .await
        {
            Ok(Ok(decoded)) => return Ok(Some((decoded, occurrence))),
            _ => {
                let mut station = cast.station.lock().await;
                if station.generation != owner {
                    return Err("Player owner changed".into());
                }
                station.player_failures += 1;
                station.player_last_issue=Some("Skipped a queued song with unavailable or invalid audio. Its saved playlist reference was kept.".into());
                occurrence += 1;
            }
        }
    }
    Err("Too many unavailable queued songs in a row. Playback stopped; repair the source and retry.".into())
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
    let finite = selection.as_ref().is_some_and(|v| v["finite"] == true);
    let mut occurrence = selection
        .as_ref()
        .and_then(|v| v["start_occurrence"].as_u64())
        .unwrap_or(0) as usize;
    let mut profile: Option<SmartPlaylist> = selection
        .as_ref()
        .and_then(|v| serde_json::from_value(v["rules"].clone()).ok());
    let cycle = SmartSession::new(&cast);
    let mut cursor = 0;
    let mut recent = Vec::new();
    let mut target = profile.as_ref().and_then(|p| p.target_energy);
    let first = if let Some(p) = &profile {
        cycle.pick(&cast, p, None, target)?
    } else {
        pick(&cast, None, &indices, &mut cursor, None, &recent, target)?
    };
    let mut current = if finite {
        let (decoded, actual) = prepare_finite(cast.clone(), occurrence, owner)
            .await?
            .ok_or("No playable songs remain")?;
        occurrence = actual;
        decoded
    } else {
        prepare_next(
            cast.clone(),
            first,
            profile.clone(),
            indices.clone(),
            None,
            recent.clone(),
            target,
            None,
            Some(cycle.clone()),
        )
        .await?
    };
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
            .and_then(|p| p.with_arc(plan.arc))
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
    let mut output_guard = AbortTask(output_task);
    let mut clock = Clock {
        limiter: amp_core::limiter::LookaheadLimiter::new(RATE as u32),
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
        if finite {
            // Occurrences retain normal order and duplicates; only invalid audio
            // is skipped, explicitly counted without changing saved references.
            let pending = finite_next(&cast, occurrence + 1).map(|_| {
                let source = cast.clone();
                AbortTask(tokio::spawn(async move {
                    prepare_finite(source, occurrence + 1, owner).await
                }))
            });
            let remaining = current.frames.saturating_sub(consumed);
            emit(&mut current, remaining, &mut input, &mut clock).await?;
            let next = if let Some(mut task) = pending {
                (&mut task.0)
                    .await
                    .map_err(|_| "Queued song preparation stopped")??
            } else {
                prepare_finite(cast.clone(), occurrence + 1, owner).await?
            };
            if let Some((next, actual)) = next {
                current = next;
                occurrence = actual;
                consumed = 0;
                continue;
            }
            input
                .shutdown()
                .await
                .map_err(|_| "Encoder could not finish")?;
            drop(input);
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), encoder.wait()).await;
            let _ =
                tokio::time::timeout(std::time::Duration::from_secs(2), &mut output_guard.0).await;
            return Ok(());
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
        if let Some(p) = &profile {
            if consumed == 0 {
                let first_frames = current.frames.min(1024);
                emit(&mut current, first_frames, &mut input, &mut clock).await?;
                consumed = first_frames;
            }
            cycle.played(&cast, p, current.index);
        }
        recent.push(show::song_id(&cast.tracks[current.index]));
        if recent.len() > 100 {
            recent.remove(0);
        }
        if let Some(plan) = &progression {
            target = Some(plan.next());
        }
        let candidate = if let Some(p) = &profile {
            cycle.pick(&cast, p, Some(current.index), target)
        } else {
            pick(
                &cast,
                None,
                &indices,
                &mut cursor,
                Some(current.index),
                &recent,
                target,
            )
        };
        let next_index = match candidate {
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
            Some(cycle.clone()),
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
        if let Some(grid) = amp_core::mix_schedule::safe_grid_at_rates(
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
                    let mixed = x * weights[0] + y * weights[1];
                    a[i..i + 4].copy_from_slice(&mixed.to_le_bytes());
                }
            }
            clock.write(&mut input, &mut a[..n]).await?;
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
    #[ignore = "requires FFmpeg; generated Smart station cycle, no physical audio device"]
    async fn smart_cycle_station_uses_entire_playable_pool_before_repeat() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("bad.wav"), b"broken").unwrap();
        for i in 0..6 {
            assert!(
                amp_core::command("ffmpeg")
                    .args(["-v", "fatal", "-f", "lavfi", "-i"])
                    .arg(format!("sine=frequency={}:duration=0.6", 400 + i * 70))
                    .arg(root.path().join(format!("t{i}.wav")))
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
        let mut profile = show::Settings::default().playlists.remove(0);
        profile.recent_repeat = 1;
        let ids: Vec<_> = (0..cast.tracks.len()).collect();
        *cast.selection.lock().unwrap() =
            Some(serde_json::json!({"kind":"smart","rules":profile,"tracks":ids}));
        let mut audio = cast.audio.subscribe();
        let mut emitted = 0;
        let response = router(cast.clone())
            .oneshot(
                axum::http::Request::post("/api/start")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let mut heard = Vec::new();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
        while heard.len() < 7 {
            while let Ok(chunk) = audio.try_recv() {
                emitted += chunk.len();
            }
            {
                let station = cast.station.lock().await;
                assert!(station.running, "{:?}", station.error);
                if let Some(index) = station.current_index {
                    assert_ne!(cast.tracks[index].path, std::path::PathBuf::from("bad.wav"));
                    if heard.last() != Some(&index) {
                        heard.push(index);
                    }
                }
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "Not enough audible handoffs: {heard:?}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        cast.shutdown().await;
        let unique: std::collections::HashSet<_> = heard[..6].iter().collect();
        assert_eq!(
            unique.len(),
            6,
            "repeat before the playable pool completed: {heard:?}"
        );
        assert_ne!(heard[5], heard[6]);
        assert!(
            emitted > 1000,
            "the cycle must emit real encoded audio, not only rotate metadata"
        );
    }
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
        let failure = prepare_next(cast, bad, None, vec![bad], None, vec![], None, None, None)
            .await
            .err()
            .unwrap();
        assert!(failure.contains("No playable next song"));
    }
}

#[cfg(test)]
mod sag65_audio_tests {
    use super::*;
    include!("../../../tests/sag65/audio_helpers.rs");
    #[test]
    #[ignore = "requires generated SAG65 fixtures and ffmpeg true-peak meter"]
    fn f10_radio_emit_path_true_peak() {
        let a = fixture("f10_hot_outgoing.wav");
        let b = fixture("f10_hot_incoming.wav");
        let n = a.len().min(b.len());
        let mut bytes = Vec::with_capacity((n + 221) * 8);
        for i in 0..n {
            let w = show::crossfade_weights(i as f32 / n as f32);
            for ch in 0..2 {
                bytes.extend_from_slice(&(a[i][ch] * w[0] + b[i][ch] * w[1]).to_le_bytes());
            }
        }
        bytes.resize(bytes.len() + 221 * 8, 0);
        let mut limiter = amp_core::limiter::LookaheadLimiter::new(44100);
        for chunk in bytes.chunks_mut(8192) {
            limit_pcm(&mut limiter, chunk);
        }
        let (frames, _) = bytes.as_chunks::<8>();
        let frames: Vec<_> = frames
            .iter()
            .map(|b| {
                [
                    f32::from_le_bytes(b[..4].try_into().unwrap()),
                    f32::from_le_bytes(b[4..].try_into().unwrap()),
                ]
            })
            .collect();
        verify_peak("radio F10", &frames);
    }
}
