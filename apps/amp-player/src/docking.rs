//! Magnetic native viewports. Positions and thresholds are logical desktop points.
use eframe::egui::{self, Pos2, Rect, Vec2, ViewportId};
use std::collections::{HashMap, HashSet};
const MAGNET: f32 = 16.;
const JOIN: f32 = 3.;
#[derive(Clone, Default)]
struct State {
    rects: HashMap<ViewportId, Rect>,
    drag: Option<Drag>,
    allowed: Option<HashSet<ViewportId>>,
}
#[derive(Clone)]
struct Drag {
    id: ViewportId,
    cursor: Pos2,
    origins: HashMap<ViewportId, Rect>,
    detach: bool,
}
fn key() -> egui::Id {
    egui::Id::unique("amp-window-docking")
}
fn overlap(a: f32, b: f32, c: f32, d: f32, t: f32) -> bool {
    a <= d + t && b >= c - t
}
fn attached(a: Rect, b: Rect) -> bool {
    (((a.right() - b.left()).abs() <= JOIN || (a.left() - b.right()).abs() <= JOIN)
        && overlap(a.top(), a.bottom(), b.top(), b.bottom(), -8.))
        || (((a.bottom() - b.top()).abs() <= JOIN || (a.top() - b.bottom()).abs() <= JOIN)
            && overlap(a.left(), a.right(), b.left(), b.right(), -8.))
}
fn group(rects: &HashMap<ViewportId, Rect>, id: ViewportId) -> HashMap<ViewportId, Rect> {
    let mut members = HashMap::new();
    if let Some(r) = rects.get(&id) {
        members.insert(id, *r);
    }
    loop {
        let next: Vec<_> = rects
            .iter()
            .filter(|(id, r)| {
                !members.contains_key(id) && members.values().any(|m| attached(**r, *m))
            })
            .map(|(id, r)| (*id, *r))
            .collect();
        if next.is_empty() {
            break;
        }
        members.extend(next);
    }
    members
}
fn correction(moving: &HashMap<ViewportId, Rect>, all: &HashMap<ViewportId, Rect>) -> Vec2 {
    let mut x = MAGNET + 1.;
    let mut y = MAGNET + 1.;
    for a in moving.values() {
        for (id, b) in all {
            if moving.contains_key(id) {
                continue;
            }
            if overlap(a.top(), a.bottom(), b.top(), b.bottom(), MAGNET) {
                for d in [b.left() - a.right(), b.right() - a.left()] {
                    if d.abs() < x.abs() {
                        x = d;
                    }
                }
            }
            if overlap(a.left(), a.right(), b.left(), b.right(), MAGNET) {
                for d in [b.top() - a.bottom(), b.bottom() - a.top()] {
                    if d.abs() < y.abs() {
                        y = d;
                    }
                }
            }
        }
    }
    if x.abs() > MAGNET {
        x = 0.;
    }
    if y.abs() > MAGNET {
        y = 0.;
    }
    // Once an edge has snapped, line up nearby corners along that edge too.
    if x != 0. && y == 0. {
        for a in moving.values() {
            for (id, b) in all {
                if moving.contains_key(id) {
                    continue;
                }
                if (a.right() + x - b.left()).abs() < JOIN
                    || (a.left() + x - b.right()).abs() < JOIN
                {
                    for d in [b.top() - a.top(), b.bottom() - a.bottom()] {
                        if d.abs() <= MAGNET && (y == 0. || d.abs() < y.abs()) {
                            y = d;
                        }
                    }
                }
            }
        }
    }
    if y != 0. && x == 0. {
        for a in moving.values() {
            for (id, b) in all {
                if moving.contains_key(id) {
                    continue;
                }
                if (a.bottom() + y - b.top()).abs() < JOIN
                    || (a.top() + y - b.bottom()).abs() < JOIN
                {
                    for d in [b.left() - a.left(), b.right() - a.right()] {
                        if d.abs() <= MAGNET && (x == 0. || d.abs() < x.abs()) {
                            x = d;
                        }
                    }
                }
            }
        }
    }
    egui::vec2(x, y)
}
pub fn retain(ctx: &egui::Context, ids: &[ViewportId]) {
    ctx.data_mut(|d| {
        let state = d.get_temp_mut_or_default::<State>(key());
        let ids: HashSet<_> = ids.iter().copied().collect();
        state.rects.retain(|id, _| ids.contains(id));
        state.allowed = Some(ids);
    });
}
pub fn title_drag(ui: &egui::Ui, response: &egui::Response) {
    let ctx = ui.ctx();
    let id = ctx.viewport_id();
    let (rects, cursor, shift, down, maximized) = ui.input(|i| {
        let rects = i
            .raw
            .viewports
            .iter()
            .filter(|(_, v)| !v.minimized.unwrap_or(false) && !v.maximized.unwrap_or(false))
            .filter_map(|(id, v)| v.outer_rect.map(|r| (*id, r)))
            .collect::<HashMap<_, _>>();
        let cursor = i
            .pointer
            .latest_pos()
            .zip(i.viewport().inner_rect)
            .map(|(p, r)| r.min + p.to_vec2());
        (
            rects,
            cursor,
            i.modifiers.shift,
            i.pointer.primary_down(),
            i.viewport().maximized.unwrap_or(false),
        )
    });
    // Wayland compositors do not expose absolute positions; use their native drag.
    if !rects.contains_key(&id) || maximized {
        if response.drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        return;
    }
    let moves = ctx.data_mut(|d| {
        let state = d.get_temp_mut_or_default::<State>(key());
        state.rects = rects
            .into_iter()
            .filter(|(id, _)| {
                state
                    .allowed
                    .as_ref()
                    .is_none_or(|allowed| allowed.contains(id))
            })
            .collect();
        if response.drag_started()
            && let Some(cursor) = cursor
        {
            let origins = if shift {
                state
                    .rects
                    .get(&id)
                    .map(|r| HashMap::from([(id, *r)]))
                    .unwrap_or_default()
            } else {
                group(&state.rects, id)
            };
            state.drag = Some(Drag {
                id,
                cursor,
                origins,
                detach: shift,
            });
        }
        let mut moves = Vec::new();
        if let Some(drag) = &state.drag
            && drag.id == id
            && let Some(cursor) = cursor
        {
            let delta = cursor - drag.cursor;
            let mut moving: HashMap<_, _> = drag
                .origins
                .iter()
                .map(|(id, r)| (*id, r.translate(delta)))
                .collect();
            if !drag.detach && !shift {
                let snap = correction(&moving, &state.rects);
                for r in moving.values_mut() {
                    *r = r.translate(snap);
                }
            }
            for (id, r) in moving {
                state.rects.insert(id, r);
                moves.push((id, r.min));
            }
            if !down {
                state.drag = None;
            }
        }
        moves
    });
    for (id, pos) in moves {
        ctx.send_viewport_cmd_to(id, egui::ViewportCommand::OuterPosition(pos));
        ctx.request_repaint_of(id);
    }
    response
        .clone()
        .on_hover_text("Drag to dock AMP windows. Hold Shift to detach a window.");
}
/// Developer smoke check moves real native viewports and records observed geometry.
pub fn smoke(ctx: &egui::Context, frame: u32, path: &std::path::Path) {
    let root = ViewportId::ROOT;
    let library = ViewportId::from_hash_of("amp-library");
    let rects = ctx.input(|i| {
        i.raw
            .viewports
            .iter()
            .filter_map(|(id, v)| v.outer_rect.map(|r| (*id, r)))
            .collect::<HashMap<_, _>>()
    });
    if frame == 5 {
        ctx.send_viewport_cmd_to(
            root,
            egui::ViewportCommand::OuterPosition(egui::pos2(40., 40.)),
        );
        ctx.send_viewport_cmd_to(
            library,
            egui::ViewportCommand::OuterPosition(egui::pos2(
                47.,
                49. + rects.get(&root).map_or(170., |r| r.height()),
            )),
        );
    }
    if frame == 15
        && let (Some(a), Some(b)) = (rects.get(&root), rects.get(&library))
    {
        let moving = HashMap::from([(library, *b)]);
        let snap = correction(&moving, &HashMap::from([(root, *a)]));
        ctx.send_viewport_cmd_to(library, egui::ViewportCommand::OuterPosition(b.min + snap));
    }
    if frame == 25 {
        let members = group(&rects, root);
        let good = members.contains_key(&library);
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("dock-smoke-before"),
                (good, members.clone()),
            )
        });
        for (id, r) in members {
            ctx.send_viewport_cmd_to(
                id,
                egui::ViewportCommand::OuterPosition(r.min + egui::vec2(30., 20.)),
            );
        }
    }
    if frame == 40 {
        let before = ctx
            .data_mut(|d| {
                d.get_temp::<(bool, HashMap<ViewportId, Rect>)>(egui::Id::unique(
                    "dock-smoke-before",
                ))
            })
            .unwrap_or_default();
        let moved = before.1.iter().all(|(id, r)| {
            rects
                .get(id)
                .is_some_and(|actual| (actual.min - (r.min + egui::vec2(30., 20.))).length() < 2.)
        });
        let joined = rects
            .get(&root)
            .zip(rects.get(&library))
            .is_some_and(|(a, b)| attached(*a, *b));
        let report = format!(
            "{{\"native_snap\":{},\"group_movement\":{},\"still_attached\":{}}}",
            before.0, moved, joined
        );
        let _ = std::fs::write(path, report);
        ctx.send_viewport_cmd_to(root, egui::ViewportCommand::Close);
    }
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(n: u64) -> ViewportId {
        ViewportId::from_hash_of(n)
    }
    fn rect(x: f32, y: f32) -> Rect {
        Rect::from_min_size(egui::pos2(x, y), egui::vec2(200., 100.))
    }
    fn input(pointer: Pos2, pressed: Option<bool>, shift: bool) -> egui::RawInput {
        let mut raw = egui::RawInput::default();
        let modifiers = egui::Modifiers {
            shift,
            ..Default::default()
        };
        raw.events.push(egui::Event::ModifiersChanged(modifiers));
        let info = raw.viewports.get_mut(&ViewportId::ROOT).unwrap();
        info.inner_rect = Some(rect(0., 0.));
        info.outer_rect = Some(rect(0., 0.));
        raw.viewports.insert(
            id(2),
            egui::ViewportInfo {
                inner_rect: Some(rect(200., 0.)),
                outer_rect: Some(rect(200., 0.)),
                ..Default::default()
            },
        );
        raw.events.push(egui::Event::PointerMoved(pointer));
        if let Some(pressed) = pressed {
            raw.events.push(egui::Event::PointerButton {
                pos: pointer,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            });
        }
        raw
    }
    #[test]
    fn title_drag_collects_group_and_shift_detaches() {
        for shift in [false, true] {
            let ctx = egui::Context::default();
            for raw in [
                input(egui::pos2(60., 20.), None, shift),
                input(egui::pos2(60., 20.), Some(true), shift),
                input(egui::pos2(90., 20.), None, shift),
            ] {
                let mut output = ctx.run_ui(raw, |ui| {
                    let response = ui.interact(
                        Rect::from_min_size(Pos2::ZERO, egui::vec2(180., 40.)),
                        egui::Id::unique("test-title"),
                        egui::Sense::click_and_drag(),
                    );
                    title_drag(ui, &response);
                });
                output.textures_delta.clear();
            }
            let state = ctx.data_mut(|d| d.get_temp::<State>(key())).unwrap();
            assert_eq!(state.drag.unwrap().origins.len(), if shift { 1 } else { 2 });
        }
    }
    #[test]
    fn edges_snap_and_corners_align() {
        let all = HashMap::from([(id(1), rect(100., 100.))]);
        let m = HashMap::from([(id(2), rect(309., 108.))]);
        assert_eq!(correction(&m, &all), egui::vec2(-9., -8.));
        let m = HashMap::from([(id(2), rect(106., 209.))]);
        assert_eq!(correction(&m, &all), egui::vec2(-6., -9.));
    }
    #[test]
    fn distant_windows_do_not_snap() {
        assert_eq!(
            correction(
                &HashMap::from([(id(2), rect(500., 500.))]),
                &HashMap::from([(id(1), rect(100., 100.))])
            ),
            Vec2::ZERO
        );
    }
    #[test]
    fn touching_chain_moves_as_one_group() {
        let all = HashMap::from([
            (id(1), rect(0., 0.)),
            (id(2), rect(200., 0.)),
            (id(3), rect(200., 100.)),
            (id(4), rect(700., 700.)),
        ]);
        let g = group(&all, id(1));
        assert_eq!(g.len(), 3);
        assert!(!g.contains_key(&id(4)));
        let delta = egui::vec2(-30., 20.);
        let moved: Vec<_> = g.values().map(|r| r.translate(delta)).collect();
        assert_eq!(moved.len(), 3);
        assert!(attached(rect(-30., 20.), rect(170., 20.)));
    }
    #[test]
    fn group_can_snap_from_any_member() {
        let moving = HashMap::from([(id(1), rect(0., 0.)), (id(2), rect(200., 0.))]);
        let all = HashMap::from([(id(3), rect(410., 0.))]);
        assert_eq!(correction(&moving, &all), egui::vec2(10., 0.));
    }
}
