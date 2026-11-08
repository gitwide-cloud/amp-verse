//! Cue-aligned mix arrangement shared with the Lab shell. No source files are modified.
use crate::{Preparation, resolve};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip {
    pub path: PathBuf,
    pub start: f64,
    pub preparation: Preparation,
    pub gain_db: f64,
}
impl Clip {
    pub fn duration(&self) -> Result<f64, String> {
        self.preparation.validate()?;
        if !self.start.is_finite()
            || self.start < 0.
            || !self.gain_db.is_finite()
            || !(-60. ..=12.).contains(&self.gain_db)
        {
            return Err("Invalid timeline start or gain".into());
        }
        self.preparation
            .cue_out
            .map(|end| end - self.preparation.cue_in)
            .ok_or_else(|| "Set cue OUT before exporting".into())
    }
}
pub fn filter(clips: &[Clip], combined: bool) -> Result<String, String> {
    if clips.is_empty() {
        return Err("Add at least one track".into());
    }
    let mut graph = Vec::new();
    for (i, clip) in clips.iter().enumerate() {
        let duration = clip.duration()?;
        let delay = if combined { clip.start } else { 0. };
        graph.push(format!("[{i}:a]atrim=start={}:duration={duration},asetpts=PTS-STARTPTS,aresample=44100,aformat=channel_layouts=stereo,volume={}dB,adelay={}:all=1[c{i}]", clip.preparation.cue_in, clip.gain_db, (delay*1000.).round()));
    }
    let inputs = (0..clips.len())
        .map(|i| format!("[c{i}]"))
        .collect::<String>();
    graph.push(format!("{inputs}amix=inputs={}:normalize=0:duration=longest,alimiter=limit=0.95:level=0:latency=1[out]", clips.len()));
    Ok(graph.join(";"))
}
pub fn render(root: &Path, clips: &[Clip], output: &Path) -> Result<(), String> {
    if output.exists() {
        return Err("Output already exists; choose a new filename".into());
    }
    let graph = filter(clips, true)?;
    let mut command = crate::command("ffmpeg");
    command.args(["-n", "-v", "error"]);
    for clip in clips {
        command
            .arg("-i")
            .arg(resolve(root, &clip.path).map_err(|e| e.to_string())?);
    }
    let result = command
        .args([
            "-filter_complex",
            &graph,
            "-map",
            "[out]",
            "-c:a",
            "libmp3lame",
            "-b:a",
            "320k",
        ])
        .arg(output)
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn clip() -> Clip {
        Clip {
            path: "song.mp3".into(),
            start: 5.,
            preparation: Preparation {
                cue_in: 2.,
                cue_out: Some(7.),
                ..Default::default()
            },
            gain_db: -3.,
        }
    }
    #[test]
    #[ignore = "requires ffmpeg and ffprobe on PATH"]
    fn renders_real_combined_and_individual_mp3() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("song.wav");
        assert!(
            crate::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=2"
                ])
                .arg(&input)
                .status()
                .unwrap()
                .success()
        );
        let clip = Clip {
            path: "song.wav".into(),
            start: 0.5,
            preparation: Preparation {
                cue_in: 0.25,
                cue_out: Some(1.25),
                ..Default::default()
            },
            gain_db: -3.,
        };
        let out = dir.path().join("mix.mp3");
        render(dir.path(), &[clip.clone(), clip.clone()], &out).unwrap();
        let probe = crate::command("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=nw=1:nk=1",
            ])
            .arg(&out)
            .output()
            .unwrap();
        let duration: f64 = String::from_utf8_lossy(&probe.stdout)
            .trim()
            .parse()
            .unwrap();
        assert!((duration - 1.5).abs() < 0.1, "duration {duration}");
        assert!(
            render(dir.path(), std::slice::from_ref(&clip), &out).is_err(),
            "must not overwrite"
        );
        let mut single = clip;
        single.start = 0.;
        let out = dir.path().join("single.mp3");
        render(dir.path(), &[single], &out).unwrap();
        assert!(
            crate::command("ffmpeg")
                .args(["-v", "error", "-i"])
                .arg(&out)
                .args(["-f", "null", "-"])
                .status()
                .unwrap()
                .success()
        );
    }
    #[test]
    fn cue_trim_and_timeline_alignment() {
        let graph = filter(&[clip(), clip()], true).unwrap();
        assert!(graph.contains("atrim=start=2:duration=5"));
        assert!(graph.contains("adelay=5000:all=1"));
        assert!(graph.contains("amix=inputs=2"));
        assert!(filter(&[clip()], false).unwrap().contains("adelay=0"));
    }
    #[test]
    fn invalid_arrangements_fail_before_encoding() {
        assert!(filter(&[], true).is_err());
        let mut c = clip();
        c.start = -1.;
        assert!(filter(&[c.clone()], true).is_err());
        c.start = 0.;
        c.preparation.cue_out = None;
        assert!(filter(&[c], true).is_err());
    }
}
