//! Process-local preview preferences. Shared music is never migrated by this UI upgrade.
use std::path::Path;

fn copy_once(source: &Path, target: &Path) -> std::io::Result<()> {
    if !target.exists() && source.is_file() {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, target)?;
    }
    Ok(())
}

pub fn initialize() -> std::io::Result<()> {
    if !cfg!(windows) || std::env::var_os("AMP_VERSE_CONFIG_DIR").is_some() {
        return Ok(());
    }
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return Ok(());
    };
    let profile = std::path::PathBuf::from(local).join("AMPVerse/V1Preview");
    let roaming = profile.join("AppData");
    let settings = profile.join("settings");
    std::fs::create_dir_all(&settings)?;
    if let Some(old) = std::env::var_os("APPDATA") {
        let old = std::path::PathBuf::from(old);
        copy_once(
            &old.join("amp-verse/music-root.txt"),
            &roaming.join("amp-verse/music-root.txt"),
        )?;
        copy_once(&old.join("AMPVerse/theme.txt"), &settings.join("theme.txt"))?;
        // Seed only completed startup catalogs, never pending writes, sessions or device identity.
        // Library loading validates the copied snapshot before using it, then refreshes in background.
        if let Ok(entries) = std::fs::read_dir(old.join("AMPVerse/catalogs")) {
            for entry in entries.flatten() {
                let source = entry.path();
                if source
                    .extension()
                    .is_some_and(|extension| extension == "json")
                    && entry.metadata().is_ok_and(|metadata| {
                        metadata.is_file() && metadata.len() <= 128 * 1024 * 1024
                    })
                {
                    copy_once(&source, &settings.join("catalogs").join(entry.file_name()))?;
                }
            }
        }
    }
    // Called before creating the app or any worker thread. These affect only this process.
    unsafe {
        std::env::set_var("APPDATA", roaming);
        std::env::set_var("AMP_VERSE_CONFIG_DIR", settings);
        std::env::set_var("AMP_DOCUMENTS_DIR", profile.join("Documents"));
        std::env::set_var("AMP_V1_MODE", "1");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_are_copied_once_without_replacing_preview_changes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("v0");
        let target = dir.path().join("preview/settings");
        std::fs::write(&source, "old").unwrap();
        copy_once(&source, &target).unwrap();
        std::fs::write(&target, "new").unwrap();
        copy_once(&source, &target).unwrap();
        assert_eq!(std::fs::read_to_string(target).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(source).unwrap(), "old");
    }
}
