//! Explicit release download; never executes the downloaded engine.
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
    let base = format!(
        "https://github.com/gitwide-cloud/amp-verse/releases/latest/download/{}",
        asset()?
    );
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into();
    let read = |url: &str, limit| -> Result<Vec<u8>, String> {
        let mut response = agent
            .get(url)
            .call()
            .map_err(|e| format!("GitHub download: {e}. Retry or use manual installation."))?;
        response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|e| e.to_string())
    };
    let checksum = read(&format!("{base}.sha256"), 1024)?;
    let bytes = read(&base, 32 * 1024 * 1024)?;
    install(
        directory,
        &bytes,
        std::str::from_utf8(&checksum).map_err(|e| e.to_string())?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
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
