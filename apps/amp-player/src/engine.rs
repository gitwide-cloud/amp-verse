//! Optional, explicitly installed analysis process. Playback never depends on it.
use amp_core::{
    Preparation,
    engine_api::{PROTOCOL, Request, Response},
};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
pub fn path() -> Option<PathBuf> {
    std::env::var_os("AMP_ENGINE_EXE")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe().ok().and_then(|p| {
                p.parent().map(|p| {
                    p.join("addons").join(if cfg!(windows) {
                        "amp-engine.exe"
                    } else {
                        "amp-engine"
                    })
                })
            })
        })
        .filter(|p| p.is_file())
}
pub fn analyze(
    root: &Path,
    relative: &Path,
    draft: Option<&Preparation>,
) -> Result<Option<Preparation>, String> {
    let (owner, path_in_source) =
        amp_core::music_sources::owner(root, relative).map_err(|e| e.to_string())?;
    analyze_with(path(), &owner, &path_in_source, draft)
}
pub fn analyze_cancellable(
    root: &Path,
    relative: &Path,
    draft: Option<&Preparation>,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Option<Preparation>, String> {
    let (owner, path_in_source) =
        amp_core::music_sources::owner(root, relative).map_err(|e| e.to_string())?;
    analyze_with_cancel(path(), &owner, &path_in_source, draft, Some(cancel))
}
fn analyze_with(
    executable: Option<PathBuf>,
    root: &Path,
    relative: &Path,
    draft: Option<&Preparation>,
) -> Result<Option<Preparation>, String> {
    analyze_with_cancel(executable, root, relative, draft, None)
}
fn analyze_with_cancel(
    executable: Option<PathBuf>,
    root: &Path,
    relative: &Path,
    draft: Option<&Preparation>,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<Option<Preparation>, String> {
    let executable = executable.ok_or("Install the optional AMP Analysis Engine from Plugin Manager to analyze songs. Playback and manual editing remain available.")?;
    let input = serde_json::to_vec(&Request {
        protocol: PROTOCOL,
        root: root.into(),
        relative: relative.into(),
        draft: draft.cloned(),
    })
    .map_err(|e| e.to_string())?;
    let mut command = Command::new(executable);
    // Share the player's bundled audio tools with an engine installed in addons/.
    if let Some(directory) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        let mut paths = vec![directory];
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        command.env(
            "PATH",
            std::env::join_paths(paths).map_err(|e| e.to_string())?,
        );
    }

    let bytes = run_process(command, &input, cancel)?;
    let response: Response = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Invalid analysis add-on response: {e}"))?;
    match response {
        Response::Prepared {
            protocol: PROTOCOL,
            preparation,
        } => {
            preparation.validate()?;
            Ok(Some(*preparation))
        }
        Response::Skipped { protocol: PROTOCOL } => Ok(None),
        Response::Error {
            protocol: PROTOCOL,
            message,
        } => Err(message),
        _ => Err("Analysis add-on protocol mismatch; update the add-on".into()),
    }
}

const CANCELLED: &str = "Analysis stopped; unfinished songs remain queued";
struct AnalysisProcess(Box<dyn process_wrap::std::ChildWrapper>);
impl Drop for AnalysisProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_process(
    mut command: Command,
    input: &[u8],
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<Vec<u8>, String> {
    use std::{
        sync::{atomic::Ordering::Relaxed, mpsc},
        time::{Duration, Instant},
    };
    if !crate::preparation_speed::wait(cancel) {
        return Err(CANCELLED.into());
    }
    crate::preparation_speed::configure(&mut command);
    // The parent owns scratch data, so even a forcibly stopped worker cannot leave
    // a song-sized input copy behind. No source audio is stored here permanently.
    let scratch = tempfile::Builder::new()
        .prefix("amp-analysis-job-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let marker = scratch.path().join("cancel");
    command
        .env("AMP_ANALYSIS_CANCEL_FILE", &marker)
        .env("AMP_PREPARATION_CACHE_DIR", scratch.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut wrapped = process_wrap::std::CommandWrap::from(command);
    #[cfg(windows)]
    wrapped
        .wrap(process_wrap::std::CreationFlags(
            windows::Win32::System::Threading::CREATE_NO_WINDOW,
        ))
        .wrap(process_wrap::std::JobObject);
    #[cfg(unix)]
    wrapped.wrap(process_wrap::std::ProcessGroup::leader());
    let mut owner = AnalysisProcess(wrapped.spawn().map_err(|e| e.to_string())?);
    let child = &mut owner.0;
    if let Err(e) = child
        .stdin()
        .take()
        .ok_or("Analysis input unavailable")?
        .write_all(input)
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e.to_string());
    }
    let stdout = child.stdout().take().ok_or("Analysis output unavailable")?;
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    let mut stopping = None;
    let status = loop {
        if cancel.is_some_and(|c| c.load(Relaxed)) {
            if stopping.is_none() {
                let _ = std::fs::write(&marker, b"stop");
                stopping = Some(Instant::now());
            }
            if stopping.is_some_and(|at| at.elapsed() >= Duration::from_millis(1500)) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CANCELLED.into());
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    };
    if stopping.is_some() {
        return Err(CANCELLED.into());
    }
    if !status.success() {
        return Err("Analysis add-on exited unexpectedly; existing metadata is preserved".into());
    }
    let bytes = rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "Analysis output did not close")??;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Analysis response is too large".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires AMP_ENGINE_TEST_EXE and ffmpeg"]
    fn real_installed_engine_round_trip() {
        let executable = PathBuf::from(std::env::var_os("AMP_ENGINE_TEST_EXE").unwrap());
        let dir = tempfile::tempdir().unwrap();
        let samples = 22050_u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + samples * 2).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&11025_u32.to_le_bytes());
        wav.extend_from_slice(&22050_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(samples * 2).to_le_bytes());
        for i in 0..samples {
            wav.extend_from_slice(
                &((i as f32 * 440. * std::f32::consts::TAU / 11025.)
                    .sin()
                    .mul_add(12000., 0.) as i16)
                    .to_le_bytes(),
            );
        }
        std::fs::write(dir.path().join("tone.wav"), wav).unwrap();
        let result = analyze_with(
            Some(executable.clone()),
            dir.path(),
            Path::new("tone.wav"),
            None,
        )
        .unwrap()
        .unwrap();
        assert!(result.analysis.is_some());
        assert!(
            analyze_with(
                Some(executable.clone()),
                dir.path(),
                Path::new("tone.wav"),
                None
            )
            .unwrap()
            .is_none()
        );
        let mut manual = result;
        manual.bpm = Some(127.);
        manual.manual_override = true;
        let draft = analyze_with(
            Some(executable),
            dir.path(),
            Path::new("tone.wav"),
            Some(&manual),
        )
        .unwrap()
        .unwrap();
        assert_eq!(draft.bpm, Some(127.));
        assert!(
            !amp_core::preparation::load(&dir.path().join("tone.wav"))
                .unwrap()
                .manual_override
        );
    }
    #[test]
    fn absent_addon_leaves_library_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.wav");
        std::fs::write(&audio, b"untouched").unwrap();
        let error = analyze_with(None, dir.path(), Path::new("song.wav"), None).unwrap_err();
        assert!(error.contains("optional AMP Analysis Engine"));
        assert!(!dir.path().join("ampdj").exists());
        assert_eq!(std::fs::read(audio).unwrap(), b"untouched");
    }
    #[test]
    fn missing_executable_does_not_fall_back_to_embedded_analysis() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            analyze_with(
                Some(dir.path().join("missing-engine")),
                dir.path(),
                Path::new("song.wav"),
                None
            )
            .is_err()
        );
        assert!(!dir.path().join("ampdj").exists());
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    #[test]
    fn stop_terminates_worker_descendants_and_cleans_only_owned_scratch() {
        use std::{
            sync::{
                Arc,
                atomic::{AtomicBool, Ordering::Relaxed},
            },
            time::{Duration, Instant},
        };
        let dir = tempfile::tempdir().unwrap();
        let beat = dir.path().join("beat");
        let ready = dir.path().join("ready");
        let scratch_path = dir.path().join("scratch-path");
        let script = r#"import os,sys,time,subprocess
from pathlib import Path
sys.stdin.buffer.read()
beat,ready,scratch=map(Path,sys.argv[1:])
stage=Path(os.environ['AMP_PREPARATION_CACHE_DIR'])
(stage/'input.audio').write_bytes(b'temporary input')
scratch.write_text(str(stage),encoding='utf-8')
worker="import sys,time;from pathlib import Path\np=Path(sys.argv[1])\nend=time.monotonic()+6\nwhile time.monotonic()<end:\n p.write_text(str(time.time_ns()));time.sleep(0.03)"
subprocess.Popen([sys.executable,'-c',worker,str(beat)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
while not beat.exists():time.sleep(0.01)
ready.write_text('ready')
time.sleep(4)
"#;
        let mut command = amp_core::command(if cfg!(windows) { "python" } else { "python3" });
        command
            .args(["-c", script])
            .arg(&beat)
            .arg(&ready)
            .arg(&scratch_path);
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let ready_file = ready.clone();
        let thread = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !ready_file.is_file() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(ready_file.is_file(), "Fake engine did not become ready");
            stop.store(true, Relaxed);
        });
        let result = run_process(command, b"{}", Some(&cancel));
        thread.join().unwrap();
        assert!(result.unwrap_err().contains("stopped"));
        let before = std::fs::read(&beat).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(
            std::fs::read(&beat).unwrap(),
            before,
            "A worker descendant kept running"
        );
        let scratch = std::fs::read_to_string(scratch_path).unwrap();
        assert!(
            !Path::new(&scratch).exists(),
            "Stopped analysis left its audio input copy behind"
        );
        assert!(
            dir.path().exists(),
            "Cancellation removed unrelated caller data"
        );
    }
}
