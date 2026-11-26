//! The operating system handles Bluetooth pairing; AMP selects its normal audio endpoint.
use eframe::egui;
use std::sync::mpsc;
pub struct Panel {
    pub selected: Option<String>,
    devices: Vec<String>,
    job: Option<mpsc::Receiver<Vec<String>>>,
    message: String,
}
fn file() -> Option<std::path::PathBuf> {
    crate::theme::settings_path().map(|p| p.with_file_name("media.ampdb"))
}
impl Default for Panel {
    fn default() -> Self {
        let selected = if cfg!(test) {
            None
        } else {
            file()
                .and_then(|f| {
                    amp_core::catalog_db::local_record(&f, "audio-output")
                        .ok()
                        .flatten()
                })
                .and_then(|v| v.as_str().map(str::to_owned))
        };
        Self {
            selected,
            devices: vec![],
            job: None,
            message: String::new(),
        }
    }
}
impl Panel {
    pub fn show(&mut self, ui: &mut egui::Ui, blocked: bool) -> bool {
        if let Some(rx) = &self.job
            && let Ok(devices) = rx.try_recv()
        {
            self.devices = devices;
            self.job = None;
        }
        ui.heading("Audio output / Bluetooth");
        ui.weak("Pair Bluetooth speakers or headphones in Windows/Linux first, then refresh. Stop playback before changing the master output; prepared decks will reload.");
        if ui
            .add_enabled(
                self.job.is_none(),
                egui::Button::new("Refresh audio outputs"),
            )
            .clicked()
        {
            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(crate::mixer::Engine::output_names());
            });
        }
        let mut desired = self.selected.clone();
        ui.add_enabled_ui(!blocked, |ui| {
            ui.selectable_value(&mut desired, None, "System default output");
            for name in &self.devices {
                ui.selectable_value(&mut desired, Some(name.clone()), name);
            }
        });
        let mut changed = false;
        if desired != self.selected {
            match file()
                .ok_or("Settings folder unavailable".to_owned())
                .and_then(|f| {
                    amp_core::catalog_db::save_local_record(
                        &f,
                        "audio-output",
                        &serde_json::json!(desired),
                    )
                }) {
                Ok(()) => {
                    self.selected = desired;
                    self.message = "Output saved".into();
                    changed = true;
                }
                Err(e) => self.message = e,
            }
        }
        ui.label(format!(
            "Selected: {}",
            self.selected.as_deref().unwrap_or("System default")
        ));
        ui.label(&self.message);
        if blocked {
            ui.weak("Stop Player, DJ and sample playback to change outputs.");
        }
        if self.job.is_some() {
            ui.spinner();
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        changed
    }
}
