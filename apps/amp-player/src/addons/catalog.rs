use super::ENTRIES;
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Default)]
pub(super) struct Local {
    pub installed: bool,
    pub version: Option<String>,
}
#[derive(Clone, Default)]
pub(super) struct Release {
    pub version: String,
    pub notes: String,
    pub url: String,
}
#[derive(Clone, Default)]
pub(super) struct Catalog {
    pub releases: [Option<Release>; 8],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Filter {
    All,
    Installed,
    Updates,
    New,
    Optional,
    Available,
}
impl Filter {
    pub const ALL: [Self; 6] = [
        Self::All,
        Self::Installed,
        Self::Updates,
        Self::New,
        Self::Optional,
        Self::Available,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All Add-ons",
            Self::Installed => "Installed",
            Self::Updates => "Updates",
            Self::New => "New",
            Self::Optional => "Optional",
            Self::Available => "Available",
        }
    }
    pub fn matches(self, index: usize, local: &Local, update: bool, new: bool) -> bool {
        match self {
            Self::All => true,
            Self::Installed => local.installed,
            Self::Updates => update,
            Self::New => new,
            Self::Optional => index >= 5,
            Self::Available => !local.installed,
        }
    }
}
pub(super) fn newer(candidate: &str, installed: &str) -> bool {
    match (
        amp_updater::release::version(candidate),
        amp_updater::release::version(installed),
    ) {
        (Ok(candidate), Ok(installed)) => candidate > installed,
        _ => false,
    }
}
pub(super) fn update(local: &Local, release: Option<&Release>) -> bool {
    local.installed
        && local
            .version
            .as_deref()
            .zip(release)
            .is_some_and(|(v, r)| newer(&r.version, v))
}
pub(super) fn inventory() -> [Local; 8] {
    let mut local = std::array::from_fn(|_| Local::default());
    for i in [0, 1, 2, 4] {
        local[i] = Local {
            installed: true,
            version: Some(env!("CARGO_PKG_VERSION").into()),
        };
    }
    let root = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    local[3].installed = root.as_ref().is_some_and(|r| {
        r.join(if cfg!(windows) {
            "amp-cast.exe"
        } else {
            "amp-cast"
        })
        .is_file()
    });
    local[5].installed = crate::engine::path().is_some();
    local[6].installed = crate::onnx_stem_worker::configured();
    local[7].installed = crate::podcast_plugin::installed();
    local[7].version = root
        .as_ref()
        .and_then(|r| std::fs::read(r.join("addons/amp-podcasts.ampplugin")).ok())
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v["version"].as_str().map(str::to_owned));
    local
}
pub(super) fn seen() -> BTreeSet<String> {
    let all = ENTRIES.iter().map(|e| e.id.to_owned()).collect();
    let Some(path) =
        crate::theme::settings_path().map(|p| p.with_file_name("addon-catalog-seen.json"))
    else {
        return all;
    };
    match std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
    {
        Some(seen) => seen,
        None => {
            save_seen(&all);
            all
        }
    }
}
pub(super) fn save_seen(seen: &BTreeSet<String>) {
    if let Some(path) =
        crate::theme::settings_path().map(|p| p.with_file_name("addon-catalog-seen.json"))
    {
        let _ = (|| -> Result<(), Box<dyn std::error::Error>> {
            std::fs::create_dir_all(path.parent().ok_or("Settings unavailable")?)?;
            let mut f = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
            serde_json::to_writer(f.as_file_mut(), seen)?;
            f.as_file().sync_all()?;
            f.persist(path)?;
            Ok(())
        })();
    }
}
const DOWNLOADS: &str = "https://github.com/gitwide-cloud/amp-verse/releases/download/";
fn asset<'a>(r: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    let tag = r["tag_name"].as_str()?;
    r["assets"]
        .as_array()?
        .iter()
        .find(|a| a["name"] == name && a["state"] == "uploaded")
        .and_then(|a| a["browser_download_url"].as_str())
        .filter(|u| *u == format!("{DOWNLOADS}{tag}/{name}"))
}
/// Metadata-only lookup. Never installs, loads remote code, or invents catalog entries.
pub(super) fn fetch() -> Result<Catalog, String> {
    let bytes = crate::media_lookup::bytes(
        "https://api.github.com/repos/gitwide-cloud/amp-verse/releases?per_page=50",
        4 * 1024 * 1024,
    )?;
    let releases: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let mut releases = releases
        .as_array()
        .ok_or("Invalid release catalog")?
        .iter()
        .filter(|r| {
            r["draft"] == false
                && r["tag_name"]
                    .as_str()
                    .zip(r["prerelease"].as_bool())
                    .is_some_and(|(tag, pre)| {
                        amp_updater::release::channel_accepts(tag, pre, env!("CARGO_PKG_VERSION"))
                    })
        })
        .collect::<Vec<_>>();
    releases.sort_by_key(|r| {
        std::cmp::Reverse(amp_updater::release::version(r["tag_name"].as_str().unwrap()).unwrap())
    });
    let mut result = Catalog::default();
    for r in releases {
        let info = Release {
            version: r["tag_name"]
                .as_str()
                .unwrap()
                .trim_start_matches('v')
                .into(),
            notes: r["body"]
                .as_str()
                .unwrap_or("No release notes published.")
                .chars()
                .take(12000)
                .collect(),
            url: r["html_url"]
                .as_str()
                .filter(|u| {
                    u.starts_with("https://github.com/gitwide-cloud/amp-verse/releases/tag/")
                })
                .unwrap_or("https://github.com/gitwide-cloud/amp-verse/releases")
                .into(),
        };
        if result.releases[0].is_none()
            && let Some(url) = asset(r, "amp-update.json")
        {
            let b = crate::media_lookup::bytes(url, 1024 * 1024)?;
            if let Ok(m) = serde_json::from_slice::<crate::update_package::Manifest>(&b)
                && m.schema == 1
                && m.version.trim_start_matches('v') == info.version
                && m.packages
                    .get(&format!(
                        "{}-{}",
                        std::env::consts::OS,
                        std::env::consts::ARCH
                    ))
                    .is_some_and(crate::update_package::valid_package)
            {
                for i in [0, 1, 2, 4] {
                    result.releases[i] = Some(info.clone());
                }
            }
        }
        // Podcast package metadata identifies its own version, independent of Android tags.
        if result.releases[7].is_none()
            && let Some(assets) = r["assets"].as_array()
            && let Some(name) = assets
                .iter()
                .filter_map(|a| a["name"].as_str())
                .find(|n| n.starts_with("amp-podcasts-") && n.ends_with(".ampplugin"))
            && let Some(url) = asset(r, name)
            && asset(r, &format!("{name}.sha256")).is_some()
        {
            let b = crate::media_lookup::bytes(url, 65536)?;
            if let Ok(p) = serde_json::from_slice::<serde_json::Value>(&b)
                && p["id"] == "amp-podcasts"
                && p["host_api"] == 1
                && let Some(v) = p["version"]
                    .as_str()
                    .filter(|v| amp_updater::release::version(v).is_ok())
            {
                result.releases[7] = Some(Release {
                    version: v.into(),
                    ..info.clone()
                });
            }
        }
        if result.releases[0].is_some() && result.releases[7].is_some() {
            break;
        }
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn badges_require_installed_and_proven_newer_version() {
        let r = Release {
            version: "1.1.0".into(),
            ..Default::default()
        };
        assert!(!update(&Local::default(), Some(&r)));
        assert!(!update(
            &Local {
                installed: true,
                version: None
            },
            Some(&r)
        ));
        assert!(update(
            &Local {
                installed: true,
                version: Some("1.1.0-rc.1".into())
            },
            Some(&r)
        ));
        assert!(!newer("0.3.1", "1.1.0-rc.1"));
        assert!(!newer("1.1.0-rc.1", "1.1.0"));
        assert!(!newer("garbage", "1.0.0"));
    }
    #[test]
    fn filters_keep_installed_updates_and_new_separate() {
        let l = Local {
            installed: true,
            version: None,
        };
        assert!(Filter::Installed.matches(5, &l, false, false));
        assert!(!Filter::Updates.matches(5, &l, false, false));
        assert!(!Filter::New.matches(5, &l, false, false));
        assert!(!Filter::Available.matches(5, &l, false, false));
        assert!(Filter::Optional.matches(5, &l, false, false));
    }
    #[test]
    fn release_asset_must_be_uploaded_and_from_this_repository() {
        let r = serde_json::json!({"tag_name":"v1.1.0","assets":[{"name":"amp-update.json","state":"uploaded","browser_download_url":"https://evil.example/file"}]});
        assert!(asset(&r, "amp-update.json").is_none());
    }
}
