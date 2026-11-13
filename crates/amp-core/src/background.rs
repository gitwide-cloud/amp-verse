//! Cooperative preparation checkpoints. Playback and lease heartbeats never wait here.
use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub const PAUSE_ENV: &str = "AMP_PREPARATION_PAUSE_FILE";
pub fn paused_at(path: &Path, now: u64) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .is_some_and(|until| until > now && until <= now.saturating_add(10_000))
}
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn checkpoint() {
    if let Some(path) = std::env::var_os(PAUSE_ENV) {
        while paused_at(Path::new(&path), now_ms()) {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
pub fn copy_input(source: &Path, destination: &Path) -> Result<(), String> {
    use std::io::{Read, Write};
    let mut input = std::fs::File::open(source).map_err(|e| e.to_string())?;
    let mut output = std::fs::File::create(destination).map_err(|e| e.to_string())?;
    let mut buffer = vec![0; 256 * 1024];
    loop {
        checkpoint();
        let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playback_gate_expires_and_does_not_trust_corrupt_or_far_future_markers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pause");
        for (text, expected) in [
            ("1200", true),
            ("900", false),
            ("bad", false),
            ("9999999", false),
        ] {
            std::fs::write(&path, text).unwrap();
            assert_eq!(paused_at(&path, 1000), expected);
        }
    }
}
