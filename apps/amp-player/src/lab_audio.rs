//! Lab uses the same supervised stem worker as DJ; only the mixed result leaves RAM.
use amp_core::lab_processing::Processing;
use eframe::egui;
use std::{
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    time::Duration,
};
pub(super) fn controls(ui: &mut egui::Ui, p: &mut Processing) {
    let installed = crate::onnx_stem_worker::configured();
    crate::skin::panel(ui, false, |ui| {
        ui.set_width(ui.available_width());
        ui.strong("Stems / EQ / Filter");
        ui.columns(4, |columns| {
            for (i, label) in ["Vocals", "Instruments", "Beat"].iter().enumerate() {
                columns[i].add_enabled_ui(installed, |ui| {
                    ui.label(*label);
                    crate::dj_mixer::dial(
                        ui,
                        label,
                        &mut p.stems[i],
                        -1. ..=1.,
                        egui::Color32::YELLOW,
                    );
                });
            }
            columns[3].label("Filter");
            crate::dj_mixer::dial(
                &mut columns[3],
                "Filter",
                &mut p.filter,
                -1. ..=1.,
                egui::Color32::YELLOW,
            );
        });
        ui.columns(3, |columns| {
            for (i, label) in ["Low", "Mid", "High"].iter().enumerate() {
                columns[i].label(*label);
                let response =
                    columns[i].add(egui::Slider::new(&mut p.eq_db[i], -24. ..=12.).suffix(" dB"));
                if response.secondary_clicked() {
                    p.eq_db[i] = 0.;
                }
            }
        });
        if ui.button("Reset processing").clicked() {
            *p = Processing::default();
        }
        ui.weak("0 = unchanged · negative stem values cut, positive boost · filter left: low-pass / right: high-pass · right-click to reset.");
        if !installed {
            ui.weak("Install the live stem module to adjust vocals, instruments and beat. EQ and filter remain available.");
        }
        ui.weak("Applied to processed previews and exports. Separated stem audio stays in memory.");
    });
}
struct Process {
    child: Arc<Mutex<Child>>,
    done: Arc<AtomicBool>,
    watcher: Option<std::thread::JoinHandle<()>>,
}
impl Process {
    fn spawn(c: &mut Command, cancel: Arc<AtomicBool>) -> Result<Self, String> {
        let child = Arc::new(Mutex::new(c.stderr(Stdio::null()).spawn().map_err(
            |e| {
                format!(
                    "Could not launch {}: {e}",
                    c.get_program().to_string_lossy()
                )
            },
        )?));
        let done = Arc::new(AtomicBool::new(false));
        let w = child.clone();
        let d = done.clone();
        let watcher = std::thread::spawn(move || {
            while !d.load(Relaxed) {
                if cancel.load(Relaxed) {
                    if let Ok(mut c) = w.lock() {
                        let _ = c.kill();
                    }
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
        Ok(Self {
            child,
            done,
            watcher: Some(watcher),
        })
    }
    fn wait(&self) -> Result<(), String> {
        loop {
            if let Some(s) = self
                .child
                .lock()
                .map_err(|_| "Lab process unavailable")?
                .try_wait()
                .map_err(|e| e.to_string())?
            {
                return if s.success() {
                    Ok(())
                } else {
                    Err("Lab audio processing failed or was canceled".into())
                };
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.done.store(true, Relaxed);
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some(w) = self.watcher.take() {
            let _ = w.join();
        }
    }
}
fn pcm(
    source: &Path,
    start: f64,
    length: f64,
    cancel: Arc<AtomicBool>,
) -> Result<Vec<f32>, String> {
    let mut c = amp_core::command("ffmpeg");
    c.args(["-nostdin", "-v", "error", "-ss"])
        .arg(start.to_string())
        .arg("-i")
        .arg(source)
        .arg("-t")
        .arg(length.to_string())
        .args(["-vn", "-ar", "44100", "-ac", "2", "-f", "f32le", "pipe:1"])
        .stdout(Stdio::piped())
        .stdin(Stdio::null());
    let p = Process::spawn(&mut c, cancel)?;
    let out = p
        .child
        .lock()
        .map_err(|_| "Decoder unavailable")?
        .stdout
        .take()
        .ok_or("Decoder pipe unavailable")?;
    let max = crate::onnx_stem_worker::MAX_FRAMES * 8;
    let mut bytes = Vec::new();
    out.take((max + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > max || !bytes.len().is_multiple_of(8) {
        return Err("Lab decoder exceeded its bounded stereo window".into());
    }
    p.wait()?;
    let values: Vec<_> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| f32::from_le_bytes(*v))
        .collect();
    if values.iter().any(|v| !v.is_finite()) {
        return Err("Invalid Lab PCM".into());
    }
    Ok(values)
}
fn mix(parts: &[Vec<f32>; 3], levels: [f32; 3]) -> Result<Vec<f32>, String> {
    let n = parts[0].len();
    if parts
        .iter()
        .any(|p| p.len() != n || p.iter().any(|v| !v.is_finite()))
    {
        return Err("Invalid separated Lab audio".into());
    }
    Ok((0..n)
        .map(|i| (0..3).map(|s| parts[s][i] * (1. + levels[s])).sum())
        .collect())
}
/// Writes a user-selected result or disposable mixed preview, never per-stem files.
pub(super) fn render(
    source: &Path,
    start: f64,
    length: f64,
    p: Processing,
    filter: &str,
    output: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    p.validate()?;
    if !start.is_finite() || start < 0. || !length.is_finite() || length <= 0. {
        return Err("Invalid Lab selection".into());
    }
    if output.exists() {
        return Err("Output exists; choose a new filename".into());
    }
    if p.stems_changed() && !crate::onnx_stem_worker::configured() {
        return Err("Install the live stem module before rendering stem edits".into());
    }
    let pending = tempfile::Builder::new()
        .prefix("amp-lab-")
        .suffix(&format!(
            ".{}",
            output.extension().unwrap_or_default().to_string_lossy()
        ))
        .tempfile_in(output.parent().ok_or("Output folder unavailable")?)
        .map_err(|e| e.to_string())?;
    let mut c = amp_core::command("ffmpeg");
    c.args(["-nostdin", "-v", "error", "-y"]);
    if p.stems_changed() {
        c.args(["-f", "f32le", "-ar", "44100", "-ac", "2", "-i", "pipe:0"])
            .stdin(Stdio::piped());
    } else {
        c.arg("-ss")
            .arg(start.to_string())
            .arg("-i")
            .arg(source)
            .stdin(Stdio::null());
    }
    c.arg("-t")
        .arg(length.to_string())
        .args(["-vn", "-af"])
        .arg(filter)
        .args(["-ar", "48000", "-ac", "2"]);
    if output
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mp3"))
    {
        c.args(["-c:a", "libmp3lame", "-b:a", "256k", "-id3v2_version", "3"]);
    } else {
        c.args(["-c:a", "pcm_s16le"]);
    }
    c.arg(pending.path()).stdout(Stdio::null());
    let encoder = Process::spawn(&mut c, cancel.clone())?;
    if p.stems_changed() {
        let mut pipe = encoder
            .child
            .lock()
            .map_err(|_| "Encoder unavailable")?
            .stdin
            .take()
            .ok_or("Encoder pipe unavailable")?;
        let total = (length * 44100.).round() as usize;
        let mut at = 0usize;
        let mut tail: Vec<f32> = Vec::new();
        let overlap = 22050;
        let hop = crate::onnx_stem_worker::MAX_FRAMES - overlap;
        while at < total {
            if cancel.load(Relaxed) {
                return Err("Lab render canceled".into());
            }
            let frames = (total - at).min(crate::onnx_stem_worker::MAX_FRAMES);
            let original = pcm(
                source,
                start + at as f64 / 44100.,
                frames as f64 / 44100.,
                cancel.clone(),
            )?;
            if original.is_empty() {
                break;
            }
            let decoded = original.len() / 2;
            let parts = crate::onnx_stem_worker::separate(
                original,
                cancel.clone(),
                Duration::from_secs(60),
            )?;
            let mut mixed = mix(&parts, p.stems)?;
            if mixed.len() != decoded * 2 {
                return Err("Stem output duration mismatch".into());
            }
            let blend = tail.len().min(mixed.len()) / 2;
            for f in 0..blend {
                let a = f as f32 / (blend.saturating_sub(1).max(1)) as f32;
                for ch in 0..2 {
                    let i = f * 2 + ch;
                    mixed[i] = tail[i] * (1. - a) + mixed[i] * a;
                }
            }
            let emit = if frames < crate::onnx_stem_worker::MAX_FRAMES || decoded < frames {
                decoded
            } else {
                hop
            };
            tail = mixed[emit * 2..].to_vec();
            let bytes: Vec<u8> = mixed[..emit * 2]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            pipe.write_all(&bytes).map_err(|e| e.to_string())?;
            at += emit;
            if decoded < frames {
                break;
            }
        }
        drop(pipe);
    }
    encoder.wait()?;
    if cancel.load(Relaxed) {
        return Err("Lab render canceled".into());
    }
    pending
        .persist_noclobber(output)
        .map_err(|e| e.error.to_string())?;
    Ok(())
}
pub(super) fn timeline(
    root: &Path,
    clips: &[amp_core::arrangement::Clip],
    output: &Path,
) -> Result<(), String> {
    timeline_cancellable(root, clips, output, Arc::new(AtomicBool::new(false)))
}
pub(super) fn timeline_cancellable(
    root: &Path,
    clips: &[amp_core::arrangement::Clip],
    output: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    if output.exists() {
        return Err("Output exists; choose a new filename".into());
    }
    let stage = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut rendered = clips.to_vec();
    let mut used = 0u64;
    for (i, c) in clips.iter().enumerate() {
        if cancel.load(Relaxed) {
            return Err("Lab render canceled".into());
        }
        c.processing.validate()?;
        if c.processing.stems_changed() {
            // Disposable mixed stereo clips only, bounded to 512 MiB per render job.
            let estimate = (c.duration()? * 32000.) as u64 + 4096;
            used = used
                .checked_add(estimate)
                .ok_or("Lab scratch budget exceeded")?;
            if used > 512 * 1024 * 1024 {
                return Err("This stem-edited timeline exceeds the 512 MiB temporary mix budget; export smaller sections".into());
            }
            let path = stage.path().join(format!("mixed-{i}.mp3"));
            let mut selected = c.processing;
            selected.eq_db = [0.; 3];
            selected.filter = 0.;
            render(
                &amp_core::arrangement::source(root, &c.path)?,
                c.preparation.cue_in,
                c.duration()?,
                selected,
                "anull",
                &path,
                cancel.clone(),
            )?;
            rendered[i].path = path;
            rendered[i].preparation.cue_in = 0.;
            rendered[i].preparation.cue_out = Some(c.duration()?);
            rendered[i].processing.stems = [0.; 3];
        }
    }
    if cancel.load(Relaxed) {
        return Err("Lab render canceled".into());
    }
    amp_core::arrangement::render(root, &rendered, output)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lab_controls_show_visible_names_at_narrow_and_wide_widths() {
        for width in [600., 1200.] {
            let ctx = egui::Context::default();
            let mut p = Processing::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 800.),
                    )),
                    ..Default::default()
                },
                |ui| controls(ui, &mut p),
            );
            output.textures_delta.clear();
            fn collect(shape: &egui::Shape, labels: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(t) => labels.push(t.galley.text().to_owned()),
                    egui::Shape::Vec(children) => {
                        for s in children {
                            collect(s, labels);
                        }
                    }
                    _ => {}
                }
            }
            let mut labels = Vec::new();
            for s in &output.shapes {
                collect(&s.shape, &mut labels);
            }
            for label in [
                "Vocals",
                "Instruments",
                "Beat",
                "Low",
                "Mid",
                "High",
                "Filter",
            ] {
                assert!(
                    labels.iter().any(|s| s == label),
                    "missing visible {label} label at {width}: {labels:?}"
                );
            }
        }
    }
    #[test]
    fn stem_levels_mix_real_parts_and_reject_mismatches() {
        let p = [vec![1., 2.], vec![3., 4.], vec![5., 6.]];
        assert_eq!(mix(&p, [-1., 0., 0.]).unwrap(), vec![8., 10.]);
        assert_eq!(mix(&p, [0.; 3]).unwrap(), vec![9., 12.]);
        assert!(mix(&[vec![1.], vec![], vec![1.]], [0.; 3]).is_err());
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    fn source(root: &Path) -> std::path::PathBuf {
        let p = root.join("source.wav");
        assert!(
            amp_core::command("ffmpeg")
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=1000:duration=14",
                    "-ac",
                    "2"
                ])
                .arg(&p)
                .status()
                .unwrap()
                .success()
        );
        p
    }
    fn levels(path: &Path) -> Vec<f32> {
        let p = amp_core::command("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-f", "f32le", "-ac", "1", "pipe:1"])
            .output()
            .unwrap();
        assert!(p.status.success());
        p.stdout
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| f32::from_le_bytes(*v))
            .collect()
    }
    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|v| v * v).sum::<f32>() / v.len() as f32).sqrt()
    }
    #[test]
    #[ignore = "requires FFmpeg; run with Lab codec acceptance"]
    fn lab_processing_render_eq_changes_audio_and_cancel_preserves_source_and_destination() {
        let d = tempfile::tempdir().unwrap();
        let s = source(d.path());
        let before = std::fs::read(&s).unwrap();
        let plain = d.path().join("plain.wav");
        let cut = d.path().join("cut.wav");
        render(
            &s,
            0.25,
            1.,
            Processing::default(),
            "anull",
            &plain,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let p = Processing {
            eq_db: [0., -18., 0.],
            ..Default::default()
        };
        render(
            &s,
            0.25,
            1.,
            p,
            &p.filters().unwrap().join(","),
            &cut,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let a = levels(&plain);
        let b = levels(&cut);
        assert_eq!(a.len(), 48000);
        assert_eq!(a.len(), b.len());
        assert!(rms(&b) < rms(&a) * 0.2);
        let canceled = d.path().join("canceled.wav");
        assert!(
            render(
                &s,
                0.,
                1.,
                Processing::default(),
                "anull",
                &canceled,
                Arc::new(AtomicBool::new(true))
            )
            .is_err()
        );
        assert!(!canceled.exists());
        assert!(
            render(
                &s,
                0.,
                1.,
                p,
                "anull",
                &plain,
                Arc::new(AtomicBool::new(false))
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&s).unwrap(), before);
    }
    #[test]
    #[ignore = "requires configured native stem model and FFmpeg"]
    fn lab_live_stem_renderer_exports_selection_without_per_stem_audio_files() {
        let d = tempfile::tempdir().unwrap();
        let s = source(d.path());
        let out = d.path().join("vocals.wav");
        let p = Processing {
            stems: [0., -1., -1.],
            ..Default::default()
        };
        render(
            &s,
            0.25,
            12.5,
            p,
            "anull",
            &out,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(levels(&out).len(), 600000);
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 2);
        let clip = amp_core::arrangement::Clip {
            path: s.clone(),
            start: 0.5,
            preparation: amp_core::Preparation {
                cue_in: 0.25,
                cue_out: Some(1.25),
                ..Default::default()
            },
            processing: p,
            ..Default::default()
        };
        let output = d.path().join("timeline.mp3");
        timeline(d.path(), &[clip], &output).unwrap();
        assert!((crate::lab_editor::duration(&output).unwrap() - 1.5).abs() < 0.1);
    }
}
