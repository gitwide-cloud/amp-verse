//! View-only column sorting. Row positions retain playlist/selection identity.
use eframe::egui;
use std::{
    cmp::Ordering,
    sync::Arc,
    time::{Duration, Instant},
};
#[derive(Clone, Default, PartialEq, Eq)]
struct Choice {
    key: String,
    descending: bool,
}
#[derive(Clone)]
struct Cache {
    choice: Choice,
    generation: u64,
    input: Arc<[usize]>,
    rows: Arc<[usize]>,
    refreshed: Instant,
}
fn id(table: &str) -> egui::Id {
    egui::Id::unique(("track-table-sort", table))
}
fn choice(ctx: &egui::Context, table: &str) -> Choice {
    ctx.data(|d| d.get_temp(id(table))).unwrap_or_default()
}
fn toggle(ctx: &egui::Context, table: &str, key: &str) {
    let mut sort = choice(ctx, table);
    if sort.key == key {
        sort.descending = !sort.descending;
    } else {
        sort.key = key.into();
        sort.descending = false;
    }
    ctx.data_mut(|d| d.insert_temp(id(table), sort));
}
pub fn label(ctx: &egui::Context, table: &str, key: &str, name: &str) -> String {
    let sort = choice(ctx, table);
    if sort.key == key {
        format!("{name} {}", if sort.descending { "▼" } else { "▲" })
    } else {
        name.into()
    }
}
pub fn header(ui: &mut egui::Ui, table: &str, key: &str, name: &str, rect: egui::Rect) {
    let response = ui
        .interact(rect, id(table).with(key), egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Click to sort; click again to reverse. Playback order stays unchanged.");
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            ui.is_enabled(),
            format!("Sort by {name}"),
        )
    });
    if response.clicked() {
        toggle(ui.ctx(), table, key);
    }
}
pub fn button(ui: &mut egui::Ui, table: &str, key: &str, name: &str, width: f32) {
    let text = label(ui.ctx(), table, key, name);
    let response = ui
        .add_sized([width, 24.], egui::Button::new(text).frame(false))
        .on_hover_text("Click to sort; click again to reverse. Playback order stays unchanged.");
    if response.clicked() {
        toggle(ui.ctx(), table, key);
    }
}
#[derive(Clone)]
enum Value {
    Number(f64),
    Text(String),
    Missing,
}
fn value(track: &amp_core::Track, key: &str, row: usize) -> Value {
    if key == "#" {
        return Value::Number(row as f64);
    }
    let text = crate::track_columns::cell(track, key);
    if text.trim().is_empty() || text == "—" {
        return Value::Missing;
    }
    if key == "length" {
        let seconds = text
            .split(':')
            .try_fold(0., |n, part| part.parse::<f64>().ok().map(|v| n * 60. + v));
        return seconds
            .filter(|v| v.is_finite())
            .map_or(Value::Missing, Value::Number);
    }
    if matches!(
        key,
        "bpm"
            | "year"
            | "track"
            | "energy"
            | "rating"
            | "bitrate"
            | "sample_rate"
            | "channels"
            | "cues"
            | "beat_offset"
            | "tempo_confidence"
            | "key_confidence"
    ) {
        return text
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .map_or(Value::Missing, Value::Number);
    }
    Value::Text(text.trim().to_lowercase())
}
fn compare(a: &Value, b: &Value, descending: bool) -> Ordering {
    let order = match (a, b) {
        (Value::Missing, Value::Missing) => return Ordering::Equal,
        (Value::Missing, _) => return Ordering::Greater,
        (_, Value::Missing) => return Ordering::Less,
        (Value::Number(a), Value::Number(b)) => a.total_cmp(b),
        (Value::Text(a), Value::Text(b)) => a.cmp(b),
        _ => Ordering::Equal,
    };
    if descending { order.reverse() } else { order }
}
pub fn rows(
    ctx: &egui::Context,
    table: &str,
    generation: u64,
    library: &amp_core::Library,
    input: &[usize],
) -> Arc<[usize]> {
    rows_with_display(ctx, table, generation, library, input, |_, _| None)
}
pub fn rows_with_display(
    ctx: &egui::Context,
    table: &str,
    generation: u64,
    library: &amp_core::Library,
    input: &[usize],
    display: impl Fn(&amp_core::Track, &str) -> Option<String>,
) -> Arc<[usize]> {
    let sort = choice(ctx, table);
    if sort.key.is_empty() {
        return (0..input.len()).collect::<Vec<_>>().into();
    }
    let cache_id = id(table).with("cache");
    if let Some(cache) = ctx.data(|d| d.get_temp::<Cache>(cache_id))
        && cache.choice == sort
        && cache.generation == generation
        && cache.input.as_ref() == input
        && cache.refreshed.elapsed() < Duration::from_secs(1)
    {
        return cache.rows;
    }
    let values: Vec<_> = input
        .iter()
        .enumerate()
        .map(|(row, i)| {
            library.tracks.get(*i).map_or(Value::Missing, |t| {
                display(t, &sort.key).map_or_else(
                    || value(t, &sort.key, row),
                    |s| Value::Text(s.to_lowercase()),
                )
            })
        })
        .collect();
    let mut rows: Vec<_> = (0..input.len()).collect();
    rows.sort_by(|a, b| compare(&values[*a], &values[*b], sort.descending).then_with(|| a.cmp(b)));
    let rows: Arc<[usize]> = rows.into();
    ctx.data_mut(|d| {
        d.insert_temp(
            cache_id,
            Cache {
                choice: sort,
                generation,
                input: input.into(),
                rows: rows.clone(),
                refreshed: Instant::now(),
            },
        )
    });
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sorting_toggles_text_and_numeric_without_reordering_playlist_membership() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.mp3", "b.mp3", "c.mp3", "d.mp3"] {
            std::fs::write(dir.path().join(name), b"fixture").unwrap();
        }
        let mut library = amp_core::scan(dir.path()).unwrap();
        for (t, (title, bpm, len)) in library.tracks.iter_mut().zip([
            ("Zulu", Some(100.), Some(600.)),
            ("alpha", Some(9.), Some(59.)),
            ("ALPHA", None, None),
            ("Beta", Some(120.), Some(90.)),
        ]) {
            t.title = title.into();
            t.preparation.bpm = bpm;
            if let Some(len) = len {
                t.preparation.extensions.insert(
                    "audio_properties".into(),
                    serde_json::json!({"duration":len}),
                );
            }
        }
        let ctx = egui::Context::default();
        let input = vec![0, 1, 2, 3, 1];
        let original = input.clone();
        toggle(&ctx, "test", "title");
        assert_eq!(
            rows(&ctx, "test", 0, &library, &input).as_ref(),
            &[1, 2, 4, 3, 0]
        );
        toggle(&ctx, "test", "title");
        assert_eq!(
            rows(&ctx, "test", 0, &library, &input).as_ref(),
            &[0, 3, 1, 2, 4]
        );
        toggle(&ctx, "test", "bpm");
        assert_eq!(
            rows(&ctx, "test", 0, &library, &input).as_ref(),
            &[1, 4, 0, 3, 2]
        );
        toggle(&ctx, "test", "bpm");
        assert_eq!(
            rows(&ctx, "test", 0, &library, &input).as_ref(),
            &[3, 0, 1, 4, 2]
        );
        toggle(&ctx, "test", "length");
        assert_eq!(
            rows(&ctx, "test", 0, &library, &input).as_ref(),
            &[1, 4, 3, 0, 2]
        );
        toggle(&ctx, "test", "#");
        assert_eq!(
            rows(&ctx, "test", 0, &library, &input).as_ref(),
            &[0, 1, 2, 3, 4]
        );
        toggle(&ctx, "test", "#");
        assert_eq!(
            rows(&ctx, "test", 0, &library, &input).as_ref(),
            &[4, 3, 2, 1, 0]
        );
        assert_eq!(input, original);
        assert_eq!(
            rows(&ctx, "another", 0, &library, &input).as_ref(),
            &[0, 1, 2, 3, 4]
        );
    }
}
