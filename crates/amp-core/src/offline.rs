//! Offline proposals are local documents. Reconnection alone never publishes.
use crate::{Preparation, coordination as c, preparation};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Clone, Serialize, Deserialize)]
pub struct Edit {
    pub version: u32,
    pub library_id: String,
    pub device_id: String,
    pub track: PathBuf,
    pub base: Preparation,
    pub proposed: Preparation,
}
#[derive(Serialize)]
pub struct Preview {
    pub base_revision: u64,
    pub shared_revision: u64,
    pub changes: Vec<String>,
    pub conflict: Option<String>,
}
impl Edit {
    pub fn new(
        library_id: String,
        track: PathBuf,
        base: Preparation,
        proposed: Preparation,
    ) -> Result<Self, String> {
        base.validate()?;
        proposed.validate()?;
        if library_id.is_empty() {
            return Err("A known Library ID is required for offline synchronization".into());
        }
        Ok(Self {
            version: 1,
            library_id,
            device_id: c::device_id()?,
            track: PathBuf::from(track.to_string_lossy().replace('\\', "/")),
            base,
            proposed,
        })
    }
    pub fn save_local(&self, path: &Path) -> Result<(), String> {
        c::directory(path.parent().ok_or("Local proposal needs a parent")?)?;
        c::atomic_write(path, self)
    }
    pub fn queue(&self) -> Result<PathBuf, String> {
        let file = c::device_config_dir()?
            .join("outbox")
            .join(format!("{}.json", c::token()));
        self.save_local(&file)?;
        Ok(file)
    }
    pub fn preview(&self, root: &Path) -> Result<Preview, String> {
        if self.version != 1 || c::library_id(root)? != self.library_id {
            return Err("Offline proposal belongs to a different library".into());
        }
        let audio = crate::resolve(root, &self.track).map_err(|e| e.to_string())?;
        let current = preparation::load(&audio)?;
        let base = serde_json::to_value(&self.base).map_err(|e| e.to_string())?;
        let proposed = serde_json::to_value(&self.proposed).map_err(|e| e.to_string())?;
        let current = serde_json::to_value(current).map_err(|e| e.to_string())?;
        let keys: std::collections::BTreeSet<_> = base
            .as_object()
            .unwrap()
            .keys()
            .chain(proposed.as_object().unwrap().keys())
            .collect();
        let changes = keys
            .into_iter()
            .filter(|k| k.as_str() != "_amp_sync" && base.get(*k) != proposed.get(*k))
            .cloned()
            .collect();
        let conflict = c::merge(
            &base,
            &current,
            &proposed,
            &self.device_id,
            &self.library_id,
        )
        .err();
        Ok(Preview {
            base_revision: c::revision(&base).revision,
            shared_revision: c::revision(&current).revision,
            changes,
            conflict,
        })
    }
    /// Call only after the user reviews the preview and explicitly approves it.
    pub fn publish_approved(&self, root: &Path) -> Result<(), String> {
        let preview = self.preview(root)?;
        if let Some(e) = preview.conflict {
            return Err(e);
        }
        let bytes = serde_json::to_vec(&self.base).map_err(|e| e.to_string())?;
        preparation::save_if_unchanged(root, &self.track, &self.proposed, Some(&bytes))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_approval_merges_offline_cue_with_independent_shared_energy() {
        let share = tempfile::tempdir().unwrap();
        let audio = share.path().join("song.wav");
        std::fs::write(&audio, b"fixture").unwrap();
        let rel = Path::new("song.wav");
        preparation::save(share.path(), rel, &Preparation::default()).unwrap();
        let base = preparation::load(&audio).unwrap();
        let mut phone = base.clone();
        phone.cue_in = 2.;
        phone.manual_override = true;
        let edit = Edit::new(
            c::library_id(share.path()).unwrap(),
            rel.into(),
            base.clone(),
            phone,
        )
        .unwrap();
        let mut other = base;
        other
            .extensions
            .insert("energy".into(), serde_json::json!(0.6));
        preparation::save(share.path(), rel, &other).unwrap();
        let before = preparation::revision(&audio);
        let preview = edit.preview(share.path()).unwrap();
        assert!(preview.conflict.is_none());
        assert_eq!(preparation::revision(&audio), before);
        edit.publish_approved(share.path()).unwrap();
        let merged = preparation::load(&audio).unwrap();
        assert_eq!(merged.cue_in, 2.);
        assert_eq!(merged.extensions["energy"], 0.6);
        assert!(merged.manual_override);
    }
    #[test]
    fn offline_reconnect_only_previews_and_conflict_preserves_both() {
        let share = tempfile::tempdir().unwrap();
        let local = tempfile::tempdir().unwrap();
        std::fs::write(share.path().join("song.wav"), b"fixture").unwrap();
        let rel = Path::new("song.wav");
        preparation::save(share.path(), rel, &Preparation::default()).unwrap();
        let base = preparation::load(&share.path().join(rel)).unwrap();
        let mut phone = base.clone();
        phone.cue_in = 2.;
        phone.manual_override = true;
        let edit = Edit::new(
            c::library_id(share.path()).unwrap(),
            rel.into(),
            base.clone(),
            phone,
        )
        .unwrap();
        let file = local.path().join("offline.json");
        edit.save_local(&file).unwrap();
        let local_bytes = std::fs::read(&file).unwrap();
        let before = preparation::revision(&share.path().join(rel));
        assert!(edit.preview(share.path()).unwrap().conflict.is_none());
        assert_eq!(preparation::revision(&share.path().join(rel)), before);
        let mut pc = base;
        pc.cue_in = 3.;
        preparation::save(share.path(), rel, &pc).unwrap();
        assert!(edit.preview(share.path()).unwrap().conflict.is_some());
        assert!(edit.publish_approved(share.path()).is_err());
        assert_eq!(
            preparation::load(&share.path().join(rel)).unwrap().cue_in,
            3.
        );
        assert_eq!(std::fs::read(file).unwrap(), local_bytes);
    }
}
