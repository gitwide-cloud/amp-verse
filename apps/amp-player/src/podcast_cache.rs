//! Opt-in retention of app-managed podcast downloads, never arbitrary library files.
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Serialize, Deserialize)]
struct Download {
    file: PathBuf,
    id: String,
    title: String,
    saved: u64,
    modified: u64,
    bytes: u64,
    pinned: bool,
    #[serde(default)]
    directory: Option<PathBuf>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    enabled: bool,
    days: u32,
    files: Vec<Download>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            days: 30,
            files: vec![],
        }
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn modified(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |t| t.as_secs())
}
fn store() -> Option<PathBuf> {
    crate::theme::settings_path().map(|p| p.with_file_name("media.ampdb"))
}
fn purge(
    root: &Path,
    settings: &Settings,
    protected: &[PathBuf],
    time: u64,
) -> Result<Vec<PathBuf>, String> {
    let directory = std::fs::symlink_metadata(root).map_err(|e| e.to_string())?;
    if !directory.is_dir() || directory.file_type().is_symlink() {
        return Err("Podcast download directory must be a real directory for auto-purge".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let protected: Vec<_> = protected
        .iter()
        .filter_map(|p| p.canonicalize().ok())
        .collect();
    let mut removed = vec![];
    for record in &settings.files {
        if record.pinned
            || time.saturating_sub(record.saved) < u64::from(settings.days.clamp(1, 3650)) * 86400
            || record
                .directory
                .as_ref()
                .is_some_and(|d| d.canonicalize().ok().as_ref() != Some(&root))
            || record.file.components().count() != 1
            || !record
                .file
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
            || record.id.len() != 64
            || !record.id.bytes().all(|c| c.is_ascii_hexdigit())
        {
            continue;
        }
        let file = root.join(&record.file);
        let required = format!(" - {}.", &record.id[..16]);
        if !record.file.to_string_lossy().contains(&required) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&file) else {
            continue;
        };
        if !meta.is_file()
            || meta.file_type().is_symlink()
            || meta.len() != record.bytes
            || modified(&meta) != record.modified
            || protected.contains(&file.canonicalize().map_err(|e| e.to_string())?)
        {
            continue;
        }
        std::fs::remove_file(&file)
            .map_err(|e| format!("Could not purge {}: {e}", record.file.display()))?;
        removed.push(record.file.clone());
    }
    Ok(removed)
}
#[derive(Default)]
pub struct Cache {
    settings: Settings,
    loaded: bool,
    last: Option<Instant>,
    job: Option<mpsc::Receiver<Result<Vec<PathBuf>, String>>>,
    message: String,
    pub changed: bool,
}
impl Cache {
    pub fn purging(&self) -> bool {
        self.job.is_some()
    }
    fn load(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        if cfg!(test) {
            return;
        }
        if let Some(file) = store() {
            match amp_core::catalog_db::local_record(&file, "podcast-downloads") {
                Ok(Some(v)) => match serde_json::from_value(v) {
                    Ok(settings) => self.settings = settings,
                    Err(e) => self.message = format!("Retention settings could not be read: {e}"),
                },
                Err(e) => self.message = e,
                _ => {}
            }
        }
    }
    fn save(&mut self) {
        if let Some(file) = store()
            && let Err(e) = amp_core::catalog_db::save_local_record(
                &file,
                "podcast-downloads",
                &serde_json::json!(self.settings),
            )
        {
            self.message = format!("Could not save podcast retention: {e}");
        }
    }
    pub fn record(&mut self, path: &Path, id: &str, title: &str) {
        self.load();
        let Ok(meta) = std::fs::metadata(path) else {
            return;
        };
        let Some(file) = path.file_name() else {
            return;
        };
        if self.settings.files.iter().any(|r| r.id == id) {
            return;
        }
        self.settings.files.push(Download {
            file: file.into(),
            id: id.into(),
            title: title.into(),
            saved: now(),
            modified: modified(&meta),
            bytes: meta.len(),
            pinned: false,
            directory: path.parent().map(Path::to_owned),
        });
        self.save();
    }
    pub fn delete(&mut self, path: &Path, id: &str) -> Result<(), String> {
        self.load();
        let record = self
            .settings
            .files
            .iter()
            .find(|r| r.id == id && Some(r.file.as_os_str()) == path.file_name())
            .ok_or("This file is not a managed podcast download")?;
        let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !meta.is_file()
            || meta.file_type().is_symlink()
            || meta.len() != record.bytes
            || modified(&meta) != record.modified
        {
            return Err("The file has changed; it was kept.".into());
        }
        if record
            .directory
            .as_ref()
            .is_some_and(|d| d != path.parent().unwrap_or(Path::new("")))
        {
            return Err("Download location changed; file kept".into());
        }
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
        self.settings.files.retain(|r| r.id != id);
        self.save();
        Ok(())
    }
    pub fn tick(&mut self, ctx: &egui::Context, protected: Vec<PathBuf>, idle: bool) {
        self.load();
        if let Some(rx) = &self.job
            && let Ok(result) = rx.try_recv()
        {
            match result {
                Ok(removed) => {
                    self.settings.files.retain(|r| !removed.contains(&r.file));
                    self.message = format!(
                        "Purged {} expired downloads. Subscriptions and pinned episodes kept.",
                        removed.len()
                    );
                    self.changed = !removed.is_empty();
                    self.save();
                }
                Err(e) => self.message = e,
            }
            self.job = None;
        }
        if self.settings.enabled
            && crate::podcast_plugin::enabled()
            && idle
            && self.job.is_none()
            && self
                .last
                .is_none_or(|t| t.elapsed() > Duration::from_secs(3600))
            && let Ok(root) = crate::podcasts::downloads()
            && root.is_dir()
        {
            self.last = Some(Instant::now());
            let settings = self.settings.clone();
            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(purge(&root, &settings, &protected, now()));
            });
        }
        if self.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    pub fn count(&mut self) -> usize {
        self.load();
        self.settings.files.len()
    }
    pub fn expiring_count(&mut self) -> usize {
        self.load();
        if !self.settings.enabled {
            return 0;
        }
        self.settings
            .files
            .iter()
            .filter(|f| {
                !f.pinned
                    && now().saturating_sub(f.saved) + 7 * 86400
                        >= u64::from(self.settings.days) * 86400
            })
            .count()
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.load();
        ui.separator();
        ui.strong("Offline storage / auto-purge");
        ui.add_enabled_ui(self.job.is_none(),|ui|{
        let mut changed = ui
            .checkbox(
                &mut self.settings.enabled,
                "Automatically remove old, unpinned downloads",
            )
            .changed();
        ui.horizontal(|ui| {
            ui.label("Keep downloads for");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut self.settings.days)
                        .range(1..=3650)
                        .suffix(" days"),
                )
                .changed();
        });
        ui.weak("Runs while playback is idle. Loaded decks, pinned episodes, changed files and all music outside the managed podcast folder are protected. Purging does not unsubscribe you.");
        egui::ScrollArea::vertical()
            .id_salt("podcast-pins")
            .max_height(100.)
            .show_rows(ui, 44., self.settings.files.len(), |ui, range| {
                for i in range {
                    let record = &mut self.settings.files[i];
                    changed |= ui
                        .checkbox(&mut record.pinned, format!("Keep: {}", record.title))
                        .changed();
                    if self.settings.enabled && !record.pinned {ui.weak(format!("Local retention due in {} days",(record.saved+u64::from(self.settings.days)*86400).saturating_sub(now()).div_ceil(86400)));}
                }
            });
        if changed {
            self.last = None;
            self.save();
        }
        });
        ui.label(&self.message);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn purge_only_expires_unchanged_unpinned_managed_files() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = Settings {
            enabled: true,
            days: 1,
            ..Default::default()
        };
        for (i, title) in ["old", "pinned", "loaded", "changed"].iter().enumerate() {
            let id = format!("{i:064x}");
            let file = PathBuf::from(format!("{title} - {}.mp3", &id[..16]));
            let path = root.path().join(&file);
            std::fs::write(&path, b"audio").unwrap();
            let meta = std::fs::metadata(&path).unwrap();
            settings.files.push(Download {
                file,
                id,
                title: title.to_string(),
                saved: 1,
                modified: modified(&meta),
                bytes: meta.len(),
                pinned: i == 1,
                directory: Some(root.path().to_owned()),
            });
        }
        std::fs::write(
            root.path().join(&settings.files[3].file),
            b"user changed audio",
        )
        .unwrap();
        let external = root.path().join("untracked.mp3");
        std::fs::write(&external, b"untracked").unwrap();
        let protected = vec![root.path().join(&settings.files[2].file)];
        let removed = purge(root.path(), &settings, &protected, 86402).unwrap();
        assert_eq!(removed, vec![settings.files[0].file.clone()]);
        assert!(external.exists());
        for r in &settings.files[1..] {
            assert!(root.path().join(&r.file).exists());
        }
    }
}
