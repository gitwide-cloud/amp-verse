//! Device preference shared by manual crossfades and AutoMix, independent of style.
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};
fn path() -> Option<PathBuf> {
    crate::theme::settings_path().map(|p| p.with_file_name("crossfade-seconds.txt"))
}
fn read(path: &Path) -> Option<f32> {
    let seconds: f32 = std::fs::read_to_string(path).ok()?.trim().parse().ok()?;
    (seconds.is_finite() && (0.5..=30.).contains(&seconds)).then_some(seconds)
}
pub fn load() -> f32 {
    path().and_then(|p| read(&p)).unwrap_or(8.)
}
fn write(path: &Path, seconds: f32) -> io::Result<()> {
    if !seconds.is_finite() || !(0.5..=30.).contains(&seconds) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Mix time must be 0.5–30 seconds",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Settings folder unavailable"))?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    writeln!(file, "{seconds}")?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}
pub fn save(seconds: f32) -> io::Result<()> {
    write(
        &path().ok_or_else(|| io::Error::other("Settings folder unavailable"))?,
        seconds,
    )
}
#[cfg(test)]
mod tests {
    #[test]
    fn duration_survives_replacement_and_bad_updates_keep_previous_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("duration.txt");
        super::write(&path, 8.).unwrap();
        super::write(&path, 3.).unwrap();
        assert_eq!(super::read(&path), Some(3.));
        for invalid in [f32::NAN, f32::INFINITY, 0., 31.] {
            assert!(super::write(&path, invalid).is_err());
            assert_eq!(super::read(&path), Some(3.));
        }
        std::fs::write(&path, "bad value").unwrap();
        assert_eq!(super::read(&path), None);
    }
}
