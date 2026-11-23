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
    pub(super) fn card_manual_playlist_controls(&mut self, ui: &mut egui::Ui) {
        let selected = self.run.choice.clone();
        let name = self
            .run
            .settings
            .playlists
            .iter()
            .find(|p| p.id == selected)
            .map(|p| p.name.clone());
        let has_music = self
            .library
            .as_ref()
            .is_some_and(|library| !library.tracks.is_empty());
        let ready = has_music && name.is_some();
        let label = name.as_deref().unwrap_or("Choose Smart Playlist");
        if ui
            .add_sized([170., 36.], egui::Button::new(label).truncate())
            .on_hover_text("Choose a saved Smart Playlist")
            .clicked()
        {
            self.open_dj_settings(ui.ctx());
        }
        if ui
            .add_enabled(ready, egui::Button::new("Load"))
            .on_disabled_hover_text("Choose a playlist and add music in Library first")
            .clicked()
            && let Err(error) = self.execute_control(control::Action::LoadSmartPlaylist {
                playlist: selected,
                execution: control::Execution::LoadOnly,
                style: None,
                energy: None,
            })
        {
            self.status = error;
        }
        if !has_music {
            ui.weak("Add music in Library");
        }
    }
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
    pub(super) fn card_rack(&mut self, ui: &mut egui::Ui, height: f32) {
        // Leave a visible outer gutter for fractional stroke/pixel rounding.
        let total = (ui.available_width() - 8.).max(1.);
        #[cfg(test)]
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::unique("native-dj-card-layout"), true));
        let gap = 8.;
        let center = if total < 1250. { 94. } else { 106. };
        let gaps = if self.rack.six { 6. } else { 4. };
        let dividers = if self.rack.six { 20. } else { 10. };
        let sidebar_budget = (total - 520. - center - gap * gaps - dividers)
            .max(if self.rack.six { 240. } else { 112. });
        let mut aux = if self.rack.six {
            (total * self.rack.strip_ratios[0])
                .clamp(128., (sidebar_budget - 112.).clamp(128., 280.))
        } else {
            0.
        };
        let mut tools = (total * self.rack.strip_ratios[1])
            .clamp(112., (sidebar_budget - aux).clamp(112., 260.));
        let old = [aux, tools];
        let main = ((total - aux - tools - center - gap * gaps - dividers) / 2.).max(260.);
        ui.spacing_mut().item_spacing.x = gap;
        ui.horizontal_top(|ui| {
            if self.rack.six {
                self.card_auxiliary(ui, aux, height);
                window_layout::splitter(
                    ui,
                    "dj-deck-left-divider",
                    &mut aux,
                    128. ..=(sidebar_budget - tools).clamp(128., 280.),
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
            window_layout::splitter(
                ui,
                "dj-deck-right-divider",
                &mut tools,
                112. ..=(sidebar_budget - aux).clamp(112., 260.),
                true,
                -1.,
                height,
            );
            self.card_sidebar(ui, tools, height);
        });
        if self.rack.six && (aux - old[0]).abs() > 0.01 {
            self.rack.strip_ratios[0] = aux / total;
        }
        if (tools - old[1]).abs() > 0.01 {
            self.rack.strip_ratios[1] = tools / total;
        }
    }
    fn card_auxiliary(&mut self, ui: &mut egui::Ui, width: f32, height: f32) {
        ui.allocate_ui_with_layout(egui::vec2(width,height),egui::Layout::top_down(egui::Align::Min),|ui|{
            egui::ScrollArea::vertical().id_salt("auxiliary-deck-cards").max_height(height).auto_shrink([false,false]).show(ui,|ui|{
            ui.spacing_mut().item_spacing.y=8.;let live=self.rack.live;let mut decks:Vec<_>=(0..6).filter(|d|!live.contains(d)).collect();decks.sort_by_key(|d|(d%2,d/2));
            for d in decks {ui.push_id(("strip",d),|ui|{
                let frame=panel(ui,None).show(ui,|ui|{
                    ui.set_width((width-24.).max(100.));ui.spacing_mut().item_spacing.y=3.;ui.spacing_mut().interact_size.y=22.;ui.set_min_height(((height-24.)/4.-20.).max(62.));
                    ui.horizontal(|ui|{if ui.selectable_label(false,egui::RichText::new(NAMES[d]).strong().color(accent(d))).on_hover_text("Show this deck on its main vinyl control; audio routing stays unchanged").clicked(){self.rack.live[d%2]=d;}
                        if transport_button(ui,d,self.playing[d],egui::vec2(22.,22.)).clicked(){self.toggle_deck(d);}
                        if ui.small_button("···").on_hover_text("Open this deck's controls").clicked(){self.card_open_tools(ui.ctx(),d,0);}
                    });
                    ui.add(egui::Label::new(egui::RichText::new(self.title(d)).size(12.)).truncate());
                    let track=self.loaded_snapshot[d].as_ref().or_else(||self.loaded[d].and_then(|i|self.library.as_ref()?.tracks.get(i)));
                    if let Some(at)=self.waveforms[d].thumbnail(ui,self.loaded_origin[d].as_deref().unwrap_or(&self.root),track,self.audio.as_ref().map_or(0.,|a|a.source_position(d)),accent(d),16.){self.select_deck_section(d,at);}
                    if let Some(delta)=self.waveforms[d].scrub.take(){self.seek_deck_relative(d,delta);}
                });
                #[cfg(test)]ui.ctx().data_mut(|data|data.insert_temp(egui::Id::unique(("rack-strip-drop",d)),(frame.response.interact_rect,frame.response.layer_id)));
                if let Some(path)=dragdrop::target(&frame.response,accent(d))&&let Err(error)=self.safe_load_path(d,path,None){self.status=error;}
            });}
            });
        });
    }
    fn card_transport(&mut self, ui: &mut egui::Ui, d: usize, width: f32) {
        let transport = ui.horizontal(|ui| {
            let button = ((width - ui.spacing().item_spacing.x * 3.) / 4.).max(25.);
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
    fn card_tempo(&mut self, ui: &mut egui::Ui, d: usize) {
        ui.horizontal(|ui| {
            ui.small("Pitch");
            if ui
                .add(
                    egui::DragValue::new(&mut self.pitch[d])
                        .speed(0.1)
                        .range(-16. ..=16.)
                        .suffix("%"),
                )
                .changed()
                && let Some(audio) = &self.audio
            {
                let _ = audio.sync(d, false);
                audio.set_rate(d, 1. + self.pitch[d] / 100.);
            }
        });
    }
    fn card_main_deck(&mut self, ui: &mut egui::Ui, d: usize, width: f32, height: f32) {
        let area = ui.allocate_ui_with_layout(
            egui::vec2(width, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                panel(ui, Some(accent(d))).show(ui, |ui| {
                    ui.set_width(width - 24.);
                    ui.set_min_height(height - 24.);
                    ui.spacing_mut().item_spacing.y = 3.;
                    let compact_card = width < 540.;
                    let wheel = (height - if compact_card { 222. } else { 180. })
                        .clamp(130., 280.)
                        .min((width * 0.43).max(130.));
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
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(NAMES[d])
                                            .size(22.)
                                            .strong()
                                            .color(accent(d)),
                                    );
                                    ui.label(
                                        egui::RichText::new(if self.playing[d] {
                                            "LIVE"
                                        } else if self.loaded_track(d).is_some() {
                                            "PAUSED"
                                        } else {
                                            "EMPTY"
                                        })
                                        .small()
                                        .strong()
                                        .color(accent(d)),
                                    );
                                    if ui
                                        .small_button("···")
                                        .on_hover_text("Keep deck controls open")
                                        .clicked()
                                    {
                                        self.card_open_tools(ui.ctx(), d, 0);
                                    }
                                });
                                ui.add(
                                    egui::Label::new(egui::RichText::new(self.title(d)).strong())
                                        .truncate(),
                                );
                                let track = self.loaded_track(d);
                                let artist = track
                                    .and_then(|t| t.preparation.extensions.get("artist"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("Artist unavailable");
                                if !compact_card {
                                    ui.add(egui::Label::new(artist).truncate());
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
                                ui.small(format!(
                                    "{} / {}",
                                    time(position),
                                    duration.map_or("—:—".into(), time)
                                ));
                                if compact_card {
                                    self.card_tempo(ui, d);
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
                                    self.card_transport(ui, d, info);
                                }
                            },
                        );
                        self.rack_platter(ui, d, wheel);
                        if !compact_card {
                            self.rack_pitch(ui, d, wheel);
                        }
                    });
                    if compact_card {
                        self.card_transport(ui, d, width - 24.);
                    }
                    ui.separator();
                    let knobs = ui.scope(|ui| {
                        ui.columns(5, |columns| {
                            for (band, label) in [(0, "LOW"), (1, "MID"), (2, "HIGH")] {
                                columns[band].vertical_centered(|ui| {
                                    ui.spacing_mut().interact_size.y = 34.;
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
                            columns[4].vertical_centered(|ui| self.deck_headphones_button(ui, d));
                        });
                    });
                    #[cfg(test)]
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(egui::Id::unique(("rack-knobs", d)), knobs.response.rect);
                        data.insert_temp(
                            egui::Id::unique(("rack-knobs", d)).with("screen"),
                            (knobs.response.rect, ui.layer_id()),
                        );
                    });
                    #[cfg(not(test))]
                    let _ = knobs;
                    ui.separator();
                    // Keep saved banks restored even when the compact card opens
                    // their pads in a panel: keyboard sampler actions stay ready.
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
                                    if self.sampler.has_saved() && self.ensure_audio().is_ok() {
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
                });
            },
        );
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::unique(("rack-jog-drop", d)),
                (area.response.interact_rect, area.response.layer_id),
            )
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
        let key = egui::Id::unique("card-headphones-open");
        let mut open = ui
            .ctx()
            .data(|data| data.get_temp::<bool>(key))
            .unwrap_or(false);
        if open {
            egui::Window::new("Headphone output")
                .id(key)
                .open(&mut open)
                .default_width(350.)
                .show(ui.ctx(), |ui| self.headphone_output(ui));
            ui.ctx().data_mut(|data| data.insert_temp(key, open));
        }
    }
    fn card_sidebar(&mut self, ui: &mut egui::Ui, width: f32, height: f32) {
        ui.allocate_ui_with_layout(
            egui::vec2(width, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                panel(ui, None).show(ui, |ui| {
                    ui.set_width(width - 24.);
                    ui.set_min_height(height - 24.);
                    egui::ScrollArea::vertical()
                        .id_salt("deck-card-tools")
                        .max_height((height - 24.).max(80.))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.strong("Deck View");
                            if ui.selectable_label(!self.rack.six, "A / B main").clicked() {
                                self.rack.six = false;
                            }
                            if ui.selectable_label(self.rack.six, "6 decks").clicked() {
                                self.rack.six = true;
                            }
                            ui.separator();
                            let mut side = ui
                                .ctx()
                                .data(|data| {
                                    data.get_temp::<usize>(egui::Id::unique("card-tools-side"))
                                })
                                .unwrap_or(0);
                            ui.horizontal(|ui| {
                                ui.selectable_value(&mut side, 0, "A");
                                ui.selectable_value(&mut side, 1, "B");
                            });
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(egui::Id::unique("card-tools-side"), side)
                            });
                            let deck = self.rack.live[side];
                            for (label, tab) in [
                                ("Effects", 0),
                                ("Stems", 1),
                                ("Controls", 0),
                                ("Advanced", 3),
                            ] {
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 34.],
                                        egui::Button::new(label),
                                    )
                                    .clicked()
                                {
                                    self.card_open_tools(ui.ctx(), deck, tab);
                                }
                            }
                            ui.separator();
                            ui.small(format!("{} selected", NAMES[deck]));
                            ui.small(
                                "Deck view changes controls only; audio routing stays unchanged.",
                            );
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
}
