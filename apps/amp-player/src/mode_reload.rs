//! Audio-mode reload follows the executed source clock, independently of manual/AutoDJ cue policy.
pub(super) fn range(
    position: f64,
    origin: f64,
    duration_frames: u64,
    sample_rate: u32,
) -> Result<(f64, f64), String> {
    if sample_rate == 0 || duration_frames == 0 {
        return Err("Wait for the current song duration before changing its audio mode.".into());
    }
    let end = origin + duration_frames as f64 / sample_rate as f64;
    if !position.is_finite()
        || !origin.is_finite()
        || !end.is_finite()
        || origin < 0.
        || position < origin
        || position >= end
    {
        return Err(
            "Choose a paused position inside this song before changing its audio mode.".into(),
        );
    }
    Ok((position, end))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reload_keeps_absolute_position_and_executed_cropped_end() {
        // AutoDJ source began nine seconds into the file; elapsed time is sixteen.
        assert_eq!(range(25., 9., 337 * 44100, 44100).unwrap(), (25., 346.));
        assert_eq!(range(78., 0., 100 * 48000, 48000).unwrap(), (78., 100.));
    }
    #[test]
    fn invalid_or_unpublished_clock_never_rewinds_or_expands_a_source() {
        for (position, origin, frames, rate) in [
            (0., 9., 44100, 44100),
            (10., 9., 44100, 44100),
            (f64::NAN, 0., 44100, 44100),
            (0., 0., 0, 44100),
            (0., 0., 44100, 0),
        ] {
            assert!(range(position, origin, frames, rate).is_err());
        }
    }
}
