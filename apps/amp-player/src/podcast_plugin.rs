//! Optional source plugin package. Host supplies UI and the existing audio engine.
use serde::{Deserialize, Serialize};
use std::{io::Write, path::PathBuf};
#[derive(Serialize, Deserialize)]
struct Package {
    id: String,
    version: String,
    host_api: u32,
}
fn file() -> Result<PathBuf, String> {
    Ok(std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Application folder unavailable")?
        .join("addons/amp-podcasts.ampplugin"))
}
pub fn installed() -> bool {
    file()
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Package>(&b).ok())
        .is_some_and(|p| p.id == "amp-podcasts" && p.host_api == 1)
}
fn preferences() -> Option<PathBuf> {
    crate::theme::settings_path().map(|p| p.with_file_name("podcast-plugin-enabled.txt"))
}
pub fn enabled() -> bool {
    installed()
        && preferences()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .is_none_or(|s| s.trim() != "disabled")
}
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let p = preferences().ok_or("Settings unavailable")?;
    std::fs::create_dir_all(p.parent().ok_or("Settings unavailable")?)
        .map_err(|e| e.to_string())?;
    std::fs::write(p, if enabled { "enabled" } else { "disabled" }).map_err(|e| e.to_string())
}
fn validate(bytes: &[u8]) -> Result<Package, String> {
    if bytes.len() > 65536 {
        return Err("Podcast package exceeds the supported size".into());
    }
    let package: Package = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if package.id != "amp-podcasts" || package.host_api != 1 || package.version.is_empty() {
        return Err("Unsupported AMP Podcasts package. Update AMP Verse first.".into());
    }
    Ok(package)
}
pub fn install(bytes: &[u8]) -> Result<(), String> {
    install_file(&file()?, bytes)?;
    set_enabled(true)
}
fn install_file(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    validate(bytes)?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let mut temp =
        tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(|e| e.to_string())?;
    temp.write_all(bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn uninstall() -> Result<(), String> {
    if let Ok(path) = file()
        && path.is_file()
    {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn download() -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let release = crate::media_lookup::bytes(
        "https://api.github.com/repos/gitwide-cloud/amp-verse/releases?per_page=20",
        2 * 1024 * 1024,
    )?;
    let value: serde_json::Value = serde_json::from_slice(&release).map_err(|e| e.to_string())?;
    let releases = value.as_array().ok_or("Invalid GitHub release response")?;
    let (assets,asset)=releases.iter().filter(|r|r["draft"].as_bool()!=Some(true)).filter_map(|r|r["assets"].as_array()).find_map(|assets|assets.iter().find(|a|a["name"].as_str().is_some_and(|s|s.starts_with("amp-podcasts-")&&s.ends_with(".ampplugin"))).map(|asset|(assets,asset))).ok_or("No AMP Podcasts package has been published. Use Install downloaded package or open GitHub Releases.")?;
    let name = asset["name"].as_str().ok_or("Missing package name")?;
    let checksum = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(&format!("{name}.sha256")))
        .ok_or("Package checksum missing; installation cancelled")?;
    let read = |asset: &serde_json::Value, limit| -> Result<Vec<u8>, String> {
        let u = asset["browser_download_url"]
            .as_str()
            .ok_or("Invalid GitHub asset")?;
        if !u.starts_with("https://github.com/gitwide-cloud/amp-verse/releases/download/") {
            return Err("Untrusted package source".into());
        }
        crate::media_lookup::bytes(u, limit)
    };
    let data = read(asset, 65536)?;
    let sum = read(checksum, 1024)?;
    let text = std::str::from_utf8(&sum).map_err(|e| e.to_string())?;
    if text.split_whitespace().next() != Some(&format!("{:x}", Sha256::digest(&data))) {
        return Err("Podcast package checksum mismatch; previous installation retained".into());
    }
    install(&data)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn package_update_and_removal_preserve_user_data() {
        let directory = tempfile::tempdir().unwrap();
        let addon = directory.path().join("addons/amp-podcasts.ampplugin");
        let data = directory.path().join("media.ampdb");
        let download = directory.path().join("episode.mp3");
        std::fs::write(&data, b"subscriptions and progress").unwrap();
        std::fs::write(&download, b"completed audio").unwrap();
        install_file(
            &addon,
            br#"{"id":"amp-podcasts","version":"1.0.16","host_api":1}"#,
        )
        .unwrap();
        let prior = std::fs::read(&addon).unwrap();
        assert!(install_file(&addon, b"broken package").is_err());
        assert_eq!(std::fs::read(&addon).unwrap(), prior);
        install_file(
            &addon,
            br#"{"id":"amp-podcasts","version":"1.0.17","host_api":1}"#,
        )
        .unwrap();
        std::fs::remove_file(addon).unwrap();
        assert_eq!(std::fs::read(data).unwrap(), b"subscriptions and progress");
        assert_eq!(std::fs::read(download).unwrap(), b"completed audio");
    }
    #[test]
    fn package_validation_rejects_wrong_plugin_and_incompatible_api() {
        assert!(validate(br#"{"id":"amp-podcasts","version":"1.0.16","host_api":1}"#).is_ok());
        for bytes in [
            br#"{"id":"other","version":"1","host_api":1}"#.as_slice(),
            br#"{"id":"amp-podcasts","version":"1","host_api":2}"#,
            b"invalid",
        ] {
            assert!(validate(bytes).is_err());
        }
    }
}
