//! Explicit release download; never executes the downloaded engine.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{io::Write, path::Path, time::Duration};

fn asset() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("amp-engine-windows-x86_64.exe"),
        ("linux", "x86_64") => Ok("amp-engine-linux-x86_64"),
        _ => Err(
            "No engine build is available for this platform. Open Releases for manual downloads."
                .into(),
        ),
    }
}

const RELEASES: &str = "https://api.github.com/repos/gitwide-cloud/amp-verse/releases";
const DOWNLOADS: &str = "https://github.com/gitwide-cloud/amp-verse/releases/download/";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    state: String,
}

#[derive(Deserialize)]
struct EngineManifest {
    schema: u32,
    component: String,
    release: String,
    platform: String,
    engine_id: String,
    protocol: u32,
    asset: String,
}

// Only stable numeric release tags participate; ordering never uses GitHub's
// publication date or the repository-wide latest pointer (also used by Android).
fn version(tag: &str) -> Option<[u64; 3]> {
    let parts: Vec<_> = tag.strip_prefix('v')?.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let mut result = [0; 3];
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty()
            || !part.bytes().all(|b| b.is_ascii_digit())
            || (part.len() > 1 && part.starts_with('0'))
        {
            return None;
        }
        result[i] = part.parse().ok()?;
    }
    Some(result)
}

fn release_asset<'a>(release: &'a Release, name: &str) -> Option<&'a str> {
    let mut matches = release.assets.iter().filter(|a| a.name == name);
    let asset = matches.next()?;
    let expected = format!("{DOWNLOADS}{}/{name}", release.tag_name);
    (matches.next().is_none()
        && asset.state == "uploaded"
        && asset.browser_download_url == expected)
        .then_some(asset.browser_download_url.as_str())
}

fn select_release<F>(
    mut releases: Vec<Release>,
    asset: &str,
    platform: &str,
    read: &mut F,
) -> Result<(String, String), String>
where
    F: FnMut(&str, u64) -> Result<Vec<u8>, String>,
{
    releases.retain(|r| !r.draft && !r.prerelease && version(&r.tag_name).is_some());
    releases.sort_by_key(|r| std::cmp::Reverse(version(&r.tag_name)));
    for release in releases {
        let (Some(binary), Some(checksum), Some(metadata)) = (
            release_asset(&release, asset),
            release_asset(&release, &format!("{asset}.sha256")),
            release_asset(&release, &format!("{asset}.json")),
        ) else {
            continue;
        };
        // Transport errors fail closed instead of silently selecting an older release.
        let bytes = read(metadata, 16 * 1024)?;
        let Ok(manifest) = serde_json::from_slice::<EngineManifest>(&bytes) else {
            continue;
        };
        if manifest.schema == 1
            && manifest.component == "amp-engine"
            && manifest.release == release.tag_name
            && manifest.platform == platform
            && manifest.engine_id == amp_core::engine_api::ENGINE_ID
            && manifest.protocol == amp_core::engine_api::PROTOCOL
            && manifest.asset == asset
        {
            return Ok((binary.into(), checksum.into()));
        }
    }
    Err("No compatible desktop engine release with verified metadata and checksum is available. Existing engine was preserved; retry after a compatible release is published.".into())
}

fn download_with<F>(
    directory: &Path,
    asset: &str,
    platform: &str,
    mut read: F,
) -> Result<(), String>
where
    F: FnMut(&str, u64) -> Result<Vec<u8>, String>,
{
    let mut releases = Vec::new();
    // Bound API work; fail closed if the catalog cannot be read completely.
    for page in 1..=10 {
        let bytes = read(
            &format!("{RELEASES}?per_page=100&page={page}"),
            4 * 1024 * 1024,
        )?;
        let batch: Vec<Release> = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let complete = batch.len() < 100;
        releases.extend(batch);
        if complete {
            let (binary, checksum) = select_release(releases, asset, platform, &mut read)?;
            let checksum = read(&checksum, 1024)?;
            let bytes = read(&binary, 32 * 1024 * 1024)?;
            return install(
                directory,
                &bytes,
                std::str::from_utf8(&checksum).map_err(|e| e.to_string())?,
            );
        }
    }
    Err("Release catalog exceeds lookup limit; existing engine was preserved.".into())
}

fn install(directory: &Path, bytes: &[u8], checksum: &str) -> Result<(), String> {
    let expected = checksum.split_whitespace().next().unwrap_or("");
    if expected.len() != 64
        || !expected.bytes().all(|b| b.is_ascii_hexdigit())
        || !format!("{:x}", Sha256::digest(bytes)).eq_ignore_ascii_case(expected)
    {
        return Err("Checksum mismatch; existing engine was preserved.".into());
    }
    if bytes.len() < 4 || !(bytes.starts_with(b"MZ") || bytes.starts_with(b"\x7fELF")) {
        return Err("Release file is not an executable; existing engine was preserved.".into());
    }
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    file.persist(directory.join(if cfg!(windows) {
        "amp-engine.exe"
    } else {
        "amp-engine"
    }))
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn download() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    download_to(
        &exe.parent()
            .ok_or("Missing application directory")?
            .join("addons"),
    )
}

fn download_to(directory: &Path) -> Result<(), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into();
    let read = |url: &str, limit| -> Result<Vec<u8>, String> {
        let mut response = agent
            .get(url)
            .header("User-Agent", "AMP-Verse-addon-installer")
            .call()
            .map_err(|e| format!("GitHub download: {e}. Retry or use manual installation."))?;
        response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|e| e.to_string())
    };
    download_with(
        directory,
        asset()?,
        &format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        read,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_fixture_installs_only_selected_verified_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let mut urls = Vec::new();
        download_with(
            directory.path(),
            "amp-engine-windows-x86_64.exe",
            "windows-x86_64",
            |url, _| {
                urls.push(url.to_string());
                Ok(if url.starts_with(RELEASES) {
                    serde_json::to_vec(&serde_json::json!([fixture("v0.3.0")])).unwrap()
                } else if url.ends_with(".json") {
                    serde_json::to_vec(&metadata("v0.3.0")).unwrap()
                } else if url.ends_with(".sha256") {
                    format!(
                        "{:x}  amp-engine-windows-x86_64.exe",
                        Sha256::digest(b"MZnew")
                    )
                    .into_bytes()
                } else {
                    b"MZnew".to_vec()
                })
            },
        )
        .unwrap();
        assert_eq!(urls.len(), 4);
        assert!(urls[1..].iter().all(|url| url.contains("/v0.3.0/")));
        assert_eq!(
            std::fs::read(directory.path().join(if cfg!(windows) {
                "amp-engine.exe"
            } else {
                "amp-engine"
            }))
            .unwrap(),
            b"MZnew"
        );
    }

    #[test]
    fn invalid_executable_and_persist_failure_leave_target_intact() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join(if cfg!(windows) {
            "amp-engine.exe"
        } else {
            "amp-engine"
        });
        std::fs::write(&target, b"existing").unwrap();
        assert!(
            install(
                directory.path(),
                b"html",
                &format!("{:x}", Sha256::digest(b"html"))
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"existing");
        std::fs::remove_file(&target).unwrap();
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep"), b"existing").unwrap();
        assert!(
            install(
                directory.path(),
                b"MZnew",
                &format!("{:x}", Sha256::digest(b"MZnew"))
            )
            .is_err()
        );
        assert_eq!(std::fs::read(target.join("keep")).unwrap(), b"existing");
    }

    fn fixture(tag: &str) -> serde_json::Value {
        let name = "amp-engine-windows-x86_64.exe";
        serde_json::json!({"tag_name":tag,"draft":false,"prerelease":false,"assets":
            [name.to_string(),format!("{name}.sha256"),format!("{name}.json")].iter().map(|name|
                serde_json::json!({"name":name,"state":"uploaded","browser_download_url":format!("{DOWNLOADS}{tag}/{name}")})).collect::<Vec<_>>()})
    }

    fn metadata(tag: &str) -> serde_json::Value {
        serde_json::json!({"schema":1,"component":"amp-engine","release":tag,
            "platform":"windows-x86_64","engine_id":amp_core::engine_api::ENGINE_ID,
            "protocol":amp_core::engine_api::PROTOCOL,"asset":"amp-engine-windows-x86_64.exe"})
    }

    fn select(
        fixtures: serde_json::Value,
        manifest: serde_json::Value,
    ) -> Result<(String, String), String> {
        select_release(
            serde_json::from_value(fixtures).unwrap(),
            "amp-engine-windows-x86_64.exe",
            "windows-x86_64",
            &mut |_, _| Ok(serde_json::to_vec(&manifest).unwrap()),
        )
    }

    #[test]
    fn android_only_latest_skipped_without_blind_legacy_fallback() {
        let mut android = fixture("v0.3.1");
        android["assets"] = serde_json::json!([]);
        let mut legacy = fixture("v0.2.5");
        legacy["assets"].as_array_mut().unwrap().pop();
        assert!(select(serde_json::json!([android, legacy]), metadata("v0.2.5")).is_err());
        let result = select(
            serde_json::json!([android, fixture("v0.3.0")]),
            metadata("v0.3.0"),
        )
        .unwrap();
        assert!(result.0.contains("/v0.3.0/"));
        assert!(result.1.contains("/v0.3.0/"));
    }

    #[test]
    fn stable_versions_ordered_numerically() {
        assert!(version("v0.10.0") > version("v0.9.0"));
        for tag in ["v01.2.3", "v1.2", "v1.2.3-rc.1", "android-v1.2.3"] {
            assert!(version(tag).is_none());
        }
        let chosen = select(
            serde_json::json!([fixture("v0.9.0"), fixture("v0.10.0")]),
            metadata("v0.10.0"),
        )
        .unwrap();
        assert!(chosen.0.contains("/v0.10.0/"));
    }

    #[test]
    fn rejects_missing_assets_unpublished_and_incompatible_metadata() {
        for index in 0..3 {
            let mut release = fixture("v0.3.0");
            release["assets"].as_array_mut().unwrap().remove(index);
            assert!(select(serde_json::json!([release]), metadata("v0.3.0")).is_err());
        }
        for field in ["draft", "prerelease"] {
            let mut release = fixture("v0.3.0");
            release[field] = true.into();
            assert!(select(serde_json::json!([release]), metadata("v0.3.0")).is_err());
        }
        for (field, value) in [
            ("schema", serde_json::json!(2)),
            ("component", serde_json::json!("android")),
            ("release", serde_json::json!("v0.2.5")),
            ("platform", serde_json::json!("linux-x86_64")),
            ("engine_id", serde_json::json!("amp-prep-heuristic-4")),
            ("engine_id", serde_json::json!("amp-prep-heuristic-6")),
            ("protocol", serde_json::json!(2)),
            ("asset", serde_json::json!("other.exe")),
        ] {
            let mut manifest = metadata("v0.3.0");
            manifest[field] = value;
            assert!(
                select(serde_json::json!([fixture("v0.3.0")]), manifest).is_err(),
                "{field}"
            );
        }
        for (field, value) in [
            ("state", "new"),
            ("browser_download_url", "https://other.invalid/file"),
        ] {
            let mut release = fixture("v0.3.0");
            release["assets"][0][field] = value.into();
            assert!(select(serde_json::json!([release]), metadata("v0.3.0")).is_err());
        }
    }

    #[test]
    fn failures_at_every_download_stage_preserve_existing_installation() {
        for failed_stage in 0..5 {
            let directory = tempfile::tempdir().unwrap();
            let target = directory.path().join(if cfg!(windows) {
                "amp-engine.exe"
            } else {
                "amp-engine"
            });
            std::fs::write(&target, b"existing").unwrap();
            let mut stage = 0;
            let result = download_with(
                directory.path(),
                "amp-engine-windows-x86_64.exe",
                "windows-x86_64",
                |_, _| {
                    let current = stage;
                    stage += 1;
                    if current == failed_stage {
                        return Err("fixture transport failure".into());
                    }
                    Ok(match current {
                        0 => serde_json::to_vec(&serde_json::json!([fixture("v0.3.0")])).unwrap(),
                        1 => serde_json::to_vec(&metadata("v0.3.0")).unwrap(),
                        2 => "0".repeat(64).into_bytes(),
                        _ => b"MZnew".to_vec(),
                    })
                },
            );
            assert!(result.is_err());
            assert_eq!(std::fs::read(&target).unwrap(), b"existing");
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn checksum_failure_preserves_installed_engine() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join(if cfg!(windows) {
            "amp-engine.exe"
        } else {
            "amp-engine"
        });
        std::fs::write(&target, b"existing").unwrap();
        assert!(install(directory.path(), b"MZnew", &"0".repeat(64)).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"existing");
        let bytes = b"MZnew";
        install(
            directory.path(),
            bytes,
            &format!("{:x}", Sha256::digest(bytes)),
        )
        .unwrap();
        assert_eq!(std::fs::read(target).unwrap(), bytes);
    }
    #[test]
    #[ignore = "downloads current public release into an isolated temporary directory"]
    fn public_release_download() {
        let directory = tempfile::tempdir().unwrap();
        download_to(directory.path()).unwrap();
        assert!(
            directory
                .path()
                .join(if cfg!(windows) {
                    "amp-engine.exe"
                } else {
                    "amp-engine"
                })
                .is_file()
        );
    }
}
