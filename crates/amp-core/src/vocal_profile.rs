//! Lightweight offline vocal activity from prepared stems, not a vocal classifier.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VocalProfile {
    pub version: u32,
    pub step_s: f64,
    pub source: String,
    pub confidence: f64,
    pub activity: Vec<f64>,
}
impl VocalProfile {
    pub fn active(&self, start: f64, duration: f64, threshold: f64) -> bool {
        if self.version != 1
            || !self.step_s.is_finite()
            || self.step_s <= 0.
            || !start.is_finite()
            || start < 0.
            || !duration.is_finite()
            || duration <= 0.
            || self.confidence < 0.8
        {
            return false;
        }
        let first = (start / self.step_s).floor() as usize;
        let end = ((start + duration) / self.step_s).ceil() as usize;
        self.activity
            .get(first..end)
            .filter(|v| !v.is_empty())
            .is_some_and(|v| {
                v.iter().all(|a| a.is_finite())
                    && v.iter().sum::<f64>() / v.len() as f64 >= threshold
            })
    }
}
fn rms_bins(path: &std::path::Path) -> Result<Vec<f64>, String> {
    use std::io::Read;
    let mut child = crate::command("ffmpeg")
        .args(["-nostdin", "-v", "error", "-threads", "1", "-i"])
        .arg(path)
        .args(["-vn", "-ac", "2", "-ar", "11025", "-f", "f32le", "pipe:1"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stream = std::io::BufReader::new(child.stdout.take().ok_or("No decoder output")?);
    let result = (|| {
        let mut bins = Vec::new();
        let mut bytes = [0u8; 4];
        let mut sum = 0.;
        let mut count = 0;
        loop {
            match stream.read_exact(&mut bytes) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.to_string()),
            }
            let sample = f32::from_le_bytes(bytes) as f64;
            if !sample.is_finite() {
                return Err("Invalid PCM".into());
            }
            sum += sample * sample;
            count += 1;
            if count == 44100 {
                bins.push((sum / count as f64).sqrt());
                sum = 0.;
                count = 0;
            }
        }
        if count > 0 {
            bins.push((sum / count as f64).sqrt());
        }
        Ok(bins)
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("Vocal profile decoder failed".into());
    }
    result
}
pub fn measure(vocal: &std::path::Path, total: &std::path::Path) -> Result<VocalProfile, String> {
    let vocal = rms_bins(vocal)?;
    let total = rms_bins(total)?;
    if vocal.len() != total.len() {
        return Err("Stem duration differs from source".into());
    }
    Ok(VocalProfile {
        version: 1,
        step_s: 2.,
        source: "htdemucs_stem_ratio".into(),
        confidence: 0.90,
        activity: vocal
            .iter()
            .zip(total)
            .map(|(v, t)| {
                if t < 1e-6 {
                    0.
                } else {
                    (v / (t + 1e-12)).clamp(0., 1.)
                }
            })
            .collect(),
    })
}
