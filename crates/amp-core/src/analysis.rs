//! Independent offline preparation estimates. Not the proprietary Mixed In Key engine.
use crate::{
    Preparation,
    preparation::{self, AnalysisInfo, Cue},
};
use std::{io::Read, path::Path, process::Stdio};
const RATE: f64 = 11025.;
const BLOCK: usize = 110;
const FFT: usize = 4096;
fn chroma(samples: &[f32], sum: &mut [f64; 12]) {
    let mut re = vec![0.; FFT];
    let mut im = vec![0.; FFT];
    for (i, x) in samples.iter().take(FFT).enumerate() {
        re[i] =
            *x as f64 * (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (FFT - 1) as f64).cos());
    }
    let mut j = 0;
    for i in 1..FFT {
        let mut bit = FFT >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut size = 2;
    while size <= FFT {
        for start in (0..FFT).step_by(size) {
            for k in 0..size / 2 {
                let angle = -std::f64::consts::TAU * k as f64 / size as f64;
                let (c, s) = (angle.cos(), angle.sin());
                let a = start + k;
                let b = a + size / 2;
                let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        size *= 2;
    }
    let power: Vec<_> = (0..FFT / 2)
        .map(|i| re[i] * re[i] + im[i] * im[i])
        .collect();
    let peak = power.iter().copied().fold(0., f64::max);
    if peak < 1e-8 {
        return;
    }
    for k in 1..FFT / 2 - 1 {
        let frequency = k as f64 * RATE / FFT as f64;
        if !(65. ..=2500.).contains(&frequency)
            || power[k] < peak * 0.025
            || power[k] < power[k - 1]
            || power[k] < power[k + 1]
        {
            continue;
        }
        let note = 69. + 12. * (frequency / 440.).log2();
        if (note - note.round()).abs() > 0.35 {
            continue;
        }
        let pc = (note.round() as i32).rem_euclid(12) as usize;
        sum[pc] += power[k].sqrt() / frequency.sqrt();
    }
}
fn correlation(a: &[f64; 12], b: &[f64; 12]) -> f64 {
    let am = a.iter().sum::<f64>() / 12.;
    let bm = b.iter().sum::<f64>() / 12.;
    let num = (0..12).map(|i| (a[i] - am) * (b[i] - bm)).sum::<f64>();
    let den = ((0..12).map(|i| (a[i] - am).powi(2)).sum::<f64>()
        * (0..12).map(|i| (b[i] - bm).powi(2)).sum::<f64>())
    .sqrt();
    if den > 0. { num / den } else { 0. }
}
fn key(chroma: &[f64; 12]) -> (Option<String>, Option<String>, f64) {
    let major = [6., 0.1, 2., 0.1, 4., 2., 0.1, 4., 0.1, 2., 0.1, 1.5];
    let minor = [6., 0.1, 2., 4., 0.1, 2., 0.1, 4., 2., 0.1, 1.5, 0.1];
    let mut scores = Vec::new();
    for tonic in 0..12 {
        for (is_minor, profile) in [(false, major), (true, minor)] {
            let rotated = std::array::from_fn(|i| profile[(i + 12 - tonic) % 12]);
            scores.push((correlation(chroma, &rotated), tonic, is_minor));
        }
    }
    scores.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (best, tonic, minor) = scores[0];
    let confidence = ((best - scores[1].0) * 4.).clamp(0., 1.);
    if best < 0.45 || confidence < 0.06 {
        return (None, None, confidence);
    }
    let names = [
        "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
    ];
    let wheel = if minor {
        [5, 12, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10]
    } else {
        [8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6, 1]
    };
    (
        Some(format!(
            "{} {}",
            names[tonic],
            if minor { "minor" } else { "major" }
        )),
        Some(format!("{}{}", wheel[tonic], if minor { "A" } else { "B" })),
        confidence,
    )
}
fn tempo(energy: &[f64]) -> (Option<f64>, Option<f64>, f64) {
    if energy.len() < 500 {
        return (None, None, 0.);
    }
    let onset: Vec<_> = energy.windows(2).map(|w| (w[1] - w[0]).max(0.)).collect();
    let peak = onset.iter().copied().fold(0., f64::max);
    if peak < 1e-4 {
        return (None, None, 0.);
    }
    let onset: Vec<_> = onset
        .into_iter()
        .map(|x| if x > peak * 0.15 { x } else { 0. })
        .collect();
    let hz = RATE / BLOCK as f64;
    let mut best = (0., 0usize);
    for lag in (hz * 60. / 180.) as usize..=(hz * 60. / 60.) as usize {
        let mut cross = 0.;
        let mut a = 0.;
        let mut b = 0.;
        for i in lag..onset.len() {
            cross += onset[i] * onset[i - lag];
            a += onset[i] * onset[i];
            b += onset[i - lag] * onset[i - lag];
        }
        let score = if a * b > 0. {
            cross / (a * b).sqrt()
        } else {
            0.
        };
        let preference = 1. - ((hz * 60. / lag as f64 - 120.).abs() / 300.);
        let weighted = score * preference;
        if weighted > best.0 {
            best = (weighted, lag);
        }
    }
    if best.0 < 0.25 {
        return (None, None, best.0.clamp(0., 1.));
    }
    let beat = onset
        .iter()
        .position(|x| *x >= peak * 0.6)
        .map(|i| (i + 1) as f64 / hz);
    (Some(hz * 60. / best.1 as f64), beat, best.0.clamp(0., 1.))
}
fn result(
    energy: &[f64],
    rhythmic: &[f64],
    chroma: &[f64; 12],
    duration: f64,
    existing: &Preparation,
) -> Result<Preparation, String> {
    let max = energy.iter().copied().fold(0., f64::max);
    let threshold = (max * 0.01).max(0.0001);
    let first = energy
        .iter()
        .position(|v| *v > threshold)
        .ok_or("No audible audio found")?;
    let last = energy.iter().rposition(|v| *v > threshold).unwrap();
    let (bpm, offset, tempo_confidence) = tempo(rhythmic);
    let (key, harmonic_key, key_confidence) = key(chroma);
    let cue_in = first as f64 * BLOCK as f64 / RATE;
    let cue_out = ((last + 1) as f64 * BLOCK as f64 / RATE).min(duration);
    let mut p = existing.clone();
    p.schema_version = Some(1);
    // Relative loudness/activity proxy, not a calibrated perceptual energy detector.
    if !p.extensions.contains_key("energy") {
        let rms = (energy.iter().map(|x| x * x).sum::<f64>() / energy.len().max(1) as f64).sqrt();
        let energy_score = ((20. * rms.max(1e-6).log10() + 48.) / 42.).clamp(0., 1.);
        p.extensions
            .insert("energy".into(), serde_json::json!(energy_score));
    }

    // Legacy hand-authored preparation is treated as manual rather than silently replaced.
    let locked = existing.manual_override
        || existing.analysis.as_ref().is_some_and(|a| a.reviewed)
        || (existing.analysis.is_none()
            && (existing.cue_in > 0. || existing.cue_out.is_some() || existing.bpm.is_some()));
    p.manual_override = locked;
    if !locked {
        p.cue_in = cue_in;
        p.cue_out = Some(cue_out);
        p.bpm = bpm;
        p.beat_offset = offset;
        p.key = key.clone();
        p.harmonic_key = harmonic_key.clone();
    }
    p.cues = vec![
        Cue {
            id: "auto-in".into(),
            label: "Audio start".into(),
            seconds: cue_in,
            kind: "boundary".into(),
            ..Default::default()
        },
        Cue {
            id: "auto-out".into(),
            label: "Audio end".into(),
            seconds: cue_out,
            kind: "boundary".into(),
            ..Default::default()
        },
    ];
    if let (Some(bpm), Some(offset)) = (bpm, offset) {
        let phrase = 64. * 60. / bpm;
        let mut at = offset;
        let mut i = 1;
        while at < cue_out && i <= 512 {
            if at >= cue_in {
                p.cues.push(Cue {
                    id: format!("auto-grid-{i}"),
                    label: format!("64-beat marker {i}"),
                    seconds: at,
                    kind: "grid".into(),
                    ..Default::default()
                });
            }
            at += phrase;
            i += 1;
        }
    }
    p.analysis = Some(AnalysisInfo {
        engine: "amp-prep-heuristic-1".into(),
        duration,
        tempo_confidence,
        key_confidence,
        reviewed: false,
    });
    if locked {
        let mut suggested = p;
        suggested.cue_in = cue_in;
        suggested.cue_out = Some(cue_out);
        suggested.bpm = bpm;
        suggested.beat_offset = offset;
        suggested.key = key;
        suggested.harmonic_key = harmonic_key;
        suggested.manual_override = false;
        suggested.extensions.remove("analysis_recommendation");
        p = existing.clone();
        p.extensions.remove("analysis_recommendation");
        p.extensions.insert(
            "analysis_recommendation".into(),
            serde_json::to_value(suggested).map_err(|e| e.to_string())?,
        );
    }
    p.validate()?;
    Ok(p)
}
pub fn analyze(path: &Path, existing: &Preparation) -> Result<Preparation, String> {
    existing.validate()?;
    let fingerprint = preparation::fingerprint(path)?;
    if existing
        .source_sha256
        .as_ref()
        .is_some_and(|old| *old != fingerprint)
    {
        return Err("Audio differs from saved preparation. Review or remove the stale sidecar before analyzing.".into());
    }
    let before = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let mut child = crate::command("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", "11025", "-f", "f32le", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let decoded = (|| {
        let mut stream =
            std::io::BufReader::new(child.stdout.take().ok_or("Decoder output unavailable")?);
        let mut buffer = [0u8; BLOCK * 4];
        let mut energy = Vec::new();
        let mut rhythmic = Vec::new();
        let mut previous = 0.;
        let mut previous2 = 0.;
        let mut spectral = Vec::new();
        let mut chroma_sum = [0.; 12];
        let mut total = 0usize;
        loop {
            let mut n = 0;
            while n < buffer.len() {
                let got = stream.read(&mut buffer[n..]).map_err(|e| e.to_string())?;
                if got == 0 {
                    break;
                }
                n += got;
            }
            if n == 0 {
                break;
            }
            if n % 4 != 0 {
                return Err("Incomplete decoded sample".into());
            }
            let samples: Vec<_> = buffer[..n]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            if samples.iter().any(|v| !v.is_finite()) {
                return Err("Non-finite decoded audio".into());
            }
            total += samples.len();
            if total as f64 > RATE * 21600. {
                return Err("Preparation currently supports tracks up to six hours".into());
            }
            energy.push(
                (samples.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / samples.len() as f64)
                    .sqrt(),
            );
            let mut transient_energy = 0.;
            for sample in &samples {
                let value = *sample as f64;
                let high = value - 2. * previous + previous2;
                previous2 = previous;
                previous = value;
                transient_energy += high * high;
            }
            rhythmic.push((transient_energy / samples.len() as f64).sqrt());
            spectral.extend(samples);
            if spectral.len() >= FFT {
                chroma(&spectral[..FFT], &mut chroma_sum);
                spectral.drain(..FFT);
            }
            if n < buffer.len() {
                break;
            }
        }
        result(
            &energy,
            &rhythmic,
            &chroma_sum,
            total as f64 / RATE,
            existing,
        )
    })();
    if decoded.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("Audio decoding failed; source and sidecar were not changed".into());
    }
    let after = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err("Audio changed while it was being analyzed".into());
    }
    let mut p = decoded?;
    p.source_sha256 = Some(fingerprint);
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn click_tempo_and_manual_cues_survive() {
        let mut energy = vec![0.; 3000];
        for i in (100..2900).step_by(50) {
            energy[i] = 1.;
        }
        let (bpm, _, confidence) = tempo(&energy);
        assert!((bpm.unwrap() - 120.).abs() < 1.);
        assert!(confidence > 0.5);
        let mut existing = Preparation {
            cue_in: 2.,
            cue_out: Some(20.),
            manual_override: true,
            ..Default::default()
        };
        existing.manual_cues.push(Cue {
            id: "local-a".into(),
            label: "My drop".into(),
            seconds: 10.,
            kind: "hotcue".into(),
            ..Default::default()
        });
        let p = result(&energy, &energy, &[0.; 12], 30., &existing).unwrap();
        assert_eq!(p.cue_in, 2.);
        assert_eq!(p.manual_cues, existing.manual_cues);
        assert_eq!(p.key, None);
    }
    #[test]
    fn approved_cues_grid_and_analysis_survive_with_a_reviewable_proposal() {
        let mut existing = Preparation {
            cue_in: 2.,
            cue_out: Some(8.),
            bpm: Some(123.),
            beat_offset: Some(0.123),
            analysis: Some(AnalysisInfo {
                engine: "reviewed".into(),
                duration: 10.,
                tempo_confidence: 1.,
                key_confidence: 1.,
                reviewed: true,
            }),
            ..Default::default()
        };
        existing.cues.push(Cue {
            id: "approved-drop".into(),
            label: "Approved".into(),
            seconds: 3.,
            kind: "transition".into(),
            ..Default::default()
        });
        existing
            .extensions
            .insert("loops".into(), serde_json::json!([{"start":3.,"end":5.}]));
        let mut energy = vec![0.; 1000];
        energy[100..900].fill(0.5);
        let p = result(&energy, &energy, &[0.; 12], 10., &existing).unwrap();
        assert_eq!(p.cue_in, existing.cue_in);
        assert_eq!(p.bpm, existing.bpm);
        assert_eq!(p.beat_offset, existing.beat_offset);
        assert_eq!(p.cues, existing.cues);
        assert_eq!(p.analysis, existing.analysis);
        assert_eq!(p.extensions["loops"], existing.extensions["loops"]);
        assert!(crate::review::proposal(&p).is_some());
        assert!(!crate::review::changes(&p).is_empty());
    }
    #[test]
    fn tonal_fixture_and_silence() {
        let mut chroma = [0.; 12];
        for n in [60., 64., 67.] {
            let samples: Vec<_> = (0..FFT)
                .map(|i| {
                    (std::f64::consts::TAU * 440. * 2f64.powf((n - 69.) / 12.) * i as f64 / RATE)
                        .sin() as f32
                })
                .collect();
            super::chroma(&samples, &mut chroma);
        }
        let (name, wheel, _) = key(&chroma);
        assert_eq!(name.as_deref(), Some("C major"));
        assert_eq!(wheel.as_deref(), Some("8B"));
        assert!(
            result(
                &vec![0.; 1000],
                &vec![0.; 1000],
                &[0.; 12],
                10.,
                &Preparation::default()
            )
            .is_err()
        );
    }
    #[test]
    #[ignore = "requires ffmpeg"]
    fn real_decoder_to_portable_preparation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        assert!(
            crate::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=3"
                ])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let p = analyze(&path, &Preparation::default()).unwrap();
        assert!((p.analysis.as_ref().unwrap().duration - 3.).abs() < 0.02);
        assert_eq!(p.source_sha256.as_ref().unwrap().len(), 64);
        preparation::save(dir.path(), Path::new("tone.wav"), &p).unwrap();
        assert_eq!(preparation::load(&path).unwrap(), p);
    }
}
