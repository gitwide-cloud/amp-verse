//! Library-only cards and artwork. Browsing never starts playback or network lookups.
use crate::{icons, theme};
use eframe::egui::{self, RichText};
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::{Arc, Mutex, mpsc},
};

pub(super) fn panel<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = theme::current(ui.ctx()).palette();
    egui::Frame::new()
        .fill(p.surface)
        .stroke(egui::Stroke::new(1., p.border))
        .corner_radius(12)
        .inner_margin(14)
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), body)
                .inner
        })
        .inner
}
pub(super) fn primary(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).strong().color(egui::Color32::BLACK))
        .fill(egui::Color32::from_rgb(255, 211, 0))
        .corner_radius(9)
        .min_size(egui::vec2(120., 38.))
}
pub(super) fn navigation(
    ui: &mut egui::Ui,
    icon: icons::Icon,
    label: &str,
    selected: bool,
    width: f32,
) -> egui::Response {
    let response = icons::button(ui, icon, label, selected, egui::vec2(width, 46.));
    if selected {
        ui.painter().rect_stroke(
            response.rect,
            9.,
            egui::Stroke::new(1., theme::current(ui.ctx()).palette().accent),
            egui::StrokeKind::Inside,
        );
    }
    response
}
#[derive(Default)]
struct ArtState {
    items: HashMap<String, Art>,
    order: VecDeque<String>,
}
enum Art {
    Pending(mpsc::Receiver<Option<egui::ColorImage>>),
    Ready(Option<egui::TextureHandle>),
}

fn image(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let rgba = reader.decode().ok()?.thumbnail(256, 256).to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}
fn poll_artwork(state: &mut ArtState, ctx: &egui::Context) {
    let completed: Vec<_> = state
        .items
        .iter()
        .filter_map(|(key, item)| {
            let Art::Pending(rx) = item else {
                return None;
            };
            match rx.try_recv() {
                Ok(image) => Some((key.clone(), image)),
                Err(mpsc::TryRecvError::Disconnected) => Some((key.clone(), None)),
                Err(_) => None,
            }
        })
        .collect();
    for (key, image) in completed {
        let texture = image.map(|image| ctx.load_texture(&key, image, Default::default()));
        state.items.insert(key, Art::Ready(texture));
    }
}
fn evict_ready(state: &mut ArtState) -> bool {
    for _ in 0..state.order.len() {
        let Some(key) = state.order.pop_front() else {
            return false;
        };
        if matches!(state.items.get(&key), Some(Art::Ready(_))) {
            state.items.remove(&key);
            return true;
        }
        state.order.push_back(key);
    }
    false
}
fn texture(ui: &egui::Ui, root: &Path, track: &amp_core::Track) -> Option<egui::TextureHandle> {
    let artwork = track
        .preparation
        .extensions
        .get("artwork_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let key = format!("{root:?}:{:?}:{artwork}", track.path);
    let state = ui.ctx().data_mut(|data| {
        let id = egui::Id::unique("library-card-artwork-v1");
        if let Some(state) = data.get_temp::<Arc<Mutex<ArtState>>>(id) {
            state
        } else {
            let state = Arc::new(Mutex::new(ArtState::default()));
            data.insert_temp(id, state.clone());
            state
        }
    });
    let mut state = state.lock().ok()?;
    poll_artwork(&mut state, ui.ctx());
    if let Some(Art::Ready(texture)) = state.items.get(&key) {
        return texture.clone();
    }
    if state.items.contains_key(&key)
        || state
            .items
            .values()
            .filter(|v| matches!(v, Art::Pending(_)))
            .count()
            >= 2
    {
        return None;
    }
    while state.items.len() >= 64 {
        if !evict_ready(&mut state) {
            return None;
        }
    }
    let (tx, rx) = mpsc::channel();
    state.items.insert(key.clone(), Art::Pending(rx));
    state.order.push_back(key);
    drop(state);
    let root = root.to_owned();
    let path = track.path.clone();
    let artwork = artwork.to_owned();
    let ctx = ui.ctx().clone();
    let _ = std::thread::Builder::new()
        .name("library-card-artwork".into())
        .spawn(move || {
            let bytes =
                amp_core::music_sources::owner(&root, &path)
                    .ok()
                    .and_then(|(owner, relative)| {
                        if artwork.is_empty() {
                            amp_core::genres::artwork(&owner.join(relative))
                                .ok()
                                .flatten()
                        } else {
                            amp_core::artwork::read(&owner, &artwork).ok().flatten()
                        }
                    });
            let _ = tx.send(bytes.as_deref().and_then(image));
            ctx.request_repaint();
        });
    None
}
pub(super) fn artwork(
    ui: &egui::Ui,
    rect: egui::Rect,
    root: &Path,
    track: Option<&amp_core::Track>,
    name: &str,
) {
    if let Some(texture) = track.and_then(|track| texture(ui, root, track)) {
        ui.painter().image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
            egui::Color32::WHITE,
        );
        return;
    }
    let hash = name
        .bytes()
        .fold(17u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    let tones = [
        egui::Color32::from_rgb(71, 30, 110),
        egui::Color32::from_rgb(9, 74, 111),
        egui::Color32::from_rgb(100, 57, 20),
        egui::Color32::from_rgb(28, 82, 77),
    ];
    let tone = tones[hash as usize % tones.len()];
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 8., tone.gamma_multiply(0.45));
    painter.circle_filled(
        rect.center() + egui::vec2(0., -rect.height() * 0.2),
        rect.width() * 0.45,
        tone.gamma_multiply(0.45),
    );
    for bar in 0..9 {
        let fraction = 0.2 + ((hash.rotate_left(bar as u32 * 3) & 15) as f32 / 15.) * 0.55;
        let width = rect.width() / 15.;
        let x = rect.left() + rect.width() * 0.17 + bar as f32 * rect.width() * 0.075;
        let height = rect.height() * fraction;
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(x, rect.bottom() - rect.height() * 0.15 - height),
                egui::vec2(width, height),
            ),
            1.,
            egui::Color32::from_rgb(224, 194, 80).gamma_multiply(0.8),
        );
    }
}
pub(super) fn collection(
    ui: &mut egui::Ui,
    root: &Path,
    track: Option<&amp_core::Track>,
    name: &str,
    detail: &str,
    selected: bool,
    width: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 74.), egui::Sense::click());
    let p = theme::current(ui.ctx()).palette();
    ui.painter().rect_filled(
        rect,
        10.,
        if selected {
            p.accent.gamma_multiply(0.12)
        } else if response.hovered() {
            p.elevated
        } else {
            p.surface
        },
    );
    ui.painter().rect_stroke(
        rect,
        10.,
        egui::Stroke::new(
            if selected { 1.5 } else { 0.7 },
            if selected { p.accent } else { p.border },
        ),
        egui::StrokeKind::Inside,
    );
    artwork(
        ui,
        egui::Rect::from_min_size(rect.min + egui::vec2(9., 9.), egui::vec2(56., 56.)),
        root,
        track,
        name,
    );
    ui.put(
        egui::Rect::from_min_size(
            rect.min + egui::vec2(79., 11.),
            egui::vec2((width - 92.).max(20.), 26.),
        ),
        egui::Label::new(RichText::new(name).size(17.).strong().color(p.text))
            .selectable(false)
            .truncate(),
    );
    ui.put(
        egui::Rect::from_min_size(
            rect.min + egui::vec2(79., 39.),
            egui::vec2((width - 92.).max(20.), 23.),
        ),
        egui::Label::new(RichText::new(detail).size(14.).color(p.muted))
            .selectable(false)
            .truncate(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), name));
    response
}
pub(super) fn track_header(ui: &mut egui::Ui, table: &str) {
    let p = theme::current(ui.ctx()).palette();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.), egui::Sense::hover());
    ui.painter().rect_filled(rect, 6., p.background);
    let names = ["#", "Title", "Artist", "BPM", "Key", "Length", "Analysis"];
    let keys = ["#", "title", "artist", "bpm", "key", "length", "analysis"];
    let labels: [String; 7] =
        std::array::from_fn(|i| crate::track_sort::label(ui.ctx(), table, keys[i], names[i]));
    columns(
        ui,
        rect,
        std::array::from_fn(|i| labels[i].as_str()),
        p.muted,
    );
    let mut x = rect.left() + 6.;
    for ((key, name), width) in keys.into_iter().zip(names).zip(column_widths(rect)) {
        if width > 0. {
            let cell = egui::Rect::from_min_size(
                egui::pos2(x, rect.top()),
                egui::vec2(width, rect.height()),
            );
            crate::track_sort::header(ui, table, key, name, cell);
        }
        x += width;
    }
}
fn column_widths(rect: egui::Rect) -> [f32; 7] {
    let wide = rect.width() >= 590.;
    let artist = if wide { rect.width() * 0.20 } else { 0. };
    let small = if wide {
        [36., 64., 52., 64., 70.]
    } else {
        [30., 52., 40., 58., 0.]
    };
    let title = (rect.width() - small.iter().sum::<f32>() - artist - 20.).max(65.);
    [
        small[0], title, artist, small[1], small[2], small[3], small[4],
    ]
}
fn columns(ui: &egui::Ui, rect: egui::Rect, values: [&str; 7], color: egui::Color32) {
    let widths = column_widths(rect);
    let mut x = rect.left() + 6.;
    for (value, width) in values.into_iter().zip(widths) {
        if width > 0. {
            let cell = egui::Rect::from_min_size(
                egui::pos2(x, rect.top()),
                egui::vec2(width, rect.height()),
            );
            ui.painter()
                .with_clip_rect(cell.shrink2(egui::vec2(3., 0.)).intersect(ui.clip_rect()))
                .text(
                    cell.left_center() + egui::vec2(3., 0.),
                    egui::Align2::LEFT_CENTER,
                    value,
                    egui::FontId::proportional(14.),
                    color,
                );
        }
        x += width;
    }
}
pub(super) fn track_row(
    ui: &mut egui::Ui,
    row: usize,
    track: &amp_core::Track,
    selected: bool,
) -> egui::Response {
    let p = theme::current(ui.ctx()).palette();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 36.),
        egui::Sense::click_and_drag(),
    );
    if selected || response.hovered() || row.is_multiple_of(2) {
        ui.painter().rect_filled(
            rect,
            6.,
            if selected {
                p.accent.gamma_multiply(0.17)
            } else if response.hovered() {
                p.elevated
            } else {
                p.elevated.gamma_multiply(0.5)
            },
        );
    }
    let number = format!("{}", row + 1);
    let artist = crate::track_columns::cell(track, "artist");
    let bpm = crate::track_columns::cell(track, "bpm");
    let key = crate::track_columns::cell(track, "key");
    let length = crate::track_columns::cell(track, "length");
    fn known(s: &str) -> &str {
        if s.is_empty() { "—" } else { s }
    }
    columns(
        ui,
        rect,
        [
            &number,
            &track.title,
            known(&artist),
            known(&bpm),
            known(&key),
            known(&length),
            if track.preparation.analysis.is_some() {
                ""
            } else {
                "—"
            },
        ],
        if selected { p.accent } else { p.text },
    );
    if rect.width() >= 590. && track.preparation.analysis.is_some() {
        let center = egui::pos2(rect.right() - 50., rect.center().y);
        let stroke = egui::Stroke::new(2.5, p.signal);
        ui.painter().line_segment(
            [center + egui::vec2(-7., 0.), center + egui::vec2(-2., 5.)],
            stroke,
        );
        ui.painter().line_segment(
            [center + egui::vec2(-2., 5.), center + egui::vec2(8., -6.)],
            stroke,
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &track.title)
    });
    response
}
pub(super) fn duration<'a>(tracks: impl Iterator<Item = &'a amp_core::Track>) -> Option<String> {
    let mut seconds = 0.;
    let mut any = false;
    for track in tracks {
        let n = track
            .preparation
            .analysis
            .as_ref()
            .map(|a| a.duration)
            .or_else(|| {
                track
                    .preparation
                    .extensions
                    .get("audio_properties")?
                    .get("duration")?
                    .as_f64()
            })?;
        if !n.is_finite() || n < 0. {
            return None;
        }
        seconds += n;
        any = true;
    }
    any.then(|| {
        if seconds >= 3600. {
            format!("{:.1} hours", seconds / 3600.)
        } else {
            format!("{} min", (seconds / 60.).ceil() as u64)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(700., 500.),
            )),
            events,
            ..Default::default()
        }
    }
    fn pointer(pos: egui::Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Default::default(),
        }
    }
    fn key(key: egui::Key, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        }
    }
    fn copied(output: &egui::FullOutput) -> String {
        output
            .platform_output
            .commands
            .iter()
            .filter_map(|c| {
                if let egui::OutputCommand::CopyText(s) = c {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .collect()
    }

    #[test]
    fn switch_captions_toggle_without_selecting_control_text() {
        for label in ["Auto Mix", "Enable sync"] {
            for drag in [false, true] {
                let ctx = egui::Context::default();
                let mut value = false;
                let mut rect = egui::Rect::NOTHING;
                let mut text = String::new();
                for frame in 0..6 {
                    let start = rect.left_center() + egui::vec2(1., 0.);
                    let end = if drag {
                        rect.right_center() - egui::vec2(1., 0.)
                    } else {
                        start
                    };
                    let events = match frame {
                        2 => vec![
                            egui::Event::PointerMoved(start),
                            pointer(start, egui::PointerButton::Primary, true),
                        ],
                        3 => vec![egui::Event::PointerMoved(end)],
                        4 => vec![pointer(end, egui::PointerButton::Primary, false)],
                        5 => vec![egui::Event::Copy],
                        _ => vec![],
                    };
                    let mut output = ctx.run_ui(input(events), |ui| {
                        crate::skin::switch(ui, &mut value, label);
                    });
                    rect = ctx
                        .data(|data| {
                            data.get_temp::<egui::Rect>(egui::Id::unique(("switch-caption", label)))
                        })
                        .unwrap();
                    text.push_str(&copied(&output));
                    output.textures_delta.clear();
                }
                assert!(
                    text.is_empty(),
                    "Switch caption must not be copied: {text:?}"
                );
                if !drag {
                    assert!(value, "Caption click must toggle {label}");
                }
            }
        }
    }
    #[test]
    fn collection_title_detail_artwork_and_blank_activate_the_same_card() {
        for appearance in [theme::Theme::Classic, theme::Theme::ModernDark] {
            for offset in [
                egui::vec2(100., 24.),
                egui::vec2(100., 49.),
                egui::vec2(35., 35.),
                egui::vec2(340., 68.),
            ] {
                let ctx = egui::Context::default();
                theme::apply(&ctx, appearance);
                let root = tempfile::tempdir().unwrap();
                let mut point = egui::Pos2::ZERO;
                let mut activated = 0;
                for frame in 0..5 {
                    let events = match frame {
                        2 => vec![
                            egui::Event::PointerMoved(point),
                            pointer(point, egui::PointerButton::Primary, true),
                        ],
                        3 => vec![pointer(point, egui::PointerButton::Primary, false)],
                        _ => vec![],
                    };
                    let mut output = ctx.run_ui(input(events), |ui| {
                        let r = collection(
                            ui,
                            root.path(),
                            None,
                            "R&B Test Playlist",
                            "Smart · 200 tracks",
                            false,
                            360.,
                        );
                        point = r.rect.min + offset;
                        if r.clicked() {
                            activated += 1;
                        }
                    });
                    output.textures_delta.clear();
                }
                assert_eq!(
                    activated, 1,
                    "Card must activate exactly once at {offset:?}"
                );
            }
        }
    }
    #[test]
    fn collection_keyboard_activation_keeps_button_semantics() {
        for k in [egui::Key::Enter, egui::Key::Space] {
            let ctx = egui::Context::default();
            let root = tempfile::tempdir().unwrap();
            let mut count = 0;
            for frame in 0..4 {
                let events = match frame {
                    2 => vec![key(k, true, Default::default())],
                    3 => vec![key(k, false, Default::default())],
                    _ => vec![],
                };
                let mut output = ctx.run_ui(input(events), |ui| {
                    let r = collection(ui, root.path(), None, "Playlist", "Smart", false, 360.);
                    if frame == 1 {
                        r.request_focus();
                    }
                    if r.clicked() {
                        count += 1;
                    }
                });
                output.textures_delta.clear();
            }
            assert_eq!(count, 1, "{k:?} must activate focused card once");
        }
    }
    #[test]
    fn dragging_collection_caption_never_copies_button_text() {
        let ctx = egui::Context::default();
        let root = tempfile::tempdir().unwrap();
        let start = egui::pos2(83., 24.);
        let end = egui::pos2(220., 24.);
        let mut text = String::new();
        for frame in 0..6 {
            let events = match frame {
                2 => vec![
                    egui::Event::PointerMoved(start),
                    pointer(start, egui::PointerButton::Primary, true),
                ],
                3 => vec![egui::Event::PointerMoved(end)],
                4 => vec![pointer(end, egui::PointerButton::Primary, false)],
                5 => vec![egui::Event::Copy],
                _ => vec![],
            };
            let mut output = ctx.run_ui(input(events), |ui| {
                collection(
                    ui,
                    root.path(),
                    None,
                    "R&B Test Playlist",
                    "Smart · 200 tracks",
                    false,
                    360.,
                );
            });
            text.push_str(&copied(&output));
            output.textures_delta.clear();
        }
        assert!(
            text.is_empty(),
            "Button caption was selected/copied: {text:?}"
        );
    }
    #[test]
    fn collection_context_menu_action_does_not_activate_parent() {
        let ctx = egui::Context::default();
        let root = tempfile::tempdir().unwrap();
        let point = egui::pos2(110., 24.);
        let mut menu_point = egui::Pos2::ZERO;
        let mut parent = 0;
        let mut menu = 0;
        for frame in 0..8 {
            let events = match frame {
                2 => vec![
                    egui::Event::PointerMoved(point),
                    pointer(point, egui::PointerButton::Secondary, true),
                ],
                3 => vec![pointer(point, egui::PointerButton::Secondary, false)],
                5 => vec![
                    egui::Event::PointerMoved(menu_point),
                    pointer(menu_point, egui::PointerButton::Primary, true),
                ],
                6 => vec![pointer(menu_point, egui::PointerButton::Primary, false)],
                _ => vec![],
            };
            let mut output = ctx.run_ui(input(events), |ui| {
                let r = collection(
                    ui,
                    root.path(),
                    None,
                    "R&B Test Playlist",
                    "Smart",
                    false,
                    360.,
                );
                if r.clicked() {
                    parent += 1;
                }
                r.context_menu(|ui| {
                    let action = ui.button("Rename playlist");
                    menu_point = action.rect.center();
                    if action.clicked() {
                        menu += 1;
                        ui.close();
                    }
                });
            });
            output.textures_delta.clear();
        }
        assert_eq!(menu, 1, "Nested action must remain actionable");
        assert_eq!(parent, 0, "Nested menu must not activate the parent card");
    }
    #[test]
    fn ordinary_information_and_editable_fields_keep_text_selection() {
        let ctx = egui::Context::default();
        let mut rect = egui::Rect::NOTHING;
        let mut text = String::new();
        for frame in 0..6 {
            let start = rect.left_center() + egui::vec2(1., 0.);
            let end = rect.right_center() - egui::vec2(1., 0.);
            let events = match frame {
                2 => vec![
                    egui::Event::PointerMoved(start),
                    pointer(start, egui::PointerButton::Primary, true),
                ],
                3 => vec![egui::Event::PointerMoved(end)],
                4 => vec![pointer(end, egui::PointerButton::Primary, false)],
                5 => vec![egui::Event::Copy],
                _ => vec![],
            };
            let mut output = ctx.run_ui(input(events), |ui| {
                rect = ui.label("Ordinary information stays selectable").rect;
            });
            text.push_str(&copied(&output));
            output.textures_delta.clear();
        }
        assert!(
            !text.is_empty(),
            "Ordinary information selection must remain enabled"
        );
        // Exercise the editable-field positive control independently: the
        // preceding label intentionally retained its own selection for Copy.
        let ctx = egui::Context::default();
        let mut field = String::from("Rename this playlist");
        let mut field_id = egui::Id::NULL;
        let mut output = ctx.run_ui(input(vec![]), |ui| {
            field_id = ui.text_edit_singleline(&mut field).id;
        });
        output.textures_delta.clear();
        ctx.memory_mut(|m| m.request_focus(field_id));
        let command = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input(vec![key(egui::Key::A, true, command)]), |ui| {
            ui.text_edit_singleline(&mut field);
        });
        output.textures_delta.clear();
        let output = ctx.run_ui(input(vec![egui::Event::Copy]), |ui| {
            ui.text_edit_singleline(&mut field);
        });
        assert_eq!(
            copied(&output),
            field,
            "Editable text must retain select-all/copy"
        );
    }
    #[test]
    fn panel_content_stays_vertical_inside_horizontal_parent() {
        let ctx = egui::Context::default();
        let mut rows = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.horizontal_top(|ui| {
                panel(ui, |ui| {
                    rows.push(ui.button("Library").rect);
                    rows.push(ui.button("Playlists").rect);
                });
            });
        });
        output.textures_delta.clear();
        assert_eq!(rows.len(), 2);
        assert!((rows[0].left() - rows[1].left()).abs() < 0.01);
        assert!(rows[1].top() >= rows[0].bottom());
    }
    #[test]
    fn artwork_completion_is_collected_after_navigating_to_another_playlist() {
        let (tx, rx) = mpsc::channel();
        let mut state = ArtState::default();
        state
            .items
            .insert("previous-playlist".into(), Art::Pending(rx));
        state.order.push_back("previous-playlist".into());
        tx.send(None).unwrap();
        poll_artwork(&mut state, &egui::Context::default());
        assert!(matches!(
            state.items.get("previous-playlist"),
            Some(Art::Ready(None))
        ));
    }
    #[test]
    fn artwork_eviction_never_discards_an_active_background_job() {
        let (_tx, rx) = mpsc::channel();
        let mut state = ArtState::default();
        state.items.insert("pending".into(), Art::Pending(rx));
        state.order.push_back("pending".into());
        state.items.insert("ready".into(), Art::Ready(None));
        state.order.push_back("ready".into());
        assert!(evict_ready(&mut state));
        assert!(state.items.contains_key("pending"));
        assert!(!state.items.contains_key("ready"));
        assert!(!evict_ready(&mut state));
    }
}
