//! Portable, user-approved stem defaults. Knobs: -1 removes, 0 original, +1 isolates.
use crate::Preparation;
pub fn knobs(prep: &Preparation) -> Result<[f32; 3], String> {
    let Some(value) = prep.extensions.get("default_stem_mix") else {
        return Ok([0.; 3]);
    };
    let knobs: [f32; 3] =
        serde_json::from_value(value.clone()).map_err(|_| "Invalid saved stem mix")?;
    if knobs
        .iter()
        .any(|v| !v.is_finite() || !(-1. ..=1.).contains(v))
    {
        return Err("Saved stem mix must be between -1 and 1".into());
    }
    Ok(knobs)
}
pub fn gains(knobs: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|stem| {
        knobs
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != stem)
            .fold(1. + knobs[stem].clamp(-1., 0.), |gain, (_, other)| {
                gain * (1. - other.clamp(0., 1.))
            })
    })
}
pub fn changed(knobs: [f32; 3]) -> bool {
    gains(knobs) != [1.; 3]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_stem_mix_round_trips_and_conflicting_edits_are_retained() {
        let dir = tempfile::tempdir().unwrap();
        let relative = std::path::Path::new("song.wav");
        let audio = dir.path().join(relative);
        std::fs::write(&audio, b"source audio").unwrap();
        let (mut draft, base) = crate::preparation::load_with_revision(&audio).unwrap();
        draft.manual_override = true;
        draft
            .extensions
            .insert("default_stem_mix".into(), serde_json::json!([-1., 0., 0.]));
        crate::preparation::save_if_unchanged(dir.path(), relative, &draft, base.as_deref())
            .unwrap();
        let mut competing = draft.clone();
        competing
            .extensions
            .insert("default_stem_mix".into(), serde_json::json!([1., 0., 0.]));
        assert!(
            crate::preparation::save_if_unchanged(
                dir.path(),
                relative,
                &competing,
                base.as_deref()
            )
            .is_err()
        );
        assert_eq!(
            knobs(&crate::preparation::load(&audio).unwrap()).unwrap(),
            [-1., 0., 0.]
        );
        assert_eq!(std::fs::read(&audio).unwrap(), b"source audio");
    }
    #[test]
    fn portable_defaults_validate_and_remove_or_isolate() {
        let mut p = Preparation::default();
        assert_eq!(knobs(&p).unwrap(), [0.; 3]);
        p.extensions
            .insert("default_stem_mix".into(), serde_json::json!([-1., 0., 0.]));
        assert_eq!(gains(knobs(&p).unwrap()), [0., 1., 1.]);
        p.extensions
            .insert("default_stem_mix".into(), serde_json::json!([1., 0., 0.]));
        assert_eq!(gains(knobs(&p).unwrap()), [1., 0., 0.]);
        p.extensions
            .insert("default_stem_mix".into(), serde_json::json!([2., 0., 0.]));
        assert!(p.validate().is_err());
    }
}
