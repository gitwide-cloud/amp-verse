//! Portable, non-destructive Lab processing shared by waveform and timeline editors.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Processing {
    /// Neutral is zero; -1 removes a component, +1 boosts it (the DJ knob convention).
    pub stems: [f32; 3],
    pub eq_db: [f32; 3],
    /// Negative is low-pass, positive is high-pass; zero bypasses the filter.
    pub filter: f32,
}
impl Processing {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .stems
            .iter()
            .chain([&self.filter])
            .any(|v| !v.is_finite() || !(-1. ..=1.).contains(v))
            || self
                .eq_db
                .iter()
                .any(|v| !v.is_finite() || !(-24. ..=12.).contains(v))
        {
            return Err("Invalid Lab stem, EQ or filter setting".into());
        }
        Ok(())
    }
    pub fn stems_changed(&self) -> bool {
        crate::stem_mix::changed(self.stems)
    }
    /// Stem separation happens in the desktop's bounded in-memory worker, never in FFmpeg EQ.
    pub fn filters(&self) -> Result<Vec<String>, String> {
        self.validate()?;
        let mut out = Vec::new();
        for (i, freq) in [180, 1000, 6000].into_iter().enumerate() {
            if self.eq_db[i].abs() > 0.001 {
                out.push(format!("equalizer=f={freq}:t=o:w=1:g={}", self.eq_db[i]));
            }
        }
        if self.filter < -0.001 {
            out.push(format!(
                "lowpass=f={:.2}",
                20000_f32 * (80_f32 / 20000.).powf(-self.filter)
            ));
        }
        if self.filter > 0.001 {
            out.push(format!(
                "highpass=f={:.2}",
                20_f32 * (12000_f32 / 20.).powf(self.filter)
            ));
        }
        Ok(out)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_projects_default_to_bypass_and_invalid_values_are_rejected() {
        let p: Processing = serde_json::from_str("{}").unwrap();
        assert_eq!(p, Processing::default());
        assert!(!p.stems_changed());
        assert!(p.filters().unwrap().is_empty());
        assert!(
            Processing {
                eq_db: [f32::NAN, 0., 0.],
                ..p
            }
            .validate()
            .is_err()
        );
        assert!(
            Processing {
                stems: [-2., 0., 0.],
                ..p
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn eq_and_filter_are_audio_processing_not_stem_substitutes() {
        let p = Processing {
            stems: [-1., 0., 0.],
            eq_db: [-6., 2., 3.],
            filter: -0.5,
        };
        assert!(p.stems_changed());
        let f = p.filters().unwrap().join(",");
        assert!(f.contains("equalizer=f=180"));
        assert!(f.contains("lowpass="));
        assert!(!f.contains("highpass="));
    }
}
