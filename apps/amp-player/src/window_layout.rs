//! Explicit window merges, independent of geometric edge snapping.
use eframe::egui::{self, ViewportId};
pub const NAMES: [&str; 6] = [
    "AMP Verse",
    "AMP Library",
    "AMP DJ",
    "AMP Lab",
    "AMP Cast",
    "Plugin Manager",
];
pub fn viewport(pane: usize) -> ViewportId {
    if pane == 0 {
        ViewportId::ROOT
    } else {
        ViewportId::from_hash_of(
            [
                "",
                "amp-library",
                "amp-dj",
                "amp-lab-arrangement",
                "amp-cast",
                "amp-addons",
            ][pane],
        )
    }
}
pub fn initial_size(pane: usize, monitor: Option<egui::Vec2>) -> [f32; 2] {
    let preferred = if pane == 2 {
        egui::vec2(1000., 760.)
    } else {
        egui::vec2(1100., 800.)
    };
    let available = monitor
        .map(|size| (size - egui::vec2(48., 80.)).max(egui::vec2(400., 300.)))
        .unwrap_or(preferred);
    let size = preferred.min(available);
    [size.x, size.y]
}
pub struct Layout {
    pub parent: [Option<usize>; 6],
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            parent: [None, Some(0), None, None, None, None],
        }
    }
}
impl Layout {
    pub fn join(&mut self, child: usize, host: usize) -> bool {
        if child == 0 || child == host || child >= 6 || host >= 6 {
            return false;
        }
        let mut at = Some(host);
        while let Some(p) = at {
            if p == child {
                return false;
            }
            at = self.parent[p];
        }
        self.parent[child] = Some(host);
        true
    }
    pub fn target(&self, ctx: &egui::Context, pane: usize, open: [bool; 6]) -> Option<usize> {
        ctx.input(|i| {
            let a = i.raw.viewports.get(&viewport(pane))?.outer_rect?;
            (0..6)
                .filter(|&p| p != pane && open[p] && self.parent[p].is_none())
                .find(|&p| {
                    i.raw
                        .viewports
                        .get(&viewport(p))
                        .and_then(|v| v.outer_rect)
                        .is_some_and(|b| touching(a, b))
                })
        })
    }
}
fn touching(a: egui::Rect, b: egui::Rect) -> bool {
    let x = a.left() < b.right() - 8. && b.left() < a.right() - 8.;
    let y = a.top() < b.bottom() - 8. && b.top() < a.bottom() - 8.;
    (x && ((a.top() - b.bottom()).abs() <= 16. || (a.bottom() - b.top()).abs() <= 16.))
        || (y && ((a.left() - b.right()).abs() <= 16. || (a.right() - b.left()).abs() <= 16.))
}
pub fn embedded(ctx: &egui::Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<bool>(egui::Id::unique("embedded-pane"))
            .unwrap_or(false)
    })
}
pub fn set_embedded(ctx: &egui::Context, value: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("embedded-pane"), value));
}
/// Borderless native windows need explicit resize hit areas.
pub fn resize_edges(ui: &mut egui::Ui) {
    if ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let rect = ui.ctx().viewport_rect();
    let size = 7.;
    use egui::{CursorIcon as C, ResizeDirection as D};
    for (id, area, direction, cursor) in [
        (
            0,
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.left() + size, rect.top() + 18.)),
            D::NorthWest,
            C::ResizeNwSe,
        ),
        (
            1,
            egui::Rect::from_min_max(
                egui::pos2(rect.right() - 18., rect.top()),
                egui::pos2(rect.right(), rect.top() + size),
            ),
            D::NorthEast,
            C::ResizeNeSw,
        ),
        (
            2,
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom() - 18.),
                egui::pos2(rect.left() + size, rect.bottom()),
            ),
            D::SouthWest,
            C::ResizeNeSw,
        ),
        (
            3,
            egui::Rect::from_min_max(rect.max - egui::vec2(18., 18.), rect.max),
            D::SouthEast,
            C::ResizeNwSe,
        ),
        (
            4,
            egui::Rect::from_min_max(
                egui::pos2(rect.left() + 18., rect.top()),
                egui::pos2(rect.right() - 18., rect.top() + size),
            ),
            D::North,
            C::ResizeVertical,
        ),
        (
            5,
            egui::Rect::from_min_max(
                egui::pos2(rect.left() + 18., rect.bottom() - size),
                egui::pos2(rect.right() - 18., rect.bottom()),
            ),
            D::South,
            C::ResizeVertical,
        ),
        (
            6,
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() + 18.),
                egui::pos2(rect.left() + size, rect.bottom() - 18.),
            ),
            D::West,
            C::ResizeHorizontal,
        ),
        (
            7,
            egui::Rect::from_min_max(
                egui::pos2(rect.right() - size, rect.top() + 18.),
                egui::pos2(rect.right(), rect.bottom() - 18.),
            ),
            D::East,
            C::ResizeHorizontal,
        ),
    ] {
        let response = ui
            .interact(
                area,
                ui.scope_id().with(("native-resize", id)),
                egui::Sense::drag(),
            )
            .on_hover_cursor(cursor);
        // Start native resizing on the button-down event, before egui's drag
        // threshold or a scaled child scene can claim the pointer gesture.
        let pointer_on_edge = ui.input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|position| area.contains(position))
        });
        if pointer_on_edge {
            ui.ctx().set_cursor_icon(cursor);
        }
        let pressed_on_edge = pointer_on_edge
            && ui.input(|input| input.pointer.button_pressed(egui::PointerButton::Primary));
        if pressed_on_edge {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
        let _ = response;
    }
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::unique("resize-grip"),
    ));
    for offset in [4., 8., 12.] {
        painter.line_segment(
            [
                rect.right_bottom() - egui::vec2(offset, 2.),
                rect.right_bottom() - egui::vec2(2., offset),
            ],
            egui::Stroke::new(1., crate::skin::accent(ui)),
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn borderless_resize_handle_requests_native_resize() {
        let ctx = egui::Context::default();
        let mut requested = false;
        for (position, press) in [
            (egui::pos2(797., 597.), None),
            (egui::pos2(797., 597.), Some(true)),
        ] {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                ..Default::default()
            };
            raw.events.push(egui::Event::PointerMoved(position));
            if let Some(pressed) = press {
                raw.events.push(egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                });
            }
            let mut output = ctx.run_ui(raw, resize_edges);
            output.textures_delta.clear();
            requested |= output
                .viewport_output
                .values()
                .flat_map(|v| &v.commands)
                .any(|c| {
                    matches!(
                        c,
                        egui::ViewportCommand::BeginResize(egui::ResizeDirection::SouthEast)
                    )
                });
        }
        assert!(
            requested,
            "Pressing the visible corner must start OS resizing immediately, without waiting for drag movement"
        );
    }
    #[test]
    fn merges_preserve_children_and_reject_cycles() {
        let mut l = Layout::default();
        assert!(l.join(2, 1));
        assert!(!l.join(1, 2));
        assert!(!l.join(0, 2));
        assert!(l.join(1, 4));
        assert_eq!(l.parent[2], Some(1));
        l.parent[1] = None;
        assert_eq!(l.parent[2], Some(1));
    }
    #[test]
    fn lights_only_for_neighboring_edges() {
        let a = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100., 100.));
        assert!(touching(a, a.translate(egui::vec2(100., 0.))));
        assert!(touching(a, a.translate(egui::vec2(0., 112.))));
        assert!(!touching(a, a.translate(egui::vec2(0., 117.))));
        assert!(!touching(a, a.translate(egui::vec2(100., 100.))));
    }
}
