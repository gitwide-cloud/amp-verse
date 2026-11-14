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

fn seed_preferences(old: &Path, roaming: &Path, settings: &Path) -> std::io::Result<()> {
    let source = old.join("amp-verse/music-root.txt");
    let target = roaming.join("amp-verse/music-root.txt");
    let has_root = std::fs::read_to_string(&target).is_ok_and(|value| !value.trim().is_empty());
    if !has_root
        && let Ok(value) = std::fs::read_to_string(&source)
        && !value.trim().is_empty()
    {
        use std::io::Write;
        let parent = target.parent().unwrap();
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(value.trim().as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(&target).map_err(|error| error.error)?;
    }
    copy_once(&old.join("AMPVerse/theme.txt"), &settings.join("theme.txt"))?;
    // Import only completed startup catalogs, never pending writes or device identity.
    if let Ok(entries) = std::fs::read_dir(old.join("AMPVerse/catalogs")) {
        for entry in entries.flatten() {
            let source = entry.path();
            if source
                .extension()
                .is_some_and(|extension| extension == "json")
                && entry
                    .metadata()
                    .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= 128 * 1024 * 1024)
            {
                copy_once(&source, &settings.join("catalogs").join(entry.file_name()))?;
            }
        }
    }
    Ok(())
}

pub fn initialize() -> std::io::Result<()> {
    if !cfg!(windows) {
        return Ok(());
    }
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return Ok(());
    };
    let profile = std::path::PathBuf::from(local).join("AMPVerse/V1Preview");
    let configured = std::env::var_os("AMP_VERSE_CONFIG_DIR");
    let roaming = if configured.is_some() {
        std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| profile.join("AppData"))
    } else {
        profile.join("AppData")
    };
    let settings = configured
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| profile.join("settings"));
    std::fs::create_dir_all(&settings)?;
    if let Some(old) = std::env::var_os("APPDATA") {
        seed_preferences(Path::new(&old), &roaming, &settings)?;
    }
    // The installed launcher sets APPDATA and CONFIG before starting us. It must
    // still recover a missing preference from the user's original Windows profile.
    if let Some(home) = std::env::var_os("USERPROFILE") {
        seed_preferences(
            &std::path::PathBuf::from(home).join("AppData/Roaming"),
            &roaming,
            &settings,
        )?;
    }
    if configured.is_some() {
        return Ok(());
    }
    // Called before creating the app or any worker thread; process-local only.
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
    #[test]
    fn launcher_profile_recovers_missing_or_blank_root_without_requiring_nas_online() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("legacy");
        let roaming = dir.path().join("preview/AppData");
        let settings = dir.path().join("preview/settings");
        std::fs::create_dir_all(old.join("amp-verse")).unwrap();
        std::fs::write(old.join("amp-verse/music-root.txt"), r"\\offline-nas\music").unwrap();
        seed_preferences(&old, &roaming, &settings).unwrap();
        let pref = roaming.join("amp-verse/music-root.txt");
        assert_eq!(
            std::fs::read_to_string(&pref).unwrap(),
            r"\\offline-nas\music"
        );
        std::fs::write(&pref, "  ").unwrap();
        seed_preferences(&old, &roaming, &settings).unwrap();
        assert!(!std::fs::read_to_string(&pref).unwrap().trim().is_empty());
        std::fs::write(&pref, "chosen-library").unwrap();
        seed_preferences(&old, &roaming, &settings).unwrap();
        assert_eq!(std::fs::read_to_string(&pref).unwrap(), "chosen-library");
    }
}
