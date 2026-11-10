//! Compact live mixer controls. Values affect the playback snapshot only.
use super::*;
use std::sync::atomic::Ordering::Relaxed;
pub struct Panel {
    gain: [f32; 2],
    kill: [[bool; 3]; 2],
    sweep: [f32; 2],
    cue: [bool; 2],
    cue_mix: f32,
    cue_volume: f32,
    output: Option<String>,
    devices: Vec<String>,
    scanned: bool,
    status: String,
}
impl Default for Panel {
    fn default() -> Self {
        Self {
            gain: [0.; 2],
            kill: [[false; 3]; 2],
            sweep: [0.; 2],
            cue: [false; 2],
            cue_mix: 0.,
            cue_volume: 0.5,
            output: None,
            devices: Vec::new(),
            scanned: false,
            status: String::new(),
        }
    }
}
impl Panel {
    pub fn apply(&self, engine: &mixer::Engine) {
        let controls = &engine.controls;
        for d in 0..2 {
            controls.trim_db[d].store(self.gain[d].to_bits(), Relaxed);
            controls.sweep[d].store(self.sweep[d].to_bits(), Relaxed);
            controls.cue[d].store(self.cue[d], Relaxed);
            for b in 0..3 {
                controls.eq_kill[d][b].store(self.kill[d][b], Relaxed);
            }
        }
        controls.cue_blend.store(self.cue_mix.to_bits(), Relaxed);
        controls
            .cue_volume
            .store(self.cue_volume.to_bits(), Relaxed);
    }
}
pub(super) fn dial(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    accent: egui::Color32,
) -> egui::Response {
    let dial_size = ui.spacing().interact_size.y.clamp(24., 44.);
    let (rect, mut response) = ui.allocate_exact_size(
        egui::vec2(dial_size + 4., dial_size),
        egui::Sense::click_and_drag(),
    );
    if response.clicked() {
        response.request_focus();
    }
    let old = *value;
    let extent = range.end() - range.start();
    if response.dragged() {
        *value -= ui.input(|i| i.pointer.delta().y) * extent / 160.;
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0. {
            *value += scroll * extent / 1000.;
            ui.input_mut(|i| i.smooth_scroll_delta.y = 0.);
        }
    }
    if response.has_focus() {
        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::ArrowRight)) {
            *value += extent / 100.;
        }
        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowLeft))
        {
            *value -= extent / 100.;
        }
    }
    if response.double_clicked()
        || (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Home)))
    {
        *value = 0.;
    }
    *value = value.clamp(*range.start(), *range.end());
    if old != *value {
        response.mark_changed();
    }
    let center = rect.center();
    let radius = dial_size * 0.42;
    ui.painter()
        .circle_filled(center, radius, ui.visuals().widgets.inactive.bg_fill);
    ui.painter().circle_stroke(
        center,
        radius,
        egui::Stroke::new(
            1.5,
            if response.has_focus() {
                accent
            } else {
                ui.visuals().widgets.inactive.bg_stroke.color
            },
        ),
    );
    let fraction = if *value < 0. {
        0.5 * (*value - range.start()) / -range.start()
    } else {
        0.5 + 0.5 * *value / range.end()
    };
    let angle = std::f32::consts::PI * (0.75 + 1.5 * fraction);
    ui.painter().line_segment(
        [
            center,
            center + egui::vec2(angle.cos(), angle.sin()) * (radius - 3.),
        ],
        egui::Stroke::new(2., accent),
    );
    response.on_hover_text(format!("{label}: {value:.1}\nDrag up/down or scroll; double-click resets. Arrow keys adjust; Home resets."))
}
impl Player {
    pub(super) fn inline_mixer(&mut self, ui: &mut egui::Ui, height: f32) {
        ui.spacing_mut().item_spacing.y = 1.;
        ui.strong("MIXER");
        ui.columns(2, |columns| {
            for (d, column) in columns.iter_mut().enumerate() {
                let accent = if d == 0 { ORANGE } else { CYAN };
                column.push_id(("mixer-channel", d), |ui| {
                    ui.colored_label(accent, if d == 0 { "A" } else { "B" });
                    ui.horizontal(|ui| { dial(ui, "Gain dB", &mut self.mixer_panel.gain[d], -12. ..=12., accent); ui.label("Gain"); });
                    for (band, label) in [(2,"High"),(1,"Mid"),(0,"Low")] {
                        ui.horizontal(|ui| {
                            if dial(ui, label, &mut self.eq[d][band], -24. ..=12., accent).changed()
                                && let Some(audio) = &self.audio { audio.set_eq(d, band, self.eq[d][band]); }
                            ui.label(label);
                            ui.toggle_value(&mut self.mixer_panel.kill[d][band], "X").on_hover_text(format!("Kill {label}; restores the dial setting when released"));
                        });
                    }
                    ui.horizontal(|ui| { dial(ui, "Filter (left: low-pass; right: high-pass)", &mut self.mixer_panel.sweep[d], -1. ..=1., accent); ui.label("Filter"); });
                    ui.horizontal(|ui| {
                        let fader_height = ((height - 500.) * 0.4).clamp(70., 180.);
                        ui.spacing_mut().slider_width = fader_height;
                        if ui.add(egui::Slider::new(&mut self.channel_gain[d], 0. ..=1.).vertical().show_value(false)).changed()
                            && let Some(audio) = &self.audio { audio.set_gain(d, self.channel_gain[d]); }
                        skin::meter(ui, self.audio.as_ref().map_or(0., |a| a.peak(d)), egui::vec2(18., fader_height));
                    });
                    ui.toggle_value(&mut self.mixer_panel.cue[d], "CUE").on_hover_text("Listen before the channel fader and crossfader. Choose a headphone output below. The deck must be playing; this does not start or stop it.");
                });
            }
        });
        ui.separator();
        egui::CollapsingHeader::new("Headphones / output").id_salt("dj-monitor-output").show(ui, |ui| {
            if !self.mixer_panel.scanned || ui.button("Refresh outputs").clicked() {
                self.mixer_panel.devices = mixer::Engine::output_names();
                self.mixer_panel.scanned = true;
            }
            let previous = self.mixer_panel.output.clone();
            egui::ComboBox::from_id_salt("dj-headphone-device").width(230.).selected_text(previous.as_deref().unwrap_or("Headphone output: off")).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.mixer_panel.output, None, "Off");
                for device in &self.mixer_panel.devices { ui.selectable_value(&mut self.mixer_panel.output, Some(device.clone()), device); }
            });
            if previous != self.mixer_panel.output {
                let result = self.ensure_audio().and_then(|()| self.audio.as_mut().unwrap().configure_monitor(self.mixer_panel.output.as_deref()));
                if let Err(error) = result { self.mixer_panel.output = previous; self.mixer_panel.status = error; }
                else { self.mixer_panel.status = "Live headphone output connected. Use a different device from the speakers for private monitoring.".into(); }
            }
            ui.add(egui::Slider::new(&mut self.mixer_panel.cue_mix, 0. ..=1.).text("Cue / Master"));
            ui.add(egui::Slider::new(&mut self.mixer_panel.cue_volume, 0. ..=1.).text("Phones"));
            ui.label(&self.mixer_panel.status);
            ui.horizontal(|ui| {
                if ui.button("Expanded EQ").clicked() { self.eq_open = true; }
                if ui.button("Filter FX").clicked() { self.fx_open = true; }
            });
        });
        if let Some(audio) = &self.audio {
            self.mixer_panel.apply(audio);
            let error = audio
                .controls
                .monitor_error
                .lock()
                .ok()
                .and_then(|mut e| e.take());
            if let Some(error) = error {
                self.mixer_panel.status = error;
            }
        }
    }
}
