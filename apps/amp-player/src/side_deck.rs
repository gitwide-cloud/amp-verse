//! Auxiliary cue playback uses the same mixer/decoder as the main decks.
use crate::mixer::Engine;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::atomic::Ordering::Relaxed};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Operation {
    Load {
        song: String,
    },
    Play,
    Pause,
    Stop,
    Seek {
        seconds: f64,
    },
    HotCue {
        number: usize,
    },
    Volume {
        value: f32,
    },
    Send {
        #[serde(default)]
        deck: crate::control::Deck,
    },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub cue: bool,
    pub collapsed: bool,
    pub volume: f32,
    pub path: Option<PathBuf>,
    pub output: Option<String>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            cue: false,
            collapsed: false,
            volume: 0.7,
            path: None,
            output: None,
        }
    }
}
pub struct SideDeck {
    pub config: Config,
    pub audio: Option<Engine>,
    pub track: Option<amp_core::Track>,
    root: PathBuf,
    side: usize,
    seek: f64,
    master: f32,
    pub status: String,
    outputs: Vec<String>,
    cached: Vec<u8>,
    pending: Option<PathBuf>,
}
impl SideDeck {
    pub fn new(side: usize) -> Self {
        let config = if cfg!(test) {
            Config::default()
        } else {
            config_path(side)
                .and_then(|path| std::fs::read(path).ok())
                .and_then(|bytes| serde_json::from_slice::<Config>(&bytes).ok())
                .filter(|config| config.volume.is_finite() && (0. ..=1.).contains(&config.volume))
                .unwrap_or_default()
        };
        Self {
            config,
            audio: None,
            track: None,
            root: PathBuf::new(),
            side,
            seek: 0.,
            master: 0.75,
            status: String::new(),
            outputs: Vec::new(),
            cached: Vec::new(),
            pending: None,
        }
    }
    pub fn ensure(&mut self) -> Result<(), String> {
        if self.audio.is_none() {
            let mut audio = Engine::with_output(self.config.output.as_deref())?;
            audio.set_master(self.master);
            audio.set_fader(0.);
            audio.set_gain(0, self.config.volume);
            if let Some(track) = &self.track {
                audio.load(0, &self.root, track, false)?;
            }
            self.audio = Some(audio);
        }
        Ok(())
    }
    pub fn playing(&self) -> bool {
        self.audio.as_ref().is_some_and(|audio| audio.playing(0))
    }
    pub fn position(&self) -> f64 {
        self.audio
            .as_ref()
            .map_or(0., |audio| audio.source_position(0))
    }
    pub fn load(&mut self, path: PathBuf) -> Result<(), String> {
        if self.playing() {
            self.pending = Some(path);
            self.status = "Replacement staged. Stop the cue track, then apply it.".into();
            return Ok(());
        }
        let path = path.canonicalize().map_err(|error| error.to_string())?;
        let root = path
            .parent()
            .ok_or("Audio file has no parent")?
            .to_path_buf();
        let relative = PathBuf::from(path.file_name().ok_or("Audio filename missing")?);
        let preparation = amp_core::preparation::load(&path)?;
        let title = preparation
            .extensions
            .get("title")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let track = amp_core::Track {
            path: relative,
            title,
            preparation,
        };
        self.ensure()?;
        self.audio.as_mut().unwrap().load(0, &root, &track, false)?;
        self.root = root;
        self.seek = track.preparation.cue_in;
        self.track = Some(track);
        self.config.path = Some(path);
        self.pending = None;
        self.status = "Prepared · press Play to audition".into();
        self.save();
        Ok(())
    }
    pub fn play(&mut self) -> Result<(), String> {
        if self.track.is_none() {
            if let Some(path) = self.config.path.clone() {
                self.load(path)?;
            } else {
                return Err("Load a cue track first".into());
            }
        }
        self.ensure()?;
        let audio = self.audio.as_mut().unwrap();
        if audio.controls.finished[0].load(Relaxed) || !audio.loaded(0) {
            audio.load(0, &self.root, self.track.as_ref().unwrap(), true)?;
        } else {
            audio.controls.play[0].store(true, Relaxed);
        }
        Ok(())
    }
    pub fn pause(&self) {
        if let Some(audio) = &self.audio {
            audio.controls.play[0].store(false, Relaxed);
        }
    }
    pub fn stop(&mut self) {
        if let Some(audio) = &mut self.audio {
            audio.stop(0);
        }
    }
    pub fn seek(&mut self, seconds: f64) -> Result<(), String> {
        if !seconds.is_finite() || seconds < 0. {
            return Err("Cue position must be a nonnegative number of seconds".into());
        }
        let mut track = self.track.clone().ok_or("Load a cue track first")?;
        let end = track.preparation.cue_out.or(track
            .preparation
            .analysis
            .as_ref()
            .map(|analysis| analysis.duration));
        track.preparation.cue_in = end.map_or(seconds, |end| seconds.min((end - 0.02).max(0.)));
        let playing = self.playing();
        self.ensure()?;
        self.audio
            .as_mut()
            .unwrap()
            .load(0, &self.root, &track, playing)?;
        self.seek = track.preparation.cue_in;
        Ok(())
    }
    pub fn gain(&mut self, value: f32) -> Result<(), String> {
        if !value.is_finite() || !(0. ..=1.).contains(&value) {
            return Err("Cue volume must be 0–1".into());
        }
        self.config.volume = value;
        if let Some(audio) = &self.audio {
            audio.set_gain(0, value);
        }
        self.save();
        Ok(())
    }
    pub fn poll(&mut self, master: f32) {
        self.master = master;
        if let Some(audio) = &self.audio {
            audio.set_master(master);
            audio.set_gain(0, self.config.volume);
            if let Some(error) = audio.take_error() {
                self.status = error;
            }
        }
        self.save();
    }
    fn save(&mut self) {
        if cfg!(test) {
            return;
        }
        let Ok(bytes) = serde_json::to_vec(&self.config) else {
            return;
        };
        if bytes == self.cached {
            return;
        }
        let result = (|| -> std::io::Result<()> {
            use std::io::Write;
            let path = config_path(self.side)
                .ok_or_else(|| std::io::Error::other("Settings folder missing"))?;
            std::fs::create_dir_all(path.parent().unwrap())?;
            let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
            file.write_all(&bytes)?;
            file.persist(path).map_err(|error| error.error)?;
            Ok(())
        })();
        match result {
            Ok(()) => self.cached = bytes,
            Err(error) => self.status = format!("Could not save side-panel settings: {error}"),
        }
    }
    pub fn header(&mut self, ui: &mut egui::Ui, accent: egui::Color32) {
        ui.horizontal(|ui| {
            if ui
                .button(if self.config.collapsed { " > " } else { " < " })
                .on_hover_text("Collapse/expand this side panel; playback continues")
                .clicked()
            {
                self.config.collapsed = !self.config.collapsed;
            }
            if !self.config.collapsed {
                ui.colored_label(
                    accent,
                    format!(
                        "{} {}",
                        if self.config.cue {
                            "Cue Deck"
                        } else {
                            "Sampler"
                        },
                        if self.side == 0 { "A" } else { "B" }
                    ),
                );
            }
        });
        if !self.config.collapsed {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.config.cue, false, "Sampler");
                ui.selectable_value(&mut self.config.cue, true, "Cue Deck");
            });
        }
    }
    pub fn show_cue(
        &mut self,
        ui: &mut egui::Ui,
        accent: egui::Color32,
    ) -> Option<(PathBuf, f64, usize)> {
        if self.track.is_none()
            && self.config.path.is_some()
            && self.audio.is_none()
            && let Err(error) = self.load(self.config.path.clone().unwrap())
        {
            self.status = error;
            self.config.path = None;
        }
        let mut send = None;
        if ui.button("Load song...").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter(
                    "Audio",
                    &["mp3", "wav", "flac", "ogg", "m4a", "aac", "opus"],
                )
                .pick_file()
            && let Err(error) = self.load(path)
        {
            self.status = error;
        }
        let response = ui.add(
            egui::Label::new(
                self.track
                    .as_ref()
                    .map_or("Drop a song here", |track| track.title.as_str()),
            )
            .wrap()
            .sense(egui::Sense::hover()),
        );
        let drop_rect =
            egui::Rect::from_min_size(response.rect.min, egui::vec2(ui.available_width(), 65.));
        let response = ui.interact(
            drop_rect,
            ui.scope_id().with("cue-drop"),
            egui::Sense::hover(),
        );
        if let Some(path) = crate::dragdrop::target(&response, accent)
            && let Err(error) = self.load(path)
        {
            self.status = error;
        }
        ui.add_space(8.);
        if let Some(track) = &self.track {
            ui.weak(format!(
                "{} BPM · {}",
                track
                    .preparation
                    .bpm
                    .map_or("--".into(), |bpm| format!("{bpm:.1}")),
                track
                    .preparation
                    .harmonic_key
                    .as_deref()
                    .or(track.preparation.key.as_deref())
                    .unwrap_or("--")
            ));
        }
        ui.label(format!(
            "{:02}:{:02}",
            self.position() as u64 / 60,
            self.position() as u64 % 60
        ));
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(if self.playing() { "Pause" } else { "Play" })
                .clicked()
            {
                if self.playing() {
                    self.pause();
                } else if let Err(error) = self.play() {
                    self.status = error;
                }
            }
            if ui.button("Stop").clicked() {
                self.stop();
            }
        });
        ui.spacing_mut().slider_width = (ui.available_width() - 45.).max(40.);
        let mut volume = self.config.volume;
        if ui
            .add(
                egui::Slider::new(&mut volume, 0. ..=1.)
                    .show_value(false)
                    .text("Vol"),
            )
            .changed()
        {
            let _ = self.gain(volume);
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.seek)
                    .range(0. ..=86400.)
                    .suffix(" s"),
            );
            if ui.button("Seek").clicked()
                && let Err(error) = self.seek(self.seek)
            {
                self.status = error;
            }
        });
        let prep = self.track.as_ref().map(|t| t.preparation.clone());
        if let Some(action) = crate::hot_cues::pads(ui, prep.as_ref(), accent) {
            let result = match action {
                crate::hot_cues::Action::Jump(seconds) => self.seek(seconds),
                _ => {
                    if let (Some(path), Some(prep)) = (&self.config.path, &prep) {
                        crate::hot_cues::edit(&self.root, path, prep, action, self.position()).map(
                            |saved| {
                                if let Some(track) = &mut self.track {
                                    crate::hot_cues::update_snapshot(&mut track.preparation, saved);
                                }
                            },
                        )
                    } else {
                        Err("Load a track first".into())
                    }
                }
            };
            if let Err(error) = result {
                self.status = error;
            }
        }
        ui.horizontal(|ui| {
            ui.label("Send →");
            for (deck, label, color) in [(0, "A", crate::skin::ORANGE), (1, "B", crate::skin::CYAN)]
            {
                if ui
                    .add(egui::Button::new(egui::RichText::new(label).color(color)))
                    .clicked()
                    && let Some(path) = &self.config.path
                {
                    send = Some((path.clone(), self.position(), deck));
                }
            }
        });
        ui.separator();
        ui.strong("Cue output");
        if ui.button("Refresh audio devices").clicked() {
            self.outputs = Engine::output_names();
        }
        let before = self.config.output.clone();
        ui.add_enabled_ui(!self.playing(), |ui| {
            egui::ComboBox::from_id_salt("cue-output")
                .width((ui.available_width() - 10.).max(50.))
                .truncate()
                .selected_text(
                    self.config
                        .output
                        .as_deref()
                        .unwrap_or("Shared default output"),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.config.output, None, "Shared default output");
                    for name in &self.outputs {
                        ui.selectable_value(&mut self.config.output, Some(name.clone()), name);
                    }
                });
        });
        if before != self.config.output {
            self.audio = None;
            self.status = "Cue output changed; prepared track retained".into();
        }
        ui.weak(if self.config.output.is_some() {
            "Cue uses the selected device. Sampler output remains separate."
        } else {
            "Shared speakers: no separate headphone monitoring. Choose a headphone device above."
        });
        if self.pending.is_some()
            && ui
                .add_enabled(!self.playing(), egui::Button::new("Apply staged track"))
                .clicked()
            && let Some(path) = self.pending.take()
            && let Err(error) = self.load(path)
        {
            self.status = error;
        }
        ui.add(egui::Label::new(&self.status).wrap());
        send
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"loaded":self.track.is_some(),"playing":self.playing(),"position":self.position(),"title":self.track.as_ref().map(|track|&track.title),"volume":self.config.volume,"output":self.config.output,"mode":if self.config.cue{"cue"}else{"sampler"}})
    }
}
fn config_path(side: usize) -> Option<PathBuf> {
    crate::theme::settings_path().map(|path| path.with_file_name(format!("dj-side-{side}.json")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires native audio and AMP_WAVEFORM_FIXTURE; output muted"]
    fn auxiliary_transport_and_mode_changes_leave_main_playing() {
        let path = PathBuf::from(std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap())
            .canonicalize()
            .unwrap();
        let mut main = Engine::new().unwrap();
        main.set_master(0.);
        let track = amp_core::Track {
            path: PathBuf::from(path.file_name().unwrap()),
            title: "Main".into(),
            preparation: amp_core::preparation::load(&path).unwrap(),
        };
        main.load(0, path.parent().unwrap(), &track, true).unwrap();
        let mut cue = SideDeck::new(0);
        cue.poll(0.);
        cue.load(path.clone()).unwrap();
        cue.play().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!(main.playing(0) && cue.playing());
        cue.config.cue = false;
        cue.config.collapsed = true;
        assert!(cue.playing());
        cue.load(path.clone()).unwrap();
        assert!(cue.pending.is_some());
        cue.seek(2.).unwrap();
        cue.pause();
        cue.stop();
        assert!(main.playing(0));
        assert!(main.source_position(0) > 0.);
        assert_eq!(track.preparation.cue_in, 0.);
        let mut player = crate::Player {
            volume: 0,
            ..Default::default()
        };
        player.safe_load_path(0, path.clone(), None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        player.audio.as_ref().unwrap().toggle(0);
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(player.audio.as_ref().unwrap().playing(0));
        let before = player.loaded_snapshot[0].clone();
        player.safe_load_path(0, path, Some(4.)).unwrap();
        assert!(player.pending_load[0].is_some());
        assert_eq!(
            player.loaded_snapshot[0]
                .as_ref()
                .unwrap()
                .preparation
                .cue_in,
            before.unwrap().preparation.cue_in
        );
        assert!(player.audio.as_ref().unwrap().playing(0));
    }
    #[test]
    fn side_modes_and_collapse_preserve_independent_state() {
        let mut left = SideDeck::new(0);
        let right = SideDeck::new(1);
        left.config.cue = true;
        left.config.collapsed = true;
        left.config.path = Some("cue.mp3".into());
        assert!(!right.config.cue && !right.config.collapsed);
        let restored: Config =
            serde_json::from_slice(&serde_json::to_vec(&left.config).unwrap()).unwrap();
        assert!(restored.cue && restored.collapsed);
        assert_eq!(restored.path, left.config.path);
        assert!(left.gain(f32::NAN).is_err());
        assert!(left.seek(-1.).is_err());
    }
}
