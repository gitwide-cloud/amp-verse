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
        .show(ui, body)
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
        egui::Label::new(RichText::new(name).size(17.).strong().color(p.text)).truncate(),
    );
    ui.put(
        egui::Rect::from_min_size(
            rect.min + egui::vec2(79., 39.),
            egui::vec2((width - 92.).max(20.), 23.),
        ),
        egui::Label::new(RichText::new(detail).size(14.).color(p.muted)).truncate(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), name));
    response
}
pub(super) fn track_header(ui: &mut egui::Ui) {
    let p = theme::current(ui.ctx()).palette();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.), egui::Sense::hover());
    ui.painter().rect_filled(rect, 6., p.background);
    columns(
        ui,
        rect,
        ["#", "Title", "Artist", "BPM", "Key", "Length", "Analysis"],
        p.muted,
    );
}
fn columns(ui: &egui::Ui, rect: egui::Rect, values: [&str; 7], color: egui::Color32) {
    let wide = rect.width() >= 590.;
    let artist = if wide { rect.width() * 0.20 } else { 0. };
    let small = if wide {
        [36., 64., 52., 64., 70.]
    } else {
        [30., 52., 40., 58., 0.]
    };
    let title = (rect.width() - small.iter().sum::<f32>() - artist - 20.).max(65.);
    let widths = [
        small[0], title, artist, small[1], small[2], small[3], small[4],
    ];
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
                "✓"
            } else {
                "—"
            },
        ],
        if selected { p.accent } else { p.text },
    );
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
