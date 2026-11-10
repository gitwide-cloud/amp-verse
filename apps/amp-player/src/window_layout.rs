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
        egui::vec2(1440., 1000.)
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
/// Fixed layout scaling, with no scene camera or wheel-pan interaction.
pub fn fitted_content(
    ui: &mut egui::Ui,
    id: &str,
    logical: egui::Vec2,
    scale: f32,
    contents: impl FnOnce(&mut egui::Ui),
) {
    let (outer, _) = ui.allocate_exact_size(ui.available_size_before_wrap(), egui::Sense::hover());
    let layer = egui::LayerId::new(ui.layer_id().order, ui.scope_id().with(id));
    let transform = egui::emath::TSTransform::from_translation(outer.min.to_vec2())
        * egui::emath::TSTransform::from_scaling(scale);
    // Register the hit-test extent in the same coordinate space as the paint
    // transform. A paint-only sublayer cannot receive inner list scrolling.
    ui.ctx().memory_mut(|memory| {
        memory.areas_mut().set_state(
            layer,
            egui::AreaState {
                pivot_pos: Some(egui::Pos2::ZERO),
                size: Some(logical),
                ..Default::default()
            },
        )
    });
    ui.ctx().set_sublayer(ui.layer_id(), layer);
    ui.ctx().set_transform_layer(layer, transform);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .layer_id(layer)
            .max_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, logical))
            .sense(egui::Sense::hover()),
    );
    child.set_clip_rect(transform.inverse() * outer.intersect(ui.clip_rect()));
    contents(&mut child);
    child.expand_to_include_rect(transform.inverse() * outer);
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
/// Physical viewport dimensions keep scaled small windows from selecting the roomy layout.
pub fn dj_pads_below(size: egui::Vec2, expanded: bool) -> bool {
    expanded && size.x >= 1200. && size.y >= 850.
}

/// Visible, bounded drag divider. Direction controls which adjacent panel grows.
pub fn splitter(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    vertical: bool,
    direction: f32,
    length: f32,
) {
    let size = if vertical {
        egui::vec2(10., length)
    } else {
        egui::vec2(ui.available_width(), length)
    };
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let response = ui.interact(rect, egui::Id::unique(id), egui::Sense::drag());
    if response.dragged() {
        let delta = ui.input(|i| i.pointer.delta());
        *value = (*value + if vertical { delta.x } else { delta.y } * direction)
            .clamp(*range.start(), *range.end());
    }
    response
        .clone()
        .on_hover_cursor(if vertical {
            egui::CursorIcon::ResizeHorizontal
        } else {
            egui::CursorIcon::ResizeVertical
        })
        .on_hover_text("Drag to resize • double-click to reset");
    if response.double_clicked() {
        *value = (range.start() + range.end()) * 0.5;
    }
    let color = if response.hovered() || response.dragged() {
        super::skin::accent(ui)
    } else {
        ui.visuals().widgets.noninteractive.bg_stroke.color
    };
    let half = if vertical {
        egui::vec2(0., length.min(48.) * 0.5)
    } else {
        egui::vec2(rect.width().min(80.) * 0.5, 0.)
    };
    ui.painter().line_segment(
        [rect.center() - half, rect.center() + half],
        egui::Stroke::new(3., color),
    );
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(id).with("bounds"), (rect, ui.layer_id())));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dividers_drag_adjacent_panels_and_clamp_their_sizes() {
        for vertical in [false, true] {
            let ctx = egui::Context::default();
            let mut value = 200.;
            let mut point = egui::pos2(30., 5.);
            for frame in 0..5 {
                let mut raw = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 400.),
                    )),
                    ..Default::default()
                };
                if frame >= 2 {
                    point += if vertical {
                        egui::vec2(80., 0.)
                    } else {
                        egui::vec2(0., 80.)
                    };
                }
                raw.events.push(egui::Event::PointerMoved(point));
                if frame == 1 || frame == 4 {
                    raw.events.push(egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 1,
                        modifiers: Default::default(),
                    });
                }
                let mut output = ctx.run_ui(raw, |ui| {
                    splitter(
                        ui,
                        "test-divider",
                        &mut value,
                        150. ..=300.,
                        vertical,
                        1.,
                        200.,
                    );
                    if frame == 0 {
                        point = ctx
                            .data(|d| {
                                d.get_temp::<(egui::Rect, egui::LayerId)>(
                                    egui::Id::unique("test-divider").with("bounds"),
                                )
                            })
                            .unwrap()
                            .0
                            .center();
                    }
                });
                output.textures_delta.clear();
            }
            assert_eq!(value, 300., "Dragging should resize and stop at the limit");
        }
    }
    #[test]
    fn wheel_scrolls_content_without_panning_scaled_window() {
        let ctx = egui::Context::default();
        let mut offsets = Vec::new();
        let mut transforms = Vec::new();
        for frame in 0..4 {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400., 300.),
                )),
                ..Default::default()
            };
            raw.events
                .push(egui::Event::PointerMoved(egui::pos2(15., 30.)));
            if frame == 2 {
                raw.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    delta: egui::vec2(0., -100.),
                    modifiers: Default::default(),
                });
            }
            let mut output = ctx.run_ui(raw, |ui| {
                fitted_content(ui, "fixed-test", egui::vec2(800., 600.), 0.5, |ui| {
                    transforms.push(ui.ctx().layer_transform_to_global(ui.layer_id()).unwrap());
                    let result = egui::ScrollArea::vertical()
                        .max_height(200.)
                        .show(ui, |ui| {
                            for row in 0..100 {
                                ui.label(format!("Song {row}"));
                            }
                        });
                    offsets.push(result.state.clamped_offset().y);
                });
            });
            output.textures_delta.clear();
        }
        assert!(
            transforms
                .iter()
                .all(|transform| *transform == transforms[0]),
            "Wheel input must not move the window layout"
        );
        assert!(
            offsets.iter().any(|offset| *offset > 0.),
            "The inner song list must still receive scrolling"
        );
    }

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
