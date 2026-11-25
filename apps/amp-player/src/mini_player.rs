use super::*;
use crate::deck_rack::NAMES;
#[derive(Default)]
pub(super) struct State {
    pub(super) minimized: bool,
    pub(super) dismissed: bool,
    dj: bool,
    side: usize,
}
impl State {
    fn visible(&mut self, minimized: bool, dj: bool) -> bool {
        if minimized && !self.minimized {
            self.dismissed = false;
            self.dj = dj;
        }
        self.minimized = minimized;
        minimized && !self.dismissed
    }
}
pub(super) fn root_minimized(ctx: &egui::Context) -> bool {
    ctx.input(|i| {
        i.raw
            .viewports
            .get(&egui::ViewportId::ROOT)
            .and_then(|v| v.minimized)
            .unwrap_or(false)
    })
}
impl Player {
    pub(super) fn minimized_controls(&mut self, ctx: &egui::Context) {
        let dj = if self.workspace.enabled {
            self.workspace.pane == 2
        } else {
            self.playback_owner == playback_owner::Owner::Dj
        };
        if !self.mini.visible(root_minimized(ctx), dj) {
            return;
        }
        let dj = self.mini.dj;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("amp-minimized-controls"),
            egui::ViewportBuilder::default()
                .with_title(if dj {
                    "AMP Verse Mini DJ"
                } else {
                    "AMP Verse Mini Player"
                })
                .with_icon(app_icon())
                .with_decorations(false)
                .with_inner_size(if dj { [400., 600.] } else { [580., 230.] })
                .with_min_inner_size(if dj { [360., 480.] } else { [500., 220.] })
                .with_window_level(egui::WindowLevel::AlwaysOnTop)
                .with_clamp_size_to_monitor_size(true),
            |ui, _| {
                if ui.input(|i| i.viewport().close_requested()) {
                    self.mini.dismissed = true;
                    return;
                }
                ui.horizontal(|ui| {
                    let header = ui.add(
                        egui::Label::new(
                            RichText::new(if dj { "AMP DJ" } else { "AMP PLAYER" }).strong(),
                        )
                        .sense(egui::Sense::drag()),
                    );
                    if header.drag_started() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if ui.button("Full window").clicked() {
                        ui.ctx().send_viewport_cmd_to(
                            egui::ViewportId::ROOT,
                            egui::ViewportCommand::Minimized(false),
                        );
                        ui.ctx().send_viewport_cmd_to(
                            egui::ViewportId::ROOT,
                            egui::ViewportCommand::Focus,
                        );
                    }
                    if ui
                        .button("X")
                        .on_hover_text("Hide compact controls; keep playing")
                        .clicked()
                    {
                        self.mini.dismissed = true;
                    }
                });
                ui.separator();
                ui.spacing_mut().slider_width = 100.;
                if dj {
                    self.mini_dj_ui(ui);
                } else {
                    self.workspace_player_track_display(ui);
                    ui.horizontal(|ui| {
                        self.workspace_player_buttons(ui);
                        ui.add(egui::Slider::new(&mut self.volume, 0..=100).text("Volume"));
                    });
                }
                self.release_jog_wheels(ui.ctx());
                ui.ctx().request_repaint_after(Duration::from_millis(33));
            },
        );
    }
    fn mini_dj_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                for side in 0..2 {
                    let d = self.rack.live[side];
                    ui.selectable_value(
                        &mut self.mini.side,
                        side,
                        format!("Deck {} - {}", if side == 0 { "A" } else { "B" }, NAMES[d]),
                    );
                }
            });
            let d = self.rack.live[self.mini.side];
            let color = if self.mini.side == 0 {
                ORANGE
            } else {
                egui::Color32::from_rgb(0, 205, 230)
            };
            ui.colored_label(color, RichText::new(self.title(d)).size(19.).strong());
            let pos = self.audio.as_ref().map_or(0., |a| a.source_position(d));
            ui.label(format!("{:02}:{:02}", pos as u64 / 60, pos as u64 % 60));
            ui.horizontal(|ui| {
                if ui
                    .button(if self.playing[d] { "Pause" } else { "Play" })
                    .clicked()
                {
                    self.toggle_deck(d);
                }
                if ui.button("Stop").clicked() {
                    self.stop(d);
                }
                if ui.button("Sync").clicked() {
                    self.sync_deck(d);
                }
                if ui.button("Loop").clicked() {
                    self.rack_loop(d);
                }
            });
            ui.vertical_centered(|ui| {
                self.rack_platter(ui, d, 180.);
            });
            ui.horizontal(|ui| {
                ui.label("A");
                ui.spacing_mut().slider_width = (ui.available_width() - 40.).max(100.);
                if ui
                    .add(egui::Slider::new(&mut self.fader, 0. ..=1.).show_value(false))
                    .changed()
                    && let Some(a) = &self.audio
                {
                    a.set_fader(self.fader);
                }
                ui.label("B");
            });
            ui.add(egui::Slider::new(&mut self.volume, 0..=100).text("Master"));
            ui.small("Hot cues");
            self.main_hot_cues_count(ui, d, color, 4);
            ui.menu_button("Sampler", |ui| self.side_panel(ui, d % 2, 330., 250., true));
            let mut auto = self.run.auto;
            if ui.checkbox(&mut auto, "Auto Mix").changed()
                && let Err(error) = self.execute_control(control::Action::AutoDj { enabled: auto })
            {
                self.status = error;
            }
            if ui.button("Mix next").clicked()
                && let Err(error) =
                    self.execute_control(control::Action::MixNext { immediate: false })
            {
                self.status = error;
            }
            ui.weak("Full window opens your six tracks, library and all DJ settings.");
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn companion_follows_minimize_restore_and_dismissal() {
        let mut s = State::default();
        assert!(!s.visible(false, false));
        assert!(s.visible(true, true));
        assert!(s.dj);
        s.dismissed = true;
        assert!(!s.visible(true, false));
        assert!(!s.visible(false, false));
        assert!(s.visible(true, false));
        assert!(!s.dj);
    }
}

#[cfg(test)]
mod viewport_tests {
    use super::*;
    #[test]
    fn minimized_status_comes_from_root_even_when_companion_has_focus() {
        let ctx = egui::Context::default();
        let mut raw = egui::RawInput::default();
        raw.viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .minimized = Some(true);
        let mut output = ctx.run_ui(raw, |ui| assert!(root_minimized(ui.ctx())));
        output.textures_delta.clear();
        let mut raw = egui::RawInput::default();
        raw.viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .minimized = Some(false);
        let mut output = ctx.run_ui(raw, |ui| assert!(!root_minimized(ui.ctx())));
        output.textures_delta.clear();
    }
}
