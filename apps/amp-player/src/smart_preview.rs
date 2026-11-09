//! Disposable audio preview. Never publishes rules, cues or playlist membership.
use crate::mixer::Engine;
use amp_core::{Track, show::Style};
use std::{
    path::{Path, PathBuf},
    sync::atomic::Ordering::Relaxed,
};

#[derive(Default)]
pub struct Preview {
    audio: Option<Engine>,
    root: PathBuf,
    tracks: Vec<Track>,
    active: usize,
    next: usize,
    transitioning: bool,
    fade: f32,
    pub status: String,
}
fn excerpt(mut track: Track) -> Track {
    let end = track.preparation.cue_out.or(track
        .preparation
        .analysis
        .as_ref()
        .map(|analysis| analysis.duration));
    let limit = track.preparation.cue_in + 20.;
    track.preparation.cue_out = Some(end.map_or(limit, |end| end.min(limit)));
    track
}
impl Preview {
    pub fn active(&self) -> bool {
        self.audio.is_some()
    }
    pub fn stop(&mut self) {
        self.audio = None;
        self.status = "Preview stopped".into();
    }
    pub fn start(
        &mut self,
        root: &Path,
        tracks: Vec<Track>,
        profile: &amp_core::show::SmartPlaylist,
        volume: f32,
    ) -> Result<(), String> {
        if tracks.is_empty() {
            return Err("Choose a source containing indexed songs first".into());
        }
        self.stop();
        self.root = root.into();
        self.tracks = tracks
            .into_iter()
            .take(3)
            .map(|mut track| {
                if profile.style == Style::Hype
                    && profile.use_manual_cues
                    && let Some(cue) = track.preparation.manual_cues.iter().find(|cue| {
                        (cue.label.to_lowercase().contains("drop")
                            || cue.label.to_lowercase().contains("build"))
                            && track
                                .preparation
                                .cue_out
                                .is_none_or(|end| cue.seconds < end)
                    })
                {
                    track.preparation.cue_in = cue.seconds;
                }
                excerpt(track)
            })
            .collect();
        self.active = 0;
        self.next = 1;
        self.transitioning = false;
        self.fade = match profile.style {
            Style::Hype => 3.,
            Style::Steady => 6.,
            Style::Chill => 10.,
        };
        let mut audio = Engine::new()?;
        audio.set_master(volume);
        audio.load(0, root, &self.tracks[0], true)?;
        audio.set_fader(0.);
        if let Some(track) = self.tracks.get(1) {
            audio.load(1, root, track, false)?;
        }
        self.audio = Some(audio);
        self.status =
            "Playing a short mix preview · up to three tracks · saved rules unchanged".into();
        Ok(())
    }
    pub fn poll(&mut self) {
        let Some(audio) = &mut self.audio else {
            return;
        };
        if let Some(error) = audio.take_error() {
            self.status = format!("Preview: {error}");
            self.audio = None;
            return;
        }
        if self.transitioning
            && !audio.fading()
            && (audio.fader() - (1 - self.active) as f32).abs() < 0.001
        {
            self.active = 1 - self.active;
            self.next += 1;
            self.transitioning = false;
            if let Some(track) = self.tracks.get(self.next)
                && let Err(error) = audio.load(1 - self.active, &self.root, track, false)
            {
                self.status = error;
                self.audio = None;
                return;
            }
        }
        let finished = audio.controls.finished[self.active].load(Relaxed);
        if !self.transitioning
            && self.next < self.tracks.len()
            && (finished
                || audio
                    .remaining(self.active)
                    .is_some_and(|remaining| remaining <= self.fade))
        {
            if !audio.loaded(1 - self.active)
                || !audio.controls.ready[1 - self.active].load(Relaxed)
            {
                return;
            }
            if finished {
                audio.controls.play[1 - self.active].store(true, Relaxed);
                audio.set_fader((1 - self.active) as f32);
                self.transitioning = true;
                return;
            }
            match audio.crossfade(self.fade) {
                Ok(()) => self.transitioning = true,
                Err(error) => {
                    self.status = error;
                    self.audio = None;
                }
            }
        } else if finished && self.next >= self.tracks.len() && !self.transitioning {
            self.audio = None;
            self.status = "Mix preview finished · saved playlist unchanged".into();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires native audio and FFmpeg; preview master is muted"]
    fn native_mix_preview_finishes_without_changing_sources_or_rules() {
        let dir = tempfile::tempdir().unwrap();
        let status = amp_core::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1.5",
            ])
            .arg(dir.path().join("tone.wav"))
            .status()
            .unwrap();
        assert!(status.success());
        let before = std::fs::read(dir.path().join("tone.wav")).unwrap();
        let track = Track {
            path: "tone.wav".into(),
            title: "Tone".into(),
            preparation: amp_core::Preparation {
                cue_out: Some(1.5),
                ..Default::default()
            },
        };
        let profile = amp_core::show::Settings::default().playlists.remove(0);
        let original = serde_json::to_value(&profile).unwrap();
        let mut preview = Preview::default();
        preview
            .start(dir.path(), vec![track.clone(), track], &profile, 0.)
            .unwrap();
        preview.fade = 0.3;
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while preview.active() && std::time::Instant::now() < until {
            preview.poll();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(!preview.active(), "Preview must finish: {}", preview.status);
        assert!(preview.status.contains("finished"), "{}", preview.status);
        assert_eq!(std::fs::read(dir.path().join("tone.wav")).unwrap(), before);
        assert_eq!(serde_json::to_value(&profile).unwrap(), original);
        assert!(!dir.path().join(".ampverse/show.json").exists());
    }
    #[test]
    fn preview_excerpts_preserve_original_preparation() {
        let track = Track {
            path: "song.mp3".into(),
            title: "Song".into(),
            preparation: amp_core::Preparation {
                cue_in: 5.,
                cue_out: Some(120.),
                ..Default::default()
            },
        };
        let original = serde_json::to_value(&track.preparation).unwrap();
        let clip = excerpt(track.clone());
        assert_eq!(clip.preparation.cue_in, 5.);
        assert_eq!(clip.preparation.cue_out, Some(25.));
        assert_eq!(serde_json::to_value(&track.preparation).unwrap(), original);
    }
}
