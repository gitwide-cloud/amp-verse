//! Conservative live-only admission; ordinary six-deck playback is independent.
use std::{
    io::{self, Write},
    path::Path,
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering::Relaxed},
    },
};
static MAXIMUM: OnceLock<AtomicUsize> = OnceLock::new();
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Policy {
    pub limit: usize,
    pub hardware_limit: usize,
    pub reason: &'static str,
}
impl Policy {
    pub fn new(requested: usize, hardware_limit: usize) -> Self {
        let hardware_limit = if matches!(hardware_limit, 4 | 6) {
            hardware_limit
        } else {
            2
        };
        let requested = if matches!(requested, 2 | 4 | 6) {
            requested
        } else {
            4
        };
        Self {
            limit: requested.min(hardware_limit),
            hardware_limit,
            reason: if hardware_limit > 2 {
                "This device passed live-stem qualification"
            } else {
                "Conservative CPU / unqualified hardware limit"
            },
        }
    }
    /// Replacing an already reserved deck does not consume a second reservation.
    pub fn allows(self, reserved: &[bool; 6], deck: usize) -> bool {
        deck < reserved.len()
            && (reserved[deck] || reserved.iter().filter(|v| **v).count() < self.limit)
    }
    pub fn error(self) -> String {
        format!(
            "Live stems are limited to {} decks: {}. Use original audio on another deck first. Normal six-deck playback remains available.",
            self.limit, self.reason
        )
    }
}
fn path() -> Option<std::path::PathBuf> {
    crate::theme::settings_path().map(|p| p.with_file_name("live-stem-maximum.txt"))
}
fn read(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .filter(|v| matches!(v, 2 | 4 | 6))
        .unwrap_or(4)
}
pub(crate) fn maximum() -> usize {
    MAXIMUM
        .get_or_init(|| AtomicUsize::new(path().map_or(4, |p| read(&p))))
        .load(Relaxed)
}
pub(crate) fn current() -> Policy {
    #[cfg(test)]
    if std::env::var("AMP_STEM_TEST_QUALIFIED_GPU").as_deref() == Ok("1")
        && std::env::var("AMP_ONNX_PROVIDER").as_deref() == Ok("cuda")
    {
        return Policy::new(6, 6);
    }
    Policy::new(
        maximum(),
        if crate::onnx_stem_worker::selected_cuda() {
            hardware_limit()
        } else {
            2
        },
    )
}
// Never infer performance from the module's qualification on somebody else's GPU.
static HARDWARE: OnceLock<AtomicUsize> = OnceLock::new();
fn hardware_limit() -> usize {
    let state = HARDWARE.get_or_init(|| {
        let state = AtomicUsize::new(2);
        let _ = std::thread::Builder::new()
            .name("amp-stem-qualification".into())
            .spawn(|| {
                if let Ok(limit) = verify_local_receipt() {
                    loop {
                        if let Some(state) = HARDWARE.get() {
                            state.store(limit, Relaxed);
                            break;
                        }
                        std::thread::yield_now();
                    }
                }
            });
        state
    });
    state.load(Relaxed)
}
fn receipt_matches(
    value: &serde_json::Value,
    host: &str,
    uuid: &str,
    worker_sha: &str,
    model_sha: &str,
    runtime_manifest_sha: &str,
) -> Option<usize> {
    let maximum = value["max_live_decks"].as_u64()? as usize;
    (value["version"] == 1
        && matches!(maximum, 4 | 6)
        && value["provider"] == "cuda"
        && value["runtime_manifest_sha256"] == runtime_manifest_sha
        && value["host"] == host
        && value["adapter_uuid"] == uuid
        && value["worker_sha256"] == worker_sha
        && value["model_sha256"] == model_sha
        && model_sha == "98228931494151762a1c4ab1ec7899a894b1f81fd4a509921a9d2f9bacc50845"
        && value["proof_source"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        && value["proof_run"].as_str().is_some_and(|s| !s.is_empty()))
    .then_some(maximum)
}
fn parse_host_identity(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.len() <= 255 && !value.chars().any(char::is_control))
        .then(|| value.to_owned())
}
#[cfg(any(target_os = "linux", test))]
fn bounded_hostname(mut input: impl std::io::Read) -> Result<String, String> {
    use std::io::Read;
    let mut value = String::new();
    input
        .by_ref()
        .take(257)
        .read_to_string(&mut value)
        .map_err(|e| e.to_string())?;
    if value.len() > 256 {
        return Err("Host identity oversized".into());
    }
    parse_host_identity(&value).ok_or_else(|| "Host identity unavailable".into())
}
fn host_identity() -> Result<String, String> {
    for key in ["COMPUTERNAME", "HOSTNAME"] {
        if let Some(host) = std::env::var(key)
            .ok()
            .and_then(|value| parse_host_identity(&value))
        {
            return Ok(host);
        }
    }
    #[cfg(target_os = "linux")]
    {
        bounded_hostname(
            std::fs::File::open("/proc/sys/kernel/hostname").map_err(|e| e.to_string())?,
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("Host identity unavailable".into())
    }
}
fn verify_local_receipt() -> Result<usize, String> {
    use sha2::{Digest, Sha256};
    use std::{
        io::Read,
        process::Stdio,
        time::{Duration, Instant},
    };
    let (worker, model, runtime_manifest) =
        crate::onnx_stem_worker::qualification_files().ok_or("No qualified CUDA runtime")?;
    let path = path()
        .ok_or("Settings unavailable")?
        .with_file_name("live-stem-qualification.json");
    let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 8192 {
        return Err("Invalid qualification receipt".into());
    }
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let host = host_identity()?;
    let mut child = amp_core::command("nvidia-smi")
        .args(["--id=0", "--query-gpu=uuid", "--format=csv,noheader"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return Err("GPU identity unavailable".into()),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.to_string());
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("GPU identity timed out".into());
            }
            _ => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    let mut uuid = String::new();
    child
        .stdout
        .take()
        .ok_or("GPU identity missing")?
        .take(256)
        .read_to_string(&mut uuid)
        .map_err(|e| e.to_string())?;
    let hash_deadline = Instant::now() + Duration::from_secs(60);
    fn digest(path: &Path, limit: u64, deadline: Instant) -> Result<String, String> {
        let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > limit {
            return Err("Qualification file is linked or oversized".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("Linked qualification file".into());
            }
        }

        let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut sha = Sha256::new();
        let mut buf = [0u8; 65536];
        let mut bytes = 0u64;
        loop {
            if Instant::now() >= deadline {
                return Err("Qualification hashing timed out".into());
            }
            let n = file.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
            if bytes > limit {
                return Err("Qualification file grew beyond limit".into());
            }
            sha.update(&buf[..n]);
        }
        Ok(format!("{:x}", sha.finalize()))
    }
    receipt_matches(
        &receipt,
        &host,
        uuid.trim(),
        &digest(&worker, 256 * 1024 * 1024, hash_deadline)?,
        &digest(&model, 32 * 1024 * 1024, hash_deadline)?,
        &verify_runtime_manifest(&runtime_manifest, hash_deadline)?,
    )
    .ok_or("Device qualification does not match this hardware/model/worker".into())
}
fn verify_runtime_manifest(path: &Path, deadline: std::time::Instant) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 16384 {
        return Err("Invalid GPU runtime manifest".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(16385)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16384 {
        return Err("GPU runtime manifest grew".into());
    }
    let manifest: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let files = manifest["files"]
        .as_array()
        .filter(|a| a.len() <= 64)
        .ok_or("Missing bounded GPU runtime inventory")?;
    let root = path.parent().ok_or("GPU runtime directory unavailable")?;
    for directory in [root.to_path_buf(), root.join("vendor")] {
        let m = std::fs::symlink_metadata(directory).map_err(|e| e.to_string())?;
        if !m.is_dir() || m.file_type().is_symlink() {
            return Err("Linked GPU runtime directory".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if m.file_attributes() & 0x400 != 0 {
                return Err("Linked GPU runtime directory".into());
            }
        }
    }
    let required = [
        "onnxruntime.dll".to_owned(),
        "onnxruntime_providers_shared.dll".to_owned(),
        "onnxruntime_providers_cuda.dll".to_owned(),
    ]
    .into_iter()
    .chain(
        crate::onnx_stem_worker::CUDA_VENDOR_DLLS
            .iter()
            .map(|name| format!("vendor/{name}")),
    );
    let mut total = 0u64;
    for relative in required {
        let entries: Vec<_> = files
            .iter()
            .filter(|file| file["path"].as_str() == Some(&relative))
            .collect();
        if entries.len() != 1 {
            return Err("GPU runtime inventory lacks a unique required file".into());
        }
        let entry = entries[0];
        let expected = entry["bytes"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 1024 * 1024 * 1024)
            .ok_or("GPU runtime file size exceeds limit")?;
        total = total
            .checked_add(expected)
            .filter(|n| *n <= 3 * 1024 * 1024 * 1024)
            .ok_or("GPU runtime inventory exceeds budget")?;
        let file_path = root.join(&relative);
        let metadata = std::fs::symlink_metadata(&file_path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() != expected {
            return Err("GPU runtime file identity mismatch".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("Linked GPU runtime file".into());
            }
        }
        let mut file = std::fs::File::open(file_path).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut read = 0u64;
        let mut chunk = [0u8; 65536];
        loop {
            if std::time::Instant::now() >= deadline {
                return Err("GPU runtime verification timed out".into());
            }
            let n = file.read(&mut chunk).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            read += n as u64;
            if read > expected {
                return Err("GPU runtime file grew".into());
            }
            hash.update(&chunk[..n]);
        }
        if read != expected
            || entry["sha256"].as_str() != Some(format!("{:x}", hash.finalize()).as_str())
        {
            return Err("GPU runtime checksum mismatch".into());
        }
    }
    let mut after = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(16385)
        .read_to_end(&mut after)
        .map_err(|e| e.to_string())?;
    if after != bytes {
        return Err("GPU runtime manifest changed while verifying".into());
    }
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn write(path: &Path, maximum: usize) -> io::Result<()> {
    if !matches!(maximum, 2 | 4 | 6) {
        return Err(io::Error::other("Choose 2, 4 or 6 live decks"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Settings folder unavailable"))?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    writeln!(file, "{maximum}")?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}
pub(crate) fn save_maximum(maximum: usize, enabled: usize) -> Result<(), String> {
    let policy = Policy::new(
        maximum,
        if crate::onnx_stem_worker::selected_cuda() {
            hardware_limit()
        } else {
            2
        },
    );
    if enabled > policy.limit {
        return Err(format!(
            "{} decks already use live stems. Switch some to original audio before lowering this limit.",
            enabled
        ));
    }
    write(&path().ok_or("Settings folder unavailable")?, maximum).map_err(|e| e.to_string())?;
    MAXIMUM
        .get_or_init(|| AtomicUsize::new(4))
        .store(maximum, Relaxed);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_four_is_clamped_by_qualification_not_core_count() {
        assert_eq!(Policy::new(4, 2).limit, 2);
        assert_eq!(Policy::new(4, 6).limit, 4);
        assert_eq!(Policy::new(6, 2).limit, 2);
        assert_eq!(Policy::new(6, 6).limit, 6);
        assert_eq!(Policy::new(2, 6).limit, 2);
    }
    #[test]
    fn reservations_reject_new_decks_but_preserve_replacements_and_existing_audio() {
        let reserved = [true, true, false, false, false, false];
        let p = Policy::new(4, 2);
        assert!(!p.allows(&reserved, 2));
        assert!(p.allows(&reserved, 0));
        assert!(!p.allows(&reserved, 6));
        assert_eq!(reserved, [true, true, false, false, false, false]);
        let p = Policy::new(4, 6);
        assert!(p.allows(&reserved, 2));
        assert!(!p.allows(&[true, true, true, true, false, false], 4));
        assert!(Policy::new(6, 6).allows(&[true, true, true, true, true, false], 5));
    }
    #[test]
    fn another_machine_or_replaced_worker_cannot_unlock_six() {
        let model = "98228931494151762a1c4ab1ec7899a894b1f81fd4a509921a9d2f9bacc50845";
        let mut receipt = serde_json::json!({"version":1,"provider":"cuda","runtime_manifest_sha256":"runtime-a","host":"qualified-host","adapter_uuid":"GPU-1","worker_sha256":"worker-a","model_sha256":model,"max_live_decks":6,"proof_source":"source-a","proof_run":"actual-six-test"});
        assert_eq!(
            receipt_matches(
                &receipt,
                "qualified-host",
                "GPU-1",
                "worker-a",
                model,
                "runtime-a"
            ),
            Some(6)
        );
        assert_eq!(
            receipt_matches(
                &receipt,
                "other-host",
                "GPU-1",
                "worker-a",
                model,
                "runtime-a"
            ),
            None
        );
        assert_eq!(
            receipt_matches(
                &receipt,
                "qualified-host",
                "GPU-2",
                "worker-a",
                model,
                "runtime-a"
            ),
            None
        );
        assert_eq!(
            receipt_matches(
                &receipt,
                "qualified-host",
                "GPU-1",
                "worker-b",
                model,
                "runtime-a"
            ),
            None
        );
        assert_eq!(
            receipt_matches(
                &receipt,
                "qualified-host",
                "GPU-1",
                "worker-a",
                model,
                "runtime-b"
            ),
            None
        );
        receipt["provider"] = serde_json::json!("cpu");
        assert_eq!(
            receipt_matches(
                &receipt,
                "qualified-host",
                "GPU-1",
                "worker-a",
                model,
                "runtime-a"
            ),
            None
        );
        receipt["provider"] = serde_json::json!("cuda");
        receipt["max_live_decks"] = serde_json::json!(4);
        assert_eq!(
            receipt_matches(
                &receipt,
                "qualified-host",
                "GPU-1",
                "worker-a",
                model,
                "runtime-a"
            ),
            Some(4)
        );
    }
    #[test]
    fn runtime_inventory_verifies_fixed_files_and_rejects_swaps_duplicates_and_deadline() {
        use sha2::{Digest, Sha256};
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::create_dir(root.join("vendor")).unwrap();
        let names = [
            "onnxruntime.dll".to_owned(),
            "onnxruntime_providers_shared.dll".to_owned(),
            "onnxruntime_providers_cuda.dll".to_owned(),
        ]
        .into_iter()
        .chain(
            crate::onnx_stem_worker::CUDA_VENDOR_DLLS
                .iter()
                .map(|name| format!("vendor/{name}")),
        );
        let mut entries = Vec::new();
        for name in names {
            let bytes = name.as_bytes();
            std::fs::write(root.join(&name), bytes).unwrap();
            entries.push(serde_json::json!({"path":name,"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(bytes))}));
        }
        let path = root.join("module.json");
        let mut manifest = serde_json::json!({"files":entries});
        let save =
            |m: &serde_json::Value| std::fs::write(&path, serde_json::to_vec(m).unwrap()).unwrap();
        save(&manifest);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let expected = format!("{:x}", Sha256::digest(std::fs::read(&path).unwrap()));
        assert_eq!(verify_runtime_manifest(&path, deadline).unwrap(), expected);
        assert!(
            verify_runtime_manifest(&path, std::time::Instant::now())
                .unwrap_err()
                .contains("timed out")
        );
        let duplicate = manifest["files"][0].clone();
        manifest["files"].as_array_mut().unwrap().push(duplicate);
        save(&manifest);
        assert!(
            verify_runtime_manifest(&path, deadline)
                .unwrap_err()
                .contains("unique")
        );
        manifest["files"].as_array_mut().unwrap().pop();
        save(&manifest);
        std::fs::write(root.join("onnxruntime.dll"), b"swapped-runtime!").unwrap();
        assert!(verify_runtime_manifest(&path, deadline).is_err());
    }
    #[test]
    fn bounded_kernel_hostname_supports_linux_without_exported_environment() {
        assert_eq!(
            bounded_hostname(std::io::Cursor::new(b"sager-host\n")).unwrap(),
            "sager-host"
        );
        assert_eq!(
            parse_host_identity("  windows-host  "),
            Some("windows-host".into())
        );
        assert!(bounded_hostname(std::io::Cursor::new(vec![b'x'; 258])).is_err());
        assert!(bounded_hostname(std::io::Cursor::new(b"bad\ninside")).is_err());
        assert!(parse_host_identity(" ").is_none());
    }
    #[test]
    fn maximum_persists_and_invalid_write_preserves_previous() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("maximum");
        assert_eq!(read(&p), 4);
        write(&p, 6).unwrap();
        assert_eq!(read(&p), 6);
        assert!(write(&p, 3).is_err());
        assert_eq!(read(&p), 6);
        write(&p, 2).unwrap();
        assert_eq!(read(&p), 2);
    }
}
