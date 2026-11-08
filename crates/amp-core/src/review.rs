//! Explicit field-level review; analysis never grants itself permission to replace approved data.
use crate::Preparation;
const FIELDS: [&str; 9] = [
    "cue_in",
    "cue_out",
    "bpm",
    "beat_offset",
    "key",
    "harmonic_key",
    "cues",
    "energy",
    "analysis",
];
pub fn proposal(p: &Preparation) -> Option<Preparation> {
    p.extensions
        .get("analysis_recommendation")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}
fn value(p: &Preparation, field: &str) -> serde_json::Value {
    let mut v = serde_json::to_value(p).unwrap_or_default()[field].clone();
    if field == "analysis"
        && let Some(o) = v.as_object_mut()
    {
        o.remove("reviewed");
    }
    v
}
pub fn changes(p: &Preparation) -> Vec<(String, serde_json::Value, serde_json::Value)> {
    let Some(candidate) = proposal(p) else {
        return vec![];
    };
    FIELDS
        .iter()
        .filter_map(|f| {
            let a = value(p, f);
            let b = value(&candidate, f);
            (a != b).then(|| (f.to_string(), a, b))
        })
        .collect()
}
pub fn decide(p: &Preparation, fields: &[String], accept: bool) -> Result<Preparation, String> {
    let mut candidate = proposal(p).ok_or("No pending recommendation")?;
    candidate.validate()?;
    let mut current = p.clone();
    current.extensions.remove("analysis_recommendation");
    let mut a = serde_json::to_value(&current).map_err(|e| e.to_string())?;
    let mut b = serde_json::to_value(&candidate).map_err(|e| e.to_string())?;
    for f in fields {
        if !FIELDS.contains(&f.as_str()) {
            return Err("Unsupported review field".into());
        }
        if accept {
            a[f] = b[f].clone();
        } else {
            b[f] = a[f].clone();
        }
    }
    current = serde_json::from_value(a).map_err(|e| e.to_string())?;
    if accept && !fields.is_empty() {
        current.manual_override = true;
        if let Some(a) = &mut current.analysis {
            a.reviewed = true;
        }
    }
    candidate = serde_json::from_value(b).map_err(|e| e.to_string())?;
    if accept {
        let a = serde_json::to_value(&current).map_err(|e| e.to_string())?;
        let mut b = serde_json::to_value(&candidate).map_err(|e| e.to_string())?;
        for f in fields {
            b[f] = a[f].clone();
        }
        candidate = serde_json::from_value(b).map_err(|e| e.to_string())?;
    }
    current.extensions.insert(
        "analysis_recommendation".into(),
        serde_json::to_value(candidate).map_err(|e| e.to_string())?,
    );
    if changes(&current).is_empty() {
        current.extensions.remove("analysis_recommendation");
    }
    current.validate()?;
    Ok(current)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejecting_grid_and_accepting_cue_are_independent() {
        let mut p = Preparation {
            cue_in: 12.,
            beat_offset: Some(0.2),
            manual_override: true,
            ..Default::default()
        };
        let mut candidate = p.clone();
        candidate.cue_in = 11.84;
        candidate.beat_offset = Some(0.1);
        p.extensions.insert(
            "analysis_recommendation".into(),
            serde_json::to_value(candidate).unwrap(),
        );
        let kept = decide(&p, &["beat_offset".into()], false).unwrap();
        assert_eq!(kept.beat_offset, Some(0.2));
        assert_eq!(kept.cue_in, 12.);
        let accepted = decide(&kept, &["cue_in".into()], true).unwrap();
        assert_eq!(accepted.cue_in, 11.84);
        assert_eq!(accepted.beat_offset, Some(0.2));
        assert!(proposal(&accepted).is_none());
    }
}
