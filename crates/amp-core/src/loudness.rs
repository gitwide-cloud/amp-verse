//! Offline BS.1770 metadata and session-only gain. No sidecars are written here.
use crate::Preparation;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Loudness {
    pub integrated_lufs: f64,
    pub true_peak_dbtp: f64,
    pub lra_lu: f64,
}
impl Loudness {
    pub fn valid(self) -> bool {
        self.integrated_lufs.is_finite()
            && self.true_peak_dbtp.is_finite()
            && self.lra_lu.is_finite()
            && self.lra_lu >= 0.
    }
}
pub fn read(p: &Preparation) -> Option<Loudness> {
    serde_json::from_value::<Loudness>(p.extensions.get("loudness")?.clone())
        .ok()
        .filter(|l| l.valid())
}
pub fn auto_gain_db(p: &Preparation) -> f32 {
    read(p).map_or(0., |l| {
        // The limiter handles sources whose peaks already exceed the attenuation clamp.
        (-14. - l.integrated_lufs)
            .min(-1. - l.true_peak_dbtp)
            .clamp(-12., 6.) as f32
    })
}
/// ffmpeg's EBU R128 meter implements gated BS.1770 loudness and oversampled peak.
/// Called only during explicit offline analysis, never from an audio callback.
pub fn measure(path: &std::path::Path) -> Result<Loudness, String> {
    let output = crate::command("ffmpeg")
        .args(["-nostdin", "-hide_banner", "-i"])
        .arg(path)
        .args(["-vn", "-af", "ebur128=peak=true", "-f", "null", "-"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Loudness measurement failed".into());
    }
    let text = String::from_utf8_lossy(&output.stderr);
    let summary = text
        .rsplit("Summary:")
        .next()
        .ok_or("No loudness summary")?;
    let value = |label: &str| -> Option<f64> {
        summary.lines().map(str::trim).find_map(|line| {
            line.strip_prefix(label)?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
    };
    let result = Loudness {
        integrated_lufs: value("I:").ok_or("No integrated loudness")?,
        true_peak_dbtp: value("Peak:").ok_or("No true peak")?,
        lra_lu: value("LRA:").ok_or("No LRA")?,
    };
    result
        .valid()
        .then_some(result)
        .ok_or_else(|| "Non-finite loudness (silence)".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gain_is_optional_clamped_and_peak_safe() {
        let mut p = Preparation::default();
        assert_eq!(auto_gain_db(&p), 0.);
        for (lufs, peak, expected) in [
            (-9.6, -1., -4.4),
            (-18., -8., 4.),
            (-40., -30., 6.),
            (-18., -2., 1.),
        ] {
            p.extensions.insert(
                "loudness".into(),
                serde_json::json!({"integrated_lufs":lufs,"true_peak_dbtp":peak,"lra_lu":2.}),
            );
            assert!((auto_gain_db(&p) - expected).abs() < 1e-5);
        }
    }
}
