use amp_core::{
    Preparation,
    mix_schedule::{self, Timing},
    transition_plan::*,
};
use serde_json::Value;
fn vectors() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/sag65/sag65_synthetic_transition_fixtures.json"
    ))
    .unwrap()
}
fn prep(v: &Value) -> Preparation {
    let mut p = Preparation {
        bpm: v["bpm"].as_f64(),
        beat_offset: v["beat_offset"].as_f64(),
        analysis: Some(amp_core::preparation::AnalysisInfo {
            engine: "SAG65 synthetic".into(),
            duration: 60.,
            tempo_confidence: v["tempo_confidence"].as_f64().unwrap_or(0.),
            key_confidence: 1.,
            reviewed: false,
        }),
        ..Default::default()
    };
    for k in ["mix_bar_anchor", "mix_phrase_anchor"] {
        if v[k].is_number() {
            p.extensions.insert(k.into(), v[k].clone());
        }
    }
    if v.get("rhythm_stable").is_some() {
        p.extensions.insert("rhythm_quality".into(),serde_json::json!({"stable":v["rhythm_stable"],"windows":v["rhythm_windows"].as_u64().unwrap_or(8)}));
    }
    p
}
#[test]
fn f01_f02_f03_fixture_planner_and_phase() {
    let vectors = vectors();
    for (index, tier) in [
        (0, TransitionTier::T0PhraseLocked),
        (1, TransitionTier::T2BeatLocked),
        (2, TransitionTier::T2BeatLocked),
    ] {
        let f = &vectors["fixtures"][index];
        let a = prep(&f["outgoing"]);
        let b = prep(&f["incoming"]);
        let decision = plan_transition(
            timing_snapshot(&a, true),
            timing_snapshot(&b, true),
            RequestedTiming::Phrase,
            Handoff::StemSwap,
            1.,
            0.16,
            4.,
            30.,
        );
        assert_eq!(decision.tier, tier, "{}", f["id"]);
        let rate = decision.tempo.unwrap().rate;
        if let Some(expected) = f["expected_pulse_factor"].as_f64() {
            assert_eq!(decision.tempo.unwrap().pulse_factor, expected);
        }
        let grid = mix_schedule::safe_grid_at_rates(
            &a,
            &b,
            b.beat_offset.unwrap(),
            0.,
            Timing::Phrase16,
            1.,
            rate,
        )
        .unwrap();
        let launch = grid.next(1.);
        for beat in 0..1000 {
            let t = beat as f64 * 60. / a.bpm.unwrap();
            let phase = (launch + t - a.beat_offset.unwrap()) / (60. / a.bpm.unwrap());
            let error = (phase + 0.5).rem_euclid(1.) - 0.5;
            assert!(error.abs() * 60. / a.bpm.unwrap() * 1000. <= 1.);
        }
    }
}
#[test]
fn f05_f06_fixture_masks_preserve_user_mutes() {
    let v = vectors();
    let incoming = &v["fixtures"][5]["user_stem_gains_incoming"];
    let mut guard = 0;
    for sample in 0..176400 {
        let m = stem_handoff(sample, 176400, 21339);
        assert_eq!(m[0][0] * m[1][0], 0.);
        assert_eq!(m[1][0] as f64 * incoming[0].as_f64().unwrap(), 0.);
        if m[0][0] == 0. && m[1][0] == 0. {
            guard += 1;
        }
    }
    assert!(guard as f64 / 176400. >= 0.05);
}
#[test]
fn f08_f11_f12_fixture_fallbacks_and_saved_bytes() {
    let v = vectors();
    let good = prep(&v["fixtures"][0]["outgoing"]);
    let mut drift = good.clone();
    drift.extensions.insert(
        "rhythm_quality".into(),
        serde_json::json!({"stable":false,"windows":8}),
    );
    let decision = plan_transition(
        timing_snapshot(&good, true),
        timing_snapshot(&drift, true),
        RequestedTiming::Phrase,
        Handoff::StemSwap,
        1.,
        0.16,
        4.,
        30.,
    );
    assert_eq!(decision.tier, TransitionTier::T4UnsyncedCleanFade);
    for case in v["fixtures"][10]["subcases"].as_array().unwrap() {
        assert!(!mix_schedule::timing_reliable(&prep(case)));
    }
    let f = &v["fixtures"][11];
    let mut manual = good.clone();
    manual.manual_override = true;
    manual.manual_cues = f["manual_cues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| amp_core::preparation::Cue {
            id: c["id"].as_str().unwrap().into(),
            label: c["id"].as_str().unwrap().into(),
            seconds: c["seconds"].as_f64().unwrap(),
            kind: c["kind"].as_str().unwrap().into(),
            ..Default::default()
        })
        .collect();
    let bytes = serde_json::to_vec(&manual).unwrap();
    let p = plan_transition(
        timing_snapshot(&manual, true),
        timing_snapshot(&good, true),
        RequestedTiming::Phrase,
        Handoff::StemSwap,
        1.,
        0.16,
        f["requested_fade_s"].as_f64().unwrap(),
        f["outgoing_remaining_s"].as_f64().unwrap(),
    );
    assert_eq!(
        p.effective_fade,
        f["expected_effective_fade_s"].as_f64().unwrap()
    );
    assert_eq!(p.tier, TransitionTier::T5EmergencyTailRescue);
    assert_eq!(bytes, serde_json::to_vec(&manual).unwrap());
}
#[test]
#[ignore = "requires generated SAG65 fixtures and ffmpeg"]
fn f07_f10_measured_energy_and_session_loudness() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/sag65-fixtures/wav");
    let energy = |name: &str| {
        let path = root.join(name);
        let loudness = amp_core::loudness::measure(&path).unwrap();
        let pcm = amp_core::command("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-ac", "1", "-ar", "11025", "-f", "f32le", "pipe:1"])
            .output()
            .unwrap();
        assert!(pcm.status.success());
        let (sample_bytes, _) = pcm.stdout.as_chunks::<4>();
        let samples: Vec<_> = sample_bytes
            .iter()
            .map(|&sample| f64::from(f32::from_le_bytes(sample)))
            .collect();
        let rms: Vec<_> = samples
            .chunks(110)
            .map(|b| (b.iter().map(|v| v * v).sum::<f64>() / b.len() as f64).sqrt())
            .collect();
        amp_core::perceptual_energy::from_rms(&rms, 11025. / 110., loudness.integrated_lufs)
            .unwrap()
    };
    let dance = energy("f07_dynamic_dance.wav");
    let pad = energy("f07_brickwalled_ambient_pad.wav");
    println!("F07 dance={dance}, pad={pad}");
    assert!(dance > pad);
    let mut adjusted = Vec::new();
    for name in ["f10_hot_outgoing.wav", "f10_quiet_incoming.wav"] {
        let path = root.join(name);
        let before = std::fs::read(&path).unwrap();
        let l = amp_core::loudness::measure(&path).unwrap();
        let mut p = Preparation::default();
        p.extensions
            .insert("loudness".into(), serde_json::to_value(l).unwrap());
        let saved = serde_json::to_vec(&p).unwrap();
        adjusted.push(l.integrated_lufs + amp_core::loudness::auto_gain_db(&p) as f64);
        assert_eq!(before, std::fs::read(&path).unwrap());
        assert_eq!(saved, serde_json::to_vec(&p).unwrap());
    }
    println!("F10 gain-adjusted LUFS={adjusted:?}");
    assert!((adjusted[0] - adjusted[1]).abs() <= 1.);
}

#[test]
fn f04_fixture_soft_harmonic_context_and_explicit_formula_resolution() {
    let v = vectors();
    let cases = v["fixtures"][3]["subcases"].as_array().unwrap();
    for case in &cases[..4] {
        let cost = harmonic_cost(
            key_code(case["out_key"].as_str().unwrap()),
            key_code(case["in_key"].as_str().unwrap()),
            1.,
            case["in_key_conf"].as_f64().unwrap(),
            1.,
            case["in_rate"].as_f64().unwrap(),
            true,
            case["key_lock"].as_bool().unwrap(),
        );
        if let Some(range) = case["expected_harmonic_cost_range"].as_array() {
            assert!((range[0].as_f64().unwrap()..=range[1].as_f64().unwrap()).contains(&cost));
        }
        if let Some(min) = case["expected_harmonic_cost_min"].as_f64() {
            assert!(cost >= min);
        }
        if case["case"] == "4c_plus6pct_with_keylock_stays_8A" {
            // The versioned resolution follows the formula, without rewriting the legacy input.
            let resolution: Value = serde_json::from_str(include_str!(
                "../../../tests/sag65/contract_resolutions.json"
            ))
            .unwrap();
            assert_eq!(resolution["schema_version"], 1);
            let expected = &resolution["resolutions"][0];
            assert_eq!(expected["case"], case["case"]);
            assert_eq!(expected["confidence"], case["in_key_conf"]);
            assert_eq!(
                expected["legacy_expected_max"],
                case["expected_harmonic_cost_max"]
            );
            assert!(
                (cost - expected["expected_cost"].as_f64().unwrap()).abs()
                    < expected["tolerance"].as_f64().unwrap()
            );
            // Keep the original inconsistency visible: it is not a literal legacy-vector pass.
            assert!(cost > case["expected_harmonic_cost_max"].as_f64().unwrap());
        }
        if case["case"] == "4d_low_key_confidence_attenuated" {
            assert!(cost < harmonic_cost(8, 2, 1., 1., 1., 1., true, true));
        }
    }
    let mut a = prep(&v["fixtures"][0]["outgoing"]);
    a.harmonic_key = Some("8A".into());
    a.extensions.insert("energy".into(), serde_json::json!(0.5));
    let mut b = a.clone();
    b.harmonic_key = Some("2A".into());
    assert_eq!(
        amp_core::smart::curate(&[&a, &b], &[0], 2).unwrap(),
        vec![0, 1]
    );
}
