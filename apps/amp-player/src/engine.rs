//! Optional, explicitly installed analysis process. Playback never depends on it.
use amp_core::{
    Preparation,
    engine_api::{PROTOCOL, Request, Response},
};
use std::{
    io::Write,
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
fn analyze_with(
    executable: Option<PathBuf>,
    root: &Path,
    relative: &Path,
    draft: Option<&Preparation>,
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

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    crate::preparation_speed::wait(None);
    crate::preparation_speed::configure(&mut command);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Err(e) = child.stdin.take().unwrap().write_all(&input) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e.to_string());
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Analysis add-on exited unexpectedly; existing metadata is preserved".into());
    }
    let response: Response = serde_json::from_slice(&output.stdout)
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
