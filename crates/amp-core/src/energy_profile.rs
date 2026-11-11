//! Portable activity profiles and section selection; not semantic verse/chorus recognition.
use crate::Preparation;
use serde::{Deserialize, Serialize};
pub const FIELD: &str = "energy_profile";
pub fn mixing_allowed(p: &Preparation) -> bool {
    p.extensions
        .get("energy_section_mixing")
        .and_then(|v| v.as_bool())
        .unwrap_or(!p.manual_override && p.manual_cues.is_empty())
}
pub fn entry_bound(p: &Preparation) -> f64 {
    if !p.manual_override && p.manual_cues.is_empty() {
        p.cues
            .iter()
            .find(|c| c.id == "auto-in")
            .map_or(p.cue_in, |c| c.seconds)
    } else {
        p.cue_in
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub id: String,
    pub name: String,
    pub start: f64,
    pub end: f64,
    pub energy: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub version: u32,
    pub duration: f64,
    pub step: f64,
    pub curve: Vec<f64>,
    pub sections: Vec<Section>,
}
impl Profile {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || !self.duration.is_finite()
            || !(0. ..=21600.).contains(&self.duration)
            || !self.step.is_finite()
            || !(0.1..=30.).contains(&self.step)
            || self.curve.len() > 216_001
            || self.sections.len() > 4096
            || self
                .curve
                .iter()
                .any(|x| !x.is_finite() || !(0. ..=1.).contains(x))
        {
            return Err("Invalid energy profile".into());
        }
        let mut end = 0.;
        let mut ids = std::collections::HashSet::new();
        for s in &self.sections {
            if s.id.is_empty()
                || s.id.len() > 160
                || !ids.insert(&s.id)
                || s.name.len() > 160
                || !s.start.is_finite()
                || !s.end.is_finite()
                || s.start < end
                || s.end <= s.start
                || s.end > self.duration + 0.001
                || !s.energy.is_finite()
                || !(0. ..=1.).contains(&s.energy)
            {
                return Err("Invalid or overlapping energy sections".into());
            }
            end = s.end;
        }
        Ok(())
    }
    pub fn at(&self, seconds: f64) -> Option<f64> {
        self.sections
            .iter()
            .find(|s| seconds >= s.start && seconds < s.end)
            .map(|s| s.energy)
            .or_else(|| {
                self.curve
                    .get((seconds.max(0.) / self.step) as usize)
                    .copied()
            })
    }
    pub fn entry(&self, target: f64, first: f64, last: f64, minimum: f64) -> Option<&Section> {
        self.sections
            .iter()
            .filter(|s| {
                s.start >= first - 0.001 && s.start + minimum <= last && s.end - s.start >= 1.
            })
            .min_by(|a, b| {
                ((a.energy - target).abs() + a.start / self.duration.max(1.) * 0.025).total_cmp(
                    &((b.energy - target).abs() + b.start / self.duration.max(1.) * 0.025),
                )
            })
    }
}
pub fn read(p: &Preparation) -> Option<Profile> {
    let profile: Profile = serde_json::from_value(p.extensions.get(FIELD)?.clone()).ok()?;
    profile.validate().ok()?;
    Some(profile)
}
pub fn validate(p: &Preparation) -> Result<(), String> {
    if p.extensions
        .get("energy_section_mixing")
        .is_some_and(|v| !v.is_boolean())
    {
        return Err("Energy section mixing must be on or off".into());
    }
    if let Some(value) = p.extensions.get(FIELD) {
        serde_json::from_value::<Profile>(value.clone())
            .map_err(|e| e.to_string())?
            .validate()?;
    }
    Ok(())
}
/// Absolute level curve shared across tracks. Deliberately not per-track min/max scaling.
#[cfg(feature = "analysis")]
pub(crate) fn build(
    rms: &[f64],
    hz: f64,
    duration: f64,
    bpm: Option<f64>,
    offset: Option<f64>,
) -> Profile {
    let bin = (hz * 2.).round().max(1.) as usize;
    let curve: Vec<f64> = rms
        .chunks(bin)
        .map(|xs| {
            let rms = (xs.iter().map(|v| v * v).sum::<f64>() / xs.len() as f64).sqrt();
            ((20. * rms.max(1e-6).log10() + 48.) / 42.).clamp(0., 1.)
        })
        .collect();
    let step = bin as f64 / hz;
    let width = bpm
        .filter(|b| b.is_finite() && *b > 0.)
        .map_or(8., |b| (16. * 60. / b).clamp(4., 24.));
    let mut boundaries = vec![0.];
    let mut next = offset.unwrap_or(0.).max(0.);
    if next < width * 0.5 {
        next += width;
    }
    while next < duration - 1. && boundaries.len() < 4096 {
        boundaries.push(next);
        next += width;
    }
    boundaries.push(duration);
    let sections = boundaries
        .windows(2)
        .enumerate()
        .filter(|(_, w)| w[1] > w[0])
        .map(|(i, w)| {
            let a = (w[0] / step).floor() as usize;
            let b = ((w[1] / step).ceil() as usize).min(curve.len());
            let values = &curve[a.min(b)..b];
            let energy = if values.is_empty() {
                0.
            } else {
                values.iter().sum::<f64>() / values.len() as f64
            };
            Section {
                id: format!("section-{}", i + 1),
                name: format!("Section {}", i + 1),
                start: w[0],
                end: w[1],
                energy,
            }
        })
        .collect();
    Profile {
        version: 1,
        duration,
        step,
        curve,
        sections,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Profile {
        Profile {
            version: 1,
            duration: 60.,
            step: 2.,
            curve: vec![0.2; 30],
            sections: vec![
                Section {
                    id: "quiet".into(),
                    name: "Opening".into(),
                    start: 0.,
                    end: 20.,
                    energy: 0.2,
                },
                Section {
                    id: "strong".into(),
                    name: "Main".into(),
                    start: 20.,
                    end: 45.,
                    energy: 0.8,
                },
                Section {
                    id: "tail".into(),
                    name: "Tail".into(),
                    start: 45.,
                    end: 60.,
                    energy: 0.4,
                },
            ],
        }
    }
    #[test]
    fn energy_entry_chooses_local_match_and_rejects_too_short_tail() {
        let p = sample();
        p.validate().unwrap();
        assert_eq!(p.entry(0.8, 0., 60., 16.).unwrap().id, "strong");
        assert_ne!(p.entry(0.4, 0., 60., 16.).unwrap().id, "tail");
        assert_eq!(p.at(25.), Some(0.8));
        let mut bad = p;
        bad.sections[1].start = 1.;
        assert!(bad.validate().is_err());
    }
    #[cfg(feature = "analysis")]
    #[test]
    fn different_songs_keep_comparable_energy_and_known_grid_sections() {
        let a = build(&vec![0.01; 6000], 100., 60., Some(120.), Some(0.));
        let b = build(&vec![0.2; 6000], 100., 60., Some(120.), Some(0.));
        a.validate().unwrap();
        b.validate().unwrap();
        assert!(b.sections[0].energy > a.sections[0].energy + 0.3);
        assert_eq!(a.sections[0].end, 8.);
        assert_eq!(a.sections.last().unwrap().end, 60.);
    }
}
