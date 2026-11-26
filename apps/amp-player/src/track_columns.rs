//! Device-local DJ column layout and metadata presentation.
use super::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
pub struct Column {
    pub key: String,
    pub visible: bool,
    pub width: f32,
}
#[derive(Serialize, Deserialize)]
pub struct Layout {
    pub columns: Vec<Column>,
}
pub const FIELDS: &[(&str, &str, f32, bool)] = &[
    ("title", "Track", 280., true),
    ("artist", "Artist", 160., true),
    ("album", "Album", 160., false),
    ("genre", "Genre", 110., false),
    ("year", "Year", 70., false),
    ("track", "Track number", 85., false),
    ("bpm", "BPM", 65., true),
    ("key", "Key", 60., true),
    ("length", "Length", 75., true),
    ("energy", "Energy", 75., false),
    ("analysis", "Analysis", 120., true),
    ("stems", "Stems", 110., true),
    ("rating", "Rating", 70., false),
    ("bitrate", "Bitrate", 80., false),
    ("sample_rate", "Sample rate", 100., false),
    ("channels", "Channels", 75., false),
    ("cues", "Cues", 65., false),
    ("beat_offset", "Beat offset", 100., false),
    ("tempo_confidence", "BPM confidence", 110., false),
    ("key_confidence", "Key confidence", 110., false),
    ("engine", "Analysis engine", 160., false),
    ("path", "File path", 300., false),
];
impl Default for Layout {
    fn default() -> Self {
        let mut value = Self {
            columns: FIELDS
                .iter()
                .map(|(key, _, width, visible)| Column {
                    key: (*key).into(),
                    width: *width,
                    visible: *visible,
                })
                .collect(),
        };
        if let Some(file) = path()
            && let Ok(bytes) = std::fs::read(file)
            && let Ok(saved) = serde_json::from_slice::<Self>(&bytes)
        {
            for column in &mut value.columns {
                if let Some(old) = saved.columns.iter().find(|c| c.key == column.key) {
                    column.visible = old.visible;
                    if old.width.is_finite() {
                        column.width = old.width.clamp(45., 800.);
                    }
                }
            }
        }
        value
    }
}
fn path() -> Option<PathBuf> {
    Some(theme::settings_path()?.parent()?.join("dj-columns.json"))
}
pub fn label(key: &str) -> &str {
    FIELDS.iter().find(|f| f.0 == key).map_or(key, |f| f.1)
}
impl Layout {
    pub fn save(&self) -> Result<(), String> {
        let file = path().ok_or("Settings folder unavailable")?;
        std::fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut temp =
            tempfile::NamedTempFile::new_in(file.parent().unwrap()).map_err(|e| e.to_string())?;
        use std::io::Write;
        temp.write_all(&serde_json::to_vec(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        temp.persist(file).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn menu(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.label("Show columns · drag header edges to resize");
        egui::ScrollArea::vertical()
            .id_salt("dj-columns-menu")
            .max_height(350.)
            .show(ui, |ui| {
                for column in &mut self.columns {
                    ui.horizontal(|ui| {
                        changed |= ui
                            .checkbox(&mut column.visible, label(&column.key))
                            .changed();
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut column.width)
                                    .range(45. ..=800.)
                                    .suffix(" px"),
                            )
                            .changed();
                    });
                }
            });
        if !self.columns.iter().any(|c| c.visible) {
            self.columns[0].visible = true;
        }
        changed
    }
}
fn tag(t: &amp_core::Track, manual: &str, embedded: &str) -> String {
    let p = &t.preparation;
    let value = p
        .extensions
        .get(manual)
        .or_else(|| p.extensions.get("embedded_tags")?.get(embedded));
    value.map(display).unwrap_or_default()
}
pub fn display(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Array(a) => a.iter().map(display).collect::<Vec<_>>().join("; "),
        _ => v.to_string(),
    }
}
pub fn cell(t: &amp_core::Track, key: &str) -> String {
    let p = &t.preparation;
    let analysis = p.analysis.as_ref();
    let ext = |key| p.extensions.get(key).map(display).unwrap_or_default();
    match key {
        "title" => {
            let title = tag(t, "title", "TrackTitle");
            if title.is_empty() {
                t.title.clone()
            } else {
                title
            }
        }
        "artist" => tag(t, "artist", "TrackArtist"),
        "album" => tag(t, "album", "AlbumTitle"),
        "year" => tag(t, "year", "RecordingDate"),
        "track" => tag(t, "track_number", "TrackNumber"),
        "genre" => amp_core::genres::values(p).join("; "),
        "bpm" => p.bpm.map(|b| format!("{b:.1}")).unwrap_or_default(),
        "key" => p.harmonic_key.clone().or(p.key.clone()).unwrap_or_default(),
        "length" => analysis
            .map(|a| a.duration)
            .or_else(|| {
                p.extensions
                    .get("audio_properties")?
                    .get("duration")?
                    .as_f64()
            })
            .or(p.cue_out)
            .map(|v| format!("{:02}:{:02}", v as u64 / 60, v as u64 % 60))
            .unwrap_or_default(),
        "analysis" => if analysis.is_some_and(|a| a.reviewed) {
            "Reviewed"
        } else if analysis.is_some() {
            "Analyzed"
        } else {
            "Not analyzed"
        }
        .into(),
        "energy" => p
            .extensions
            .get("energy")
            .and_then(|v| v.as_f64())
            .map(|v| format!("{:.0}%", v * 100.))
            .unwrap_or_default(),
        "rating" => ext("rating"),
        "cues" => (p.cues.len() + p.manual_cues.len()).to_string(),
        "beat_offset" => p
            .beat_offset
            .map(|v| format!("{v:.3}s"))
            .unwrap_or_default(),
        "tempo_confidence" => analysis
            .map(|a| format!("{:.0}%", a.tempo_confidence * 100.))
            .unwrap_or_default(),
        "key_confidence" => analysis
            .map(|a| format!("{:.0}%", a.key_confidence * 100.))
            .unwrap_or_default(),
        "engine" => analysis.map(|a| a.engine.clone()).unwrap_or_default(),
        "path" => t.path.display().to_string(),
        "bitrate" | "sample_rate" | "channels" => p
            .extensions
            .get("audio_properties")
            .and_then(|v| v.get(key))
            .map(display)
            .unwrap_or_default(),
        _ => String::new(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn columns_show_embedded_artist_and_manual_override_without_inventing_analysis() {
        let mut t = amp_core::Track {
            path: "song.mp3".into(),
            title: "filename".into(),
            preparation: Default::default(),
        };
        t.preparation.extensions.insert(
            "embedded_tags".into(),
            serde_json::json!({"TrackArtist":["Singer"],"AlbumTitle":["Album"]}),
        );
        assert_eq!(cell(&t, "artist"), "Singer");
        assert_eq!(cell(&t, "analysis"), "Not analyzed");
        t.preparation
            .extensions
            .insert("artist".into(), serde_json::json!("Edited"));
        assert_eq!(cell(&t, "artist"), "Edited");
        assert_eq!(cell(&t, "album"), "Album");
        assert_eq!(cell(&t, "bpm"), "");
    }
}
