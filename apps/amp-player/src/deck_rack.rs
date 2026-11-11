//! Stable six-deck identities: interleaved by side, never selected by track index.
use super::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Rack {
    pub live: [usize; 2],
    pub expanded: bool,
    pub six: bool,
}
impl Default for Rack {
    fn default() -> Self {
        Self {
            live: [0, 1],
            expanded: false,
            six: true,
        }
    }
}
pub const NAMES: [&str; 6] = ["A1", "B1", "A2", "B2", "A3", "B3"];
impl Player {
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
            .fill(if playing {
                color
            } else {
                ui.visuals().widgets.inactive.bg_fill
            })
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
        if playing { egui::Color32::BLACK } else { color },
    );
    response
}
impl Player {
    pub(super) fn six_deck_ui(&mut self, ui: &mut egui::Ui) {
        self.release_jog_wheels(ui.ctx());
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
        ui.horizontal_wrapped(|ui|{
            self.performance_controls(ui);
            self.clock_controls(ui);
            if ui.selectable_label(self.rack.expanded,"Expanded").clicked(){self.rack.expanded = !self.rack.expanded;}
            ui.menu_button("Setup",|ui|{
                if ui.button("Use original two-deck layout").clicked(){self.rack.six=false;ui.close();}
                let previous=self.run.transitions;
                transition_controls::show(ui,&mut self.run.transitions,&mut self.wave_span,false);
                if previous!=self.run.transitions {let _=transition_styles::save(self.run.transitions);}
                if ui.button("Assign crossfader to LIVE pair").on_hover_text("Pause both the current and requested pair first. Background layers keep playing. Auto DJ must be off.").clicked(){self.status=if self.run.auto{"Stop Auto DJ before assigning a different pair".into()}else if let Err(e)=self.ensure_audio().and_then(|_|self.audio.as_ref().unwrap().assign_pair(self.rack.live)){e}else{"Crossfade assignment queued; applies only if both pairs remain paused".into()};}
            });
        });
        ui.separator();
        ui.columns(2, |columns| {
            for (side, column) in columns.iter_mut().enumerate() {
                let d = self.rack.live[side];
                column.horizontal(|ui| {
                    ui.colored_label(accent(d), NAMES[d]);
                    ui.add_sized(
                        [(ui.available_width() - 190.).max(80.), 20.],
                        egui::Label::new(egui::RichText::new(self.title(d)).strong()).truncate(),
                    );
                    if let Some(track) = self.loaded_track(d) {
                        ui.colored_label(
                            accent(d),
                            format!(
                                "{}  {}",
                                track.preparation.bpm.map_or("--".into(), |b| format!(
                                    "{:.1}",
                                    b * (1. + self.pitch[d] as f64 / 100.)
                                )),
                                track.preparation.harmonic_key.as_deref().unwrap_or("--")
                            ),
                        );
                    }
                    if let Some(a) = &self.audio {
                        ui.label(format!("−{:.0}s", a.remaining(d).unwrap_or(0.)));
                    }
                });
                let t = self.loaded_snapshot[d]
                    .as_ref()
                    .or_else(|| self.loaded[d].and_then(|i| self.library.as_ref()?.tracks.get(i)));
                self.waveforms[d].hide_caption = true;
                self.waveforms[d].height = Some(if self.rack.expanded { 110. } else { 86. });
                self.waveforms[d].show(
                    column,
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
        });
        ui.add_space(6.);
        let library_id = egui::Id::unique("dj-library-height");
        let max_library = (ui.available_height() - 380.).max(160.);
        let mut library_height = ui
            .ctx()
            .data_mut(|d| d.get_persisted::<f32>(library_id))
            .unwrap_or(210.)
            .clamp(160., max_library);
        let height = (ui.available_height()
            - if self.dj_library_hidden {
                40.
            } else {
                library_height + 58.
            })
        .max(320.);
        let strips = if self.rack.expanded { 190. } else { 155. };
        let mixer = 136.;
        let jog_width = ((ui.available_width() - strips * 2. - mixer - 40.) / 2.).max(220.);
        ui.horizontal_top(|ui| {
            self.deck_strips(ui, 0, strips, height);
            self.jog_section(ui, self.rack.live[0], jog_width, height);
            self.rack_mixer(ui, mixer, height);
            self.jog_section(ui, self.rack.live[1], jog_width, height);
            self.deck_strips(ui, 1, strips, height);
        });
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
                160. ..=max_library,
                false,
                -1.,
                18.,
            );
            ui.ctx()
                .data_mut(|d| d.insert_persisted(library_id, library_height));
            self.dj_library_ui(ui);
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
    fn deck_strips(&mut self, ui: &mut egui::Ui, side: usize, width: f32, height: f32) {
        ui.allocate_ui_with_layout(egui::vec2(width,height),egui::Layout::top_down(egui::Align::Min),|ui|{
            for d in (side..6).step_by(2){ui.push_id(("strip",d),|ui|{
                let selected=self.rack.live[side]==d;
                let frame=egui::Frame::group(ui.style()).fill(theme::current(ui.ctx()).palette().surface).stroke(egui::Stroke::new(if selected{2.}else{1.},if selected{accent(d)}else{ui.visuals().widgets.noninteractive.bg_stroke.color})).show(ui,|ui|{
                    ui.set_width(width-18.);
                    ui.horizontal(|ui|{
                        ui.colored_label(accent(d),NAMES[d]);
                        ui.menu_button("···",|ui|self.strip_options(ui,d));
                        if ui.add(egui::Button::new(egui::RichText::new("LIVE").color(if selected { egui::Color32::BLACK } else { accent(d) })).fill(if selected { accent(d) } else { ui.visuals().widgets.inactive.bg_fill })).on_hover_text("Select this deck for the jog wheel. Its timeline and crossfade routing stay unchanged.").clicked(){self.rack.live[side]=d;}
                        if transport_button(ui,d,self.playing[d],egui::vec2(26.,24.)).clicked(){self.toggle_deck(d);}
                    });
                    ui.add(egui::Label::new(self.title(d)).truncate());
                    let track=self.loaded_snapshot[d].as_ref().or_else(||self.loaded[d].and_then(|i|self.library.as_ref()?.tracks.get(i)));
                    self.waveforms[d].thumbnail(ui,self.loaded_origin[d].as_deref().unwrap_or(&self.root),track,self.audio.as_ref().map_or(0.,|a|a.source_position(d)),accent(d));
                    ui.horizontal(|ui|{
                        if ui.small_button("L").on_hover_text("Toggle an independent four-beat loop").clicked(){self.rack_loop(d);}
                        if ui.small_button("SYNC").clicked(){self.sync_deck(d);}
                        if ui.small_button("Fade").clicked()&& let Some(a)=&self.audio{let _=a.fade_deck(d,0.,self.fade_seconds,true);}
                    });
                    if ui.add(egui::Slider::new(&mut self.channel_gain[d],0. ..=1.).show_value(false)).changed() && let Some(a)=&self.audio{a.set_gain(d,self.channel_gain[d]);}
                    if self.rack.expanded {ui.small(if self.audio.as_ref().is_some_and(|a|a.controls.looping[d].load(Relaxed)){"LOOPING"}else if self.playing[d]{"PLAYING"}else{"PAUSED"});}

                });
                if let Some(path)=dragdrop::target(&frame.response,accent(d)) && let Err(e)=self.safe_load_path(d,path,None){self.status=e;}
            });}
        });
    }
    fn strip_options(&mut self, ui: &mut egui::Ui, d: usize) {
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
    fn jog_section(&mut self, ui: &mut egui::Ui, d: usize, width: f32, height: f32) {
        ui.allocate_ui_with_layout(
            egui::vec2(width, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                egui::Frame::group(ui.style())
                    .fill(theme::current(ui.ctx()).palette().surface)
                    .show(ui, |ui| {
                        ui.set_width(width - 18.);
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.small("PITCH");
                                ui.spacing_mut().slider_width = (height - 185.)
                                    .clamp(100., if self.rack.expanded { 150. } else { 220. });
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
                            });
                            let wheel_size = ((width - 180.).max(115.))
                                .min((height - 160.).max(115.))
                                .min(if self.rack.expanded { 200. } else { 240. });
                            let wheel = skin::vinyl_platter(
                                ui,
                                wheel_size,
                                accent(d),
                                self.playing[d],
                                self.audio.as_ref().map_or(0., |a| a.source_position(d)) as f32,
                            );
                            self.jog_wheel(ui, d, &wheel);
                            if width > 335. {
                                ui.vertical(|ui| {
                                    self.rack_stem_knobs(ui, d);
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        for (band, name) in [(0, "LOW"), (1, "MID"), (2, "HIGH")] {
                                            ui.vertical(|ui| {
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
                                });
                            }
                        });
                        ui.horizontal(|ui| {
                            if skin::deck_button(ui, "CUE", accent(d), 55., false).clicked() {
                                self.recue_deck(d);
                            }
                            if transport_button(ui, d, self.playing[d], egui::vec2(65., 38.))
                                .clicked()
                            {
                                self.toggle_deck(d);
                            }
                            if skin::deck_button(ui, "SYNC", accent(d), 55., false).clicked() {
                                self.sync_deck(d);
                            }
                            if skin::deck_button(ui, "LOOP", accent(d), 55., false).clicked() {
                                self.rack_loop(d);
                            }
                        });
                        ui.menu_button("Advanced", |ui| {
                            if width <= 335. {
                                self.rack_stem_knobs(ui, d);
                            }
                            if ui.button("Stop deck").clicked() {
                                self.stop(d);
                            }
                            if let Some(a) = &self.audio {
                                let mut enabled = a.controls.key_lock[d].load(Relaxed);
                                if ui
                                    .add_enabled(
                                        a.controls.key_lock_available[d].load(Relaxed),
                                        egui::Checkbox::new(
                                            &mut enabled,
                                            "Key Lock · preserve pitch",
                                        ),
                                    )
                                    .changed()
                                {
                                    a.controls.key_lock[d].store(enabled, Relaxed);
                                }
                                ui.small("Signalsmith Stretch · vinyl scratch bypasses Key Lock");
                            }
                            if let Some(a) = &self.audio {
                                ui.small(format!(
                                    "Underruns: {}",
                                    a.controls.underruns[d].load(Relaxed)
                                ));
                            }
                        });
                    });
                ui.horizontal(|ui| {
                    ui.small(if self.rack.expanded || self.dj_cues[d] {
                        "Hot Cues (1–8)"
                    } else {
                        "Hot Cues (1–4)"
                    });
                    ui.toggle_value(&mut self.dj_cues[d], "Expand");
                });
                self.main_hot_cues_count(
                    ui,
                    d,
                    accent(d),
                    if self.dj_cues[d] || self.rack.expanded {
                        8
                    } else {
                        4
                    },
                );
                ui.horizontal(|ui| {
                    ui.small(if self.rack.expanded {
                        "Sampler · full bank"
                    } else {
                        "Sampler · favorites"
                    });
                    ui.menu_button("Expand / Cue Deck", |ui| {
                        self.side_panel(ui, d % 2, width - 30., 300., true);
                    });
                });
                let actions = if d.is_multiple_of(2) {
                    self.sampler
                        .inline_pads(ui, self.audio.as_ref(), self.rack.expanded)
                } else {
                    self.right_sampler.inline_pads(
                        ui,
                        self.right_sample_audio.as_ref(),
                        self.rack.expanded,
                    )
                };
                for action in actions {
                    self.side_sample_action(d % 2, action);
                }
            },
        );
    }
    fn rack_mixer(&mut self, ui: &mut egui::Ui, width: f32, height: f32) {
        let pair = self.pair();
        ui.allocate_ui_with_layout(
            egui::vec2(width, height),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                egui::Frame::group(ui.style())
                    .fill(theme::current(ui.ctx()).palette().surface)
                    .show(ui, |ui| {
                        ui.set_width(width - 18.);
                        ui.horizontal(|ui| {
                            for d in pair {
                                ui.vertical(|ui| {
                                    ui.colored_label(accent(d), NAMES[d]);
                                    skin::meter(
                                        ui,
                                        self.audio.as_ref().map_or(0., |a| a.peak(d)),
                                        egui::vec2(18., (height - 185.).clamp(90., 230.)),
                                    );
                                    ui.spacing_mut().slider_width = 80.;
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
                            self.inline_mixer(ui, 600.);
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
}
