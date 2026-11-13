use super::*;

pub struct Workspace {
    pub enabled: bool,
    pub pane: usize,
    pub rendering: bool,
}
impl Default for Workspace {
    fn default() -> Self {
        let enabled = std::env::var_os("AMP_V1_MODE").is_some();
        let pane = std::env::var("AMP_SCREENSHOT_VIEW")
            .ok()
            .and_then(|view| match view.as_str() {
                "v1-dj" => Some(2),
                "v1-player" => Some(0),
                _ => None,
            })
            .unwrap_or(1);
        Self {
            enabled,
            pane,
            rendering: false,
        }
    }
}
impl Player {
    fn workspace_player(&mut self, ui: &mut egui::Ui) {
        self.player_ui(ui);
        ui.add_space(12.);
        let track = self.loaded_track(0).cloned();
        let position = self
            .audio
            .as_ref()
            .map_or(0., |audio| audio.source_position(0));
        self.waveforms[0].height = Some(110.);
        self.waveforms[0].show(
            ui,
            self.loaded_origin[0].as_deref().unwrap_or(&self.root),
            track.as_ref(),
            position,
            self.wave_span,
            0,
        );
        ui.add_space(12.);
        ui.horizontal(|ui| {
            ui.heading("Play queue");
            ui.weak(format!("{} songs", self.queue.len()));
            if ui.button("Browse music").clicked() {
                self.workspace.pane = 1;
            }
        });
        let mut play = None;
        egui::ScrollArea::vertical()
            .id_salt("v1-player-queue")
            .max_height(ui.available_height().max(40.))
            .show(ui, |ui| {
                if self.queue.is_empty() {
                    ui.weak("Choose songs or a playlist in Library to build your queue.");
                }
                if let Some(library) = &self.library {
                    for (order, index) in self.queue.iter().enumerate() {
                        if let Some(track) = library.tracks.get(*index)
                            && ui
                                .add_sized(
                                    [ui.available_width(), 32.],
                                    egui::Button::new(format!("{}   {}", order + 1, track.title))
                                        .selected(self.loaded[0] == Some(*index)),
                                )
                                .clicked()
                        {
                            play = Some(*index);
                        }
                    }
                }
            });
        if let Some(index) = play {
            self.play(0, index);
        }
    }
    pub(super) fn workspace_ui(&mut self, ui: &mut egui::Ui) {
        self.header(
            ui,
            match self.workspace.pane {
                0 => "PLAYER",
                1 => "AMP Library",
                2 => "AMP DJ",
                3 => "AMP Lab",
                4 => "AMP Cast",
                _ => "Plugin Manager",
            },
            false,
        );
        ui.horizontal_wrapped(|ui| {
            for (pane, label) in [
                (0, "Player"),
                (1, "Library"),
                (2, "DJ"),
                (3, "Lab"),
                (4, "Cast & Remote"),
                (5, "Add-ons"),
            ] {
                if ui
                    .add_sized(
                        [104., 36.],
                        egui::Button::new(label).selected(self.workspace.pane == pane),
                    )
                    .clicked()
                {
                    self.workspace.pane = pane;
                    self.lab_open = pane == 3;
                }
            }
            ui.weak("1.0.13 Preview");
            if ui
                .button("Classic windows")
                .on_hover_text("Return to the separate player and dockable windows")
                .clicked()
            {
                self.workspace.enabled = false;
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(400., 150.)));
            }
        });
        ui.separator();
        let content_height = (ui.available_height() - 70.).max(60.);
        let content = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(ui.available_width(), content_height),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(content.intersect(ui.clip_rect()));
        self.workspace.rendering = true;
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("v1-workspace-content"), true));
        window_layout::set_embedded(ui.ctx(), true);
        if self.workspace.pane == 0 {
            self.workspace_player(&mut child);
        } else if self.workspace.pane == 2 {
            let available = child.available_size().max(egui::vec2(1., 1.));
            let scale = (available.x / 980.).min(available.y / 850.).min(1.);
            window_layout::fitted_content(
                &mut child,
                "v1-dj-workspace",
                available / scale,
                scale,
                |ui| self.pane_ui(ui, 2),
            );
        } else {
            self.pane_ui(&mut child, self.workspace.pane);
        }
        window_layout::set_embedded(ui.ctx(), false);
        self.workspace.rendering = false;
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("v1-workspace-content"), false));
        ui.advance_cursor_after_rect(content);
        ui.separator();
        ui.horizontal(|ui| {
            let owner = self.playback_owner;
            ui.label(
                RichText::new(if owner == playback_owner::Owner::Dj {
                    "DJ OUTPUT"
                } else {
                    "PLAYER OUTPUT"
                })
                .color(GOLD),
            );
            for (label, action) in [
                (
                    "Play",
                    control::Action::Play {
                        deck: control::Deck::Auto,
                    },
                ),
                (
                    "Pause",
                    control::Action::Pause {
                        deck: control::Deck::Auto,
                    },
                ),
                (
                    "Stop",
                    control::Action::Stop {
                        deck: control::Deck::Auto,
                    },
                ),
            ] {
                if ui.add_sized([80., 32.], egui::Button::new(label)).clicked()
                    && let Err(error) = self.execute_control(action)
                {
                    self.status = error;
                }
            }
            if ui
                .add_sized([90., 32.], egui::Button::new("Next"))
                .clicked()
            {
                let action = if owner == playback_owner::Owner::Dj {
                    control::Action::MixNext { immediate: true }
                } else {
                    control::Action::PlayerNext
                };
                if let Err(error) = self.execute_control(action) {
                    self.status = error;
                }
            }
            let deck = if owner == playback_owner::Owner::Player {
                0
            } else {
                self.pair()[usize::from(self.fader > 0.5)]
            };
            let title = self
                .loaded_track(deck)
                .map(|track| track.title.as_str())
                .unwrap_or("No track loaded");
            ui.add(egui::Label::new(title).truncate())
                .on_hover_text(&self.status);
            #[cfg(test)]
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::unique("v1-transport-bounds"), ui.min_rect())
            });
        });
        window_layout::resize_edges(ui);
        let id = egui::Id::unique("quick-cast-open");
        let mut open = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        if open {
            egui::Window::new("Cast current audio")
                .id(egui::Id::unique("quick-cast-window"))
                .open(&mut open)
                .default_width(480.)
                .resizable(true)
                .vscroll(true)
                .show(ui.ctx(), |ui| self.cast.quick_cast(ui));
            ui.ctx().data_mut(|d| d.insert_temp(id, open));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changing_workspaces_keeps_playback_owner_and_transport_inside_window() {
        for size in [
            egui::vec2(760., 540.),
            egui::vec2(1280., 900.),
            egui::vec2(1920., 1080.),
        ] {
            let ctx = egui::Context::default();
            let mut player = Player {
                workspace: Workspace {
                    enabled: true,
                    pane: 1,
                    rendering: false,
                },
                playback_owner: playback_owner::Owner::Dj,
                ..Default::default()
            };
            for pane in [1, 2, 0, 5] {
                let raw = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(raw, |ui| {
                    let module = match pane {
                        0 => skin::Module::Verse,
                        1 => skin::Module::Library,
                        2 => skin::Module::Dj,
                        _ => skin::Module::Addons,
                    };
                    player.activate(ui, Some(module));
                    player.workspace_ui(ui);
                });
                assert_eq!(player.workspace.pane, pane);
                assert_eq!(player.playback_owner, playback_owner::Owner::Dj);
                let bounds = ctx
                    .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("v1-transport-bounds")))
                    .unwrap();
                assert!(
                    bounds.bottom() <= size.y && bounds.right() <= size.x,
                    "{pane} at {size:?}: {bounds:?}"
                );
                assert!(
                    !output.viewport_output[&egui::ViewportId::ROOT]
                        .commands
                        .iter()
                        .any(|command| matches!(command, egui::ViewportCommand::InnerSize(_))),
                    "Navigation must not resize the native window"
                );
                output.textures_delta.clear();
            }
        }
    }
}
