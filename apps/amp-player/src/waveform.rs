//! Track-relative peak envelopes, decoded away from the audio callback and UI.
use eframe::egui::{self, Color32, Stroke};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};
const BIN: usize = 160; // 20 ms, independent of device sample rate.
#[derive(Default)]
pub struct Lane {
    pub height: Option<f32>,
    pub overview: bool,
    pub seek: Option<f64>,
    path: Option<PathBuf>,
    cancel: Option<Arc<AtomicBool>>,
    rx: Option<mpsc::Receiver<Result<Vec<[f32; 2]>, String>>>,
    peaks: Vec<[f32; 2]>,
    error: Option<String>,
}
fn decode(path: &Path, cancel: &AtomicBool) -> Result<Vec<[f32; 2]>, String> {
    let mut child = amp_core::command("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args([
            "-vn", "-ac", "1", "-ar", "8000", "-t", "21600", "-f", "f32le", "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let result = (|| {
        let mut stdout = child.stdout.take().ok_or("No waveform decoder output")?;
        let mut peaks = Vec::new();
        let mut bytes = [0u8; BIN * 4];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Waveform replaced".into());
            }
            let mut n = 0;
            while n < bytes.len() {
                let read = stdout.read(&mut bytes[n..]).map_err(|e| e.to_string())?;
                if read == 0 {
                    break;
                }
                n += read;
            }
            if n == 0 {
                break;
            }
            let mut peak = [0f32; 2];
            for b in bytes[..n].as_chunks::<4>().0 {
                let v = f32::from_le_bytes(*b);
                if v.is_finite() {
                    peak[0] = peak[0].min(v);
                    peak[1] = peak[1].max(v);
                }
            }
            peaks.push(peak);
            if n < bytes.len() {
                break;
            }
        }
        Ok(peaks)
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("Waveform decoding failed; check audio tools / source file".into());
    }
    result
}
fn time_x(time: f64, position: f64, span: f64, rect: egui::Rect) -> f32 {
    rect.left() + ((time - position) / span + 0.3) as f32 * rect.width()
}
// Keep a real graph area even in compact lanes; headers must not invert it.
fn graph_rect(rect: egui::Rect) -> egui::Rect {
    let top = rect.top() + 16_f32.min(rect.height() * 0.3);
    egui::Rect::from_min_max(
        egui::pos2(rect.left(), top),
        egui::pos2(rect.right(), rect.bottom() - 3.),
    )
}
impl Lane {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        root: &Path,
        track: Option<&amp_core::Track>,
        position: f64,
        span: f64,
        deck: usize,
    ) {
        let path = track.and_then(|t| amp_core::resolve(root, &t.path).ok());
        if path != self.path {
            if let Some(cancel) = self.cancel.take() {
                cancel.store(true, Ordering::Relaxed);
            }
            self.path = path.clone();
            self.peaks.clear();
            self.error = None;
            self.rx = None;
            if let Some(path) = path {
                let cancel = Arc::new(AtomicBool::new(false));
                self.cancel = Some(cancel.clone());
                let (tx, rx) = mpsc::channel();
                self.rx = Some(rx);
                std::thread::spawn(move || {
                    let _ = tx.send(decode(&path, &cancel));
                });
            }
        }
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(p) => self.peaks = p,
                        Err(e) => self.error = Some(e),
                    };
                    self.rx = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("Waveform worker stopped".into());
                    self.rx = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
        }
        let color = if deck == 0 {
            super::skin::ORANGE
        } else {
            super::skin::CYAN
        };
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), self.height.unwrap_or(80.)),
            egui::Sense::click(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3., Color32::from_rgb(5, 7, 10));
        let graph = graph_rect(rect);
        let camera = if self.overview { span * 0.3 } else { position };
        let start = camera - span * 0.3;
        if response.clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            self.seek =
                Some((start + (at.x - rect.left()) as f64 / rect.width() as f64 * span).max(0.));
        }
        if let Some(t) = track {
            if let (Some(bpm), Some(offset)) = (
                t.preparation.bpm.filter(|b| *b > 0.),
                t.preparation.beat_offset,
            ) {
                let beat = 60. / bpm;
                let first = ((start - offset) / beat).ceil().max(0.) as usize;
                for n in first..first + ((span / beat).ceil() as usize).min(1024) {
                    let x = time_x(offset + n as f64 * beat, camera, span, rect);
                    painter.line_segment(
                        [egui::pos2(x, graph.top()), egui::pos2(x, graph.bottom())],
                        Stroke::new(1., Color32::from_gray(35)),
                    );
                }
            }
            let lo = (start.max(0.) * 50.) as usize;
            let hi = (((start + span).max(0.) * 50.).ceil() as usize).min(self.peaks.len());
            let stride = ((span * 50. / rect.width().max(1.) as f64).ceil() as usize).max(1);
            for i in (lo..hi).step_by(stride) {
                let x = time_x(i as f64 / 50., camera, span, rect);
                let p = self.peaks[i..(i + stride).min(hi)]
                    .iter()
                    .fold([0f32; 2], |a, b| [a[0].min(b[0]), a[1].max(b[1])]);
                painter.line_segment(
                    [
                        egui::pos2(
                            x,
                            graph.center().y - p[1].clamp(0., 1.) * graph.height() / 2.,
                        ),
                        egui::pos2(
                            x,
                            graph.center().y - p[0].clamp(-1., 0.) * graph.height() / 2.,
                        ),
                    ],
                    Stroke::new((rect.width() / (span as f32 * 50.)).max(1.), color),
                );
            }
            if self.overview
                && let Some(profile) = amp_core::energy_profile::read(&t.preparation)
            {
                let points: Vec<_> = profile
                    .curve
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        egui::pos2(
                            time_x((i as f64 + 0.5) * profile.step, camera, span, rect),
                            graph.bottom() - *e as f32 * graph.height(),
                        )
                    })
                    .collect();
                painter.add(egui::Shape::line(
                    points,
                    Stroke::new(1.5, Color32::LIGHT_GREEN),
                ));
            }
            let mut markers: Vec<(f64, &str, Color32)> = t
                .preparation
                .cues
                .iter()
                .map(|c| {
                    (
                        c.seconds,
                        c.label.as_str(),
                        match c.kind.as_str() {
                            "drop" => super::skin::accent(ui),
                            "energy-start" | "mix-in" => super::skin::signal(ui),
                            _ => crate::theme::current(ui.ctx()).palette().muted,
                        },
                    )
                })
                .collect();
            markers.extend(
                t.preparation
                    .manual_cues
                    .iter()
                    .map(|c| (c.seconds, c.label.as_str(), super::skin::GOLD)),
            );
            markers.push((t.preparation.cue_in, "IN", Color32::GREEN));
            if let Some(out) = t.preparation.cue_out {
                markers.push((out, "OUT", Color32::LIGHT_RED));
            }
            markers.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut label_edge = [f32::NEG_INFINITY; 2];
            for (i, (seconds, label, c)) in markers.iter().enumerate() {
                let x = time_x(*seconds, camera, span, rect);
                if rect.x_range().contains(x) {
                    painter.line_segment(
                        [
                            egui::pos2(x, rect.top() + 16.),
                            egui::pos2(x, rect.bottom()),
                        ],
                        Stroke::new(1., *c),
                    );
                    let row = i % 2;
                    let short: String = label.chars().take(22).collect();
                    if x >= label_edge[row] {
                        painter.text(
                            egui::pos2(
                                x + 3.,
                                if row == 0 {
                                    rect.top() + 17.
                                } else {
                                    rect.bottom() - 13.
                                },
                            ),
                            egui::Align2::LEFT_TOP,
                            &short,
                            egui::FontId::monospace(10.),
                            *c,
                        );
                        label_edge[row] = x + short.len() as f32 * 6. + 12.;
                    }
                    if response.hover_pos().is_some_and(|p| (p.x - x).abs() < 5.) {
                        response
                            .clone()
                            .on_hover_text(format!("{label} · {seconds:.3} s (source)"));
                    }
                }
            }
        }
        let playhead = time_x(position, camera, span, rect);
        painter.line_segment(
            [
                egui::pos2(playhead, rect.top() + 16.),
                egui::pos2(playhead, rect.bottom()),
            ],
            Stroke::new(2., Color32::WHITE),
        );
        let name = track.map_or("Load a track", |t| t.title.as_str());
        painter.text(
            rect.left_top() + egui::vec2(5., 2.),
            egui::Align2::LEFT_TOP,
            format!(
                "{} · {position:.2}s · {name}",
                if deck == 0 { "A" } else { "B" }
            ),
            egui::FontId::monospace(12.),
            color,
        );
        if self.peaks.is_empty() && track.is_some() {
            painter.text(
                graph.center(),
                egui::Align2::CENTER_CENTER,
                self.error.as_deref().unwrap_or("Building track waveform…"),
                egui::FontId::proportional(12.),
                Color32::WHITE,
            );
        }
    }
}
impl Drop for Lane {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires ffmpeg and AMP_WAVEFORM_FIXTURE"]
    fn real_audio_has_silence_then_signal() {
        let path = std::env::var_os("AMP_WAVEFORM_FIXTURE").expect("fixture path");
        let peaks = decode(Path::new(&path), &AtomicBool::new(false)).unwrap();
        assert!((490..=510).contains(&peaks.len()));
        assert!(
            peaks[..40]
                .iter()
                .all(|p| p[0].abs().max(p[1].abs()) < 0.001)
        );
        assert!(peaks[60..400].iter().any(|p| p[1] > 0.01 && p[0] < -0.01));
        assert!(decode(Path::new(&path), &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn compact_waveforms_have_positive_visible_signal_area() {
        for height in [26., 56., 72., 140.] {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., height));
            let graph = graph_rect(rect);
            assert!(graph.height() >= height * 0.5);
            assert!(rect.contains_rect(graph));
        }
    }
    #[test]
    fn cues_scroll_past_fixed_playhead() {
        let r = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 80.));
        assert_eq!(time_x(10., 10., 20., r), 300.);
        assert_eq!(time_x(12., 10., 20., r), 400.);
        assert_eq!(time_x(12., 12., 20., r), 300.);
        assert_eq!(time_x(10., 12., 20., r), 200.);
    }
}
