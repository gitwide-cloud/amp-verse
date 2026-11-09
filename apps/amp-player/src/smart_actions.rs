//! Shared actions for Smart Playlist menus in Library and DJ.
use super::*;
use show_editor::PlaylistAction;
pub(super) fn change_saved(
    root: &Path,
    settings: &amp_core::show::Settings,
    id: &str,
    action: PlaylistAction,
) -> Result<amp_core::show::Settings, String> {
    let mut next = settings.clone();
    let index = next
        .playlists
        .iter()
        .position(|p| p.id == id)
        .ok_or("Smart Playlist no longer exists; resync the library")?;
    match action {
        PlaylistAction::Delete => {
            next.playlists.remove(index);
        }
        PlaylistAction::Duplicate => {
            let mut copy = next.playlists[index].clone();
            copy.id = format!(
                "copy-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
            );
            copy.name = format!("{} (copy)", copy.name);
            next.playlists.push(copy);
        }
        _ => return Err("This action does not modify saved rules".into()),
    }
    next.save(root)?;
    amp_core::show::Settings::load(root)
}
impl Player {
    pub(super) fn smart_playlist_action(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        action: PlaylistAction,
    ) {
        if matches!(action, PlaylistAction::Delete | PlaylistAction::Duplicate)
            && self.show_editor.has_unsaved()
        {
            self.status =
                "Save or reload your Smart Playlist draft before changing saved playlists.".into();
            return;
        }
        match action {
            PlaylistAction::Edit => {
                self.library_page = 1;
                self.edit_smart_in_library(ui, id);
            }
            PlaylistAction::Resync => match amp_core::show::Settings::load(&self.root) {
                Ok(fresh) => {
                    self.show_editor.refresh_saved(&fresh);
                    self.run.settings = fresh;
                    if self.scan.is_none() {
                        self.scan();
                    }
                    self.status="Resyncing saved Smart Playlist rules and music folders. Active decks keep their snapshots.".into();
                }
                Err(error) => self.status = error,
            },
            PlaylistAction::Delete | PlaylistAction::Duplicate => {
                match change_saved(&self.root, &self.run.settings, id, action) {
                    Ok(fresh) => {
                        self.show_editor.refresh_saved(&fresh);
                        self.run.settings = fresh;
                        self.status = if action == PlaylistAction::Delete {
                            "Smart Playlist deleted. Music and active playback were kept."
                        } else {
                            "Smart Playlist duplicated."
                        }
                        .into();
                    }
                    Err(error) => self.status = format!("Smart Playlist unchanged: {error}"),
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_delete_and_stale_edits_preserve_saved_rules() {
        let root = tempfile::tempdir().unwrap();
        let initial = amp_core::show::Settings::default();
        initial.save(root.path()).unwrap();
        let saved = amp_core::show::Settings::load(root.path()).unwrap();
        let id = saved.playlists[0].id.clone();
        let copy = change_saved(root.path(), &saved, &id, PlaylistAction::Duplicate).unwrap();
        assert_eq!(copy.playlists.len(), saved.playlists.len() + 1);
        assert_ne!(copy.playlists.last().unwrap().id, id);
        assert_eq!(
            copy.playlists.last().unwrap().folders,
            saved.playlists[0].folders
        );
        assert!(change_saved(root.path(), &saved, &id, PlaylistAction::Delete).is_err());
        assert_eq!(
            amp_core::show::Settings::load(root.path())
                .unwrap()
                .playlists
                .len(),
            copy.playlists.len()
        );
        let deleted = change_saved(root.path(), &copy, &id, PlaylistAction::Delete).unwrap();
        assert!(!deleted.playlists.iter().any(|p| p.id == id));
        assert_eq!(deleted.playlists.len(), saved.playlists.len());
    }
}
