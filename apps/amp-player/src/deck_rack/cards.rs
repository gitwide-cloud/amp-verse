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
    fn card_live_button(&mut self, ui: &mut egui::Ui, deck: usize) {
        let live = self.pair()[deck % 2] == deck;
        let response = ui.add_sized([48., 26.], egui::Button::new(
            egui::RichText::new("LIVE").strong().color(if live { egui::Color32::BLACK } else { accent(deck) }))
            .fill(if live { accent(deck) } else { ui.visuals().widgets.inactive.bg_fill }))
            .on_hover_text("Assign this deck to its crossfader side. Takes over Auto Mix without pausing, seeking or starting a deck.");
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Button,
                true,
                format!(
                    "{} Live{}",
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
        if response.clicked() {
            self.make_deck_live(deck);
        }
    }
    fn card_six_decks(&mut self, ui: &mut egui::Ui, height: f32) {
        if ui.available_width() < 660. {
            egui::ScrollArea::horizontal()
                .id_salt("six-full-bank-overflow")
                .max_height(height + 12.)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_min_width(660.);
                    self.card_six_banks(ui, height);
                });
        } else {
            self.card_six_banks(ui, height);
        }
    }
    fn card_six_banks(&mut self, ui: &mut egui::Ui, height: f32) {
        #[cfg(test)]
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::unique("native-dj-card-layout"), true));
        let gap = 8.;
        let width = (ui.available_width() - 24.).max(640.);
        let center = 94.;
        let main = (width - center - gap * 2.) / 2.;
        let row = ((height - gap * 2.) / 3.).max(232.);
        ui.spacing_mut().item_spacing.x = gap;
        ui.horizontal_top(|ui| {
            // Stable identities: A1/A2/A3 left; B1/B2/B3 right. No deck is
            // replaced by a tiny selector when a different one becomes LIVE.
            for side in 0..2 {
                if side == 1 {
                    self.card_crossfader(ui, center, height);
                }
                ui.allocate_ui_with_layout(
                    egui::vec2(main, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let bank = egui::ScrollArea::vertical()
                            .id_salt(("six-full-decks", side))
                            .max_height(height)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = gap;
                                for deck in (side..6).step_by(2) {
                                    ui.push_id(("six-full-deck", deck), |ui| {
                                        self.card_main_deck(ui, deck, main - 10., row)
                                    });
                                }
                            });
                        #[cfg(test)]
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(
                                egui::Id::unique(("six-bank-viewport", side)),
                                (bank.inner_rect, ui.layer_id()),
                            )
                        });
                        #[cfg(not(test))]
                        let _ = bank;
                    },
                );
            }
        });
    }
    pub(super) fn card_rack(&mut self, ui: &mut egui::Ui, height: f32) {
        if self.rack.six {
            self.card_six_decks(ui, height);
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
        let auxiliary_height = if self.rack.six {
            if short_native { 42. } else { 64. }
        } else {
            0.
        };
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
        let decks: Vec<_> = (0..6)
            .filter(|d| d % 2 == side && !self.rack.live.contains(d))
            .collect();
        let short_native = (ui
            .ctx()
            .layer_transform_to_global(ui.layer_id())
            .unwrap_or_default()
            * ui.clip_rect())
        .height()
            < 760.;
        let mini_width = (width - 8.) / 2.;
        ui.allocate_ui_with_layout(egui::vec2(width, if short_native {0.} else {64.}), egui::Layout::left_to_right(egui::Align::Min), |ui| {
            for d in decks {
                ui.push_id(("strip", d), |ui| {
                    let frame = panel(ui, None).inner_margin(egui::Margin::symmetric(10,6)).show(ui, |ui| {
                        ui.set_width(mini_width - 24.);
                        ui.spacing_mut().interact_size.x = 26.;
                        ui.spacing_mut().button_padding.x = 2.;
                        ui.spacing_mut().item_spacing.x = 3.;
                        ui.spacing_mut().item_spacing.y = 3.;
                        ui.horizontal(|ui| {
                            let select = ui.selectable_label(false, egui::RichText::new(NAMES[d]).strong().color(accent(d)))
                                .on_hover_text("Show this deck's vinyl and controls; audio routing stays unchanged");
                            #[cfg(test)] ui.ctx().data_mut(|data| data.insert_temp(egui::Id::unique(("rack-aux-select", d)), (select.rect, ui.layer_id())));
                            select.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button,true,format!("{}: {}. Show deck controls",NAMES[d],self.title(d))));
                            let select = select.on_hover_text(self.title(d));
                            if select.clicked() { self.rack.live[side] = d; }
                            let play = transport_button(ui, d, self.playing[d], egui::vec2(26., 26.));
                            if play.clicked() { self.toggle_deck(d); }
                            let load = ui.add_enabled(self.selected.is_some(), egui::Button::new("").min_size(egui::vec2(26.,26.))).on_hover_text("Load the selected song on this deck without starting it");
                            load.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button,self.selected.is_some(),"Load selected song"));
                            icons::draw(ui,egui::Rect::from_center_size(load.rect.center(),egui::vec2(22.,22.)),icons::Icon::Folder,skin::foreground(ui).gamma_multiply(if self.selected.is_some(){1.}else{0.4}));
                            if load.clicked() && let Some(index) = self.selected { self.load_deck(d, index, false); }
                            let tools = ui.small_button("···").on_hover_text("Deck controls, stems and sampler");
                            if tools.clicked() { self.card_open_tools(ui.ctx(), d, 0); }
                            #[cfg(test)] ui.ctx().data_mut(|data| {
                                for (name, rect) in [("rack-aux-play",play.rect),("rack-aux-load",load.rect),("rack-aux-tools",tools.rect)] {
                                    data.insert_temp(egui::Id::unique((name, d)), (rect, ui.layer_id()));
                                }
                            });
                        });
                        if !short_native {ui.add(egui::Label::new(egui::RichText::new(self.title(d)).size(15.)).truncate());}
                    });
                    #[cfg(test)] ui.ctx().data_mut(|data| {
                        data.insert_temp(egui::Id::unique(("rack-strip-drop", d)), (frame.response.interact_rect, frame.response.layer_id));
                        data.insert_temp(egui::Id::unique(("rack-aux-extent", d)), (frame.response.rect, frame.response.layer_id));
                    });
                    if let Some(path) = dragdrop::target(&frame.response, accent(d)) && let Err(error) = self.safe_load_path(d, path, None) { self.status = error; }
                });
            }
        });
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
                    ui.colored_label(accent(pair[0]), NAMES[pair[0]]);
                    ui.spacing_mut().slider_width = (width - 420.).max(140.);
                    if ui
                        .add(egui::Slider::new(&mut self.fader, 0. ..=1.).show_value(false))
                        .changed()
                        && let Some(audio) = &self.audio
                    {
                        self.run.cancel_automation();
                        audio.set_fader(self.fader);
                    }
                    ui.colored_label(accent(pair[1]), NAMES[pair[1]]);
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
            ui.selectable_value(&mut self.rack.six, true, "6 decks");
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
        ui.allocate_ui_with_layout(egui::vec2(width,height),egui::Layout::top_down(egui::Align::Min),|ui|{
            ui.vertical(|ui|{
            ui.spacing_mut().item_spacing.y=8.;let live=self.rack.live;let mut decks:Vec<_>=(0..6).filter(|d|d%2==side&&!live.contains(d)).collect();decks.sort_by_key(|d|(d%2,d/2));
            for d in decks {ui.push_id(("strip",d),|ui|{
                let frame=panel(ui,None).show(ui,|ui|{
                    ui.set_width((width-24.).max(100.));ui.spacing_mut().item_spacing.y=3.;ui.spacing_mut().interact_size.y=22.;ui.set_min_height(((height-8.)/2.-24.).clamp(100.,160.));
                    ui.horizontal(|ui|{if ui.selectable_label(false,egui::RichText::new(NAMES[d]).strong().color(accent(d))).on_hover_text("Show this deck on its main vinyl control; audio routing stays unchanged").clicked(){self.rack.live[d%2]=d;}
                        if transport_button(ui,d,self.playing[d],egui::vec2(22.,22.)).clicked(){self.toggle_deck(d);}
                        if ui.small_button("···").on_hover_text("Open this deck's controls").clicked(){self.card_open_tools(ui.ctx(),d,0);}
                    });
                    ui.add(egui::Label::new(egui::RichText::new(self.title(d)).size(15.)).truncate());
                    let track=self.loaded_snapshot[d].as_ref().or_else(||self.loaded[d].and_then(|i|self.library.as_ref()?.tracks.get(i)));
                    if let Some(at)=self.waveforms[d].thumbnail(ui,self.loaded_origin[d].as_deref().unwrap_or(&self.root),track,self.audio.as_ref().map_or(0.,|a|a.source_position(d)),accent(d),16.){self.select_deck_section(d,at);}
                    if let Some(delta)=self.waveforms[d].scrub.take(){self.seek_deck_relative(d,delta);}
                    ui.columns(3,|cols|{for (column,stem,label) in [(0,0,"VOC"),(1,2,"BEAT"),(2,1,"INST")] {cols[column].vertical_centered(|ui|self.card_stem_dial(ui,d,stem,label));}});
                });
                #[cfg(test)]ui.ctx().data_mut(|data|{
                    data.insert_temp(egui::Id::unique(("rack-strip-drop",d)),(frame.response.interact_rect,frame.response.layer_id));
                    data.insert_temp(egui::Id::unique(("rack-aux-extent",d)),(frame.response.rect,frame.response.layer_id));
                });
                if let Some(path)=dragdrop::target(&frame.response,accent(d))&&let Err(error)=self.safe_load_path(d,path,None){self.status=error;}
            });}
            });
        });
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
                        let six_card = self.rack.six;
                        let compact_card = width < 1000.;
                        let short_controls = height < 280.;
                        let minimum_wheel = if six_card {
                            86.
                        } else if short_controls {
                            140.
                        } else {
                            160.
                        };
                        let wheel = (height
                            - if six_card {
                                150.
                            } else if compact_card {
                                222.
                            } else {
                                180.
                            })
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
                                            egui::RichText::new(NAMES[d])
                                                .size(22.)
                                                .strong()
                                                .color(accent(d)),
                                        );
                                        self.card_live_button(ui, d);
                                        if six_card {
                                            let load = ui.add_enabled(self.selected.is_some(), egui::Button::new("Load").min_size(egui::vec2(48., 26.))).on_hover_text("Load the selected library song on this deck without starting it").on_disabled_hover_text("Select a song in the DJ Library first");
                                            #[cfg(test)] ui.ctx().data_mut(|data| data.insert_temp(egui::Id::unique(("deck-load-button", d)), (load.rect, ui.layer_id())));
                                            if load.clicked() && let Some(index) = self.selected { self.load_deck(d, index, false); }
                                        }
                                        if info >= 160.
                                            && ui
                                                .small_button("···")
                                                .on_hover_text("Keep deck controls open")
                                                .clicked()
                                        {
                                            self.card_open_tools(ui.ctx(), d, 0);
                                        }
                                    });
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(self.title(d)).size(18.).strong(),
                                        )
                                        .truncate(),
                                    );
                                    let track = self.loaded_track(d);
                                    let artist = track
                                        .and_then(|t| t.preparation.extensions.get("artist"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("Artist unavailable");
                                    if width >= 400. && !six_card {
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
                            self.card_transport(ui, d, width - 24., short_controls && !six_card);
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
                                if six_card
                                    && ui
                                        .small_button("···")
                                        .on_hover_text("Load, cues, sampler and deck tools")
                                        .clicked()
                                {
                                    self.card_open_tools(ui.ctx(), d, 0);
                                }
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
                            if six_card {
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
                                return;
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
                        if height < 370. && !six_card {
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
                                ui.colored_label(accent(d), NAMES[d]);
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
    fn six_full_cards_keep_bank_parity_and_equal_direct_controls() {
        for appearance in [theme::Theme::Classic, theme::Theme::ModernDark] {
            for size in [
                egui::vec2(1280., 1100.),
                egui::vec2(1570., 1000.),
                egui::vec2(1000., 700.),
                egui::vec2(740., 480.),
            ] {
                let ctx = egui::Context::default();
                crate::skin::configure(&ctx);
                theme::apply(&ctx, appearance);
                let mut player = Player::default();
                player.rack.six = true;
                let root = tempfile::tempdir().unwrap();
                player.root = root.path().into();
                let saved = player.rack.strip_ratios;
                let live = player.rack.live;
                for _ in 0..3 {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ui| player.card_rack(ui, size.y - 30.),
                    );
                    output.textures_delta.clear();
                }
                let main: Vec<_> = (0..6)
                    .map(|d| screen_rect(&ctx, "rack-main-extent", d))
                    .collect();
                for row in 0..3 {
                    let a = main[row * 2];
                    let b = main[row * 2 + 1];
                    assert!(
                        a.right() < b.left(),
                        "A/B bank parity lost at{size:?}: {a:?} {b:?}"
                    );
                    assert!(
                        (a.width() - b.width()).abs() < 1. && (a.height() - b.height()).abs() < 1.,
                        "Each side must expose equal full cards"
                    );
                    assert!(
                        a.left() >= -1. && b.right() <= size.x + 1.,
                        "Full cards clipped horizontally at{size:?}: {a:?} {b:?}"
                    );
                    if row > 0 {
                        assert!(
                            main[(row - 1) * 2].bottom() <= a.top()
                                && main[(row - 1) * 2 + 1].bottom() <= b.top(),
                            "Cards overlap vertically"
                        );
                    }
                }
                for (deck, card) in main.iter().enumerate() {
                    for name in ["deck-live-button", "deck-load-button"] {
                        let button = screen_rect(&ctx, name, deck);
                        assert!(
                            card.expand(1.).contains_rect(button),
                            "{name}/{deck} escapes fullcard at{size:?}"
                        );
                        assert!(
                            button.width() >= 47. && button.height() >= 25.,
                            "LIVE target shrunk"
                        );
                    }
                    for name in ["rack-main-play", "deck-cue-bounds"] {
                        let button = ctx
                            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique((name, deck))))
                            .unwrap();
                        assert!(
                            card.expand(1.).contains_rect(button) && button.height() >= 37.,
                            "{name}/{deck} must remain a full card transport"
                        );
                    }
                    let (_, layer) = ctx
                        .data(|d| {
                            d.get_temp::<(egui::Rect, egui::LayerId)>(egui::Id::unique((
                                "rack-main-extent",
                                deck,
                            )))
                        })
                        .unwrap();
                    let transform = ctx.layer_transform_to_global(layer).unwrap_or_default();
                    for stem in 0usize..3 {
                        let knob = ctx
                            .data(|d| {
                                d.get_temp::<egui::Rect>(egui::Id::unique((
                                    "card-stem-knob",
                                    deck,
                                    stem,
                                )))
                            })
                            .expect("All six cards require three direct stem knobs");
                        let knob = transform * knob;
                        assert!(
                            card.expand(1.).contains_rect(knob)
                                && knob.width() > 20.
                                && knob.height() > 20.,
                            "Stem{stem}/{deck} must be complete in its card at{size:?}: {knob:?} {card:?}"
                        );
                    }
                }
                assert_eq!(player.rack.strip_ratios, saved);
                assert_eq!(player.rack.live, live);
                assert!(player.audio.is_none());
            }
        }
    }
    #[test]
    fn six_full_card_live_targets_are_reachable_without_starting_audio() {
        for size in [
            egui::vec2(1280., 1100.),
            egui::vec2(1570., 1000.),
            egui::vec2(740., 480.),
        ] {
            for deck in 0..6 {
                let ctx = egui::Context::default();
                crate::skin::configure(&ctx);
                let mut player = Player::default();
                player.rack.six = true;
                let root = tempfile::tempdir().unwrap();
                player.root = root.path().into();
                player.loaded = std::array::from_fn(Some);
                player.playing = [true, false, true, false, true, false];
                let settings = amp_core::show::Settings::default();
                player.show_editor.new_playlist(&player.root, &settings);
                let drafts = serde_json::to_value(player.show_editor.playlists(&settings)).unwrap();
                let loaded = player.loaded;
                let playing = player.playing;
                let live = player.rack.live;
                let ratios = player.rack.strip_ratios;
                let _lease = player
                    .output_admission
                    .acquire(&std::sync::atomic::AtomicBool::new(false))
                    .unwrap();
                let mut target = egui::Rect::NOTHING;
                let mut card = egui::Rect::NOTHING;
                for frame in 0..64 {
                    let mut events = vec![];
                    // Give smooth wheel corrections 0.4s of simulated settling time.
                    if frame >= 2 && frame < 56 && frame % 8 == 2 {
                        let bank = screen_rect(&ctx, "six-bank-viewport", deck % 2);
                        if !bank.expand(1.).contains_rect(card) {
                            let delta = (bank.center().y - card.center().y).clamp(-150., 150.);
                            events = vec![
                                egui::Event::PointerMoved(bank.center()),
                                egui::Event::MouseWheel {
                                    unit: egui::MouseWheelUnit::Point,
                                    phase: egui::TouchPhase::Move,
                                    source: egui::MouseWheelSource::Unknown,
                                    delta: egui::vec2(0., delta),
                                    modifiers: Default::default(),
                                },
                            ];
                        }
                    }
                    if frame == 60 || frame == 61 {
                        events = vec![
                            egui::Event::PointerMoved(target.center()),
                            egui::Event::PointerButton {
                                pos: target.center(),
                                button: egui::PointerButton::Primary,
                                pressed: frame == 60,
                                modifiers: Default::default(),
                            },
                        ];
                    }
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            events,
                            time: Some(frame as f64 / 20.),
                            ..Default::default()
                        },
                        |ui| player.card_rack(ui, size.y - 30.),
                    );
                    output.textures_delta.clear();
                    target = screen_rect(&ctx, "deck-live-button", deck);
                    card = screen_rect(&ctx, "rack-main-extent", deck);
                    if frame == 58 {
                        let bank = screen_rect(&ctx, "six-bank-viewport", deck % 2);
                        assert!(
                            bank.expand(1.).contains_rect(card),
                            "Whole card{deck} must be reachable after bank scroll at{size:?}: {card:?} bank{bank:?}"
                        );
                        for stem in 0usize..3 {
                            let knob = ctx
                                .data(|d| {
                                    d.get_temp::<egui::Rect>(egui::Id::unique((
                                        "card-stem-knob",
                                        deck,
                                        stem,
                                    )))
                                })
                                .unwrap();
                            assert!(
                                bank.expand(1.).contains_rect(knob),
                                "Stem{stem}/{deck} must be fully visible when its full card is reached"
                            );
                        }
                        assert!(
                            bank.expand(1.).contains_rect(target),
                            "LIVE{deck} must be fully reachable after its bank scroll at{size:?}: {target:?} bank{bank:?}"
                        );
                    }
                }
                assert!(
                    player.status.contains("Stop listening"),
                    "Actual LIVE{deck} click must reach output-admission gate: {}",
                    player.status
                );
                assert_eq!(player.loaded, loaded);
                assert_eq!(player.playing, playing);
                assert_eq!(player.rack.live, live);
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
    fn six_full_banks_fit_the_actual_workspace_width() {
        for size in [
            egui::vec2(1280., 1100.),
            egui::vec2(1570., 1000.),
            egui::vec2(740., 480.),
        ] {
            let ctx = egui::Context::default();
            crate::skin::configure(&ctx);
            let mut player = Player::default();
            player.rack.six = true;
            player.workspace.enabled = true;
            player.workspace.pane = 2;
            for _ in 0..3 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| player.workspace_ui(ui),
                );
                output.textures_delta.clear();
            }
            for deck in 0..6 {
                let rect = screen_rect(&ctx, "rack-main-extent", deck);
                assert!(
                    rect.left() >= -1. && rect.right() <= size.x + 1.,
                    "Full deck{deck} clipped in workspace at{size:?}: {rect:?}; bankA={:?}; bankB={:?}; cardA1={:?}; cardB1={:?}; rawBankA={:?}; rawBankB={:?}; bankTransforms={:?}; cardTransforms={:?}; content={:?}; viewport={:?}",
                    screen_rect(&ctx, "six-bank-viewport", 0),
                    screen_rect(&ctx, "six-bank-viewport", 1),
                    screen_rect(&ctx, "rack-main-extent", 0),
                    screen_rect(&ctx, "rack-main-extent", 1),
                    ctx.data(
                        |d| d.get_temp::<(egui::Rect, egui::LayerId)>(egui::Id::unique((
                            "six-bank-viewport",
                            0usize
                        )))
                    ),
                    ctx.data(
                        |d| d.get_temp::<(egui::Rect, egui::LayerId)>(egui::Id::unique((
                            "six-bank-viewport",
                            1usize
                        )))
                    ),
                    (0usize..2)
                        .map(|side| {
                            let (_, layer) = ctx
                                .data(|d| {
                                    d.get_temp::<(egui::Rect, egui::LayerId)>(egui::Id::unique((
                                        "six-bank-viewport",
                                        side,
                                    )))
                                })
                                .unwrap();
                            ctx.layer_transform_to_global(layer)
                        })
                        .collect::<Vec<_>>(),
                    (0usize..2)
                        .map(|side| {
                            let (_, layer) = ctx
                                .data(|d| {
                                    d.get_temp::<(egui::Rect, egui::LayerId)>(egui::Id::unique((
                                        "rack-main-extent",
                                        side,
                                    )))
                                })
                                .unwrap();
                            ctx.layer_transform_to_global(layer)
                        })
                        .collect::<Vec<_>>(),
                    ctx.content_rect(),
                    ctx.viewport_rect()
                );
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
        let mut player = Player::default();
        player.root = root.path().into();
        player.rack.six = true;
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
            let bank = screen_rect(&ctx, "six-bank-viewport", 0);
            assert!(
                bank.expand(1.).contains_rect(target),
                "A2 LIVE must be fully visible and clickable: {target:?} bank{bank:?}"
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
        for deck in 0..3 {
            let position = audio.source_position(deck);
            assert!(
                position > before_positions[deck] + 0.1
                    && position <= before_positions[deck] + elapsed + 0.25,
                "Deck{deck} must advance continuously without seeks: before{} after{position} elapsed{elapsed}",
                before_positions[deck]
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
