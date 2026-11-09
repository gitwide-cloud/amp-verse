use super::*;
#[derive(Clone, Copy)]
pub enum Icon {
    Folder,
    Layers,
    Play,
    Pause,
    Stop,
    Previous,
    Next,
    Search,
    Device,
    History,
    Analyze,
    List,
}
pub fn draw(ui: &egui::Ui, rect: egui::Rect, icon: Icon, color: egui::Color32) {
    let p = ui.painter();
    let r = rect.shrink(rect.width() * 0.2);
    let point = |x: f32, y: f32| egui::pos2(r.left() + x * r.width(), r.top() + y * r.height());
    let stroke = egui::Stroke::new(1.8, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([point(a.0, a.1), point(b.0, b.1)], stroke);
    };
    match icon {
        Icon::Play => {
            p.add(egui::Shape::convex_polygon(
                vec![point(0.2, 0.), point(1., 0.5), point(0.2, 1.)],
                color,
                egui::Stroke::NONE,
            ));
        }
        Icon::Pause => {
            for x in [0.2, 0.7] {
                p.rect_filled(
                    egui::Rect::from_min_max(point(x, 0.), point(x + 0.16, 1.)),
                    1.,
                    color,
                );
            }
        }
        Icon::Stop => {
            p.rect_filled(
                egui::Rect::from_min_max(point(0.1, 0.1), point(0.9, 0.9)),
                1.,
                color,
            );
        }
        Icon::Previous | Icon::Next => {
            let flip = matches!(icon, Icon::Previous);
            let x = |v| if flip { 1. - v } else { v };
            p.add(egui::Shape::convex_polygon(
                vec![point(x(0.1), 0.), point(x(0.8), 0.5), point(x(0.1), 1.)],
                color,
                egui::Stroke::NONE,
            ));
            line((x(0.95), 0.), (x(0.95), 1.));
        }
        Icon::Folder => {
            p.add(egui::Shape::closed_line(
                vec![
                    point(0., 0.2),
                    point(0.4, 0.2),
                    point(0.5, 0.35),
                    point(1., 0.35),
                    point(1., 0.95),
                    point(0., 0.95),
                ],
                stroke,
            ));
        }
        Icon::Search => {
            p.circle_stroke(point(0.4, 0.4), r.width() * 0.35, stroke);
            line((0.67, 0.67), (1., 1.));
        }
        Icon::Device => {
            p.rect_stroke(
                egui::Rect::from_min_max(point(0., 0.), point(1., 0.75)),
                2.,
                stroke,
                egui::StrokeKind::Inside,
            );
            line((0.5, 0.75), (0.5, 1.));
            line((0.25, 1.), (0.75, 1.));
        }
        Icon::History => {
            p.circle_stroke(r.center(), r.width() * 0.48, stroke);
            line((0.5, 0.2), (0.5, 0.5));
            line((0.5, 0.5), (0.75, 0.65));
        }
        Icon::Analyze => {
            for (x, h) in [(0.1, 0.3), (0.3, 0.7), (0.5, 1.), (0.7, 0.5), (0.9, 0.2)] {
                line((x, 0.5 - h / 2.), (x, 0.5 + h / 2.));
            }
        }
        Icon::Layers => {
            for y in [0.1, 0.4, 0.7] {
                line((0., y), (0.5, y + 0.2));
                line((0.5, y + 0.2), (1., y));
            }
        }
        Icon::List => {
            for y in [0.2, 0.5, 0.8] {
                p.circle_filled(point(0.05, y), 1.8, color);
                line((0.25, y), (1., y));
            }
        }
    }
}
pub fn button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    selected: bool,
    size: egui::Vec2,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let palette = theme::current(ui.ctx()).palette();
    let primary = selected && label.is_empty();
    let mut fill = if primary {
        palette.accent
    } else if selected {
        palette.accent.gamma_multiply(0.22)
    } else if response.hovered() {
        palette.elevated
    } else {
        egui::Color32::TRANSPARENT
    };
    if response.is_pointer_button_down_on() {
        fill = palette.accent.gamma_multiply(0.55);
    }
    if !ui.is_enabled() {
        fill = fill.gamma_multiply(0.4);
    }
    ui.painter().rect_filled(rect, 8., fill);
    let color = if primary {
        palette.background
    } else if selected {
        palette.accent
    } else {
        palette.text
    };
    draw(
        ui,
        egui::Rect::from_min_size(
            rect.min
                + egui::vec2(
                    if label.is_empty() {
                        (rect.width() - 24.) / 2.
                    } else {
                        9.
                    },
                    (rect.height() - 24.) / 2.,
                ),
            egui::vec2(24., 24.),
        ),
        icon,
        color,
    );
    if !label.is_empty() {
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique(("navigation-button", label)), rect));
        ui.painter()
            .with_clip_rect(rect.intersect(ui.clip_rect()))
            .text(
                rect.min + egui::vec2(43., rect.height() / 2.),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(16.),
                color,
            );
    }
    let accessible = if !label.is_empty() {
        label
    } else {
        match icon {
            Icon::Play => "Play",
            Icon::Pause => "Pause",
            Icon::Stop => "Stop",
            Icon::Previous => "Previous track",
            Icon::Next => "Next track",
            _ => "Navigation",
        }
    };
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), accessible));
    response
}
