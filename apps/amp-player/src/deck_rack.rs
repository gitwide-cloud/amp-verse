//! Stable six-deck identities: interleaved by side, never selected by track index.
use super::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Rack {
    pub live: [usize; 2],
    pub expanded: bool,
    pub six: bool,
    #[serde(default)]
    pub overlay_waveforms: bool,
    #[serde(default = "strip_ratios")]
    pub strip_ratios: [f32; 2],
}
impl Default for Rack {
    fn default() -> Self {
        Self {
            live: [0, 1],
            expanded: false,
            six: true,
            strip_ratios: strip_ratios(),
            overlay_waveforms: !cfg!(test)
                && waveform_preference()
                    .is_some_and(|p| std::fs::read_to_string(p).is_ok_and(|v| v == "overlay")),
        }
    }
}
fn strip_ratios() -> [f32; 2] {
    if !cfg!(test)
        && let Some(path) = strip_preference()
        && let Ok(bytes) = std::fs::read(path)
        && let Ok(values) = serde_json::from_slice::<[f32; 2]>(&bytes)
        && values
            .iter()
            .all(|v| v.is_finite() && (0.02..=0.45).contains(v))
    {
        return values;
    }
    [0.14; 2]
}
fn strip_preference() -> Option<PathBuf> {
    Some(
        theme::settings_path()?
            .parent()?
            .join("dj-deck-widths.json"),
    )
}
fn waveform_preference() -> Option<PathBuf> {
    Some(
        theme::settings_path()?
            .parent()?
            .join("dj-waveform-view.txt"),
    )
}
pub const NAMES: [&str; 6] = ["A1", "B1", "A2", "B2", "A3", "B3"];
impl Player {
    pub(super) fn deck_rate(&self, d: usize) -> f64 {
        self.audio
            .as_ref()
            .map_or(1. + self.pitch[d] as f64 / 100., |a| {
                a.controls.effective_rate(d) as f64
            })
    }
    pub(super) fn pair(&self) -> [usize; 2] {
        self.audio
            .as_ref()
            .map_or([0, 1], |a| a.controls.pair_indices())
    }
    pub(super) fn other_deck(&self, d: usize) -> usize {
        self.pair()[1 - d % 2]
    }
}
use std::sync::atomic::Ordering::Relaxed;
fn accent(deck: usize) -> egui::Color32 {
    if deck.is_multiple_of(2) { ORANGE } else { CYAN }
}
fn transport_button(
    ui: &mut egui::Ui,
    deck: usize,
    playing: bool,
    size: egui::Vec2,
) -> egui::Response {
    let color = accent(deck);
    let response = ui.add(
        egui::Button::new("")
            .min_size(size)
            .corner_radius(7)
            .fill(color)
            .stroke(egui::Stroke::new(1., color)),
    );
    icons::draw(
        ui,
        egui::Rect::from_center_size(response.rect.center(), egui::vec2(20., 20.)),
        if playing {
            icons::Icon::Pause
        } else {
            icons::Icon::Play
        },
        egui::Color32::BLACK,
    );
    #[cfg(test)]
    if size.y >= 38. {
        let transform = ui
            .ctx()
            .layer_transform_to_global(ui.layer_id())
            .unwrap_or_default();
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::unique(("rack-main-play", deck)),
                transform * response.rect,
            )
        });
    }
    response
}
impl Player {
    pub(super) fn six_deck_ui(&mut self, ui: &mut egui::Ui) {
        let compact = ui.available_width() < 1280. || ui.available_height() < 900.;
        ui.spacing_mut().item_spacing.y = if compact { 3. } else { 5. };
        let scope = self
            .audio
            .as_ref()
            .map(|a| {
                (0..a.controls.scope[0].len())
                    .map(|i| {
                        (0..6)
                            .map(|d| f32::from_bits(a.controls.scope[d][i].load(Relaxed)))
                            .sum::<f32>()
                            .clamp(0., 1.)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::unique("six-deck-header-scope"), scope));
        self.header(ui, "AMP DJ", true);
        ui.horizontal_wrapped(|ui| {
            self.performance_controls(ui);
            ui.menu_button("Waveforms", |ui| {
                let old = self.rack.overlay_waveforms;
                ui.selectable_value(&mut self.rack.overlay_waveforms, false, "Stacked A / B");
                ui.selectable_value(
                    &mut self.rack.overlay_waveforms,
                    true,
                    "Overlay · blue / red",
                );
                if old != self.rack.overlay_waveforms
                    && let Some(path) = waveform_preference()
                {
                    let result = std::fs::create_dir_all(path.parent().unwrap()).and_then(|()| {
                        std::fs::write(
                            path,
                            if self.rack.overlay_waveforms {
                                "overlay"
                            } else {
                                "stacked"
                            },
                        )
                    });
                    if let Err(error) = result {
                        self.status = format!("Could not remember waveform view: {error}");
                    }
                }
            });

            if self.auto_dj_controls || self.run.auto {
                if compact {
                    let bpm = self.audio.as_ref().map_or(120., |a| {
                        f32::from_bits(a.controls.master_bpm.load(Relaxed))
                    });
                    ui.menu_button(format!("{bpm:.1} BPM"), |ui| self.clock_controls(ui))
                        .response
                        .on_hover_text("Master tempo, quantization and Sync All");
                } else {
                    self.clock_controls(ui);
                }
                let previous = self.run.transitions;
                if transition_controls::show(
                    ui,
                    &mut self.run.transitions,
                    &mut self.wave_span,
                    self.run.pending.is_none() && !self.audio.as_ref().is_some_and(|a| a.fading()),
                ) {
                    self.run.force_mashup = true;
                    if let Err(error) = self.execute_control(control::Action::AutoFade) {
                        self.run.force_mashup = false;
                        self.status = error;
                    }
                }
                if previous != self.run.transitions {
                    let _ = transition_styles::save(self.run.transitions);
                }
                self.run.scratch_assist = self.run.transitions.enabled;
            }
            if compact {
                ui.menu_button("View", |ui| self.rack_view_options(ui, true));
            } else {
                if ui
                    .selectable_label(self.rack.expanded, "Expanded")
                    .clicked()
                {
                    self.rack.expanded = !self.rack.expanded;
                }
                ui.menu_button("Setup", |ui| self.rack_view_options(ui, false));
            }
        });
        ui.separator();
        for side in 0..2 {
            let d = self.rack.live[side];
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(NAMES[d])
                        .size(17.)
                        .strong()
                        .color(accent(d)),
                );
                let title_width = (ui.available_width() - 190.).max(80.);
                ui.allocate_ui_with_layout(
                    egui::vec2(title_width, 20.),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(title_width);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(self.title(d))
                                    .size(15.)
                                    .strong()
                                    .color(accent(d)),
                            )
                            .truncate(),
                        );
                    },
                );
                if let Some(track) = self.loaded_track(d) {
                    ui.colored_label(
                        accent(d),
                        format!(
                            "{}  {}",
                            track
                                .preparation
                                .bpm
                                .map_or("--".into(), |b| format!("{:.1}", b * self.deck_rate(d))),
                            track.preparation.harmonic_key.as_deref().unwrap_or("--")
                        ),
                    );
                }
                if let Some(a) = &self.audio {
                    ui.label(format!("−{:.0}s", a.remaining(d).unwrap_or(0.)));
                }
            });
            if self.rack.overlay_waveforms {
                continue;
            }
            self.waveforms[d].overlay = false;
            self.waveforms[d].ignore_seek = false;
            self.waveforms[d].tint = None;
            let t = self.loaded_snapshot[d]
                .as_ref()
                .or_else(|| self.loaded[d].and_then(|i| self.library.as_ref()?.tracks.get(i)));
            self.waveforms[d].hide_caption = true;
            self.waveforms[d].overview = false;
            self.waveforms[d].height = Some(if compact {
                24.
            } else if self.rack.expanded {
                38.
            } else {
                30.
            });
            self.waveforms[d].show(
                ui,
                self.loaded_origin[d].as_deref().unwrap_or(&self.root),
                t,
                self.audio.as_ref().map_or(0., |a| a.source_position(d)),
                self.wave_span,
                d,
            );
            if let Some(at) = self.waveforms[d].seek.take() {
                let position = self.audio.as_ref().map_or(0., |a| a.source_position(d));
                self.seek_deck_relative(d, at - position);
            }
        }
        if self.rack.overlay_waveforms {
            let height = if compact {
                48.
            } else if self.rack.expanded {
                76.
            } else {
                60.
            };
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), height),
                egui::Sense::click(),
            );
            let response = response.on_hover_text(
                "Click to seek live B; Shift-click to seek live A. Jog wheels remain independent.",
            );
            if response.clicked()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let d = self.rack.live[if ui.input(|i| i.modifiers.shift) {
                    0
                } else {
                    1
                }];
                let delta = ((pointer.x - rect.left()) / rect.width()) as f64 * self.wave_span
                    - self.wave_span * 0.3;
                self.seek_deck_relative(d, delta);
            }
            for side in 0..2 {
                let d = self.rack.live[side];
                let mut lane = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt(("overlay-waveform", d))
                        .max_rect(rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                let track = self.loaded_snapshot[d]
                    .as_ref()
                    .or_else(|| self.loaded[d].and_then(|i| self.library.as_ref()?.tracks.get(i)));
                self.waveforms[d].overlay = side == 1;
                self.waveforms[d].ignore_seek = true;
                self.waveforms[d].tint = Some(if side == 0 {
                    egui::Color32::from_rgba_unmultiplied(60, 170, 255, 200)
                } else {
                    egui::Color32::from_rgba_unmultiplied(255, 75, 80, 170)
                });
                self.waveforms[d].overview = false;
                self.waveforms[d].hide_caption = true;
                self.waveforms[d].height = Some(height);
                self.waveforms[d].show(
                    &mut lane,
                    self.loaded_origin[d].as_deref().unwrap_or(&self.root),
                    track,
                    self.audio.as_ref().map_or(0., |a| a.source_position(d)),
                    self.wave_span,
                    d,
                );
                if let Some(at) = self.waveforms[d].seek.take() {
                    let position = self.audio.as_ref().map_or(0., |a| a.source_position(d));
                    self.seek_deck_relative(d, at - position);
                }
            }
        }
        ui.add_space(6.);
        let library_id = egui::Id::unique("dj-library-height");
        let minimum_library = if compact { 112. } else { 160. };
        let max_library = (ui.available_height() - 370.).max(minimum_library);
        let mut library_height = ui
            .ctx()
            .data_mut(|d| d.get_persisted::<f32>(library_id))
            .unwrap_or(if compact { 145. } else { 230. })
            .clamp(minimum_library, max_library);
        let height = (ui.available_height()
            - if self.dj_library_hidden {
                40.
            } else {
                library_height + 52.
            })
        .max(if compact { 290. } else { 320. })
        .min(if self.rack.expanded { 550. } else { 490. });
        let rack_width = ui.available_width();
        let mixer = if compact { 126. } else { 146. };
        let half = (rack_width - mixer - 20. - ui.spacing().item_spacing.x * 6.) / 2.;
        let minimum = if compact { 112. } else { 140. };
        let min_jog = 280f32.min((half - minimum).max(240.));
        let maximum = (half - min_jog).clamp(minimum, 400.);
        let mut strips = self
            .rack
            .strip_ratios
            .map(|ratio| (rack_width * ratio).clamp(minimum, maximum));
        let saved_id = egui::Id::unique("dj-deck-widths-saved");
        let saved = ui
            .ctx()
            .data_mut(|d| *d.get_temp_mut_or_insert_with(saved_id, || self.rack.strip_ratios));
        let rendered = strips;
        ui.horizontal_top(|ui| {
            self.deck_strips(ui, 0, rendered[0], height, compact);
            window_layout::splitter(
                ui,
                "dj-deck-left-divider",
                &mut strips[0],
                minimum..=maximum,
                true,
                1.,
                height,
            );
            ui.push_id(("live-jog", self.rack.live[0]), |ui| {
                self.jog_section(ui, self.rack.live[0], half - rendered[0], height)
            });
            self.rack_mixer(ui, mixer, height);
            ui.push_id(("live-jog", self.rack.live[1]), |ui| {
                self.jog_section(ui, self.rack.live[1], half - rendered[1], height)
            });
            window_layout::splitter(
                ui,
                "dj-deck-right-divider",
                &mut strips[1],
                minimum..=maximum,
                true,
                -1.,
                height,
            );
            self.deck_strips(ui, 1, rendered[1], height, compact);
        });
        // Only deliberate drags change the stored ratios; ordinary window resizing must not clamp them permanently.
        for (side, width) in strips.into_iter().enumerate() {
            let old = (rack_width * self.rack.strip_ratios[side]).clamp(minimum, maximum);
            if (old - width).abs() > 0.01 {
                self.rack.strip_ratios[side] = width / rack_width;
            }
        }
        if saved != self.rack.strip_ratios
            && ui.input(|i| i.pointer.any_released())
            && let Some(path) = strip_preference()
        {
            let result = if cfg!(test) {
                Ok(())
            } else {
                amp_core::coordination::atomic_write(&path, &self.rack.strip_ratios)
            };
            match result {
                Ok(()) => {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(saved_id, self.rack.strip_ratios);
                    });
                }
                Err(error) => self.status = format!("Could not remember deck widths: {error}"),
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(if self.dj_library_hidden {
                    "Show Library"
                } else {
                    "Hide Library"
                })
                .clicked()
            {
                self.dj_library_hidden = !self.dj_library_hidden;
            }
            ui.spacing_mut().slider_width = 100.;
            ui.add(egui::Slider::new(&mut self.volume, 0..=100).text("Master"));
            let pair = self.pair();
            ui.label(format!(
                "3+3 decks · LIVE {} / {} · Crossfade {} ↔ {}",
                NAMES[self.rack.live[0]], NAMES[self.rack.live[1]], NAMES[pair[0]], NAMES[pair[1]]
            ));
            ui.add(egui::Label::new(&self.status).truncate());
        });
        #[cfg(test)]
        ui.ctx().data_mut(|d| {
            d.insert_temp(egui::Id::unique("six-deck-content-bounds"), ui.min_rect())
        });
        if !self.dj_library_hidden {
            window_layout::splitter(
                ui,
                "dj-library-divider",
                &mut library_height,
                minimum_library..=max_library,
                false,
                -1.,
                12.,
            );
            ui.ctx()
                .data_mut(|d| d.insert_persisted(library_id, library_height));
            #[cfg(test)]
            {
                let transform = ui
                    .ctx()
                    .layer_transform_to_global(ui.layer_id())
                    .unwrap_or_default();
                let top = transform * ui.next_widget_position();
                ui.ctx()
                    .data_mut(|data| data.insert_temp(egui::Id::unique("rack-library-top"), top.y));
            }
            self.dj_library_ui(ui);
        }
    }
    fn rack_view_options(&mut self, ui: &mut egui::Ui, include_expand: bool) {
        if !self.auto_dj_controls && !self.run.auto {
            ui.menu_button("Manual tempo / Sync All", |ui| self.clock_controls(ui));
        }

        if include_expand {
            ui.toggle_value(&mut self.rack.expanded, "Expanded controls");
        }
        if ui.button("Use original two-deck layout").clicked() {
            self.rack.six = false;
            ui.close();
        }
        if ui.button("Assign crossfader to LIVE pair").on_hover_text("Pause both the current and requested pair first; stop Auto DJ. Background layers keep playing.").clicked() {
            self.status=if self.run.auto {"Stop Auto DJ before assigning a different pair".into()}
            else if let Err(e)=self.ensure_audio().and_then(|_|self.audio.as_ref().unwrap().assign_pair(self.rack.live)) {e}
            else {"Crossfade assignment queued; applies only if both pairs remain paused".into()};
        }
    }
    fn clock_controls(&mut self, ui: &mut egui::Ui) {
        let mut bpm = self.audio.as_ref().map_or(120., |a| {
            f32::from_bits(a.controls.master_bpm.load(Relaxed))
        });
        let mut q = self
            .audio
            .as_ref()
            .map_or(4, |a| a.controls.quantization.load(Relaxed));
        ui.label("BPM");
        let mut changed = ui
            .add(egui::DragValue::new(&mut bpm).range(40. ..=240.).speed(0.1))
            .changed();
        egui::ComboBox::from_id_salt("rack-quantization")
            .width(86.)
            .selected_text(match q {
                0 => "Q Off",
                1 => "Q Beat",
                4 => "Q 1 Bar",
                8 => "Q 2 Bars",
                16 => "Q 4 Bars",
                _ => "Q 8 Bars",
            })
            .show_ui(ui, |ui| {
                for (beats, label) in [
                    (0, "Off"),
                    (1, "Beat"),
                    (4, "1 Bar"),
                    (8, "2 Bars"),
                    (16, "4 Bars"),
                    (32, "8 Bars"),
                ] {
                    changed |= ui.selectable_value(&mut q, beats, label).changed();
                }
            });
        if changed
            && let Err(error) = self
                .ensure_audio()
                .and_then(|_| self.audio.as_ref().unwrap().set_clock(bpm, q))
        {
            self.status = error;
        }
        if ui.button("Sync All").clicked()
            && let Some(audio) = &self.audio
        {
            for deck in 0..6 {
                if audio.loaded(deck)
                    && let Err(error) = audio.sync(deck, true)
                {
                    self.status = error;
                }
            }
        }
    }
    fn deck_strips(
        &mut self,
        ui: &mut egui::Ui,
        side: usize,
        width: f32,
        height: f32,
        compact: bool,
    ) {
        ui.allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
            for d in (side..6).step_by(2) {
                ui.push_id(("strip", d), |ui| {
                    let selected = self.rack.live[side] == d;
                    let frame = skin::rack_panel(ui, selected.then_some(accent(d)), |ui| {
                        ui.set_width(width - 18.);
                        ui.set_min_height((height - 24.) / 3. - 18.);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(NAMES[d]).strong().color(accent(d)));
                            if ui.add(egui::Button::new(egui::RichText::new("LIVE").size(11.).color(if selected {egui::Color32::BLACK} else {accent(d)}))
                                .fill(if selected {accent(d)} else {ui.visuals().widgets.inactive.bg_fill}))
                                .on_hover_text("Control this deck with the jog wheel; keep its audio routing unchanged.").clicked() { self.rack.live[side] = d; }
                            if transport_button(ui, d, self.playing[d], egui::vec2(24.,24.)).on_hover_text(if self.playing[d] {"Pause deck"} else {"Play deck"}).clicked() { self.toggle_deck(d); }
                        });
                        ui.add(egui::Label::new(self.title(d)).truncate());
                        let track=self.loaded_snapshot[d].as_ref().or_else(||self.loaded[d].and_then(|i|self.library.as_ref()?.tracks.get(i)));
                        self.waveforms[d].thumbnail(ui,self.loaded_origin[d].as_deref().unwrap_or(&self.root),track,self.audio.as_ref().map_or(0.,|a|a.source_position(d)),accent(d), ((height - 16.) / 3. - if compact { 68. } else { 130. }).clamp(26., 140.));
                        if !compact {
                        ui.horizontal(|ui| {
                            if ui.small_button("LOOP").on_hover_text("Toggle a four-beat loop on this deck").clicked() { self.rack_loop(d); }
                            ui.menu_button("More", |ui| self.strip_options(ui,d));
                        });
                        ui.spacing_mut().slider_width = width - 28.;
                        if ui.add(egui::Slider::new(&mut self.channel_gain[d],0. ..=1.).show_value(false)).on_hover_text("This deck's volume").changed()
                            && let Some(a)=&self.audio { a.set_gain(d,self.channel_gain[d]); }
                        }
                    });
                    if let Some(path)=dragdrop::target(&frame.response,accent(d)) && let Err(e)=self.safe_load_path(d,path,None) {self.status=e;}
                });
            }
        });
    }
    fn strip_options(&mut self, ui: &mut egui::Ui, d: usize) {
        ui.label(format!("{} controls", NAMES[d]));
        if ui
            .add(egui::Slider::new(&mut self.channel_gain[d], 0. ..=1.).text("Volume"))
            .changed()
            && let Some(audio) = &self.audio
        {
            audio.set_gain(d, self.channel_gain[d]);
        }
        if ui.button("Toggle four-beat loop").clicked() {
            self.rack_loop(d);
        }
        if ui.button("Sync to master").clicked() {
            self.sync_deck(d);
        }
        if ui.button("Fade out this deck").clicked()
            && let Some(audio) = &self.audio
        {
            let _ = audio.fade_deck(d, 0., self.fade_seconds, true);
        }
        if ui.button("Load selected Library song").clicked() {
            if let Some(i) = self.selected {
                self.load_deck(d, i, false);
            }
            ui.close();
        }
        if ui.button("Stop this deck").clicked() {
            self.stop(d);
            ui.close();
        }
        ui.label("Stem preset");
        for (name, gains) in [
            ("Full", [1., 1., 1.]),
            ("Vocals", [1., 0., 0.]),
            ("Beats", [0., 0., 1.]),
            ("Instrumental", [0., 1., 0.]),
        ] {
            if ui.button(name).clicked()
                && let Some(a) = &self.audio
            {
                if a.controls.stems_loaded[d].load(Relaxed) {
                    if a.controls.pair_indices().contains(&d) {
                        self.run.cancel_effects(a);
                    }
                    for (i, gain) in gains.into_iter().enumerate() {
                        a.controls.stem_level[d][i].store(f32::to_bits(gain), Relaxed);
                        a.controls.stem_knobs[d][i].store(f32::to_bits(gain - 1.), Relaxed);
                    }
                } else {
                    self.status = "Prepare stems before recalling a preset".into();
                }
            }
        }
    }
    fn rack_stem_knobs(&mut self, ui: &mut egui::Ui, d: usize) {
        if let Some(knobs) = self.stem_panel.live_knobs(ui, d, self.audio.as_ref())
            && let Some(audio) = &self.audio
        {
            if audio.controls.pair_indices().contains(&d) {
                self.run.cancel_effects(audio);
            }
            for (i, gain) in amp_core::stem_mix::gains(knobs).into_iter().enumerate() {
                audio.controls.stem_knobs[d][i].store(knobs[i].to_bits(), Relaxed);
                audio.controls.stem_level[d][i].store(gain.to_bits(), Relaxed);
            }
        }
    }
    fn rack_loop(&mut self, d: usize) {
        let seconds = self
            .loaded_track(d)
            .and_then(|t| t.preparation.bpm)
            .map_or(2., |bpm| 240. / bpm) as f32;
        if let Some(a) = &self.audio
            && let Err(e) = a.loop_toggle(d, seconds)
        {
            self.status = e;
        }
    }
    fn rack_platter(&mut self, ui: &mut egui::Ui, d: usize, size: f32) {
        let wheel = skin::vinyl_platter(
            ui,
            size,
            accent(d),
            self.playing[d],
            self.audio.as_ref().map_or(0., |a| a.source_position(d)) as f32,
        );
        #[cfg(test)]
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::unique(("rack-wheel", d)), wheel.rect));
        self.jog_wheel(ui, d, &wheel);
    }
    fn rack_pitch(&mut self, ui: &mut egui::Ui, d: usize, size: f32) {
        ui.allocate_ui_with_layout(
            egui::vec2(34., size),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                ui.small("PITCH");
                ui.spacing_mut().slider_width = (size - 42.).max(70.);
                if ui
                    .add(
                        egui::Slider::new(&mut self.pitch[d], -16. ..=16.)
                            .vertical()
                            .show_value(false),
                    )
                    .changed()
                    && let Some(a) = &self.audio
                {
                    let _ = a.sync(d, false);
                    a.set_rate(d, 1. + self.pitch[d] / 100.);
                }
                ui.small(format!("{:+.1}%", self.pitch[d]));
            },
        );
    }
    fn rack_knob_bank(&mut self, ui: &mut egui::Ui, d: usize, width: f32) {
        let response = ui.allocate_ui_with_layout(
            egui::vec2(width, 1.),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(width);
                ui.spacing_mut().interact_size.y = ((width - 20.) / 2.).clamp(32., 56.);
                ui.columns(2, |columns| {
                    if let Some(knobs) =
                        self.stem_panel
                            .live_knobs_vertical(&mut columns[0], d, self.audio.as_ref())
                        && let Some(audio) = &self.audio
                    {
                        if audio.controls.pair_indices().contains(&d) {
                            self.run.cancel_effects(audio);
                        }
                        for (i, gain) in amp_core::stem_mix::gains(knobs).into_iter().enumerate() {
                            audio.controls.stem_knobs[d][i].store(knobs[i].to_bits(), Relaxed);
                            audio.controls.stem_level[d][i].store(gain.to_bits(), Relaxed);
                        }
                    }
                    for (band, name) in [(0, "LOW"), (1, "MID"), (2, "HIGH")] {
                        columns[1].vertical_centered(|ui| {
                            if dj_mixer::dial(
                                ui,
                                name,
                                &mut self.eq[d][band],
                                -24. ..=12.,
                                accent(d),
                            )
                            .changed()
                                && let Some(a) = &self.audio
                            {
                                a.set_eq(d, band, self.eq[d][band]);
                            }
                            ui.small(name);
                        });
                    }
                });
            },
        );
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(egui::Id::unique(("rack-knobs", d)), response.response.rect)
        });
        #[cfg(not(test))]
        let _ = response;
    }
    fn jog_section(&mut self, ui: &mut egui::Ui, d: usize, width: f32, height: f32) {
        let expanded_cues = (self.rack.expanded || self.dj_cues[d]) && height >= 440.;
        let full_bank = self.rack.expanded && width >= 380.;
        ui.allocate_ui_with_layout(
            egui::vec2(width, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                skin::rack_panel(ui, None, |ui| {
                    ui.set_width(width - 18.);
                    let room = width - 18.;
                    let bank_width = if width > 335. {
                        (room * 0.31).clamp(130., 190.)
                    } else {
                        0.
                    };
                    let wheel_size =
                        (room - bank_width - 42. - if bank_width > 0. { 24. } else { 8. })
                            .min((height - if expanded_cues { 290. } else { 210. }).max(110.))
                            .clamp(110., 340.);
                    ui.horizontal_top(|ui| {
                        if d.is_multiple_of(2) {
                            if bank_width > 0. {
                                self.rack_knob_bank(ui, d, bank_width);
                            }
                            self.rack_pitch(ui, d, wheel_size);
                            ui.add_space((ui.available_width() - wheel_size - 8.).max(0.));
                            self.rack_platter(ui, d, wheel_size);
                        } else {
                            self.rack_platter(ui, d, wheel_size);
                            ui.add_space((ui.available_width() - bank_width - 42. - 24.).max(0.));
                            self.rack_pitch(ui, d, wheel_size);
                            if bank_width > 0. {
                                self.rack_knob_bank(ui, d, bank_width);
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        if skin::deck_button(
                            ui,
                            "CUE",
                            accent(d),
                            if width <= 335. { 50. } else { 60. },
                            false,
                        )
                        .clicked()
                        {
                            self.recue_deck(d);
                        }
                        if transport_button(ui, d, self.playing[d], egui::vec2(60., 38.)).clicked()
                        {
                            self.toggle_deck(d);
                        }
                        if skin::deck_button(
                            ui,
                            "SYNC",
                            accent(d),
                            if width <= 335. { 50. } else { 60. },
                            false,
                        )
                        .clicked()
                        {
                            self.sync_deck(d);
                        }
                        if skin::deck_button(
                            ui,
                            "LOOP",
                            accent(d),
                            if width <= 335. { 50. } else { 60. },
                            false,
                        )
                        .clicked()
                        {
                            self.rack_loop(d);
                        }
                        ui.menu_button(if width <= 335. { "..." } else { "Controls" }, |ui| {
                            self.rack_deck_controls(ui, d, width)
                        })
                        .response
                        .on_hover_text(
                            "Deck volume, filter, EQ, stems, headphone cue and Key Lock",
                        );
                    });
                });
                ui.horizontal(|ui| {
                    ui.small(if expanded_cues {
                        "Hot Cues (1–8)"
                    } else {
                        "Hot Cues (1–4)"
                    });
                    if height < 440. {
                        let button = ui.button("8 cues");
                        egui::Popup::menu(&button).show(|ui| {
                            ui.set_width(310.);
                            self.main_hot_cues_count(ui, d, accent(d), 8);
                        });
                    } else if !self.rack.expanded {
                        ui.toggle_value(&mut self.dj_cues[d], "8 cues");
                    }
                });
                self.main_hot_cues_count(ui, d, accent(d), if expanded_cues { 8 } else { 4 });
                ui.horizontal(|ui| {
                    ui.small(if full_bank {
                        "Sampler · full bank"
                    } else {
                        "Sampler · favorites"
                    });
                    ui.menu_button("Bank / Cue Deck", |ui| {
                        self.side_panel(ui, d % 2, width - 30., 300., true);
                    });
                });
                let actions = if d.is_multiple_of(2) {
                    self.sampler.inline_pads(ui, self.audio.as_ref(), full_bank)
                } else {
                    self.right_sampler
                        .inline_pads(ui, self.right_sample_audio.as_ref(), full_bank)
                };
                for action in actions {
                    self.side_sample_action(d % 2, action);
                }
            },
        );
    }
    fn rack_deck_controls(&mut self, ui: &mut egui::Ui, d: usize, width: f32) {
        if ui.button("Fade out this deck").clicked()
            && let Err(error) = self.execute_control(control::Action::Fade {
                deck: control::Deck::PHYSICAL[d],
                master: false,
                target: 0.,
                seconds: self.fade_seconds,
                stop_after: true,
            })
        {
            self.status = error;
        }
        if ui
            .add(egui::Slider::new(&mut self.channel_gain[d], 0. ..=1.).text("Deck volume"))
            .changed()
            && let Some(audio) = &self.audio
        {
            audio.set_gain(d, self.channel_gain[d]);
        }
        if self.mixer_panel.deck_tools(ui, d, accent(d))
            && let Some(audio) = &self.audio
        {
            if self.pair().contains(&d) {
                self.run.cancel_effects(audio);
            }
            self.mixer_panel.apply(audio);
        }
        if width <= 335. {
            self.rack_stem_knobs(ui, d);
            for (band, label) in [(0, "Low EQ"), (1, "Mid EQ"), (2, "High EQ")] {
                if ui
                    .add(egui::Slider::new(&mut self.eq[d][band], -24. ..=12.).text(label))
                    .changed()
                    && let Some(audio) = &self.audio
                {
                    if self.pair().contains(&d) {
                        self.run.cancel_effects(audio);
                    }
                    audio.set_eq(d, band, self.eq[d][band]);
                }
            }
        }
        if ui.button("Stop deck").clicked() {
            self.stop(d);
        }
        if let Some(a) = &self.audio {
            let mut enabled = a.controls.key_lock[d].load(Relaxed);
            if ui
                .add_enabled(
                    a.loaded(d) && a.controls.key_lock_available[d].load(Relaxed),
                    egui::Checkbox::new(&mut enabled, "Key Lock · preserve pitch"),
                )
                .changed()
            {
                a.controls.key_lock[d].store(enabled, Relaxed);
            }
            ui.small("Pitch stays fixed when tempo changes; scratching follows your hand.");
        }
        if let Some(a) = &self.audio {
            ui.small(format!(
                "Audio dropouts: {}",
                a.controls.underruns[d].load(Relaxed)
            ));
        }
    }
    fn rack_mixer(&mut self, ui: &mut egui::Ui, width: f32, height: f32) {
        let pair = self.pair();
        ui.allocate_ui_with_layout(
            egui::vec2(width, height),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                skin::rack_panel(ui, None, |ui| {
                    ui.set_width(width - 18.);
                    ui.horizontal(|ui| {
                        for d in pair {
                            ui.vertical(|ui| {
                                ui.colored_label(accent(d), NAMES[d]);
                                ui.horizontal(|ui| {
                                    skin::meter(
                                        ui,
                                        self.audio.as_ref().map_or(0., |a| a.peak(d)),
                                        egui::vec2(18., (height - 190.).clamp(90., 250.)),
                                    );
                                    ui.spacing_mut().slider_width =
                                        (height - 190.).clamp(90., 250.);
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut self.channel_gain[d], 0. ..=1.)
                                                .vertical()
                                                .show_value(false),
                                        )
                                        .changed()
                                        && let Some(a) = &self.audio
                                    {
                                        a.set_gain(d, self.channel_gain[d]);
                                    }
                                });
                            });
                        }
                    });
                    if ui
                        .add_sized(
                            [width - 24., 52.],
                            egui::Button::new(
                                egui::RichText::new("Auto Crossfade").color(ORANGE).strong(),
                            )
                            .fill(egui::Color32::from_rgb(65, 48, 12))
                            .stroke(egui::Stroke::new(1., ORANGE)),
                        )
                        .clicked()
                    {
                        self.auto_crossfade();
                    }
                    ui.spacing_mut().slider_width = 90.;
                    if ui
                        .add(egui::Slider::new(&mut self.fader, 0. ..=1.).show_value(false))
                        .changed()
                        && let Some(a) = &self.audio
                    {
                        self.run.cancel_automation();
                        a.set_fader(self.fader);
                    }
                    let previous = self.fade_seconds;
                    ui.horizontal(|ui| {
                        if ui.small_button("−").clicked() {
                            self.fade_seconds -= 0.5;
                        }
                        ui.add(
                            egui::DragValue::new(&mut self.fade_seconds)
                                .range(0.5..=30.)
                                .suffix("s"),
                        );
                        if ui.small_button("+").clicked() {
                            self.fade_seconds += 0.5;
                        }
                    });
                    self.fade_seconds = self.fade_seconds.clamp(0.5, 30.);
                    if previous != self.fade_seconds {
                        let _ = crossfade_settings::save(self.fade_seconds);
                    }
                    ui.menu_button("Headphones", |ui| {
                        self.headphone_output(ui);
                    });
                });
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deck_strip_grabbers_resize_each_side_independently() {
        for (id, delta, side) in [
            ("dj-deck-left-divider", 45., 0usize),
            ("dj-deck-right-divider", -45., 1usize),
        ] {
            let ctx = egui::Context::default();
            let mut player = Player::default();
            let mut point = egui::Pos2::ZERO;
            for frame in 0..5 {
                let mut raw = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1570., 1000.),
                    )),
                    ..Default::default()
                };
                if frame == 3 {
                    point.x += delta;
                }
                raw.events.push(egui::Event::PointerMoved(point));
                if frame == 2 || frame == 4 {
                    raw.events.push(egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 2,
                        modifiers: Default::default(),
                    });
                }
                let mut output = ctx.run_ui(raw, |ui| player.group_ui(ui, 2));
                output.textures_delta.clear();
                if frame == 1 {
                    let (rect, layer) = ctx
                        .data(|d| {
                            d.get_temp::<(egui::Rect, egui::LayerId)>(
                                egui::Id::unique(id).with("bounds"),
                            )
                        })
                        .unwrap();
                    point =
                        ctx.layer_transform_to_global(layer).unwrap_or_default() * rect.center();
                }
            }
            assert!(
                player.rack.strip_ratios[side] > 0.15,
                "Grabber did not widen its deck strip: {:?}",
                player.rack.strip_ratios
            );
            assert_eq!(player.rack.strip_ratios[1 - side], 0.14);
        }
    }
    #[test]
    fn overlay_waveforms_share_the_same_full_width_rectangle() {
        let ctx = egui::Context::default();
        let mut player = Player {
            rack: Rack {
                overlay_waveforms: true,
                ..Default::default()
            },
            ..Default::default()
        };
        for _ in 0..3 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1570., 1000.),
                    )),
                    ..Default::default()
                },
                |ui| player.group_ui(ui, 2),
            );
            output.textures_delta.clear();
        }
        let rect = |d| {
            ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::unique(("waveform-lane", d))))
                .unwrap()
        };
        let a = rect(0usize);
        let b = rect(1usize);
        assert_eq!(a, b);
        assert!(a.width() > 1500.);
        let legacy: Rack =
            serde_json::from_str(r#"{"live":[0,1],"expanded":false,"six":true}"#).unwrap();
        assert!(!legacy.overlay_waveforms);
    }
    #[test]
    fn wide_rack_keeps_wheels_by_mixer_and_knobs_on_outer_sides() {
        for size in [egui::vec2(1600., 1000.), egui::vec2(2560., 1440.)] {
            let ctx = egui::Context::default();
            let mut player = Player::default();
            for _ in 0..3 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| player.group_ui(ui, 2),
                );
                output.textures_delta.clear();
            }
            let rect = |kind: &str, d: usize| {
                ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::unique((kind, d))))
                    .unwrap()
            };
            let wave_a = rect("waveform-lane", 0);
            let wave_b = rect("waveform-lane", 1);
            assert!(wave_a.bottom() <= wave_b.top());
            assert!((wave_a.left() - wave_b.left()).abs() < 1.);
            assert!((wave_a.width() - wave_b.width()).abs() < 1.);
            assert!(wave_a.width() > size.x * 0.9);
            let a = rect("rack-wheel", 0);
            let b = rect("rack-wheel", 1);
            assert!(rect("rack-knobs", 0).right() <= a.left());
            assert!(rect("rack-knobs", 1).left() >= b.right());
            assert!(
                b.left() - a.right() < 250.,
                "Wheels drifted away from mixer: {a:?} {b:?}"
            );
            assert!(a.width() <= 340. && b.width() <= 340.);
            assert!(a.right() < size.x && b.right() < size.x);
        }
    }
    #[test]
    fn remote_background_stop_leaves_auto_dj_enabled() {
        let mut player = Player::default();
        player.run.auto = true;
        player
            .execute_control(control::Action::Stop {
                deck: control::Deck::A2,
            })
            .unwrap();
        assert!(player.run.auto);
    }
    #[test]
    fn live_selection_does_not_change_crossfade_assignment_or_other_decks() {
        let mut player = Player {
            loaded: [Some(1), Some(2), Some(3), Some(4), Some(5), Some(6)],
            ..Default::default()
        };
        let before = player.loaded;
        player
            .execute_control(control::Action::SelectLive {
                deck: control::Deck::A2,
            })
            .unwrap();
        player
            .execute_control(control::Action::SelectLive {
                deck: control::Deck::B3,
            })
            .unwrap();
        assert_eq!(player.rack.live, [2, 5]);
        assert_eq!(player.pair(), [0, 1]);
        assert_eq!(player.loaded, before);
    }
    #[test]
    fn six_deck_layout_stays_inside_resized_viewport() {
        let ctx = egui::Context::default();
        let mut player = Player::default();
        for (size, expanded) in [
            (egui::vec2(1570., 1000.), false),
            (egui::vec2(1570., 1000.), true),
            (egui::vec2(1000., 700.), false),
            (egui::vec2(740., 480.), false),
        ] {
            player.rack.expanded = expanded;
            for _ in 0..3 {
                let raw = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(raw, |ui| {
                    player.group_ui(ui, 2);
                    assert!(
                        ui.min_rect().right() <= size.x + 1.
                            && ui.min_rect().bottom() <= size.y + 1.,
                        "{size:?} {:?}",
                        ui.min_rect()
                    );
                });
                output.textures_delta.clear();
            }
        }
    }
    #[test]
    fn reduced_window_keeps_readable_transport_and_a_visible_library() {
        let ctx = egui::Context::default();
        theme::apply(&ctx, theme::Theme::ModernDark);
        let mut player = Player::default();
        for (size, expanded, locked) in [
            (egui::vec2(1000., 700.), false, false),
            (egui::vec2(1000., 700.), false, true),
            (egui::vec2(1000., 700.), true, false),
            (egui::vec2(1280., 800.), false, false),
            (egui::vec2(1570., 1000.), false, false),
            (egui::vec2(1570., 1000.), true, false),
        ] {
            player.rack.expanded = expanded;
            player.run.locked = locked;
            for _ in 0..3 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| player.group_ui(ui, 2),
                );
                output.textures_delta.clear();
            }
            let play = ctx
                .data(|data| {
                    data.get_temp::<egui::Rect>(egui::Id::unique(("rack-main-play", 0usize)))
                })
                .unwrap();
            let library = ctx
                .data(|data| data.get_temp::<f32>(egui::Id::unique("rack-library-top")))
                .unwrap();
            assert!(
                play.height() >= 37.,
                "Transport was shrunk at {size:?}: {play:?}"
            );
            assert!(
                library <= size.y - 140.,
                "The music browser was pushed out at {size:?}: {library}"
            );
        }
    }
}
