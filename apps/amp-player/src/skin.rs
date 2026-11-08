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

pub fn panel<R>(ui: &mut egui::Ui, _purple: bool, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let theme = crate::theme::current(ui.ctx());
    let p = theme.palette();
    let bg = ui.painter().add(egui::Shape::Noop);
    let response = egui::Frame::new()
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
    ui.painter()
        .set(bg, gradient(ui.painter(), rect, top, bottom));
    ui.painter().rect_stroke(
        rect.shrink(2.),
        2.,
        Stroke::new(1., p.border.gamma_multiply(0.5)),
        egui::StrokeKind::Inside,
    );
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

pub enum Module {
    Verse,
    Library,
    Dj,
    Cast,
    Addons,
    LabSetup,
}

pub fn heading(ui: &mut egui::Ui, title: &str, _purple: bool) -> Option<Module> {
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
        "AMP VERSE",
        FontId::proportional(21.),
        p.text,
    );
    let menu_rect = Rect::from_min_max(rect.min, egui::pos2(rect.left() + 220., rect.bottom()));
    let center_left = rect.left() + 235.;
    let center_right = rect.right() - 120.;
    if center_right - center_left > 140. {
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
        for (label, module) in [
            ("AMP Player", Module::Verse),
            ("AMP Library", Module::Library),
            ("AMP DJ", Module::Dj),
            ("AMP Cast", Module::Cast),
            ("AMP Lab", Module::LabSetup),
            ("Plugin Manager", Module::Addons),
        ] {
            if ui.button(label).clicked() {
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
    let controls_left = rect.right() - 110.;
    let drag_rect = Rect::from_min_max(
        egui::pos2(menu_rect.right(), rect.top()),
        egui::pos2(controls_left, rect.bottom()),
    );
    let drag = ui.interact(
        drag_rect,
        ui.scope_id().with("title-drag"),
        egui::Sense::click_and_drag(),
    );
    let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
    crate::docking::title_drag(ui, &drag);
    if drag.double_clicked() {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    for (offset, text, command) in [
        (0., "_", egui::ViewportCommand::Minimized(true)),
        (35., "[]", egui::ViewportCommand::Maximized(!maximized)),
        (70., "X", egui::ViewportCommand::Close),
    ] {
        let button_rect = Rect::from_min_size(
            egui::pos2(controls_left + offset, rect.top() + 5.),
            egui::vec2(31., 31.),
        );
        if ui
            .put(
                button_rect,
                egui::Button::new(RichText::new(text).color(p.accent)),
            )
            .clicked()
        {
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
pub fn platter(ui: &mut egui::Ui, size: f32, accent: C, playing: bool, phase: f32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), size), egui::Sense::hover());
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
    let angle = if playing { phase * 1.8 } else { -2.1 };
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
