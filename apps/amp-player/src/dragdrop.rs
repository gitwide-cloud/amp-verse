use eframe::egui;
use std::path::PathBuf;

#[derive(Clone)]
struct SourceArea {
    rect: egui::Rect,
    layer: egui::LayerId,
    id: egui::Id,
    path: PathBuf,
}
#[derive(Clone, Default)]
struct Sources {
    pass: u64,
    current: Vec<SourceArea>,
    previous: Vec<SourceArea>,
}
#[derive(Clone, Default)]
struct Gesture {
    pass: Option<u64>,
    cancelled: bool,
}
fn source_key(ctx: &egui::Context) -> egui::Id {
    egui::Id::unique(("track-drag-sources", ctx.viewport_id()))
}
/// Recover a fast native drag even when press, movement and release arrive in
/// one repaint. Decks render before Library, so use the last painted source map.
pub fn prepare_drag(ctx: &egui::Context) {
    let key = source_key(ctx);
    let pass = ctx.cumulative_pass_nr();
    let frame = ctx.cumulative_frame_nr();
    let (pressed, released, down, origin, position, escape) = ctx.input(|input| {
        let press = input.raw.events.iter().rev().find_map(|event| match event {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                ..
            } => Some(*pos),
            _ => None,
        });
        let release = input.raw.events.iter().rev().find_map(|event| match event {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                ..
            } => Some(*pos),
            _ => None,
        });
        (
            press.is_some(),
            release.is_some(),
            input.pointer.primary_down(),
            press.or(input.pointer.press_origin()),
            release.or(input.pointer.hover_pos()),
            input.raw.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::Escape,
                        pressed: true,
                        ..
                    }
                )
            }),
        )
    });
    let ready = ctx.data_mut(|data| {
        let gesture = data.get_temp_mut_or_default::<Gesture>(key.with("gesture"));
        if gesture.pass == Some(frame) {
            return false;
        }
        gesture.pass = Some(frame);
        if pressed {
            gesture.cancelled = false;
        }
        if escape {
            gesture.cancelled = true;
        }
        !gesture.cancelled
    });
    if !ready || egui::DragAndDrop::has_any_payload(ctx) || (!down && !released) {
        return;
    }
    let (Some(origin), Some(position)) = (origin, position) else {
        return;
    };
    if origin.distance(position) < 6. {
        return;
    }
    let origin_layer = ctx.layer_id_at(origin);
    let source = ctx.data(|data| {
        let sources = data.get_temp::<Sources>(key)?;
        if pass.saturating_sub(sources.pass) > 2 {
            return None;
        }
        let areas = if sources.pass == pass {
            &sources.previous
        } else {
            &sources.current
        };
        areas
            .iter()
            .rev()
            .find(|source| source.rect.contains(origin) && origin_layer == Some(source.layer))
            .cloned()
    });
    if let Some(source) = source {
        if down {
            ctx.set_dragged_id(source.id);
        }
        egui::DragAndDrop::set_payload(ctx, source.path);
    }
}
pub fn source(response: &egui::Response, path: PathBuf) {
    let pass = response.ctx.cumulative_pass_nr();
    let key = source_key(&response.ctx);
    let rect = response
        .ctx
        .layer_transform_to_global(response.layer_id)
        .unwrap_or_default()
        * response.interact_rect;
    response.ctx.data_mut(|data| {
        let sources = data.get_temp_mut_or_default::<Sources>(key);
        if sources.pass != pass {
            sources.previous = std::mem::take(&mut sources.current);
            sources.pass = pass;
        }
        sources.current.push(SourceArea {
            rect,
            layer: response.layer_id,
            id: response.id,
            path: path.clone(),
        });
    });
    response.dnd_set_drag_payload(path);
    if response.hovered() || egui::DragAndDrop::has_payload_of_type::<PathBuf>(&response.ctx) {
        hand_cursor(
            &response.ctx,
            egui::DragAndDrop::has_payload_of_type::<PathBuf>(&response.ctx),
        );
    }
}
pub fn target(response: &egui::Response, accent: egui::Color32) -> Option<PathBuf> {
    prepare_drag(&response.ctx);
    let internal = egui::DragAndDrop::has_payload_of_type::<PathBuf>(&response.ctx);
    if internal {
        hand_cursor(&response.ctx, true);
    }
    let dragging = internal
        || response
            .ctx
            .input(|input| !input.raw.hovered_files.is_empty());
    let hovered = response
        .ctx
        .layer_transform_to_global(response.layer_id)
        .unwrap_or_default()
        * response.interact_rect;
    let hovered = response.ctx.pointer_hover_pos().is_some_and(|position| {
        hovered.contains(position) && response.ctx.layer_id_at(position) == Some(response.layer_id)
    });
    if dragging && hovered {
        response.ctx.layer_painter(response.layer_id).rect_stroke(
            response.rect.shrink(2.),
            4.,
            egui::Stroke::new(2., accent),
            egui::StrokeKind::Inside,
        );
    }
    // Container responses can be covered by their own children (waveforms,
    // buttons and platters). Use the same clipped, transformed hit area as the
    // highlight instead of Response::contains_pointer's widget-level result.
    (hovered
        && response
            .ctx
            .input(|input| input.pointer.button_released(egui::PointerButton::Primary)))
    .then(|| egui::DragAndDrop::take_payload::<PathBuf>(&response.ctx))
    .flatten()
    .map(|path| (*path).clone())
    .or_else(|| {
        if hovered {
            response.ctx.input(|input| {
                input
                    .raw
                    .dropped_files
                    .first()
                    .map(|file| file.path().to_path_buf())
            })
        } else {
            None
        }
    })
}

// Winit maps both Grab and Grabbing to the same four-way move cursor on
// Windows. Draw distinct vector hands so "grab" and "carrying" stay visible.
fn hand_cursor(ctx: &egui::Context, closed: bool) {
    ctx.set_cursor_icon(egui::CursorIcon::None);
    let Some(position) = ctx.pointer_hover_pos() else {
        return;
    };
    let id = egui::Id::unique(("track-hand-cursor", ctx.viewport_id()));
    let pass = ctx.cumulative_pass_nr();
    if ctx.data_mut(|data| {
        let old = data.get_temp::<u64>(id);
        data.insert_temp(id, pass);
        old == Some(pass)
    }) {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, id));
    let origin = position + egui::vec2(-5., -4.);
    let fill = egui::Color32::from_rgb(245, 245, 245);
    let stroke = egui::Stroke::new(1.3, egui::Color32::BLACK);
    let rect = |x: f32, y: f32, w: f32, h: f32| {
        egui::Rect::from_min_size(origin + egui::vec2(x, y), egui::vec2(w, h))
    };
    if closed {
        painter.rect(
            rect(0., 7., 23., 18.),
            5.,
            fill,
            stroke,
            egui::StrokeKind::Inside,
        );
        for x in [0., 6., 12., 18.] {
            painter.rect(
                rect(x, 3., 6., 10.),
                3.,
                fill,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        painter.rect(
            rect(-2., 12., 17., 7.),
            3.,
            fill,
            stroke,
            egui::StrokeKind::Inside,
        );
    } else {
        for (x, y, h) in [(0., 3., 15.), (6., 0., 18.), (12., 2., 16.), (18., 6., 12.)] {
            painter.rect(
                rect(x, y, 6., h),
                3.,
                fill,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        painter.rect(
            rect(0., 12., 24., 16.),
            5.,
            fill,
            stroke,
            egui::StrokeKind::Inside,
        );
        painter.add(egui::Shape::convex_polygon(
            vec![
                origin + egui::vec2(3., 21.),
                origin + egui::vec2(-7., 14.),
                origin + egui::vec2(-9., 16.),
                origin + egui::vec2(1., 27.),
            ],
            fill,
            stroke,
        ));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_drag_reaches_scaled_container_once_and_uses_distinct_hand_feedback() {
        for scale in [1., 0.6] {
            let ctx = egui::Context::default();
            let mut start = egui::Pos2::ZERO;
            let mut destination = egui::Pos2::ZERO;
            let mut deliveries = Vec::new();
            for frame in 0..8 {
                let point = if frame < 3 { start } else { destination };
                let mut raw = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 500.),
                    )),
                    ..Default::default()
                };
                raw.events.push(egui::Event::PointerMoved(point));
                if frame == 2 || frame == 5 {
                    raw.events.push(egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 2,
                        modifiers: Default::default(),
                    });
                }
                let mut output = ctx.run_ui(raw, |ui| {
                    let response = ui.add(
                        egui::Button::new("Library track").sense(egui::Sense::click_and_drag()),
                    );
                    start = response.rect.center();
                    source(&response, PathBuf::from("song.mp3"));
                    crate::window_layout::fitted_content(
                        ui,
                        "drag-fixture",
                        egui::vec2(600., 400.),
                        scale,
                        |ui| {
                            let area = egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.add_sized(
                                    [240., 160.],
                                    egui::Button::new("Deck waveform")
                                        .sense(egui::Sense::click_and_drag()),
                                );
                            });
                            destination = ctx
                                .layer_transform_to_global(area.response.layer_id)
                                .unwrap_or_default()
                                * area.response.rect.center();
                            if let Some(path) = target(&area.response, egui::Color32::YELLOW) {
                                deliveries.push(path);
                            }
                            // Even an overlapping second consumer must not load it twice.
                            assert!(target(&area.response, egui::Color32::YELLOW).is_none());
                        },
                    );
                });
                if frame == 1 || frame == 4 {
                    assert_eq!(
                        output.platform_output.cursor_icon,
                        egui::CursorIcon::None,
                        "Custom hand must replace Windows' four-way move cursor"
                    );
                }
                output.textures_delta.clear();
            }
            assert_eq!(
                deliveries,
                vec![PathBuf::from("song.mp3")],
                "scaled container {scale}"
            );
        }
    }
    #[test]
    fn coalesced_native_press_move_release_uses_previous_source_and_delivers_once() {
        let ctx = egui::Context::default();
        let mut delivered = Vec::new();
        let origin = egui::pos2(90., 270.);
        let destination = egui::pos2(90., 90.);
        for frame in 0..4 {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500., 400.),
                )),
                ..Default::default()
            };
            raw.events.push(egui::Event::PointerMoved(origin));
            if frame == 2 {
                raw.events.extend([
                    egui::Event::PointerButton {
                        pos: origin,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                    egui::Event::PointerMoved(destination),
                    egui::Event::PointerButton {
                        pos: destination,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    },
                ]);
            }
            let mut output = ctx.run_ui(raw, |ui| {
                let response = ui.interact(
                    egui::Rect::from_min_size(egui::pos2(20., 20.), egui::vec2(200., 180.)),
                    egui::Id::unique("deck"),
                    egui::Sense::hover(),
                );
                if let Some(path) = target(&response, egui::Color32::YELLOW) {
                    delivered.push(path);
                }
                assert!(target(&response, egui::Color32::YELLOW).is_none());
                let response = ui.interact(
                    egui::Rect::from_min_size(egui::pos2(20., 250.), egui::vec2(200., 40.)),
                    egui::Id::unique("track"),
                    egui::Sense::click_and_drag(),
                );
                source(&response, PathBuf::from("fast.mp3"));
            });
            output.textures_delta.clear();
        }
        assert_eq!(delivered, vec![PathBuf::from("fast.mp3")]);
    }
}
