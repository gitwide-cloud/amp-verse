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
    pub fn new(requested: usize, qualified_cuda: bool) -> Self {
        let hardware_limit = if qualified_cuda { 6 } else { 2 };
        let requested = if matches!(requested, 2 | 4 | 6) {
            requested
        } else {
            4
        };
        Self {
            limit: requested.min(hardware_limit),
            hardware_limit,
            reason: if qualified_cuda {
                "Qualified GPU processing"
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
        return Policy::new(6, true);
    }
    Policy::new(maximum(), crate::onnx_stem_worker::qualified_cuda())
}
fn write(path: &Path, maximum: usize) -> io::Result<()> {
    if !matches!(maximum, 2 | 4 | 6) {
        return Err(io::Error::other("Choose2,4or6 live decks"));
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
    let policy = Policy::new(maximum, crate::onnx_stem_worker::qualified_cuda());
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
        assert_eq!(Policy::new(4, false).limit, 2);
        assert_eq!(Policy::new(4, true).limit, 4);
        assert_eq!(Policy::new(6, false).limit, 2);
        assert_eq!(Policy::new(6, true).limit, 6);
        assert_eq!(Policy::new(2, true).limit, 2);
    }
    #[test]
    fn reservations_reject_new_decks_but_preserve_replacements_and_existing_audio() {
        let reserved = [true, true, false, false, false, false];
        let p = Policy::new(4, false);
        assert!(!p.allows(&reserved, 2));
        assert!(p.allows(&reserved, 0));
        assert!(!p.allows(&reserved, 6));
        assert_eq!(reserved, [true, true, false, false, false, false]);
        let p = Policy::new(4, true);
        assert!(p.allows(&reserved, 2));
        assert!(!p.allows(&[true, true, true, true, false, false], 4));
        assert!(Policy::new(6, true).allows(&[true, true, true, true, true, false], 5));
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
