use eframe::egui::{self, Color32 as C, FontId, Rect, RichText, Stroke, Vec2};
pub const GOLD: C = C::from_rgb(255, 194, 26);
pub const GREEN: C = C::from_rgb(130, 245, 61);
pub const ORANGE: C = C::from_rgb(255, 170, 28);
pub const CYAN: C = C::from_rgb(36, 208, 235);
pub const BLACK: C = C::from_rgb(7, 8, 12);

pub fn configure(ctx: &egui::Context) {
    crate::theme::apply(ctx, crate::theme::load());
}
pub fn accent(ui: &egui::Ui) -> C {
    crate::theme::current(ui.ctx()).palette().accent
}
pub fn signal(ui: &egui::Ui) -> C {
    crate::theme::current(ui.ctx()).palette().signal
}
pub fn foreground(ui: &egui::Ui) -> C {
    crate::theme::current(ui.ctx()).palette().text
}
pub fn color(ui: &egui::Ui, value: C) -> C {
    if value == GOLD {
        return accent(ui);
    }
    if value == GREEN {
        return signal(ui);
    }
    if crate::theme::current(ui.ctx()) == crate::theme::Theme::ModernLight {
        if value == ORANGE {
            return C::from_rgb(150, 75, 0);
        }
        if value == CYAN {
            return C::from_rgb(0, 105, 128);
        }
    }
    value
}

fn gradient(painter: &egui::Painter, rect: Rect, top: C, bottom: C) -> egui::Shape {
    let mut mesh = egui::Mesh::default();
    for (pos, color) in [
        (rect.left_top(), top),
        (rect.right_top(), top),
        (rect.left_bottom(), bottom),
        (rect.right_bottom(), bottom),
    ] {
        mesh.colored_vertex(pos, color);
    }
    mesh.indices = vec![0, 1, 2, 2, 1, 3];
    let _ = painter;
    egui::Shape::mesh(mesh)
}

pub fn panel<R>(ui: &mut egui::Ui, purple: bool, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let theme = crate::theme::current(ui.ctx());
    let p = theme.palette();
    let bg = ui.painter().add(egui::Shape::Noop);
    let response = egui::Frame::new()
        .corner_radius(if theme == crate::theme::Theme::Classic {
            2
        } else {
            10
        })
        .inner_margin(8)
        .stroke(Stroke::new(1., p.border))
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), body)
                .inner
        });
    let rect = response.response.rect;
    let (top, bottom) = if theme != crate::theme::Theme::Classic {
        (p.surface, p.surface)
    } else {
        (p.elevated, p.surface)
    };
    if theme == crate::theme::Theme::Classic || (purple && theme == crate::theme::Theme::ModernDark)
    {
        ui.painter().set(
            bg,
            gradient(
                ui.painter(),
                rect,
                if purple { p.elevated } else { top },
                bottom,
            ),
        );
    } else {
        ui.painter()
            .set(bg, egui::Shape::rect_filled(rect, 10., p.surface));
    }
    if theme == crate::theme::Theme::Classic {
        let frame = rect.shrink(2.);
        ui.painter().rect_stroke(
            frame,
            2.,
            Stroke::new(2., C::from_rgb(12, 10, 21)),
            egui::StrokeKind::Inside,
        );
        ui.painter().rect_stroke(
            rect.shrink(4.),
            1.,
            Stroke::new(1., p.border),
            egui::StrokeKind::Inside,
        );
        ui.painter().line_segment(
            [
                rect.left_top() + Vec2::splat(2.),
                rect.right_top() + Vec2::new(-2., 2.),
            ],
            Stroke::new(2., C::from_rgb(195, 189, 208)),
        );
        ui.painter().line_segment(
            [
                rect.left_bottom() + Vec2::new(3., -2.),
                rect.right_bottom() + Vec2::new(-3., -2.),
            ],
            Stroke::new(2., C::from_rgb(9, 8, 16)),
        );
    }
    response.inner
}

/// Recessed audio display, shared by the Classic player and modern palettes.
pub fn display<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = crate::theme::current(ui.ctx()).palette();
    egui::Frame::new()
        .fill(
            if crate::theme::current(ui.ctx()) == crate::theme::Theme::Classic {
                BLACK
            } else {
                p.background
            },
        )
        .stroke(Stroke::new(2., p.border))
        .inner_margin(10)
        .show(ui, body)
        .inner
}

#[derive(Clone, Copy)]
pub struct Magnet {
    pub enabled: bool,
    pub attached: bool,
    pub clicked: bool,
}

pub enum Module {
    Verse,
    Library,
    Dj,
    Cast,
    Addons,
    LabSetup,
}

pub fn heading(ui: &mut egui::Ui, title: &str, purple: bool) -> Option<Module> {
    heading_mode(ui, title, purple, !crate::window_layout::embedded(ui.ctx()))
}
pub fn heading_mode(ui: &mut egui::Ui, title: &str, _purple: bool, native: bool) -> Option<Module> {
    let theme = crate::theme::current(ui.ctx());
    let p = theme.palette();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 42.), egui::Sense::hover());
    ui.painter().add(gradient(
        ui.painter(),
        rect,
        p.elevated,
        if theme == crate::theme::Theme::Classic {
            BLACK
        } else {
            p.surface
        },
    ));
    ui.painter().rect_stroke(
        rect,
        3.,
        Stroke::new(1., p.border),
        egui::StrokeKind::Inside,
    );
    bolt(
        ui.painter(),
        Rect::from_min_size(rect.min + Vec2::new(12., 7.), Vec2::new(19., 28.)),
        p.accent,
    );
    ui.painter().text(
        rect.min + Vec2::new(38., 21.),
        egui::Align2::LEFT_CENTER,
        if title == "AMP DJ" {
            "AMP DJ"
        } else {
            "AMP VERSE"
        },
        FontId::proportional(21.),
        p.text,
    );
    let menu_rect = Rect::from_min_max(rect.min, egui::pos2(rect.left() + 220., rect.bottom()));
    let center_left = rect.left() + 235.;
    let center_right = rect.right() - 155.;
    let scope = if title == "AMP DJ" {
        ui.ctx()
            .data(|data| data.get_temp::<Vec<f32>>(egui::Id::unique("six-deck-header-scope")))
    } else {
        None
    };
    if let Some(scope) = scope {
        let area = Rect::from_min_max(
            egui::pos2(center_left, rect.top() + 6.),
            egui::pos2(center_right, rect.bottom() - 7.),
        );
        ui.painter().rect_filled(area, 3., BLACK);
        for (index, level) in scope.iter().enumerate() {
            let x = area.left() + index as f32 * area.width() / scope.len().max(1) as f32;
            let height = level.sqrt() * (area.height() - 3.);
            ui.painter().rect_filled(
                Rect::from_min_size(
                    egui::pos2(x, area.bottom() - height),
                    Vec2::new(
                        (area.width() / scope.len().max(1) as f32 - 1.).max(1.),
                        height.max(1.),
                    ),
                ),
                0.,
                p.accent,
            );
        }
    } else if center_right - center_left > 140. {
        let mid = (center_left + center_right) / 2.;
        let label = if title == "AMP VERSE" {
            "PLAYER"
        } else {
            title
        };
        let text_width = label.len() as f32 * 9.;
        if theme == crate::theme::Theme::Classic {
            for y in [14., 19., 24.] {
                for (a, b) in [
                    (center_left, mid - text_width / 2. - 12.),
                    (mid + text_width / 2. + 12., center_right),
                ] {
                    if b > a {
                        ui.painter().line_segment(
                            [egui::pos2(a, rect.top() + y), egui::pos2(b, rect.top() + y)],
                            Stroke::new(2., p.accent),
                        );
                    }
                }
            }
        }
        ui.painter().text(
            egui::pos2(mid, rect.center().y),
            egui::Align2::CENTER_CENTER,
            label,
            FontId::proportional(20.),
            p.text,
        );
    }
    let response = ui.interact(
        menu_rect,
        ui.scope_id().with("module-menu"),
        egui::Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            format!("{title} / Open module menu"),
        )
    });
    let arrow = rect.min + Vec2::new(204., 20.);
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            arrow + Vec2::new(-5., -3.),
            arrow + Vec2::new(5., -3.),
            arrow + Vec2::new(0., 4.),
        ],
        p.accent,
        Stroke::NONE,
    ));
    if response.hovered() {
        ui.painter().rect_stroke(
            menu_rect.shrink(2.),
            2.,
            Stroke::new(1., p.accent),
            egui::StrokeKind::Inside,
        );
    }
    let mut selected = None;
    egui::Popup::menu(&response).show(|ui| {
        ui.set_min_width(220.);
        let height = (ui.ctx().viewport_rect().height() - 65.).max(55.);
        egui::ScrollArea::vertical()
            .id_salt("module-menu-scroll")
            .max_height(height)
            .show(ui, |ui| {
                for (label, module) in [
                    ("AMP Player", Module::Verse),
                    ("AMP Library", Module::Library),
                    ("AMP DJ", Module::Dj),
                    ("AMP Cast", Module::Cast),
                    ("AMP Lab", Module::LabSetup),
                    ("Plugin Manager", Module::Addons),
                ] {
                    let button = ui.button(label);
                    #[cfg(test)]
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(
                            egui::Id::unique(("module-menu-item", label)),
                            (button.rect, ui.clip_rect(), ui.layer_id()),
                        )
                    });
                    if button.clicked() {
                        selected = Some(module);
                        ui.close();
                    }
                }
                ui.separator();
                ui.label("Appearance");
                crate::theme::selector(ui);
                ui.separator();
                ui.hyperlink_to(
                    "AMP Lab downloads",
                    "https://github.com/gitwide-cloud/mp3-lab/releases",
                );
                ui.hyperlink_to("AMP Cast / local web UI", "http://127.0.0.1:8787");
            });
    });
    let controls_left = rect.right() - 110.;
    let drag_rect = Rect::from_min_max(
        egui::pos2(menu_rect.right(), rect.top()),
        egui::pos2(controls_left - 35., rect.bottom()),
    );
    let drag = ui.interact(
        drag_rect,
        ui.scope_id().with("title-drag"),
        egui::Sense::click_and_drag(),
    );
    let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
    if native {
        crate::docking::title_drag(ui, &drag);
    }
    if native && drag.double_clicked() {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    let magnet_id = egui::Id::unique("pane-magnet");
    if let Some(mut magnet) = ui.ctx().data(|d| d.get_temp::<Magnet>(magnet_id)) {
        let r = Rect::from_min_size(
            egui::pos2(controls_left - 35., rect.top() + 5.),
            egui::vec2(31., 31.),
        );
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("magnet-button-rect"), r));
        let response = ui.put(
            r,
            egui::Button::new("")
                .selected(magnet.attached)
                .sense(if magnet.enabled {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                }),
        );
        let color = if magnet.enabled {
            p.accent
        } else {
            p.text.gamma_multiply(0.3)
        };
        // Draw a small horseshoe magnet without depending on emoji fonts.
        let c = r.center();
        ui.painter().add(egui::Shape::line(
            vec![
                c + Vec2::new(-7., -7.),
                c + Vec2::new(-7., 4.),
                c + Vec2::new(-4., 8.),
                c + Vec2::new(4., 8.),
                c + Vec2::new(7., 4.),
                c + Vec2::new(7., -7.),
            ],
            Stroke::new(3., color),
        ));
        response.clone().on_hover_text(if magnet.attached {
            "Detach window"
        } else if magnet.enabled {
            "Attach to touching window"
        } else {
            "Move against another window to attach"
        });
        magnet.clicked = magnet.enabled && response.clicked();
        ui.ctx().data_mut(|d| d.insert_temp(magnet_id, magnet));
    }
    for (offset, text, command) in [
        (0., "_", egui::ViewportCommand::Minimized(true)),
        (35., "[]", egui::ViewportCommand::Maximized(!maximized)),
        (70., "X", egui::ViewportCommand::Close),
    ]
    .into_iter()
    .filter(|_| native)
    {
        let button_rect = Rect::from_min_size(
            egui::pos2(controls_left + offset, rect.top() + 5.),
            egui::vec2(31., 31.),
        );
        let response = ui.interact(
            button_rect,
            ui.scope_id().with(("window-control", text)),
            egui::Sense::click(),
        );
        let close = text == "X";
        let fill = if response.is_pointer_button_down_on() {
            p.accent.gamma_multiply(0.4)
        } else if response.hovered() {
            if close {
                C::from_rgb(170, 40, 45)
            } else {
                p.elevated
            }
        } else {
            C::TRANSPARENT
        };
        ui.painter().rect_filled(button_rect, 6., fill);
        let stroke = Stroke::new(
            1.5,
            if close && response.hovered() {
                C::WHITE
            } else {
                p.text
            },
        );
        let r = button_rect.shrink(10.);
        match text {
            "_" => {
                ui.painter().line_segment(
                    [
                        egui::pos2(r.left(), r.center().y),
                        egui::pos2(r.right(), r.center().y),
                    ],
                    stroke,
                );
            }
            "[]" => {
                if maximized {
                    ui.painter().rect_stroke(
                        r.translate(Vec2::new(2., -2.)),
                        0.,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
                ui.painter()
                    .rect_stroke(r, 0., stroke, egui::StrokeKind::Inside);
            }
            _ => {
                ui.painter()
                    .line_segment([r.left_top(), r.right_bottom()], stroke);
                ui.painter()
                    .line_segment([r.right_top(), r.left_bottom()], stroke);
            }
        }
        let label = if close {
            "Close"
        } else if text == "_" {
            "Minimize"
        } else if maximized {
            "Restore"
        } else {
            "Maximize"
        };
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
        if response.clone().on_hover_text(label).clicked() {
            ui.ctx().send_viewport_cmd(command);
        }
    }
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(2., p.accent.gamma_multiply(0.7)),
    );
    selected
}
pub fn label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).strong().size(17.).color(accent(ui)));
}

pub fn button(ui: &mut egui::Ui, text: &str, accent: C) -> egui::Response {
    let theme = crate::theme::current(ui.ctx());
    let p = theme.palette();
    let accent = color(ui, accent);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(text.len() as f32 * 7.4 + 24., 32.),
        egui::Sense::click(),
    );
    let active = response.is_pointer_button_down_on();
    ui.painter().rect_filled(rect, 3., p.background);
    ui.painter().add(gradient(
        ui.painter(),
        rect.shrink(2.),
        if theme != crate::theme::Theme::Classic {
            p.elevated
        } else if active {
            C::from_rgb(85, 65, 20)
        } else {
            C::from_rgb(130, 134, 140)
        },
        if theme != crate::theme::Theme::Classic {
            p.elevated
        } else if active {
            C::from_rgb(43, 30, 9)
        } else {
            C::from_rgb(54, 57, 62)
        },
    ));
    ui.painter().rect_stroke(
        rect,
        3.,
        Stroke::new(1., if response.hovered() { accent } else { p.border }),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(14.),
        accent,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    response
}

/// Scalable DJ transport matching the deck's amber/cyan identity.
pub fn deck_button(
    ui: &mut egui::Ui,
    text: &str,
    accent: C,
    width: f32,
    primary: bool,
) -> egui::Response {
    let accent = if primary {
        color(ui, GOLD)
    } else {
        color(ui, accent)
    };
    let text = RichText::new(text).size(15.).strong().color(if primary {
        C::WHITE
    } else {
        foreground(ui)
    });
    let mut button = egui::Button::new(text).corner_radius(8).stroke(Stroke::new(
        1.,
        if primary {
            accent
        } else {
            crate::theme::current(ui.ctx()).palette().border
        },
    ));
    if primary {
        button = button.fill(C::from_rgb(105, 75, 12));
    }
    ui.add_sized([width, 42.], button)
}

pub fn bolt(painter: &egui::Painter, rect: Rect, color: C) {
    let p = |x: f32, y: f32| rect.min + egui::vec2(rect.width() * x, rect.height() * y);
    painter.add(egui::Shape::convex_polygon(
        vec![
            p(0.65, 0.),
            p(0.1, 0.57),
            p(0.45, 0.57),
            p(0.32, 1.),
            p(0.95, 0.35),
            p(0.57, 0.35),
        ],
        color,
        Stroke::new(1., C::from_rgb(124, 76, 0)),
    ));
}

/// Decorative vinyl grooves; no fabricated meter or waveform data.
pub fn platter(
    ui: &mut egui::Ui,
    size: f32,
    accent: C,
    _playing: bool,
    phase: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::drag());
    let center = rect.center();
    let radius = size * 0.46;
    ui.painter()
        .circle_filled(center, radius + 5., C::from_rgb(9, 9, 16));
    ui.painter().circle_stroke(
        center,
        radius + 5.,
        Stroke::new(3., C::from_rgb(151, 145, 166)),
    );
    ui.painter()
        .circle_filled(center, radius, accent.gamma_multiply(0.76));
    for ring in 10..50 {
        ui.painter().circle_stroke(
            center,
            radius * ring as f32 / 50.,
            Stroke::new(
                0.65,
                accent.gamma_multiply(if ring % 2 == 0 { 0.5 } else { 0.9 }),
            ),
        );
    }
    let angle = phase * std::f32::consts::TAU / 1.8;
    ui.painter().line_segment(
        [
            center,
            center + egui::vec2(angle.cos(), angle.sin()) * radius,
        ],
        Stroke::new(6., C::from_rgb(27, 26, 40)),
    );
    ui.painter()
        .circle_filled(center, radius * 0.29, C::from_rgb(23, 21, 36));
    ui.painter().circle_stroke(
        center,
        radius * 0.29,
        Stroke::new(2., C::from_rgb(124, 116, 145)),
    );
    bolt(
        ui.painter(),
        Rect::from_center_size(center, egui::vec2(size * 0.08, size * 0.13)),
        GOLD,
    );
    response.on_hover_cursor(egui::CursorIcon::Grab)
}

pub fn vinyl_platter(
    ui: &mut egui::Ui,
    size: f32,
    accent: C,
    _playing: bool,
    phase: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::drag());
    let center = rect.center();
    let radius = size * 0.46;
    ui.painter()
        .circle_filled(center, radius + 5., C::from_rgb(9, 9, 16));
    ui.painter()
        .circle_stroke(center, radius + 5., Stroke::new(3., accent));
    ui.painter()
        .circle_filled(center, radius, C::from_rgb(22, 27, 32));
    for ring in 10..50 {
        ui.painter().circle_stroke(
            center,
            radius * ring as f32 / 50.,
            Stroke::new(
                0.65,
                if ring % 2 == 0 {
                    C::from_rgb(65, 70, 75)
                } else {
                    C::from_rgb(31, 36, 42)
                },
            ),
        );
    }
    let angle = phase * std::f32::consts::TAU / 1.8;
    ui.painter().line_segment(
        [
            center,
            center + egui::vec2(angle.cos(), angle.sin()) * radius,
        ],
        Stroke::new(3., accent),
    );
    ui.painter()
        .circle_filled(center, radius * 0.29, C::from_rgb(23, 21, 36));
    ui.painter().circle_stroke(
        center,
        radius * 0.29,
        Stroke::new(2., C::from_rgb(124, 116, 145)),
    );
    bolt(
        ui.painter(),
        Rect::from_center_size(center, egui::vec2(size * 0.08, size * 0.13)),
        accent,
    );
    response.on_hover_cursor(egui::CursorIcon::Grab)
}

pub fn meter(ui: &mut egui::Ui, peak: f32, size: Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(rect, 1., BLACK);
    for i in 0..20 {
        let level = i as f32 / 20.;
        let y = rect.bottom() - (i + 1) as f32 * rect.height() / 20.;
        let active = peak.clamp(0., 1.) > level;
        let color = if i > 16 {
            C::from_rgb(250, 55, 20)
        } else if i > 12 {
            GOLD
        } else {
            GREEN
        };
        ui.painter().rect_filled(
            Rect::from_min_size(
                egui::pos2(rect.left() + 3., y + 1.),
                egui::vec2(rect.width() - 6., rect.height() / 20. - 2.),
            ),
            0.,
            if active {
                color
            } else {
                color.gamma_multiply(0.12)
            },
        );
    }
}

/// Compact switch with an explicit on/off thumb.
pub fn switch(ui: &mut egui::Ui, value: &mut bool, label: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(160., 26.),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(44., 24.), egui::Sense::click());
            if response.clicked() {
                *value = !*value;
            }
            let color = if *value {
                accent(ui)
            } else {
                crate::theme::current(ui.ctx()).palette().border
            };
            ui.painter().rect_filled(rect, 12., color);
            ui.painter().circle_filled(
                egui::pos2(
                    if *value {
                        rect.right() - 12.
                    } else {
                        rect.left() + 12.
                    },
                    rect.center().y,
                ),
                9.,
                C::WHITE,
            );
            ui.label(label);
        },
    );
}
