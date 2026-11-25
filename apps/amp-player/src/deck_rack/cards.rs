//! Graphite DJ cards use the existing transport, scratch, cue and mixer actions.
use super::*;
fn panel(ui: &egui::Ui, color: Option<egui::Color32>) -> egui::Frame {
    egui::Frame::new()
        .fill(ui.visuals().panel_fill)
        .stroke(egui::Stroke::new(
            if color.is_some() { 1.5 } else { 1. },
            color.unwrap_or(ui.visuals().widgets.noninteractive.bg_stroke.color),
        ))
        .corner_radius(12)
        .inner_margin(10)
}
fn tool_key(deck: usize) -> egui::Id {
    egui::Id::unique(("card-deck-tools-open", deck))
}
impl Player {
    pub(super) fn card_open_tools(&self, ctx: &egui::Context, deck: usize, tab: u8) {
        ctx.data_mut(|d| {
            d.insert_temp(tool_key(deck), true);
            d.insert_temp(tool_key(deck).with("tab"), tab);
        });
    }
    pub(super) fn card_tool_windows(&mut self, ctx: &egui::Context) {
        for (deck, name) in NAMES.iter().enumerate() {
            let mut open = ctx
                .data(|d| d.get_temp::<bool>(tool_key(deck)))
                .unwrap_or(false);
            if !open {
                continue;
            }
            let mut tab = ctx
                .data(|d| d.get_temp::<u8>(tool_key(deck).with("tab")))
                .unwrap_or(0);
            egui::Window::new(format!("{name} · Deck controls"))
                .id(tool_key(deck).with("window"))
                .open(&mut open)
                .default_width(380.)
                .max_height((ctx.content_rect().height() - 90.).max(240.))
                .vscroll(true)
                .show(ctx, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(
                                self.selected.is_some(),
                                egui::Button::new("Load selected song"),
                            )
                            .clicked()
                            && let Some(index) = self.selected
                        {
                            self.load_deck(deck, index, false);
                        }
                        if ui.button("Loop").clicked() {
                            self.rack_loop(deck);
                        }
                        if ui.button("Sync").clicked() {
                            self.sync_deck(deck);
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (value, label) in [
                            (0, "Mixer"),
                            (1, "Stems"),
                            (2, "Sampler / Cue Deck"),
                            (3, "Advanced"),
                        ] {
                            let response = ui.selectable_value(&mut tab, value, label);
                            #[cfg(test)]
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(
                                    egui::Id::unique(("card-tool-tab", deck, value)),
                                    (response.rect, ui.layer_id()),
                                )
                            });
                            #[cfg(not(test))]
                            let _ = response;
                        }
                    });
                    ui.separator();
                    match tab {
                        1 => self.rack_stem_preparation(ui, deck),
                        2 => self.side_panel(ui, deck % 2, ui.available_width(), 450., true),
                        3 => {
                            ui.strong("All eight hot cues");
                            self.main_hot_cues_count(ui, deck, accent(deck), 8);
                            ui.separator();
                            self.strip_options(ui, deck);
                            ui.separator();
                            self.rack_deck_controls(ui, deck, 320.);
                        }
                        _ => self.rack_deck_controls(ui, deck, 320.),
                    }
                });
            ctx.data_mut(|d| {
                d.insert_temp(tool_key(deck), open);
                d.insert_temp(tool_key(deck).with("tab"), tab);
            });
        }
    }
    // LIVE is an explicit manual takeover, never a transport toggle.
    pub(super) fn make_deck_live(&mut self, deck: usize) {
        if deck >= NAMES.len() {
            self.status = "Choose a valid deck".into();
            return;
        }
        let cleanup = self.run.live_takeover_cleanup();
        let result = self.ensure_audio().and_then(|_| {
            self.audio
                .as_ref()
                .unwrap()
                .select_live_with_cleanup(deck, cleanup)
        });
        match result {
            Ok(()) => {
                self.run.finish_live_takeover();
                self.auto = false;
                self.rack.live[deck % 2] = deck;
                self.status = format!(
                    "{} live · manual mixing · playing decks keep running",
                    NAMES[deck]
                );
            }
            Err(error) => self.status = error,
        }
    }
    fn card_clear_button(&mut self, ui: &mut egui::Ui, deck: usize) {
        let loaded = self.loaded[deck].is_some()
            || self.loaded_snapshot[deck].is_some()
            || self.pending_load[deck].is_some();
        let response = ui
            .add_enabled(
                loaded,
                egui::Button::new("×").min_size(egui::vec2(24., 26.)),
            )
            .on_hover_text(format!(
                "Clear {} · stop and unload this track",
                NAMES[deck]
            ));
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, loaded, format!("Clear {}", NAMES[deck]))
        });
        if response.clicked() {
            self.clear_deck(deck);
        }
    }
    fn card_live_button(&mut self, ui: &mut egui::Ui, deck: usize) {
        let live = self.pair()[deck % 2] == deck;
        let response = ui.add_sized([68., 26.], egui::Button::new(
            egui::RichText::new(if live { "On Deck" } else { "To Deck" }).strong().color(if live { egui::Color32::BLACK } else { accent(deck) }))
            .fill(if live { accent(deck) } else { ui.visuals().widgets.inactive.bg_fill }))
            .on_hover_text("Show this quick track on its main jog wheel and mixer side. Keeps its position and transport; other tracks keep playing.");
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Button,
                true,
                format!(
                    "{} Load to main deck{}",
                    NAMES[deck],
                    if live { " · assigned" } else { "" }
                ),
            )
        });
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::unique(("deck-live-button", deck)),
                (response.rect, ui.layer_id()),
            )
        });
        if response.clicked() && !live {
            self.make_deck_live(deck);
        }
    }
    pub(super) fn card_rack(&mut self, ui: &mut egui::Ui, height: f32) {
        if self.rack.six && ui.available_width() < 900. {
            egui::ScrollArea::horizontal()
                .id_salt("quick-slots-rack-overflow")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_min_width(900.);
                    self.card_narrow_rack(ui, height);
                });
            return;
        }
        if ui.available_width() < if self.rack.six { 1100. } else { 950. } {
            self.card_narrow_rack(ui, height);
            return;
        }
        let available = (ui.available_width() - 40.).max(1.);
        let total = available.min(1900.);
        let gap = 8.;
        let center = if total < 1250. { 94. } else { 108. };
        let mut banks = if self.rack.six {
            self.rack
                .strip_ratios
                .map(|r| (total * r).clamp(148., 280.))
        } else {
            [0.; 2]
        };
        let budget = (total - center - 2. * 300. - gap * 6. - 20.).max(296.);
        if banks.iter().sum::<f32>() > budget {
            banks = [budget / 2.; 2];
        }
        let rendered_banks = banks;
        let main = ((total
            - banks.iter().sum::<f32>()
            - center
            - gap * if self.rack.six { 6. } else { 2. }
            - if self.rack.six { 20. } else { 0. })
            / 2.)
            .max(260.);
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("native-dj-card-layout"), true));
        ui.spacing_mut().item_spacing.x = gap;
        ui.horizontal_top(|ui| {
            if available > total {
                ui.add_space((available - total) / 2.);
            }
            if self.rack.six {
                self.card_auxiliary(ui, banks[0], height, 0);
                window_layout::splitter(
                    ui,
                    "dj-deck-left-divider",
                    &mut banks[0],
                    148. ..=280.,
                    true,
                    1.,
                    height,
                );
            }
            let a = self.rack.live[0];
            let b = self.rack.live[1];
            ui.push_id(("live-jog", a), |ui| {
                self.card_main_deck(ui, a, main, height)
            });
            self.card_crossfader(ui, center, height);
            ui.push_id(("live-jog", b), |ui| {
                self.card_main_deck(ui, b, main, height)
            });
            if self.rack.six {
                window_layout::splitter(
                    ui,
                    "dj-deck-right-divider",
                    &mut banks[1],
                    148. ..=280.,
                    true,
                    -1.,
                    height,
                );
                self.card_auxiliary(ui, banks[1], height, 1);
            }
        });
        if self.rack.six {
            for side in 0..2 {
                if (banks[side] - rendered_banks[side]).abs() > 0.01 {
                    self.rack.strip_ratios[side] = banks[side] / total;
                }
            }
        }
    }
    // A/B remain side by side. Narrow windows move the mixer below them and
    // reflow the auxiliary controls under their own side, without changing IDs.
    fn card_narrow_rack(&mut self, ui: &mut egui::Ui, height: f32) {
        let gap = 8.;
        let width = (ui.available_width() - 24.).max(1.);
        let main = (width - gap) / 2.;
        let short_native = (ui
            .ctx()
            .layer_transform_to_global(ui.layer_id())
            .unwrap_or_default()
            * ui.clip_rect())
        .height()
            < 760.;
        let auxiliary_height = if self.rack.six { 130. } else { 0. };
        let main_height = (height - 56. - auxiliary_height - if self.rack.six { 12. } else { 6. })
            .max(if short_native { 224. } else { 244. });
        ui.spacing_mut().item_spacing.x = gap;
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("native-dj-card-layout"), true));
        ui.horizontal_top(|ui| {
            for d in self.rack.live {
                ui.push_id(("live-jog", d), |ui| {
                    self.card_main_deck(ui, d, main, main_height)
                });
            }
        });
        if self.rack.six {
            ui.horizontal_top(|ui| {
                self.card_auxiliary_row(ui, main, 0);
                self.card_auxiliary_row(ui, main, 1);
            });
        }
        self.card_narrow_crossfader(ui, width);
    }
    fn card_auxiliary_row(&mut self, ui: &mut egui::Ui, width: f32, side: usize) {
        let slot_width = (width - 12.) / 3.;
        ui.allocate_ui_with_layout(
            egui::vec2(width, 130.),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                for deck in (side..6).step_by(2) {
                    ui.push_id(("quick-slot", deck), |ui| {
                        self.card_quick_track(ui, deck, slot_width, 130.)
                    });
                }
            },
        );
    }
    fn card_narrow_crossfader(&mut self, ui: &mut egui::Ui, width: f32) {
        let pair = self.pair();
        let short_native = (ui
            .ctx()
            .layer_transform_to_global(ui.layer_id())
            .unwrap_or_default()
            * ui.clip_rect())
        .height()
            < 760.;
        let frame = panel(ui, None)
            .inner_margin(egui::Margin::symmetric(
                10,
                if short_native { 1 } else { 10 },
            ))
            .show(ui, |ui| {
                ui.set_width(width - 24.);
                ui.horizontal(|ui| {
                    ui.colored_label(
                        spectrum_color(pair[0], self.rack.spectrum_palette),
                        NAMES[pair[0]],
                    );
                    ui.spacing_mut().slider_width = (width - 420.).max(140.);
                    if ui
                        .add(egui::Slider::new(&mut self.fader, 0. ..=1.).show_value(false))
                        .changed()
                        && let Some(audio) = &self.audio
                    {
                        self.run.cancel_automation();
                        audio.set_fader(self.fader);
                    }
                    ui.colored_label(
                        spectrum_color(pair[1], self.rack.spectrum_palette),
                        NAMES[pair[1]],
                    );
                    let fade = ui.add_sized(
                        [62., 32.],
                        egui::Button::new(
                            egui::RichText::new("Auto")
                                .strong()
                                .color(egui::Color32::BLACK),
                        )
                        .fill(ORANGE),
                    );
                    #[cfg(test)]
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(
                            egui::Id::unique("dj-auto-fade-bounds").with("screen"),
                            (fade.rect, ui.layer_id()),
                        )
                    });
                    if fade.clicked() {
                        self.auto_crossfade();
                    }
                    if ui
                        .add(
                            egui::DragValue::new(&mut self.fade_seconds)
                                .range(0.5..=30.)
                                .speed(0.5)
                                .suffix("s"),
                        )
                        .changed()
                    {
                        let _ = crossfade_settings::save(self.fade_seconds);
                    }
                    let headphones = ui.add_sized([32., 32.], egui::Button::new(""));
                    icons::draw(
                        ui,
                        egui::Rect::from_center_size(
                            headphones.rect.center(),
                            egui::vec2(22., 22.),
                        ),
                        icons::Icon::Headphones,
                        skin::foreground(ui),
                    );
                    headphones.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), "Headphones")
                    });
                    if headphones
                        .on_hover_text("Headphones · choose private cue output")
                        .clicked()
                    {
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(egui::Id::unique("card-headphones-open"), true)
                        });
                    }
                });
            });
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::unique("narrow-crossfader-extent"),
                (frame.response.rect, frame.response.layer_id),
            )
        });
        #[cfg(not(test))]
        let _ = frame;
        self.card_headphones_window(ui.ctx());
    }
    pub(crate) fn card_view_tools(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.rack.six, false, "A / B main");
            ui.selectable_value(&mut self.rack.six, true, "6 quick tracks");
            ui.separator();
            for (label, tab) in [
                ("Effects", 0),
                ("Stems", 1),
                ("Controls", 0),
                ("Advanced", 3),
            ] {
                ui.menu_button(label, |ui| {
                    for side in 0..2 {
                        let deck = self.rack.live[side];
                        if ui.button(format!("{} · {label}", NAMES[deck])).clicked() {
                            self.card_open_tools(ui.ctx(), deck, tab);
                            ui.close();
                        }
                    }
                });
            }
        });
    }
    fn card_auxiliary(&mut self, ui: &mut egui::Ui, width: f32, height: f32, side: usize) {
        ui.allocate_ui_with_layout(
            egui::vec2(width, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing.y = 8.;
                let slot_height = ((height - 16.) / 3.).max(126.);
                for deck in (side..6).step_by(2) {
                    ui.push_id(("quick-slot", deck), |ui| {
                        self.card_quick_track(ui, deck, width, slot_height)
                    });
                }
            },
        );
    }
    fn card_quick_track(&mut self, ui: &mut egui::Ui, deck: usize, width: f32, height: f32) {
        let selected = self.rack.live[deck % 2] == deck;
        let frame = panel(ui, selected.then(|| accent(deck)))
            .inner_margin(6)
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width((width - 14.).max(112.));
                    ui.set_min_height(height - 14.);
                    ui.spacing_mut().item_spacing = egui::vec2(3., 3.);
                    ui.spacing_mut().button_padding = egui::vec2(3., 2.);
                    ui.spacing_mut().interact_size.y = 26.;
                    ui.horizontal(|ui| {
                        ui.strong(
                            egui::RichText::new(NAMES[deck])
                                .color(spectrum_color(deck, self.rack.spectrum_palette)),
                        );
                        self.card_live_button(ui, deck);
                        ui.menu_button("···", |ui| {
                            if ui
                                .add_enabled(
                                    self.selected.is_some(),
                                    egui::Button::new("Load selected song into slot"),
                                )
                                .clicked()
                                && let Some(index) = self.selected
                            {
                                self.load_deck(deck, index, false);
                                ui.close();
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        self.card_clear_button(ui, deck);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(self.title(deck))
                                    .color(spectrum_color(deck, self.rack.spectrum_palette)),
                            )
                            .truncate(),
                        )
                        .on_hover_text(self.title(deck));
                    });
                    let position = self
                        .audio
                        .as_ref()
                        .map_or(0., |audio| audio.source_position(deck));
                    ui.small(format!(
                        "{:02}:{:02}",
                        (position / 60.) as u64,
                        position as u64 % 60
                    ));
                    let control_width = ((width - 20.) / 3.).max(30.);
                    ui.horizontal(|ui| {
                        let play = transport_button(
                            ui,
                            deck,
                            self.playing[deck],
                            egui::vec2(control_width, 32.),
                        );
                        if play.clicked() {
                            self.toggle_deck(deck);
                        }
                        let sync = ui.add_sized([control_width, 32.], egui::Button::new("Sync"));
                        if sync.clicked() {
                            self.sync_deck(deck);
                        }
                        let loop_button =
                            ui.add_sized([control_width, 32.], egui::Button::new("Loop"));
                        if loop_button.clicked() {
                            self.rack_loop(deck);
                        }
                        #[cfg(test)]
                        ui.ctx().data_mut(|data| {
                            for (name, rect) in [
                                ("rack-aux-play", play.rect),
                                ("rack-aux-sync", sync.rect),
                                ("rack-aux-loop", loop_button.rect),
                            ] {
                                data.insert_temp(
                                    egui::Id::unique((name, deck)),
                                    (rect, ui.layer_id()),
                                );
                            }
                        });
                    });
                    ui.small("Drop a song here to load");
                });
            });
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::unique(("rack-strip-drop", deck)),
                (frame.response.interact_rect, frame.response.layer_id),
            );
            data.insert_temp(
                egui::Id::unique(("rack-aux-extent", deck)),
                (frame.response.rect, frame.response.layer_id),
            );
        });
        if let Some(path) = dragdrop::target(&frame.response, accent(deck))
            && let Err(error) = self.safe_load_path(deck, path, None)
        {
            self.status = error;
        }
    }
    fn card_stem_dial(&mut self, ui: &mut egui::Ui, d: usize, stem: usize, label: &str) {
        let active = self
            .audio
            .as_ref()
            .is_some_and(|a| a.controls.stems_loaded[d].load(Relaxed));
        let mut value = self.audio.as_ref().map_or(0., |a| {
            f32::from_bits(a.controls.stem_knobs[d][stem].load(Relaxed))
        });
        let response = ui
            .add_enabled_ui(active, |ui| {
                dj_mixer::dial(ui, label, &mut value, -1. ..=1., accent(d))
            })
            .inner;
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(egui::Id::unique(("card-stem-knob", d, stem)), response.rect)
        });
        if response.changed()
            && let Some(audio) = &self.audio
        {
            if self.pair().contains(&d) {
                self.run.cancel_effects(audio);
            }
            let mut knobs = std::array::from_fn(|i| {
                f32::from_bits(audio.controls.stem_knobs[d][i].load(Relaxed))
            });
            knobs[stem] = value;
            for (i, gain) in amp_core::stem_mix::gains(knobs).into_iter().enumerate() {
                audio.controls.stem_knobs[d][i].store(knobs[i].to_bits(), Relaxed);
                audio.controls.stem_level[d][i].store(gain.to_bits(), Relaxed);
            }
        }
        response.on_disabled_hover_text(
            "Enable live or prepared stems from this deck’s Stems controls.",
        );
        ui.small(label);
    }
    fn card_transport(&mut self, ui: &mut egui::Ui, d: usize, width: f32, phones: bool) {
        let transport = ui.horizontal(|ui| {
            if phones {
                ui.spacing_mut().item_spacing.x = 3.;
                ui.spacing_mut().button_padding.x = 2.;
            }
            let button = ((width
                - ui.spacing().item_spacing.x * if phones { 4. } else { 3. }
                - if phones { 32. } else { 0. })
                / 4.)
                .max(25.);
            let cue = skin::deck_button(ui, "CUE", accent(d), button, false)
                .on_hover_text("Return to the prepared cue IN and pause");
            #[cfg(test)]
            {
                let transform = ui
                    .ctx()
                    .layer_transform_to_global(ui.layer_id())
                    .unwrap_or_default();
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        egui::Id::unique(("deck-cue-bounds", d)),
                        transform * cue.rect,
                    );
                });
            }
            if cue.clicked() {
                self.recue_deck(d);
            }
            if transport_button(ui, d, self.playing[d], egui::vec2(button, 38.)).clicked() {
                self.toggle_deck(d);
            }
            if skin::deck_button(ui, "SYNC", accent(d), button, false).clicked() {
                self.sync_deck(d);
            }
            if skin::deck_button(ui, "LOOP", accent(d), button, false).clicked() {
                self.rack_loop(d);
            }
            if phones {
                self.deck_headphones_button(ui, d);
            }
        });
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::unique(("dj-transport", d)).with("screen"),
                (transport.response.rect, ui.layer_id()),
            )
        });
        #[cfg(not(test))]
        let _ = transport;
    }
    fn card_tempo_value(&mut self, ui: &mut egui::Ui, d: usize) {
        if ui
            .add(
                egui::DragValue::new(&mut self.pitch[d])
                    .speed(0.1)
                    .range(-16. ..=16.)
                    .suffix("%"),
            )
            .on_hover_text("Pitch · tempo adjustment (%)")
            .changed()
            && let Some(audio) = &self.audio
        {
            let _ = audio.sync(d, false);
            audio.set_rate(d, 1. + self.pitch[d] / 100.);
        }
    }
    fn card_tempo(&mut self, ui: &mut egui::Ui, d: usize) {
        ui.horizontal(|ui| {
            ui.small("Pitch");
            self.card_tempo_value(ui, d);
        });
    }
    fn card_restore_samples(&mut self, d: usize) {
        if d.is_multiple_of(2) {
            if !self.sampler.started {
                self.sampler.started = true;
                if self.sampler.has_saved() && self.ensure_audio().is_ok() {
                    self.sampler.restore();
                }
            }
        } else if !self.right_sampler.started {
            self.right_sampler.started = true;
            if self.right_sampler.has_saved() && self.ensure_right_samples().is_ok() {
                self.right_sampler.restore();
            }
        }
    }
    fn card_main_deck(&mut self, ui: &mut egui::Ui, d: usize, width: f32, height: f32) {
        let area = ui.allocate_ui_with_layout(
            egui::vec2(width, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                panel(ui, Some(accent(d)))
                    .inner_margin(if height < 280. {
                        egui::Margin::symmetric(10, 6)
                    } else {
                        egui::Margin::same(10)
                    })
                    .show(ui, |ui| {
                        ui.set_width(width - 24.);
                        ui.set_min_height(height - 24.);
                        ui.spacing_mut().item_spacing.y = 3.;
                        let compact_card = width < 1000.;
                        let short_controls = height < 280.;
                        let minimum_wheel = if short_controls { 140. } else { 160. };
                        let wheel = (height - if compact_card { 222. } else { 180. })
                            .clamp(minimum_wheel, 218.)
                            .min((width * 0.43).max(minimum_wheel));
                        let info = (width
                            - 20.
                            - wheel
                            - if compact_card { 0. } else { 34. }
                            - ui.spacing().item_spacing.x * if compact_card { 1. } else { 2. })
                        .max(96.);
                        ui.horizontal_top(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(info, wheel),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_width(info);
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            egui::RichText::new(NAMES[d]).size(22.).strong().color(
                                                spectrum_color(d, self.rack.spectrum_palette),
                                            ),
                                        );
                                        ui.colored_label(
                                            spectrum_color(d, self.rack.spectrum_palette),
                                            if d.is_multiple_of(2) {
                                                "DECK A"
                                            } else {
                                                "DECK B"
                                            },
                                        );
                                        if info >= 160.
                                            && ui
                                                .small_button("···")
                                                .on_hover_text("Keep deck controls open")
                                                .clicked()
                                        {
                                            self.card_open_tools(ui.ctx(), d, 0);
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        self.card_clear_button(ui, d);
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(self.title(d))
                                                    .size(18.)
                                                    .strong()
                                                    .color(spectrum_color(
                                                        d,
                                                        self.rack.spectrum_palette,
                                                    )),
                                            )
                                            .truncate(),
                                        );
                                    });
                                    let track = self.loaded_track(d);
                                    let artist = track
                                        .and_then(|t| t.preparation.extensions.get("artist"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("Artist unavailable");
                                    if width >= 400. {
                                        ui.add(
                                            egui::Label::new(egui::RichText::new(artist).size(16.))
                                                .truncate(),
                                        );
                                    }
                                    let bpm = track
                                        .and_then(|t| t.preparation.bpm)
                                        .map(|b| b * self.deck_rate(d));
                                    let key = track
                                        .and_then(|t| t.preparation.harmonic_key.as_deref())
                                        .unwrap_or("—");
                                    ui.label(format!(
                                        "{} BPM   {key}",
                                        bpm.map_or("—".into(), |b| format!("{b:.1}"))
                                    ));
                                    let position =
                                        self.audio.as_ref().map_or(0., |a| a.source_position(d));
                                    let duration = self
                                        .audio
                                        .as_ref()
                                        .and_then(|a| a.remaining(d))
                                        .map(|r| position + r as f64)
                                        .or_else(|| {
                                            track.and_then(|t| {
                                                t.preparation.analysis.as_ref().map(|a| a.duration)
                                            })
                                        });
                                    let time = |s: f64| {
                                        format!(
                                            "{:02}:{:02}",
                                            (s.max(0.) / 60.) as u64,
                                            (s.max(0.) as u64) % 60
                                        )
                                    };
                                    let timing = format!(
                                        "{} / {}",
                                        time(position),
                                        duration.map_or("—:—".into(), time)
                                    );
                                    if short_controls {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing.x = 4.;
                                            ui.small(timing);
                                            self.card_tempo_value(ui, d);
                                        });
                                    } else {
                                        ui.small(timing);
                                        if compact_card {
                                            self.card_tempo(ui, d);
                                        }
                                    }
                                    let mut seek = position as f32;
                                    ui.spacing_mut().slider_width = info;
                                    let response = ui.add_enabled(
                                        duration.is_some(),
                                        egui::Slider::new(
                                            &mut seek,
                                            0. ..=duration.unwrap_or(1.).max(0.01) as f32,
                                        )
                                        .show_value(false),
                                    );
                                    if response.drag_stopped()
                                        || response.changed() && !response.dragged()
                                    {
                                        self.mix_to_waveform_section(ui.ctx(), d, seek as f64);
                                    }
                                    if !compact_card {
                                        self.card_transport(ui, d, info, false);
                                    }
                                },
                            );
                            self.rack_platter(ui, d, wheel);
                            if !compact_card {
                                self.rack_pitch(ui, d, wheel);
                            }
                        });
                        if compact_card {
                            self.card_transport(ui, d, width - 24., short_controls);
                        }
                        ui.separator();
                        // Sampling readiness is independent of the responsive presentation.
                        self.card_restore_samples(d);
                        let lower_controls = |ui: &mut egui::Ui| {
                            let mode_key = egui::Id::unique(("card-knob-mode-stems", d));
                            let mut stems = ui
                                .ctx()
                                .data(|data| data.get_temp::<bool>(mode_key))
                                .unwrap_or(true);
                            ui.horizontal_wrapped(|ui| {
                                ui.selectable_value(&mut stems, false, "EQ");
                                ui.selectable_value(&mut stems, true, "Stems");
                                if stems && ui.small_button("Enable / reload").clicked() {
                                    self.card_open_tools(ui.ctx(), d, 1);
                                }
                            });
                            ui.ctx().data_mut(|data| data.insert_temp(mode_key, stems));
                            let knobs = ui.scope(|ui| {
                                ui.columns(5, |columns| {
                                    for (band, label) in [(0, "LOW"), (1, "MID"), (2, "HIGH")] {
                                        columns[band].vertical_centered(|ui| {
                                            ui.spacing_mut().interact_size.y = 34.;
                                            if stems {
                                                let (stem, label) =
                                                    [(0, "VOC"), (2, "BEAT"), (1, "INST")][band];
                                                self.card_stem_dial(ui, d, stem, label);
                                                return;
                                            }
                                            if dj_mixer::dial(
                                                ui,
                                                label,
                                                &mut self.eq[d][band],
                                                -24. ..=12.,
                                                accent(d),
                                            )
                                            .changed()
                                                && let Some(audio) = &self.audio
                                            {
                                                if self.pair().contains(&d) {
                                                    self.run.cancel_effects(audio);
                                                }
                                                audio.set_eq(d, band, self.eq[d][band]);
                                            }
                                            ui.small(label);
                                        });
                                    }
                                    columns[3].vertical_centered(|ui| {
                                        ui.spacing_mut().interact_size.y = 34.;
                                        if self.mixer_panel.filter_knob(ui, d, accent(d))
                                            && let Some(audio) = &self.audio
                                        {
                                            if self.pair().contains(&d) {
                                                self.run.cancel_effects(audio);
                                            }
                                            self.mixer_panel.apply(audio);
                                        }
                                        ui.small("FILTER");
                                    });
                                    columns[4]
                                        .vertical_centered(|ui| self.deck_headphones_button(ui, d));
                                });
                            });
                            #[cfg(test)]
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(
                                    egui::Id::unique(("rack-knobs", d)),
                                    knobs.response.rect,
                                );
                                data.insert_temp(
                                    egui::Id::unique(("rack-knobs", d)).with("screen"),
                                    (knobs.response.rect, ui.layer_id()),
                                );
                            });
                            #[cfg(not(test))]
                            let _ = knobs;
                            ui.separator();
                            if self.rack.six {
                                ui.horizontal(|ui| {
                                    ui.small("Volume");
                                    ui.spacing_mut().slider_width = (width - 115.).max(90.);
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut self.channel_gain[d], 0. ..=1.)
                                                .show_value(false),
                                        )
                                        .changed()
                                        && let Some(audio) = &self.audio
                                    {
                                        audio.set_gain(d, self.channel_gain[d]);
                                    }
                                });
                            }
                            if width < 400. {
                                let cues = ui.scope(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.small("HOT CUES");
                                        if ui.small_button("1–8").clicked() {
                                            self.card_open_tools(ui.ctx(), d, 3);
                                            self.dj_cues[d] = true;
                                        }
                                        if ui.small_button("Sampler / Cue").clicked() {
                                            self.card_open_tools(ui.ctx(), d, 2);
                                        }
                                    });
                                    self.main_hot_cues_count(ui, d, accent(d), 4);
                                });
                                #[cfg(test)]
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(
                                        egui::Id::unique(("dj-cue-pads", d)).with("screen"),
                                        (cues.response.rect, ui.layer_id()),
                                    )
                                });
                                #[cfg(not(test))]
                                let _ = cues;
                            } else {
                                ui.columns(2, |columns| {
                                    let cues = columns[0].scope(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.small("HOT CUES");
                                            if ui.small_button("1–8").clicked() {
                                                self.card_open_tools(ui.ctx(), d, 3);
                                                self.dj_cues[d] = true;
                                            }
                                        });
                                        self.main_hot_cues_count(ui, d, accent(d), 4);
                                    });
                                    #[cfg(test)]
                                    columns[0].ctx().data_mut(|data| {
                                        data.insert_temp(
                                            egui::Id::unique(("dj-cue-pads", d)).with("screen"),
                                            (cues.response.rect, columns[0].layer_id()),
                                        )
                                    });
                                    #[cfg(not(test))]
                                    let _ = cues;
                                    columns[1].horizontal(|ui| {
                                        ui.small("SAMPLER");
                                        if ui.small_button("Bank / Cue").clicked() {
                                            self.card_open_tools(ui.ctx(), d, 2);
                                        }
                                    });
                                    let actions = if d.is_multiple_of(2) {
                                        if !self.sampler.started {
                                            self.sampler.started = true;
                                            if self.sampler.has_saved()
                                                && self.ensure_audio().is_ok()
                                            {
                                                self.sampler.restore();
                                            }
                                        }
                                        self.sampler.inline_pads(
                                            &mut columns[1],
                                            self.audio.as_ref(),
                                            false,
                                        )
                                    } else {
                                        if !self.right_sampler.started {
                                            self.right_sampler.started = true;
                                            if self.right_sampler.has_saved()
                                                && self.ensure_right_samples().is_ok()
                                            {
                                                self.right_sampler.restore();
                                            }
                                        }
                                        self.right_sampler.inline_pads(
                                            &mut columns[1],
                                            self.right_sample_audio.as_ref(),
                                            false,
                                        )
                                    };
                                    for action in actions {
                                        self.side_sample_action(d % 2, action);
                                    }
                                });
                            }
                        };
                        if height < 370. {
                            // A short performance card keeps the vinyl and transport pinned.
                            // Real, named control panels replace a clipped fragment of knobs.
                            let tools = ui.horizontal_wrapped(|ui| {
                                for (label, tab) in
                                    [("EQ", 0), ("Stems", 1), ("Cues", 3), ("Sampler", 2)]
                                {
                                    let action = ui.small_button(label);
                                    #[cfg(test)]
                                    ui.ctx().data_mut(|data| {
                                        data.insert_temp(
                                            egui::Id::unique(("card-visible-tool", d, tab)),
                                            (action.rect, ui.layer_id(), ui.clip_rect()),
                                        );
                                    });
                                    if action.clicked() {
                                        self.card_open_tools(ui.ctx(), d, tab);
                                        if tab == 3 {
                                            self.dj_cues[d] = true;
                                        }
                                    }
                                }
                                if !short_controls {
                                    self.deck_headphones_button(ui, d);
                                }
                            });
                            // Geometry consumers follow the current controls surface, not
                            // the previous tall layout's knobs retained in egui data.
                            #[cfg(test)]
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(
                                    egui::Id::unique(("rack-knobs", d)),
                                    tools.response.rect,
                                );
                                data.insert_temp(
                                    egui::Id::unique(("rack-knobs", d)).with("screen"),
                                    (tools.response.rect, ui.layer_id()),
                                );
                            });
                            #[cfg(not(test))]
                            let _ = tools;
                        } else {
                            ui.scope(lower_controls);
                        }
                    });
            },
        );
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::unique(("rack-jog-drop", d)),
                (area.response.interact_rect, area.response.layer_id),
            );
            data.insert_temp(
                egui::Id::unique(("rack-main-extent", d)),
                (area.response.rect, area.response.layer_id),
            );
        });
        if let Some(path) = dragdrop::target(&area.response, accent(d))
            && let Err(error) = self.safe_load_path(d, path, None)
        {
            self.status = error;
        }
    }
    fn card_crossfader(&mut self, ui: &mut egui::Ui, width: f32, height: f32) {
        let pair = self.pair();
        ui.allocate_ui_with_layout(
            egui::vec2(width, height),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                panel(ui, None).show(ui, |ui| {
                    ui.set_width(width - 24.);
                    ui.set_min_height(height - 24.);
                    ui.columns(2, |columns| {
                        for (side, d) in pair.into_iter().enumerate() {
                            columns[side].vertical_centered(|ui| {
                                ui.colored_label(
                                    spectrum_color(d, self.rack.spectrum_palette),
                                    NAMES[d],
                                );
                                skin::meter(
                                    ui,
                                    self.audio.as_ref().map_or(0., |a| a.peak(d)),
                                    egui::vec2(20., (height - 190.).clamp(100., 280.)),
                                );
                            });
                        }
                    });
                    ui.vertical_centered(|ui| {
                        ui.small("Crossfade");
                        let fade = ui.add_sized(
                            [width - 24., 32.],
                            egui::Button::new(
                                egui::RichText::new("Auto")
                                    .strong()
                                    .color(egui::Color32::BLACK),
                            )
                            .fill(ORANGE),
                        );
                        #[cfg(test)]
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(
                                egui::Id::unique("dj-auto-fade-bounds").with("screen"),
                                (fade.rect, ui.layer_id()),
                            )
                        });
                        if fade.clicked() {
                            self.auto_crossfade();
                        }
                        ui.spacing_mut().slider_width = width - 26.;
                        if ui
                            .add(egui::Slider::new(&mut self.fader, 0. ..=1.).show_value(false))
                            .changed()
                            && let Some(audio) = &self.audio
                        {
                            self.run.cancel_automation();
                            audio.set_fader(self.fader);
                        }
                        let previous = self.fade_seconds;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.;
                            if ui.small_button("−").clicked() {
                                self.fade_seconds -= 0.5;
                            }
                            ui.small(format!("{:.1}s", self.fade_seconds));
                            if ui.small_button("+").clicked() {
                                self.fade_seconds += 0.5;
                            }
                        });
                        self.fade_seconds = self.fade_seconds.clamp(0.5, 30.);
                        if previous != self.fade_seconds {
                            let _ = crossfade_settings::save(self.fade_seconds);
                        }
                        let headphones = ui.add_sized([width - 24., 34.], egui::Button::new(""));
                        icons::draw(
                            ui,
                            egui::Rect::from_center_size(
                                headphones.rect.center(),
                                egui::vec2(22., 22.),
                            ),
                            icons::Icon::Headphones,
                            skin::foreground(ui),
                        );
                        headphones.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Button,
                                ui.is_enabled(),
                                "Headphones",
                            )
                        });
                        if headphones
                            .on_hover_text("Headphones · choose private cue output")
                            .clicked()
                        {
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(egui::Id::unique("card-headphones-open"), true)
                            });
                        }
                    });
                });
            },
        );
        self.card_headphones_window(ui.ctx());
    }
    fn card_headphones_window(&mut self, ctx: &egui::Context) {
        let key = egui::Id::unique("card-headphones-open");
        let mut open = ctx.data(|data| data.get_temp::<bool>(key)).unwrap_or(false);
        if open {
            egui::Window::new("Headphone output")
                .id(key)
                .open(&mut open)
                .default_width(350.)
                .show(ctx, |ui| self.headphone_output(ui));
            ctx.data_mut(|data| data.insert_temp(key, open));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn screen_rect(ctx: &egui::Context, name: &str, deck: usize) -> egui::Rect {
        let (rect, layer) = ctx
            .data(|d| d.get_temp::<(egui::Rect, egui::LayerId)>(egui::Id::unique((name, deck))))
            .unwrap_or_else(|| panic!("Missing actual layout rectangle {name}/{deck}"));
        ctx.layer_transform_to_global(layer).unwrap_or_default() * rect
    }
    #[test]
    fn two_main_jogs_and_six_quick_tracks_keep_a_visible_selectable_browser() {
        for appearance in [theme::Theme::Classic, theme::Theme::ModernDark] {
            for workspace in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let mut library = amp_core::scan(root.path()).unwrap();
                let path = std::path::PathBuf::from("missing-browser-fixture.wav");
                library.tracks.push(amp_core::Track {
                    path: path.clone(),
                    title: "QA Selectable Browser Song".into(),
                    preparation: Default::default(),
                });
                let ctx = egui::Context::default();
                crate::skin::configure(&ctx);
                theme::apply(&ctx, appearance);
                let mut player = Player {
                    root: root.path().into(),
                    library: Some(library),
                    rack: Rack {
                        six: true,
                        ..Default::default()
                    },
                    workspace: crate::workspace::Workspace {
                        enabled: workspace,
                        pane: 2,
                        rendering: false,
                    },
                    ..Default::default()
                };
                let settings = amp_core::show::Settings::default();
                player.show_editor.new_playlist(&player.root, &settings);
                let drafts = serde_json::to_value(player.show_editor.playlists(&settings)).unwrap();
                let loaded = player.loaded;
                let live = player.rack.live;
                let gains = player.channel_gain;
                let ratios = player.rack.strip_ratios;
                let mut point = egui::Pos2::ZERO;
                let mut row_bounds = egui::Rect::NOTHING;
                let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280., 1100.));
                for frame in 0..6 {
                    let events = match frame {
                        2 => vec![
                            egui::Event::PointerMoved(point),
                            egui::Event::PointerButton {
                                pos: point,
                                button: egui::PointerButton::Primary,
                                pressed: true,
                                modifiers: Default::default(),
                            },
                        ],
                        3 => vec![egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: Default::default(),
                        }],
                        _ => vec![],
                    };
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(window),
                            events,
                            time: Some(frame as f64 / 30.),
                            ..Default::default()
                        },
                        |ui| {
                            if workspace {
                                player.workspace_ui(ui);
                            } else {
                                player.group_ui(ui, 2);
                            }
                        },
                    );
                    output.textures_delta.clear();
                    let (row, layer) = ctx
                        .data(|d| {
                            d.get_temp::<(egui::Rect, egui::LayerId)>(egui::Id::unique(
                                "dj-browser-first-track",
                            ))
                        })
                        .expect("One fixture track must render in the actual browser");
                    let row = ctx.layer_transform_to_global(layer).unwrap_or_default() * row;
                    point = row.center();
                    row_bounds = row;
                    assert!(
                        row.height() >= 27. && window.expand(1.).contains_rect(row),
                        "First selectable browser row must be complete without scrolling workspace={workspace}: {row:?}, viewport{window:?}, bankA{:?}, bankB{:?}",
                        screen_rect(&ctx, "rack-aux-extent", 0),
                        screen_rect(&ctx, "rack-aux-extent", 1)
                    );
                }
                let left = screen_rect(&ctx, "rack-main-extent", 0);
                let right = screen_rect(&ctx, "rack-main-extent", 1);
                assert!(left.right() <= right.left());
                for deck in 0..6 {
                    let slot = screen_rect(&ctx, "rack-aux-extent", deck);
                    assert!(
                        window.expand(1.).contains_rect(slot),
                        "Quick slot{deck}: {slot:?} viewport{window:?} firstrow{row_bounds:?}"
                    );
                    assert!(if deck.is_multiple_of(2) {
                        slot.center().x < left.center().x
                    } else {
                        slot.center().x > right.center().x
                    });
                    for name in [
                        "deck-live-button",
                        "rack-aux-play",
                        "rack-aux-sync",
                        "rack-aux-loop",
                    ] {
                        assert!(
                            slot.expand(1.).contains_rect(screen_rect(&ctx, name, deck)),
                            "Complete {name}/{deck}"
                        );
                    }
                    if deck >= 2 {
                        assert!(
                            ctx.data(|d| d.get_temp::<(egui::Rect, egui::LayerId)>(
                                egui::Id::unique(("rack-jog-drop", deck))
                            ))
                            .is_none(),
                            "Only selected two tracks have jog surfaces"
                        );
                        assert!(
                            ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique((
                                "card-stem-knob",
                                deck,
                                0usize
                            ))))
                            .is_none(),
                            "No mini stem knobs"
                        );
                    }
                }
                assert_eq!(player.library.as_ref().unwrap().tracks.len(), 1);
                assert_eq!(
                    player
                        .selected
                        .and_then(|index| player.library.as_ref()?.tracks.get(index))
                        .map(|track| &track.path),
                    Some(&path),
                    "Visible row click must select the actual song"
                );
                assert_eq!(player.selected, Some(0));
                assert_eq!(player.loaded, loaded);
                assert_eq!(player.rack.live, live);
                assert_eq!(player.channel_gain, gains);
                assert_eq!(player.rack.strip_ratios, ratios);
                assert!(player.audio.is_none());
                assert!(player.show_editor.has_unsaved());
                assert_eq!(
                    serde_json::to_value(player.show_editor.playlists(&settings)).unwrap(),
                    drafts
                );
            }
        }
    }
    #[test]
    fn intermediate_cards_show_complete_actionable_tools_in_both_themes() {
        for appearance in [theme::Theme::Classic, theme::Theme::ModernDark] {
            for height in [296., 329., 369.] {
                let ctx = egui::Context::default();
                crate::skin::configure(&ctx);
                theme::apply(&ctx, appearance);
                let mut player = Player::default();
                player.rack.six = false;
                let root = tempfile::tempdir().unwrap();
                player.root = root.path().into();
                let loaded = player.loaded;
                let live = player.rack.live;
                let ratios = player.rack.strip_ratios;
                let mut stems = egui::Pos2::ZERO;
                for frame in 0..5 {
                    let events = match frame {
                        2 => vec![
                            egui::Event::PointerMoved(stems),
                            egui::Event::PointerButton {
                                pos: stems,
                                button: egui::PointerButton::Primary,
                                pressed: true,
                                modifiers: Default::default(),
                            },
                        ],
                        3 => vec![egui::Event::PointerButton {
                            pos: stems,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: Default::default(),
                        }],
                        _ => vec![],
                    };
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1000., 1024.),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| player.card_main_deck(ui, 0, 480., height),
                    );
                    output.textures_delta.clear();
                    let main = screen_rect(&ctx, "rack-main-extent", 0);
                    for tab in [0u8, 1, 3, 2] {
                        let (rect, layer, clip) = ctx
                            .data(|d| {
                                d.get_temp::<(egui::Rect, egui::LayerId, egui::Rect)>(
                                    egui::Id::unique(("card-visible-tool", 0usize, tab)),
                                )
                            })
                            .expect("Actual named tool must be rendered");
                        let transform = ctx.layer_transform_to_global(layer).unwrap_or_default();
                        let control = transform * rect;
                        assert!(
                            main.expand(1.).contains_rect(control)
                                && (transform * clip).expand(1.).contains_rect(control)
                                && control.height() > 15.,
                            "Tool {tab} clipped: {control:?} card{main:?} clip{clip:?}"
                        );
                        if tab == 1 {
                            stems = control.center();
                        }
                    }
                    let phones = ctx
                        .data(|d| {
                            d.get_temp::<egui::Rect>(egui::Id::unique((
                                "deck-phones-bounds",
                                0usize,
                            )))
                        })
                        .unwrap();
                    assert!(
                        main.expand(1.).contains_rect(phones) && phones.height() >= 37.,
                        "Private cue must remain complete"
                    );
                }
                assert!(
                    ctx.data(|d| d.get_temp::<bool>(tool_key(0)))
                        .unwrap_or(false),
                    "Actual Stems button must open its persistent controls"
                );
                assert_eq!(
                    ctx.data(|d| d.get_temp::<u8>(tool_key(0).with("tab"))),
                    Some(1)
                );
                assert_eq!(player.loaded, loaded);
                assert_eq!(player.rack.live, live);
                assert_eq!(player.rack.strip_ratios, ratios);
                assert!(player.audio.is_none());
            }
        }
    }
    #[test]
    fn responsive_rack_keeps_real_controls_inside_native_and_fitted_windows() {
        for theme in [theme::Theme::Classic, theme::Theme::ModernDark] {
            {
                let six = false;
                for (size, fitted) in [
                    (egui::vec2(640., 900.), false),
                    (egui::vec2(641., 700.), false),
                    (egui::vec2(740., 900.), false),
                    (egui::vec2(1000., 700.), false),
                    (egui::vec2(1000., 1100.), false),
                    (egui::vec2(1280., 900.), false),
                    (egui::vec2(740., 1100.), true),
                    (egui::vec2(1000., 700.), true),
                    (egui::vec2(1280., 900.), true),
                ] {
                    let ctx = egui::Context::default();
                    crate::skin::configure(&ctx);
                    theme::apply(&ctx, theme);
                    let mut player = Player::default();
                    player.rack.six = six;
                    let root = tempfile::tempdir().unwrap();
                    player.root = root.path().into();
                    // Real imported metadata is often much longer than a deck card.
                    // Missing local files prevent waveform workers/audio startup.
                    player.loaded_snapshot = std::array::from_fn(|d| {
                        Some(amp_core::Track {
                            path: format!("unavailable-{d}.mp3").into(),
                            title: format!(
                                "{d} · An intentionally long song title and remix description "
                            )
                            .repeat(3),
                            preparation: amp_core::Preparation {
                                bpm: Some(128.),
                                harmonic_key: Some("8A".into()),
                                manual_override: true,
                                extensions: std::collections::BTreeMap::from([(
                                    "artist".into(),
                                    serde_json::json!(
                                        "A long contributing artist name; second artist; featured guest"
                                    ),
                                )]),
                                ..Default::default()
                            },
                        })
                    });
                    player.playing[0] = true;

                    let ratios = [0.19, 0.23];
                    player.rack.strip_ratios = ratios;
                    let library_id = egui::Id::unique("dj-library-height");
                    ctx.data_mut(|d| d.insert_persisted(library_id, 260f32));
                    for _ in 0..3 {
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    size,
                                )),
                                ..Default::default()
                            },
                            |ui| {
                                if fitted {
                                    player.group_ui(ui, 2);
                                } else {
                                    player.deck_workspace_ui(ui);
                                }
                            },
                        );
                        output.textures_delta.clear();
                    }
                    let a = screen_rect(&ctx, "rack-main-extent", 0);
                    let b = screen_rect(&ctx, "rack-main-extent", 1);
                    assert!(
                        a.right() <= b.left() + 1.,
                        "A stays left of B: {size:?} {a:?} {b:?}"
                    );
                    for (deck, rect) in [(0, a), (1, b)] {
                        assert!(
                            rect.left() >= -1. && rect.right() <= size.x + 1.,
                            "Unclipped card {deck} exceeds actual width {size:?},fitted={fitted},six={six}: {rect:?}"
                        );
                        for name in ["rack-main-play", "deck-cue-bounds"] {
                            let control = ctx
                                .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique((name, deck))))
                                .unwrap();
                            assert!(
                                control.left() >= -1.
                                    && control.right() <= size.x + 1.
                                    && control.bottom() <= size.y,
                                "{name}/{deck} clipped at {size:?}: {control:?}"
                            );
                        }
                    }
                    if six {
                        for d in [2, 3, 4, 5] {
                            let rect = screen_rect(&ctx, "rack-aux-extent", d);
                            assert!(
                                rect.left() >= -1. && rect.right() <= size.x + 1.,
                                "Auxiliary {d} clipped at{size:?}: {rect:?}"
                            );
                            let middle = (a.center().x + b.center().x) / 2.;
                            assert!(
                                if d.is_multiple_of(2) {
                                    rect.center().x < middle
                                } else {
                                    rect.center().x > middle
                                },
                                "Auxiliary deck crossed banks: {d} {rect:?}"
                            );
                        }
                    }
                    let library = ctx
                        .data(|d| d.get_temp::<f32>(egui::Id::unique("rack-library-top")))
                        .unwrap();
                    assert!(
                        library < size.y - 100.,
                        "Library lost at {size:?}, fitted={fitted}, six={six}: top{library}, mainA{a:?}, mainB{b:?}, requested{:?}, mixer{:?}",
                        ctx.data(
                            |d| d.get_temp::<f32>(egui::Id::unique("dj-requested-rack-height"))
                        ),
                        ctx.data(
                            |d| d.get_temp::<(egui::Rect, egui::LayerId)>(egui::Id::unique(
                                "narrow-crossfader-extent"
                            ))
                        )
                    );
                    if let Some((rect, layer)) = ctx.data(|d| {
                        d.get_temp::<(egui::Rect, egui::LayerId)>(egui::Id::unique(
                            "narrow-crossfader-extent",
                        ))
                    }) {
                        let cross = ctx.layer_transform_to_global(layer).unwrap_or_default() * rect;
                        assert!(
                            cross.left() >= -1.
                                && cross.right() <= size.x + 1.
                                && cross.bottom() < library,
                            "Mixer must remain visible above the browser at{size:?}: {cross:?},library{library}"
                        );
                        if six {
                            for d in [2, 3, 4, 5] {
                                let aux = screen_rect(&ctx, "rack-aux-extent", d);
                                assert!(
                                    aux.bottom() <= cross.top() + 1.,
                                    "Auxiliary must not overlap mixer"
                                );
                                for name in [
                                    "rack-aux-select",
                                    "rack-aux-play",
                                    "rack-aux-load",
                                    "rack-aux-tools",
                                ] {
                                    let control = screen_rect(&ctx, name, d);
                                    assert!(
                                        aux.expand(1.).contains_rect(control),
                                        "{name}/{d} exceeds its compact card at{size:?}: {control:?} card{aux:?}"
                                    );
                                }
                            }
                        }
                    }

                    assert_eq!(
                        player.rack.strip_ratios, ratios,
                        "Window resize must not replace saved side widths"
                    );
                    assert_eq!(
                        ctx.data_mut(|d| d.get_persisted::<f32>(library_id)),
                        Some(260.),
                        "Window resize must not replace the browser splitter preference"
                    );
                    assert!(player.audio.is_none());
                }
            }
        }
    }

    #[test]
    fn quick_track_promotion_targets_reach_admission_without_mutating_manual_state() {
        for size in [egui::vec2(1280., 1100.), egui::vec2(1570., 1000.)] {
            for deck in 2..6 {
                let ctx = egui::Context::default();
                crate::skin::configure(&ctx);
                let root = tempfile::tempdir().unwrap();
                let mut player = Player {
                    root: root.path().into(),
                    loaded: std::array::from_fn(Some),
                    playing: [true, false, true, false, true, false],
                    ..Default::default()
                };
                player.rack.six = true;
                let settings = amp_core::show::Settings::default();
                player.show_editor.new_playlist(&player.root, &settings);
                let drafts = serde_json::to_value(player.show_editor.playlists(&settings)).unwrap();
                let state = (
                    player.loaded,
                    player.playing,
                    player.rack.live,
                    player.channel_gain,
                    player.rack.strip_ratios,
                );
                let _lease = player
                    .output_admission
                    .acquire(&std::sync::atomic::AtomicBool::new(false))
                    .unwrap();
                let mut point = egui::Pos2::ZERO;
                for frame in 0..5 {
                    let events = if frame == 2 || frame == 3 {
                        vec![
                            egui::Event::PointerMoved(point),
                            egui::Event::PointerButton {
                                pos: point,
                                button: egui::PointerButton::Primary,
                                pressed: frame == 2,
                                modifiers: Default::default(),
                            },
                        ]
                    } else {
                        vec![]
                    };
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            events,
                            ..Default::default()
                        },
                        |ui| player.card_rack(ui, 470.),
                    );
                    output.textures_delta.clear();
                    let target = screen_rect(&ctx, "deck-live-button", deck);
                    let slot = screen_rect(&ctx, "rack-aux-extent", deck);
                    assert!(slot.expand(1.).contains_rect(target));
                    point = target.center();
                }
                assert!(
                    player.status.contains("Stop listening"),
                    "Actual To Deck hit: {}",
                    player.status
                );
                assert_eq!(
                    (
                        player.loaded,
                        player.playing,
                        player.rack.live,
                        player.channel_gain,
                        player.rack.strip_ratios
                    ),
                    state
                );
                assert_eq!(
                    serde_json::to_value(player.show_editor.playlists(&settings)).unwrap(),
                    drafts
                );
                assert!(player.audio.is_none());
            }
        }
    }
    #[test]
    fn deck_controls_remain_open_after_mouse_selects_stems_tab() {
        let ctx = egui::Context::default();
        let mut player = Player::default();
        player.card_open_tools(&ctx, 0, 0);
        let mut point = egui::Pos2::ZERO;
        for frame in 0..5 {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280., 900.),
                )),
                ..Default::default()
            };
            if frame >= 2 {
                raw.events.push(egui::Event::PointerMoved(point));
                if frame == 2 || frame == 3 {
                    raw.events.push(egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 2,
                        modifiers: Default::default(),
                    });
                }
            }
            let mut output = ctx.run_ui(raw, |ui| player.card_tool_windows(ui.ctx()));
            output.textures_delta.clear();
            if frame == 1 {
                let (rect, layer) = ctx
                    .data(|data| {
                        data.get_temp::<(egui::Rect, egui::LayerId)>(egui::Id::unique((
                            "card-tool-tab",
                            0usize,
                            1u8,
                        )))
                    })
                    .unwrap();
                point = ctx.layer_transform_to_global(layer).unwrap_or_default() * rect.center();
            }
        }
        assert_eq!(
            ctx.data(|data| data.get_temp::<bool>(tool_key(0))),
            Some(true)
        );
        assert_eq!(
            ctx.data(|data| data.get_temp::<u8>(tool_key(0).with("tab"))),
            Some(1)
        );
        assert!(
            player.playing.iter().all(|playing| !*playing),
            "Opening controls must not start audio"
        );
    }
    #[test]
    #[ignore = "requires ffmpeg and a real clocked silent audio output (CI PulseAudio null sink); no stem model"]
    fn actual_live_button_preserves_three_clocked_sources_and_manual_state() {
        use std::sync::atomic::Ordering::Relaxed;
        use std::time::{Duration, Instant};
        let root = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for deck in 0..3 {
            let path = root.path().join(format!("live-button-{deck}.wav"));
            // Small standard PCM RIFF fixture, without an additional crate.
            let frames = 44100u32 * 12;
            let data_bytes = frames * 4;
            let mut wav = Vec::with_capacity(data_bytes as usize + 44);
            wav.extend_from_slice(b"RIFF");
            wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            wav.extend_from_slice(&16u32.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&2u16.to_le_bytes());
            wav.extend_from_slice(&44100u32.to_le_bytes());
            wav.extend_from_slice(&176400u32.to_le_bytes());
            wav.extend_from_slice(&4u16.to_le_bytes());
            wav.extend_from_slice(&16u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&data_bytes.to_le_bytes());
            for frame in 0..frames {
                let phase =
                    frame as f32 * std::f32::consts::TAU * (220. + deck as f32 * 110.) / 44100.;
                let sample = (phase.sin() * 1600.) as i16;
                wav.extend_from_slice(&sample.to_le_bytes());
                wav.extend_from_slice(&sample.to_le_bytes());
            }
            std::fs::write(&path, wav).unwrap();
            paths.push(path);
        }
        let mut player = Player {
            root: root.path().into(),
            rack: Rack {
                six: true,
                ..Default::default()
            },
            ..Default::default()
        };
        for (deck, path) in paths.into_iter().enumerate() {
            player.safe_load_path(deck, path, None).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        while !(0..3).all(|deck| player.audio.as_ref().unwrap().controls.ready[deck].load(Relaxed))
        {
            assert!(
                Instant::now() < deadline,
                "Three real decoded sources did not become ready within 8s: {}",
                player.status
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        for deck in 0..3 {
            player.toggle_deck(deck);
        }
        player.fader = 0.37;
        {
            let audio = player.audio.as_ref().unwrap();
            audio.set_fader(player.fader);
            for (deck, gain) in [0.63, 0.71, 0.82].into_iter().enumerate() {
                audio.set_gain(deck, gain);
            }
        }
        while !(0..3).all(|deck| {
            let audio = player.audio.as_ref().unwrap();
            audio.playing(deck) && audio.source_position(deck) > 0.05
        }) {
            assert!(
                Instant::now() < deadline,
                "Clocked silent output must actually advance all three decks within 8s"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        while (f32::from_bits(
            player
                .audio
                .as_ref()
                .unwrap()
                .controls
                .crossfader
                .load(Relaxed),
        ) - 0.37)
            .abs()
            > 0.0001
        {
            assert!(
                Instant::now() < deadline,
                "Clocked mixer did not apply initial fader"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let settings = amp_core::show::Settings::default();
        player.show_editor.new_playlist(&player.root, &settings);
        let drafts = serde_json::to_value(player.show_editor.playlists(&settings)).unwrap();
        let snapshots: Vec<_> = player
            .loaded_snapshot
            .iter()
            .map(|t| t.as_ref().map(|t| t.path.clone()))
            .collect();
        let loaded = player.loaded;
        let ratios = player.rack.strip_ratios;
        player.run.auto = true;
        player.auto = true;
        let before_time = Instant::now();
        let audio = player.audio.as_ref().unwrap();
        let before_positions: [f64; 3] = std::array::from_fn(|d| audio.source_position(d));
        let generations: [u64; 3] =
            std::array::from_fn(|d| audio.controls.deck_generation[d].load(Relaxed));
        let gains: [u32; 3] = std::array::from_fn(|d| audio.controls.gain[d].load(Relaxed));
        let fader = audio.controls.crossfader.load(Relaxed);
        assert_eq!(audio.controls.pair_indices(), [0, 1]);
        let ctx = egui::Context::default();
        crate::skin::configure(&ctx);
        let size = egui::vec2(1570., 1100.);
        let mut point = egui::Pos2::ZERO;
        for frame in 0..5 {
            let events = match frame {
                2 => vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ],
                3 => vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
                _ => vec![],
            };
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ui| player.card_rack(ui, 1070.),
            );
            output.textures_delta.clear();
            let target = screen_rect(&ctx, "deck-live-button", 2);
            let card = screen_rect(&ctx, "rack-aux-extent", 2);
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            assert!(
                viewport.contains_rect(card)
                    && viewport.contains_rect(target)
                    && card.contains_rect(target),
                "A2 To Deck and its complete mini card must be visible before clicking: {target:?} card{card:?} viewport{viewport:?}"
            );
            point = target.center();
        }
        while player.audio.as_ref().unwrap().controls.pair_indices() != [2, 1] {
            assert!(
                Instant::now() < deadline,
                "Actual A2 LIVE click did not reach clocked engine: {}",
                player.status
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let continuity_until = Instant::now() + Duration::from_millis(250);
        while Instant::now() < continuity_until {
            assert!(
                Instant::now() < deadline,
                "LIVE integration exceeded bounded 8s wait"
            );
            let audio = player.audio.as_ref().unwrap();
            for deck in 0..3 {
                assert!(audio.playing(deck), "LIVE must not pause deck{deck}");
                assert_eq!(
                    audio.controls.deck_generation[deck].load(Relaxed),
                    generations[deck],
                    "LIVE must not reload deck{deck}"
                );
                assert_eq!(
                    audio.controls.gain[deck].load(Relaxed),
                    gains[deck],
                    "LIVE must not reset gain{deck}"
                );
            }
            assert_eq!(
                audio.controls.crossfader.load(Relaxed),
                fader,
                "LIVE must not reset fader"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let elapsed = before_time.elapsed().as_secs_f64();
        let audio = player.audio.as_ref().unwrap();
        for (deck, before_position) in before_positions.into_iter().enumerate() {
            let position = audio.source_position(deck);
            assert!(
                position > before_position + 0.1 && position <= before_position + elapsed + 0.25,
                "Deck{deck} must advance continuously without seeks: before{} after{position} elapsed{elapsed}",
                before_position
            );
        }
        assert_eq!(player.rack.live, [2, 1]);
        assert!(
            !player.run.auto && !player.auto,
            "Direct LIVE is manual takeover"
        );
        assert_eq!(player.loaded, loaded);
        assert_eq!(player.rack.strip_ratios, ratios);
        assert_eq!(
            player
                .loaded_snapshot
                .iter()
                .map(|t| t.as_ref().map(|t| t.path.clone()))
                .collect::<Vec<_>>(),
            snapshots
        );
        assert!(player.show_editor.has_unsaved());
        assert_eq!(
            serde_json::to_value(player.show_editor.playlists(&settings)).unwrap(),
            drafts
        );
    }
}

#[cfg(test)]
mod clear_deck_tests {
    use super::*;
    #[test]
    fn clear_deck_removes_only_the_requested_track_and_pending_resume() {
        for deck in 0..6 {
            let mut player = Player {
                loaded: [Some(42); 6],
                playing: [true; 6],
                ..Default::default()
            };
            player.pending_load[deck] = Some((PathBuf::from("pending.mp3"), None));
            player.jog_resume[deck] = Some((true, egui::ViewportId::ROOT));
            let live = player.rack.live;
            player.clear_deck(deck);
            for other in 0..6 {
                assert_eq!(
                    player.loaded[other],
                    if other == deck { None } else { Some(42) }
                );
                assert_eq!(player.playing[other], other != deck);
            }
            assert!(player.pending_load[deck].is_none());
            assert!(player.jog_resume[deck].is_none());
            assert_eq!(player.rack.live, live);
        }
    }
}
