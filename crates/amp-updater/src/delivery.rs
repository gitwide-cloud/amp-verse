use crate::{
    package::{self, Manifest, Package},
    release::{self, Release},
};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
pub const GITHUB: &str = "https://github.com/gitwide-cloud/amp-verse";
pub const FEED: &str = "https://api.github.com/repos/gitwide-cloud/amp-verse/releases?per_page=100";
pub fn read(url: &str, limit: u64, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Update cancelled".into());
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(1800)))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_per_read(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .header("User-Agent", "AMPVerse-Updater")
        .call()
        .map_err(|e| format!("Update download failed: {e}"))?;
    let mut data = Vec::new();
    let mut buf = [0; 65536];
    let mut input = response.body_mut().as_reader();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Update cancelled".into());
        }
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if data.len() as u64 + n as u64 > limit {
            return Err("Update exceeds expected size".into());
        }
        data.extend_from_slice(&buf[..n]);
    }
    Ok(data)
}
pub fn url(tag: &str, name: &str) -> Result<String, String> {
    release::version(tag)?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        || name.contains("..")
    {
        return Err("Invalid release asset".into());
    }
    Ok(format!("{GITHUB}/releases/download/{tag}/{name}"))
}
pub struct Update {
    pub version: String,
    pub tag: String,
    pub package: Package,
    pub packages: std::collections::BTreeMap<String, Package>,
}
pub fn resolve(installed: &str, key: &str, cancel: &AtomicBool) -> Result<Option<Update>, String> {
    let releases: Vec<Release> =
        serde_json::from_slice(&read(FEED, 4 * 1024 * 1024, cancel)?).map_err(|e| e.to_string())?;
    let Some(release) = release::select(&releases, installed)? else {
        return Ok(None);
    };
    let manifest: Manifest = serde_json::from_slice(&read(
        &url(&release.tag_name, "amp-update.json")?,
        1048576,
        cancel,
    )?)
    .map_err(|e| format!("Invalid update manifest: {e}"))?;
    if manifest.schema != 1
        || release::version(&manifest.version)? != release::version(&release.tag_name)?
    {
        return Err("Update manifest version mismatch".into());
    }
    let package = manifest
        .packages
        .get(key)
        .filter(|p| {
            package::valid_package(p)
                && p.name.ends_with(".zip")
                && release.assets.iter().any(|a| a.name == p.name)
        })
        .ok_or("This release has no complete compatible update package")?
        .clone();
    Ok(Some(Update {
        version: manifest.version,
        tag: release.tag_name,
        package,
        packages: manifest.packages,
    }))
}
pub fn extract(
    update: &Update,
    root: &Path,
    cancel: &AtomicBool,
) -> Result<(tempfile::TempDir, Vec<PathBuf>), String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let temp = tempfile::TempDir::new_in(root).map_err(|e| e.to_string())?;
    let archive = temp.path().join(&update.package.name);
    std::fs::write(
        &archive,
        read(
            &url(&update.tag, &update.package.name)?,
            update.package.size,
            cancel,
        )?,
    )
    .map_err(|e| e.to_string())?;
    package::verify(&archive, &update.package)?;
    let files = package::extract(&archive, &temp.path().join("files"))?;
    Ok((temp, files))
}
