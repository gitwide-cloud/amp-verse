//! Nine preloaded one-shot pads. File decoding and settings writes stay off the audio callback.
use crate::mixer::Engine;
use eframe::egui::{self, RichText};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, mpsc},
};
pub type Frames = Arc<[[f32; 2]]>;
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Bank {
    paths: [Option<PathBuf>; 9],
    gains: [f32; 9],
    master: f32,
    names: [String; 9],
    loops: [bool; 9],
}
impl Default for Bank {
    fn default() -> Self {
        Self {
            paths: Default::default(),
            gains: [1.; 9],
            master: 0.7,
            names: Default::default(),
            loops: [false; 9],
        }
    }
}
fn settings() -> Option<PathBuf> {
    crate::theme::settings_path().map(|p| p.with_file_name("sampler.json"))
}
fn load_bank() -> Bank {
    load_bank_at(settings())
}
fn load_bank_at(path: Option<PathBuf>) -> Bank {
    if cfg!(test) {
        return Bank::default();
    }
    let bank = path
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Bank>(&b).ok())
        .unwrap_or_default();
    if std::iter::once(bank.master)
        .chain(bank.gains)
        .all(|g| g.is_finite() && (0. ..=1.).contains(&g))
    {
        bank
    } else {
        Bank::default()
    }
}
fn decode(path: &Path, rate: u32) -> Result<Frames, String> {
    let max = (rate as usize * 30).min(12 * 1024 * 1024 / 8);
    let mut child = amp_core::command("ffmpeg")
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(path)
        .args([
            "-vn",
            "-ac",
            "2",
            "-ar",
            &rate.to_string(),
            "-t",
            "31",
            "-f",
            "f32le",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let result = (|| {
        let mut input = BufReader::new(child.stdout.take().ok_or("No sampler decoder output")?);
        let mut frames = Vec::new();
        let mut bytes = [0; 8];
        loop {
            match input.read_exact(&mut bytes) {
                Ok(()) => {
                    if frames.len() >= max {
                        return Err("Effect exceeds the 30-second / 12 MiB per-pad limit".into());
                    }
                    let a = f32::from_le_bytes(bytes[..4].try_into().unwrap());
                    let b = f32::from_le_bytes(bytes[4..].try_into().unwrap());
                    if !a.is_finite() || !b.is_finite() {
                        return Err("Effect contains invalid audio samples".into());
                    }
                    frames.push([a, b]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        if frames.is_empty() {
            return Err("No playable audio in this effect".into());
        }
        Ok(Arc::from(frames))
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if result.is_ok() && !status.success() {
        return Err("Unable to decode this sound effect".into());
    }
    result
}
struct Loaded {
    pad: usize,
    path: PathBuf,
    result: Result<Frames, String>,
}
pub enum Action {
    Load(usize),
    Trigger(usize),
    Stop(usize),
    Clear(usize),
    StopAll,
}
pub struct Sampler {
    bank: Bank,
    settings_path: Option<PathBuf>,
    ready: [bool; 9],
    selected_pad: usize,
    queue: VecDeque<(usize, PathBuf)>,
    job: Option<mpsc::Receiver<Loaded>>,
    loading: Option<usize>,
    pub started: bool,
    pub status: String,
}
impl Default for Sampler {
    fn default() -> Self {
        Self {
            bank: load_bank(),
            settings_path: settings(),
            ready: [false; 9],
            selected_pad: 0,
            queue: VecDeque::new(),
            job: None,
            loading: None,
            started: false,
            status: "Load sound effects into the nine pads. Shortcuts: 1–9 while DJ is focused."
                .into(),
        }
    }
}
impl Sampler {
    pub fn needs_audio(&self) -> bool {
        !self.queue.is_empty() || self.job.is_some()
    }
    pub fn has_saved(&self) -> bool {
        self.bank.paths.iter().any(Option::is_some)
    }
    pub fn separate_bank(name: &str) -> Self {
        let settings_path = crate::theme::settings_path().map(|path| path.with_file_name(name));
        Self {
            bank: load_bank_at(settings_path.clone()),
            settings_path,
            ..Default::default()
        }
    }
    fn save(&mut self) {
        use std::io::Write;
        let result = (|| -> Result<(), String> {
            let path = self
                .settings_path
                .as_ref()
                .ok_or("Settings folder unavailable")?;
            let directory = path.parent().ok_or("Settings folder unavailable")?;
            std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
            let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
            file.write_all(&serde_json::to_vec_pretty(&self.bank).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            file.persist(path).map_err(|e| e.to_string())?;
            Ok(())
        })();
        if let Err(error) = result {
            self.status = format!("Sampler settings were not saved: {error}");
        }
    }
    pub fn compact(
        &mut self,
        ui: &mut egui::Ui,
        engine: Option<&Engine>,
        accent: egui::Color32,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        ui.label("Volume");
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 65.).max(50.);
            let mut changed = ui
                .add(egui::Slider::new(&mut self.bank.master, 0. ..=1.).show_value(false))
                .changed();
            changed |= ui
                .add(
                    egui::DragValue::new(&mut self.bank.master)
                        .range(0. ..=1.)
                        .speed(0.01)
                        .fixed_decimals(2),
                )
                .changed();
            if changed {
                self.save();
            }
        });
        ui.add_space(8.);
        let side = ((ui.available_width() - 12.) / 3.).max(34.);
        egui::Grid::new("compact-pads")
            .spacing([4., 6.])
            .show(ui, |ui| {
                for pad in 0..9 {
                    ui.push_id(pad, |ui| {
                        ui.vertical(|ui| {
                            let playing = engine.is_some_and(|audio| {
                                audio.controls.sample_playing[pad]
                                    .load(std::sync::atomic::Ordering::Relaxed)
                            });
                            let name = if self.bank.names[pad].is_empty() {
                                format!("PAD {}", pad + 1)
                            } else {
                                self.bank.names[pad].clone()
                            };
                            let response = ui
                                .add_sized(
                                    [side, side],
                                    egui::Button::new(RichText::new(name).color(accent).size(12.))
                                        .wrap()
                                        .selected(playing),
                                )
                                .on_hover_text(
                                    self.bank.paths[pad]
                                        .as_ref()
                                        .map(|path| path.display().to_string())
                                        .unwrap_or_else(|| {
                                            "Drop audio here or click to load".into()
                                        }),
                                );
                            response.context_menu(|ui| {
                                if ui.button("Replace...").clicked() {
                                    actions.push(Action::Load(pad));
                                    ui.close();
                                }
                                ui.label("Pad name");
                                if ui.text_edit_singleline(&mut self.bank.names[pad]).changed() {
                                    self.save();
                                }
                                if ui
                                    .checkbox(&mut self.bank.loops[pad], "Loop playback")
                                    .changed()
                                {
                                    self.save();
                                }
                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.bank.gains[pad], 0. ..=1.)
                                            .text("Level"),
                                    )
                                    .changed()
                                {
                                    self.save();
                                }
                                if ui.button("Stop").clicked() {
                                    actions.push(Action::Stop(pad));
                                    ui.close();
                                }
                                if ui.button("Clear").clicked() {
                                    actions.push(Action::Clear(pad));
                                    ui.close();
                                }
                            });
                            if let Some(path) = crate::dragdrop::target(&response, accent) {
                                if playing {
                                    self.status =
                                        "Stop this pad before replacing its sample".into();
                                } else {
                                    self.request_load(pad, path);
                                }
                            }
                            if response.clicked() {
                                self.selected_pad = pad;
                                actions.push(if self.ready[pad] {
                                    Action::Trigger(pad)
                                } else {
                                    Action::Load(pad)
                                });
                            }
                        });
                    });
                    if pad % 3 == 2 {
                        ui.end_row();
                    }
                }
            });
        ui.add_space(18.);
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 12.) / 3.;
            for (label, action) in [
                ("Play", Action::Trigger(self.selected_pad)),
                ("Stop", Action::Stop(self.selected_pad)),
                ("Clear", Action::Clear(self.selected_pad)),
            ] {
                let response = ui.add_sized([width, 36.], egui::Button::new(label));
                if response.clicked() {
                    actions.push(action);
                }
                response.context_menu(|ui| {
                    if ui.button("Stop all pads").clicked() {
                        actions.push(Action::StopAll);
                        ui.close();
                    }
                });
            }
        });
        if !self.status.starts_with("Load sound effects") {
            ui.add(egui::Label::new(&self.status).wrap());
        }
        actions
    }
    pub fn restore(&mut self) {
        self.started = true;
        for (pad, path) in self.bank.paths.iter().enumerate() {
            if let Some(path) = path {
                self.queue.push_back((pad, path.clone()));
            }
        }
    }
    pub fn request_load(&mut self, pad: usize, path: PathBuf) {
        self.queue.push_back((pad, path));
        self.status = format!("Loading pad {}…", pad + 1);
    }
    pub fn poll(&mut self, engine: &mut Engine) {
        engine.sample_maintenance();
        engine.sample_levels(self.bank.master, self.bank.gains);
        for pad in 0..9 {
            engine.controls.sample_looping[pad]
                .store(self.bank.loops[pad], std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok(loaded) => {
                    match loaded
                        .result
                        .and_then(|frames| engine.sample_load(loaded.pad, Some(frames)))
                    {
                        Ok(()) => {
                            self.bank.paths[loaded.pad] = Some(loaded.path);
                            self.ready[loaded.pad] = true;
                            self.status = format!("Pad {} ready", loaded.pad + 1);
                            self.save();
                        }
                        Err(e) => self.status = format!("Pad {}: {e}", loaded.pad + 1),
                    }
                    self.job = None;
                    self.loading = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.status = "Sampler loader stopped unexpectedly".into();
                    self.job = None;
                    self.loading = None;
                }
                Err(mpsc::TryRecvError::Empty) => (),
            }
        }
        if self.job.is_none()
            && let Some((pad, path)) = self.queue.pop_front()
        {
            let rate = engine.sample_rate;
            let (tx, rx) = mpsc::channel();
            self.loading = Some(pad);
            self.job = Some(rx);
            std::thread::spawn(move || {
                let result = decode(&path, rate);
                let _ = tx.send(Loaded { pad, path, result });
            });
        }
    }
    pub fn clear(&mut self, pad: usize, engine: &mut Engine) -> Result<(), String> {
        engine.sample_load(pad, None)?;
        self.ready[pad] = false;
        self.bank.paths[pad] = None;
        self.status = format!("Pad {} cleared", pad + 1);
        self.save();
        Ok(())
    }
    pub fn show(&mut self, ui: &mut egui::Ui, engine: Option<&Engine>) -> Vec<Action> {
        crate::skin::panel(ui, true, |ui| self.content(ui, engine))
    }
    fn content(&mut self, ui: &mut egui::Ui, engine: Option<&Engine>) -> Vec<Action> {
        let mut actions = Vec::new();
        let busy = self.job.is_some() || !self.queue.is_empty();
        egui::CollapsingHeader::new("SAMPLER · 9 SOUND EFFECT PADS").id_salt("sampler-bank").default_open(true).show(ui,|ui|{
            ui.horizontal(|ui|{
                if ui.add(egui::Slider::new(&mut self.bank.master,0. ..=1.).text("Sampler volume")).changed() {self.save();}
                if ui.button("Stop all pads").clicked() {actions.push(Action::StopAll);}
                ui.weak("One-shot · 1–9 · independent of crossfader");
            });
            let pad_width=((ui.available_width()-24.)/3.).clamp(144.,192.);
            let grid_width=pad_width*3.+24.;
            ui.horizontal(|ui|{
            ui.add_space(((ui.available_width()-grid_width)/2.).max(0.));
            egui::Grid::new("sampler-pads").num_columns(3).spacing([12.,12.]).show(ui,|ui|{
                for pad in 0..9 {
                    ui.push_id(pad,|ui|ui.vertical(|ui|{
                        let playing=engine.is_some_and(|e|e.controls.sample_playing[pad].load(std::sync::atomic::Ordering::Relaxed));
                        let name=self.bank.paths[pad].as_ref().and_then(|p|p.file_stem()).map(|s|s.to_string_lossy().into_owned()).unwrap_or_else(||"Load effect…".into());
                        let loading=self.loading==Some(pad)||self.queue.iter().any(|(p,_)|*p==pad);
                        let short:String=name.chars().take(34).collect();
                        let label=format!("PAD {}\n\n{}{}",pad+1,if loading {"Loading…"} else {&short},if playing {"\nPLAYING"} else {""});
                        ui.set_width(pad_width);
                        let text=RichText::new(label).size(18.).strong().color(if playing {crate::skin::signal(ui)} else {crate::skin::accent(ui)});
                        let response=ui.add_enabled_ui(!loading,|ui|ui.add_sized([pad_width,pad_width],egui::Button::new(text).wrap().selected(playing))).inner.on_hover_text(self.bank.paths[pad].as_ref().map(|p|format!("{}\nClick to play/restart. Key {}. Right-click for pad volume.",p.display(),pad+1)).unwrap_or_else(||"Load a short sound effect (up to 30 seconds / 12 MiB decoded).".into()));
                        response.context_menu(|ui|{
                            if ui.add(egui::Slider::new(&mut self.bank.gains[pad],0. ..=1.).text("Pad volume")).changed() {self.save();}
                        });
                        if response.clicked() {
                            if self.ready[pad] {actions.push(Action::Trigger(pad));} else {actions.push(Action::Load(pad));}
                        }
                        ui.horizontal(|ui|{
                            ui.spacing_mut().item_spacing.x=6.;
                            let width=(pad_width-12.)/3.;
                            for (label,enabled,action) in [("Load",!busy,Action::Load(pad)),("Stop",playing,Action::Stop(pad)),("Clear",!busy&&self.bank.paths[pad].is_some(),Action::Clear(pad))] {
                                if ui.add_enabled_ui(enabled,|ui|ui.add_sized([width,44.],egui::Button::new(label))).inner.clicked(){actions.push(action);}
                            }
                        });
                    }));
                    if pad%3==2 {ui.end_row();}
                }
            });
            });
            ui.weak(&self.status);
        });
        if !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.focused) {
            for event in ui.input(|i| i.events.clone()) {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } = event
                    && modifiers == egui::Modifiers::NONE
                    && let Some(pad) = shortcut(key)
                    && self.ready[pad]
                {
                    actions.push(Action::Trigger(pad));
                }
            }
        }
        if let Some(engine) = engine {
            engine.sample_levels(self.bank.master, self.bank.gains);
        }
        actions
    }
}
pub(super) fn shortcut(key: egui::Key) -> Option<usize> {
    [
        egui::Key::Num1,
        egui::Key::Num2,
        egui::Key::Num3,
        egui::Key::Num4,
        egui::Key::Num5,
        egui::Key::Num6,
        egui::Key::Num7,
        egui::Key::Num8,
        egui::Key::Num9,
    ]
    .iter()
    .position(|k| *k == key)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bank_replacement_persists_independent_assignments() {
        let dir = tempfile::tempdir().unwrap();
        let mut left = Sampler {
            settings_path: Some(dir.path().join("left.json")),
            ..Default::default()
        };
        let mut right = Sampler {
            settings_path: Some(dir.path().join("right.json")),
            ..Default::default()
        };
        left.bank.names[0] = "Left".into();
        right.bank.names[0] = "Right".into();
        left.save();
        right.save();
        left.bank.loops[0] = true;
        left.save();
        let a: Bank =
            serde_json::from_slice(&std::fs::read(left.settings_path.unwrap()).unwrap()).unwrap();
        let b: Bank =
            serde_json::from_slice(&std::fs::read(right.settings_path.unwrap()).unwrap()).unwrap();
        assert!(a.loops[0] && !b.loops[0]);
        assert_eq!(a.names[0], "Left");
        assert_eq!(b.names[0], "Right");
    }
    #[test]
    fn all_nine_shortcuts_and_bank_paths_round_trip() {
        assert_eq!(shortcut(egui::Key::Num1), Some(0));
        assert_eq!(shortcut(egui::Key::Num9), Some(8));
        assert_eq!(shortcut(egui::Key::Num0), None);
        let mut b = Bank::default();
        b.paths[8] = Some(PathBuf::from("effects/air horn.wav"));
        b.gains[8] = 0.4;
        b.names[8] = "Air horn".into();
        b.loops[8] = true;
        let restored: Bank = serde_json::from_slice(&serde_json::to_vec(&b).unwrap()).unwrap();
        assert_eq!(restored.paths, b.paths);
        assert_eq!(restored.gains, b.gains);
        assert_eq!(restored.names, b.names);
        assert_eq!(restored.loops, b.loops);
        assert!(serde_json::from_str::<Bank>(r#"{"paths":[null,null,null,null,null,null,null,null,null],"gains":[1,1,1,1,1,1,1,1,1],"master":0.7}"#).is_ok());
    }
    #[test]
    #[ignore = "requires native audio, ffmpeg and AMP_WAVEFORM_FIXTURE; muted"]
    fn native_nine_pad_playback_and_stop() {
        let path = std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap();
        let mut engine = Engine::new().unwrap();
        engine.set_master(0.);
        let frames = decode(Path::new(&path), engine.sample_rate).unwrap();
        for pad in 0..9 {
            engine.sample_load(pad, Some(frames.clone())).unwrap();
            engine.sample_trigger(pad).unwrap();
        }
        let wait = |predicate: &dyn Fn() -> bool| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while !predicate() {
                assert!(std::time::Instant::now() < until);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        wait(&|| {
            engine
                .controls
                .sample_playing
                .iter()
                .all(|p| p.load(std::sync::atomic::Ordering::Relaxed))
        });
        engine.sample_stop(Some(8)).unwrap();
        wait(&|| !engine.controls.sample_playing[8].load(std::sync::atomic::Ordering::Relaxed));
        assert!(engine.controls.sample_playing[0].load(std::sync::atomic::Ordering::Relaxed));
        engine.sample_stop(None).unwrap();
        wait(&|| {
            engine
                .controls
                .sample_playing
                .iter()
                .all(|p| !p.load(std::sync::atomic::Ordering::Relaxed))
        });
        assert!(engine.take_error().is_none());
        engine.sample_maintenance();
    }
    #[test]
    #[ignore = "requires ffmpeg and AMP_WAVEFORM_FIXTURE"]
    fn actual_effect_decoder_produces_device_rate_stereo() {
        let path = std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap();
        let frames = decode(Path::new(&path), 48000).unwrap();
        assert!((479990..=480010).contains(&frames.len()));
        assert!(frames[48000..].iter().any(|f| f[0].abs() > 0.01));
        assert!(decode(Path::new("missing-effect.wav"), 48000).is_err());
    }
}
