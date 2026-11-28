use serde::Deserialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub [u32; 5]);
pub fn version(value: &str) -> Result<Version, String> {
    let value = value.strip_prefix('v').unwrap_or(value);
    let (base, rank, sequence) = if let Some((base, tail)) = value.split_once('-') {
        let (channel, number) = tail.split_once('.').ok_or("Invalid release version")?;
        let rank = match channel {
            "beta" => 0,
            "rc" => 1,
            _ => return Err("Unsupported release channel".into()),
        };
        let number = number
            .parse::<u32>()
            .map_err(|_| "Invalid release sequence")?;
        if number == 0 || !tail.ends_with(&format!(".{number}")) {
            return Err("Invalid release sequence".into());
        }
        (base, rank, number)
    } else {
        (value, 2, 0)
    };
    let pieces: Vec<_> = base.split('.').collect();
    if pieces.len() != 3 {
        return Err("Expected a three-part version".into());
    }
    let mut result = [0, 0, 0, rank, sequence];
    for (i, part) in pieces.iter().enumerate() {
        if part.is_empty()
            || part.len() > 8
            || !part.bytes().all(|b| b.is_ascii_digit())
            || (part.len() > 1 && part.starts_with('0'))
        {
            return Err("Invalid version".into());
        }
        result[i] = part.parse().map_err(|_| "Invalid version")?;
    }
    Ok(Version(result))
}
#[derive(Clone, Deserialize)]
pub struct Asset {
    pub name: String,
}
#[derive(Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}
pub fn select(releases: &[Release], installed: &str) -> Result<Option<Release>, String> {
    let current = version(installed)?;
    Ok(releases
        .iter()
        .filter(|r| {
            let Ok(v) = version(&r.tag_name) else {
                return false;
            };
            let allowed = v.0[3] == 2 || (current.0[3] < 2 && v.0[3] >= current.0[3]);
            !r.draft
                && r.prerelease == (v.0[3] < 2)
                && allowed
                && v > current
                && r.assets.iter().any(|a| a.name == "amp-update.json")
        })
        .max_by_key(|r| version(&r.tag_name).unwrap())
        .cloned())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn release(tag: &str, pre: bool) -> Release {
        Release {
            tag_name: tag.into(),
            draft: false,
            prerelease: pre,
            assets: vec![Asset {
                name: "amp-update.json".into(),
            }],
        }
    }
    #[test]
    fn rc_order_and_stable_promotion() {
        let releases = vec![
            release("v1.1.0-rc.9", true),
            release("v1.1.0-rc.11", true),
            release("v0.3.1", false),
        ];
        assert_eq!(
            select(&releases, "1.1.0-rc.10").unwrap().unwrap().tag_name,
            "v1.1.0-rc.11"
        );
        assert!(select(&releases, "1.1.0").unwrap().is_none());
        assert_eq!(
            select(&[release("v1.1.0", false)], "1.1.0-rc.11")
                .unwrap()
                .unwrap()
                .tag_name,
            "v1.1.0"
        );
        assert!(
            select(&[release("v1.2.0-beta.1", true)], "1.1.0-rc.10")
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn drafts_incomplete_and_mislabeled_are_rejected() {
        let mut r = release("v1.1.0-rc.12", true);
        r.draft = true;
        assert!(select(&[r.clone()], "1.1.0-rc.10").unwrap().is_none());
        r.draft = false;
        r.assets.clear();
        assert!(select(&[r], "1.1.0-rc.10").unwrap().is_none());
        assert!(
            select(&[release("v1.1.0-rc.12", false)], "1.1.0-rc.10")
                .unwrap()
                .is_none()
        );
        for s in [
            "../1.1.0",
            "1.1.0-rc.0",
            "1.1.0-rc.01",
            "1.1.0-preview.1",
            "1.01.0",
        ] {
            assert!(version(s).is_err(), "{s}");
        }
    }
}
