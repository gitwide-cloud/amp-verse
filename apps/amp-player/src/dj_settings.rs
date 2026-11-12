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
            ui.heading("Music selection");
            ui.weak("Select a saved Smart Playlist. Choosing it here does not interrupt the playing song.");
            let mut selected=self.run.choice.clone();
            egui::ScrollArea::vertical().id_salt("dj-settings-playlists").max_height(130.).show(ui,|ui|{
                for list in &self.run.settings.playlists {ui.selectable_value(&mut selected,list.id.clone(),&list.name);}
            });
            if selected!=self.run.choice {self.dj_browser.select_smart(&selected);self.run.choice=selected;}
            if ui.add_enabled(!self.run.choice.is_empty(),egui::Button::new("Load selected playlist")).clicked()
                && let Err(error)=self.execute_control(control::Action::LoadSmartPlaylist{playlist:self.run.choice.clone(),execution:control::Execution::LoadOnly,style:None,energy:None}) {self.status=error;}
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
            ui.horizontal_wrapped(|ui|{ui.selectable_value(&mut self.rack.six,true,"Six-deck strips");ui.selectable_value(&mut self.rack.six,false,"Original two-deck view");});
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
