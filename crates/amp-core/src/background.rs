//! Cooperative preparation checkpoints. Playback and lease heartbeats never wait here.
use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub const PAUSE_ENV: &str = "AMP_PREPARATION_PAUSE_FILE";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Idle,
    Slow,
    Pause,
}
pub fn activity_at(path: &Path, now: u64) -> Activity {
    let value = std::fs::read_to_string(path).unwrap_or_default();
    let mut parts = value.split_whitespace();
    let until = parts
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    if until <= now || until > now.saturating_add(10_000) {
        return Activity::Idle;
    }
    match parts.next() {
        Some("slow") => Activity::Slow,
        _ => Activity::Pause,
    }
}
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub const CANCEL_ENV: &str = "AMP_ANALYSIS_CANCEL_FILE";
pub fn checkpoint() -> Result<(), String> {
    let pause = std::env::var_os(PAUSE_ENV).map(std::path::PathBuf::from);
    let cancel = std::env::var_os(CANCEL_ENV).map(std::path::PathBuf::from);
    checkpoint_at(pause.as_deref(), cancel.as_deref())
}
fn checkpoint_at(pause: Option<&Path>, cancel: Option<&Path>) -> Result<(), String> {
    loop {
        if cancel.is_some_and(Path::is_file) {
            return Err("Analysis stopped; unfinished songs remain queued".into());
        }
        match pause.map_or(Activity::Idle, |path| activity_at(path, now_ms())) {
            Activity::Pause => std::thread::sleep(Duration::from_millis(50)),
            Activity::Slow => {
                std::thread::sleep(Duration::from_millis(20));
                return Ok(());
            }
            Activity::Idle => return Ok(()),
        }
    }
}

pub fn copy_input(source: &Path, destination: &Path) -> Result<(), String> {
    use std::io::{Read, Write};
    let mut input = std::fs::File::open(source).map_err(|e| e.to_string())?;
    let mut output = std::fs::File::create(destination).map_err(|e| e.to_string())?;
    let mut buffer = vec![0; 256 * 1024];
    loop {
        checkpoint()?;
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
            assert_eq!(activity_at(&path, 1000) == Activity::Pause, expected);
        }
        std::fs::write(&path, "1200 slow").unwrap();
        assert_eq!(activity_at(&path, 1000), Activity::Slow);
        std::fs::write(&path, "1200 pause").unwrap();
        assert_eq!(activity_at(&path, 1000), Activity::Pause);
    }
}

#[cfg(test)]
mod cancellation_tests {
    #[test]
    fn cancellation_interrupts_a_playback_pause() {
        let dir = tempfile::tempdir().unwrap();
        let pause = dir.path().join("pause");
        let cancel = dir.path().join("cancel");
        std::fs::write(&pause, format!("{} pause", super::now_ms() + 9000)).unwrap();
        std::fs::write(&cancel, b"stop").unwrap();
        let started = std::time::Instant::now();
        assert!(
            super::checkpoint_at(Some(&pause), Some(&cancel))
                .unwrap_err()
                .contains("stopped")
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
}
