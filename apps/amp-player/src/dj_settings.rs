//! One settings window for top-level DJ choices; no nested selection menus.
use super::*;
impl Player {
    pub(super) fn open_dj_settings(&self, ctx: &egui::Context) {
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::unique("dj-settings-open"), true);
        });
    }
    pub(super) fn dj_settings_window(&mut self, ctx: &egui::Context) {
        let id = egui::Id::unique("dj-settings-open");
        let mut open = ctx.data_mut(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        if !open {
            return;
        }
        egui::Window::new("DJ settings").id(egui::Id::unique("dj-settings-window")).open(&mut open).default_width(760.).default_height(680.).max_width((ctx.viewport_rect().width()-32.).max(260.)).max_height((ctx.viewport_rect().height()-60.).max(200.)).resizable(true).vscroll(true).show(ctx,|ui|{
            ui.heading("Auto Mix source");
            if let Some((kind, name)) = self.loaded_auto_playlist_info() {
                ui.label(format!("Playlist | {kind} | {name}"));
            } else {
                ui.weak("Load a playlist for Auto Mix from the DJ Library below.");
            }
            if ui.add_enabled(self.run.auto,egui::Button::new("Mix Next")).clicked()
                && let Err(error)=self.execute_control(control::Action::MixNext{immediate:true}) { self.status=error; }
            ui.separator();ui.heading("Mixing style");self.performance_mix_options(ui);
            ui.separator();ui.heading("Song length");
            let old=self.run.transitions;
            ui.horizontal_wrapped(|ui|{for pace in [amp_core::dj_phrasing::Pace::Quick,amp_core::dj_phrasing::Pace::Balanced,amp_core::dj_phrasing::Pace::Full] {ui.selectable_value(&mut self.run.transitions.pace,pace,pace.label());}});
            ui.weak("Quick changes songs sooner. Balanced uses section/phrase estimates. Full track keeps the prepared end. Manual cue-outs take priority.");
            if old!=self.run.transitions && let Err(error)=transition_styles::save(self.run.transitions){self.status=error;}
            ui.separator();ui.heading("Crossfade");ui.weak("Auto Mix handles crossfading automatically. Set the mix duration below.");
            let old=self.fade_seconds;
            ui.horizontal_wrapped(|ui|{ui.label("Mix duration");if ui.button("−").clicked(){self.fade_seconds-=0.5;}ui.add(egui::DragValue::new(&mut self.fade_seconds).range(0.5..=30.).speed(0.1).suffix(" seconds"));if ui.button("+").clicked(){self.fade_seconds+=0.5;}});
            self.fade_seconds=self.fade_seconds.clamp(0.5,30.);
            if old!=self.fade_seconds && let Err(error)=crossfade_settings::save(self.fade_seconds){self.status=error.to_string();}
            ui.separator();
            egui::CollapsingHeader::new("Transitions and effects").show(ui, |ui| {
                let previous = self.run.transitions;
                if transition_controls::inline(ui, &mut self.run.transitions, &mut self.wave_span,
                    self.run.auto && self.run.pending.is_none() && !self.audio.as_ref().is_some_and(|a| a.fading())) {
                    self.run.force_mashup=true;
                    if let Err(error)=self.execute_control(control::Action::AutoFade) {self.run.force_mashup=false;self.status=error;}
                }
                if previous!=self.run.transitions && let Err(error)=transition_styles::save(self.run.transitions) {self.status=error;}
                self.run.scratch_assist=self.run.transitions.enabled;
            });
            ui.separator();ui.heading("Tempo and quantization");self.clock_settings(ui);
            ui.separator();ui.heading("Waveforms");
            let old=self.rack.overlay_waveforms;
            ui.horizontal_wrapped(|ui|{ui.selectable_value(&mut self.rack.overlay_waveforms,false,"Stacked A / B");ui.selectable_value(&mut self.rack.overlay_waveforms,true,"Overlay blue / red");});
            if old!=self.rack.overlay_waveforms && let Some(path)=deck_rack::waveform_preference(){
                let result=std::fs::create_dir_all(path.parent().unwrap()).and_then(|()|std::fs::write(path,if self.rack.overlay_waveforms {"overlay"}else{"stacked"}));if let Err(error)=result {self.status=error.to_string();}
            }
            ui.add(egui::Slider::new(&mut self.wave_span,4. ..=60.).text("Visible seconds"));
            ui.separator();ui.heading("Layout");
            skin::switch(ui,&mut self.rack.expanded,"Expanded controls");
            self.card_view_tools(ui);
            if ui.button("Assign crossfader to LIVE pair").on_hover_text("Pause both current and requested deck pairs first; stop Auto DJ.").clicked(){
                self.status=if self.run.auto {"Stop Auto DJ before changing the crossfade pair".into()}else if let Err(error)=self.ensure_audio().and_then(|_|self.audio.as_ref().unwrap().assign_pair(self.rack.live)){error}else{"Crossfade assignment queued".into()};
            }
            ui.separator();ui.weak(&self.status);
        });
        ctx.data_mut(|d| {
            d.insert_temp(id, open);
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn status_frame(
        ctx: &egui::Context,
        player: &mut Player,
        events: Vec<egui::Event>,
    ) -> (String, egui::Rect) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280., 900.),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                ui.horizontal_wrapped(|ui| player.performance_controls(ui));
            },
        );
        let text = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        output.textures_delta.clear();
        let gear = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("dj-status-settings-button")))
            .unwrap();
        (text, gear)
    }
    #[test]
    fn status_shows_only_loaded_source_and_gear_does_not_load_or_start_music() {
        let ctx = egui::Context::default();
        let mut player = Player::default();
        player.run.choice = "remembered-choice".into();
        player.dj_browser.select_smart("remembered-choice");
        let (text, _) = status_frame(&ctx, &mut player, vec![]);
        assert!(text.contains("Auto Mix") && text.contains("DJ Settings"));
        assert!(!text.contains("Playlist") && !text.contains("remembered-choice"));
        let mut profile = player.run.settings.playlists[0].clone();
        profile.name = "R&B loaded snapshot".into();
        player.run.profile = Some(profile);
        let (text, gear) = status_frame(&ctx, &mut player, vec![]);
        assert!(
            text.contains("Playlist")
                && text.contains("Smart")
                && text.contains("R&B loaded snapshot")
        );
        assert!(
            !text.contains("remembered-choice")
                && !text.contains("Mix Next")
                && !text.contains("Waveforms")
                && !text.contains("Load")
        );
        let owner = player.playback_owner;
        for pressed in [true, false] {
            status_frame(
                &ctx,
                &mut player,
                vec![
                    egui::Event::PointerMoved(gear.center()),
                    egui::Event::PointerButton {
                        pos: gear.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert!(
            ctx.data(|d| d.get_temp::<bool>(egui::Id::unique("dj-settings-open")))
                .unwrap_or(false)
        );
        assert!(!player.run.auto);
        assert_eq!(player.playback_owner, owner);
        assert!(player.loaded.iter().all(Option::is_none));
        assert_eq!(
            player.run.profile.as_ref().unwrap().name,
            "R&B loaded snapshot"
        );
        player.run.load_regular_playlist("Wedding order".into());
        let (text, _) = status_frame(&ctx, &mut player, vec![]);
        assert!(
            text.contains("Regular")
                && text.contains("Wedding order")
                && !text.contains("R&B loaded snapshot")
        );
    }
    #[test]
    fn remembered_browser_choice_cannot_start_an_unloaded_auto_mix() {
        let ctx = egui::Context::default();
        let mut player = Player::default();
        player.run.choice = player.run.settings.playlists[0].id.clone();
        player.dj_browser.select_smart(&player.run.choice.clone());
        status_frame(&ctx, &mut player, vec![]);
        let rect = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("auto-dj-switch")))
            .unwrap();
        for pressed in [true, false] {
            status_frame(
                &ctx,
                &mut player,
                vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert!(!player.run.auto && player.run.profile.is_none());
        assert!(player.status.contains("DJ Library first"));
    }
    #[test]
    fn opening_settings_does_not_change_mix_or_resize_the_deck_layout() {
        let ctx = egui::Context::default();
        let mut player = Player::default();
        let settings = player.run.transitions;
        let fade = player.fade_seconds;
        player.open_dj_settings(&ctx);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 700.),
                )),
                ..Default::default()
            },
            |ui| {
                let before = ui.next_widget_position();
                player.dj_settings_window(ui.ctx());
                assert_eq!(before, ui.next_widget_position());
            },
        );
        output.textures_delta.clear();
        assert_eq!(player.run.transitions, settings);
        assert_eq!(player.fade_seconds, fade);
        assert!(!player.run.auto);
    }
}
