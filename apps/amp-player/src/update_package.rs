//! Validated release format and transactional portable-package installation.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};
#[derive(Clone, Deserialize, Serialize)]
pub struct Package {
    pub name: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub version_code: u64,
}
#[derive(Deserialize, Serialize)]
pub struct Manifest {
    pub schema: u32,
    pub version: String,
    pub packages: std::collections::BTreeMap<String, Package>,
}
pub fn version(s: &str) -> Result<[u32; 3], String> {
    let parts = s
        .strip_prefix('v')
        .unwrap_or(s)
        .split('.')
        .map(|p| {
            if p.is_empty() || p.len() > 8 || !p.bytes().all(|b| b.is_ascii_digit()) {
                Err("Invalid version".into())
            } else {
                p.parse().map_err(|_| "Invalid version".into())
            }
        })
        .collect::<Result<Vec<u32>, String>>()?;
    parts
        .try_into()
        .map_err(|_| "Expected a three-part version".into())
}
pub fn valid_package(p: &Package) -> bool {
    !p.name.is_empty()
        && p.name.len() < 180
        && p.name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        && !p.name.contains("..")
        && p.size > 0
        && p.size <= 1024 * 1024 * 1024
        && p.sha256.len() == 64
        && p.sha256.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn verify(path: &Path, p: &Package) -> Result<(), String> {
    if !valid_package(p) {
        return Err("Invalid update package description".into());
    }
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        size += n as u64;
        if size > p.size {
            return Err("Update size mismatch".into());
        }
        hash.update(&buf[..n]);
    }
    if size != p.size || !format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&p.sha256) {
        return Err("Update checksum mismatch; installed files were kept".into());
    }
    Ok(())
}
pub fn extract(archive: &Path, stage: &Path) -> Result<Vec<PathBuf>, String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if zip.len() > 20000 {
        return Err("Update contains too many files".into());
    }
    let mut total = 0u64;
    let mut seen = std::collections::HashSet::new();
    let mut paths = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let path = entry.enclosed_name().ok_or("Unsafe update path")?;
        if path
            .components()
            .any(|p| !matches!(p, std::path::Component::Normal(_)))
            || entry.name().contains(['\\', ':'])
            || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err("Unsafe update entry".into());
        }
        if !seen.insert(path.to_string_lossy().to_lowercase()) {
            return Err("Duplicate update path".into());
        }
        total = total.checked_add(entry.size()).ok_or("Update too large")?;
        if total > 4 * 1024 * 1024 * 1024 {
            return Err("Update expands beyond limit".into());
        }
        let target = stage.join(&path);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        std::fs::create_dir_all(target.parent().ok_or("Invalid update path")?)
            .map_err(|e| e.to_string())?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                &target,
                std::fs::Permissions::from_mode(entry.unix_mode().unwrap_or(0o644) & 0o777),
            )
            .map_err(|e| e.to_string())?;
        }
        paths.push(path);
    }
    Ok(paths)
}
fn no_links(root: &Path, relative: &Path) -> Result<(), String> {
    let mut path = root.to_path_buf();
    for part in relative.components() {
        path.push(part);
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if meta.is_symlink() {
                return Err("Update destination contains a symbolic link".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("Update destination contains a redirected path".into());
                }
            }
        }
    }
    Ok(())
}
pub fn install(stage: &Path, destination: &Path, files: &[PathBuf]) -> Result<(), String> {
    let root = destination.canonicalize().map_err(|e| e.to_string())?;
    let backup = tempfile::TempDir::new_in(&root)
        .map_err(|e| format!("Application folder is not writable: {e}"))?;
    let mut done: Vec<PathBuf> = Vec::new();
    for relative in files {
        let apply = (|| -> Result<(), String> {
            if relative.as_os_str().is_empty()
                || relative
                    .components()
                    .any(|p| !matches!(p, std::path::Component::Normal(_)))
            {
                return Err("Unsafe install path".into());
            }
            no_links(&root, relative)?;
            let target = root.join(relative);
            let source = stage.join(relative);
            let parent = target.parent().ok_or("Missing install folder")?;
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            let mut prepared =
                tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
            std::io::copy(
                &mut std::fs::File::open(&source).map_err(|e| e.to_string())?,
                &mut prepared,
            )
            .map_err(|e| e.to_string())?;
            prepared
                .as_file()
                .set_permissions(
                    std::fs::metadata(source)
                        .map_err(|e| e.to_string())?
                        .permissions(),
                )
                .map_err(|e| e.to_string())?;
            prepared.as_file().sync_all().map_err(|e| e.to_string())?;
            if target.exists() {
                let saved = backup.path().join(relative);
                std::fs::create_dir_all(saved.parent().unwrap()).map_err(|e| e.to_string())?;
                std::fs::rename(&target, saved)
                    .map_err(|e| format!("Close other AMP apps and retry: {e}"))?;
            }
            done.push(relative.clone());
            prepared.persist(&target).map_err(|e| e.to_string())?;
            Ok(())
        })();
        if let Err(error) = apply {
            let mut recovery_errors = Vec::new();
            for path in done.iter().rev() {
                let target = root.join(path);
                if target.exists()
                    && let Err(e) = std::fs::remove_file(&target)
                {
                    recovery_errors.push(e.to_string());
                    continue;
                }
                let old = backup.path().join(path);
                if old.exists()
                    && let Err(e) = std::fs::rename(old, target)
                {
                    recovery_errors.push(e.to_string());
                }
            }
            if !recovery_errors.is_empty() {
                let kept = backup.keep();
                return Err(format!(
                    "{error}. Backup retained at {}: {}",
                    kept.display(),
                    recovery_errors.join("; ")
                ));
            }
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verifies_exact_download_and_rejects_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("update.zip");
        std::fs::write(&file, b"verified").unwrap();
        let package = Package {
            name: "amp-v1.2.3.zip".into(),
            size: 8,
            sha256: format!("{:x}", Sha256::digest(b"verified")),
            version_code: 0,
        };
        assert!(verify(&file, &package).is_ok());
        std::fs::write(&file, b"modified").unwrap();
        assert!(verify(&file, &package).is_err());
        assert!(version("v1.2.3").unwrap() > version("1.2.2").unwrap());
        assert!(version("1.2.3-dev").is_err());
    }
    #[test]
    fn failed_install_restores_old_files() {
        let stage = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        std::fs::write(stage.path().join("player"), b"new").unwrap();
        std::fs::write(destination.path().join("player"), b"old").unwrap();
        assert!(
            install(
                stage.path(),
                destination.path(),
                &["player".into(), "missing".into()]
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read(destination.path().join("player")).unwrap(),
            b"old"
        );
        install(stage.path(), destination.path(), &["player".into()]).unwrap();
        assert_eq!(
            std::fs::read(destination.path().join("player")).unwrap(),
            b"new"
        );
        assert!(install(stage.path(), destination.path(), &["../escape".into()]).is_err());
    }
    #[test]
    fn archive_cannot_escape_or_replace_case_alias() {
        for entries in [vec!["../outside"], vec!["Player", "player"]] {
            let dir = tempfile::tempdir().unwrap();
            let archive = dir.path().join("update.zip");
            let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
            for name in entries {
                writer
                    .start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(b"data").unwrap();
            }
            writer.finish().unwrap();
            assert!(extract(&archive, &dir.path().join("stage")).is_err());
            assert!(!dir.path().join("outside").exists());
        }
    }
}
