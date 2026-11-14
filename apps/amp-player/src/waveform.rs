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
type Peaks = Arc<[[f32; 2]]>;
const BIN: usize = 160; // 20 ms, independent of device sample rate.
#[derive(Default)]
pub struct Lane {
    pub height: Option<f32>,
    pub overview: bool,
    pub overlay: bool,
    pub ignore_seek: bool,
    pub tint: Option<Color32>,
    pub hide_caption: bool,
    pub seek: Option<f64>,
    path: Option<PathBuf>,
    cancel: Option<Arc<AtomicBool>>,
    rx: Option<mpsc::Receiver<Result<Peaks, String>>>,
    peaks: Arc<[[f32; 2]]>,
    error: Option<String>,
}
#[derive(Default)]
struct CacheEntry {
    peaks: Option<std::sync::Weak<[[f32; 2]]>>,
    building: bool,
}
fn shared_decode(path: &Path, cancel: &AtomicBool) -> Result<Arc<[[f32; 2]]>, String> {
    use std::sync::{Condvar, Mutex, OnceLock};
    type Entry = Arc<(Mutex<CacheEntry>, Condvar)>;
    static CACHE: OnceLock<Mutex<std::collections::HashMap<PathBuf, Entry>>> = OnceLock::new();
    let entry = CACHE
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "Waveform cache unavailable")?
        .entry(path.to_path_buf())
        .or_insert_with(|| Arc::new((Mutex::new(CacheEntry::default()), Condvar::new())))
        .clone();
    let mut state = entry.0.lock().map_err(|_| "Waveform cache unavailable")?;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Waveform replaced".into());
        }
        if let Some(peaks) = state.peaks.as_ref().and_then(std::sync::Weak::upgrade) {
            return Ok(peaks);
        }
        if !state.building {
            state.building = true;
            break;
        }
        state = entry
            .1
            .wait_timeout(state, std::time::Duration::from_millis(100))
            .map_err(|_| "Waveform cache unavailable")?
            .0;
    }
    drop(state);
    let result = decode(path, cancel).map(Arc::<[[f32; 2]]>::from);
    let mut state = entry.0.lock().map_err(|_| "Waveform cache unavailable")?;
    state.building = false;
    if let Ok(peaks) = &result {
        state.peaks = Some(Arc::downgrade(peaks));
    }
    entry.1.notify_all();
    result
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
    fn prepare(&mut self, ui: &egui::Ui, root: &Path, track: Option<&amp_core::Track>) {
        let path = track.and_then(|t| amp_core::resolve(root, &t.path).ok());
        if path != self.path {
            if let Some(cancel) = self.cancel.take() {
                cancel.store(true, Ordering::Relaxed);
            }
            self.path = path.clone();
            self.peaks = Arc::default();
            self.error = None;
            self.rx = None;
            if let Some(path) = path {
                let cancel = Arc::new(AtomicBool::new(false));
                self.cancel = Some(cancel.clone());
                let (tx, rx) = mpsc::channel();
                self.rx = Some(rx);
                std::thread::spawn(move || {
                    let _ = tx.send(shared_decode(&path, &cancel));
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
    }
    pub fn thumbnail(
        &mut self,
        ui: &mut egui::Ui,
        root: &Path,
        track: Option<&amp_core::Track>,
        position: f64,
        accent: egui::Color32,
        height: f32,
    ) -> Option<f64> {
        self.prepare(ui, root, track);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(1.), height.max(1.)),
            egui::Sense::click(),
        );
        let painter = ui.painter_at(rect);
        let profile = track
            .and_then(|t| amp_core::energy_profile::read(&t.preparation))
            .filter(|p| p.duration > 0. && !p.sections.is_empty());
        let mut choice = None;
        if let (Some(profile), Some(track)) = (&profile, track) {
            for (index, section) in profile.sections.iter().enumerate() {
                let section_rect = egui::Rect::from_min_max(
                    egui::pos2(
                        rect.left() + (section.start / profile.duration) as f32 * rect.width(),
                        rect.top(),
                    ),
                    egui::pos2(
                        rect.left() + (section.end / profile.duration) as f32 * rect.width(),
                        rect.bottom(),
                    ),
                )
                .intersect(rect);
                let allowed = section.start
                    >= amp_core::energy_profile::entry_bound(&track.preparation)
                    && track
                        .preparation
                        .cue_out
                        .is_none_or(|out| section.start < out);
                let current = position >= section.start && position < section.end;
                let hovered = response
                    .hover_pos()
                    .is_some_and(|p| section_rect.contains(p));
                let color = if allowed { accent } else { egui::Color32::GRAY };
                painter.rect_filled(
                    section_rect,
                    1.,
                    color.gamma_multiply(if current {
                        0.28
                    } else if hovered {
                        0.2
                    } else {
                        0.08
                    }),
                );
                painter.line_segment(
                    [section_rect.left_top(), section_rect.left_bottom()],
                    Stroke::new(1., color),
                );
                let band = egui::Rect::from_min_max(
                    egui::pos2(section_rect.left(), (rect.bottom() - 14.).max(rect.top())),
                    section_rect.right_bottom(),
                );
                painter.rect_filled(band, 0., color.gamma_multiply(0.5));
                if section_rect.width() >= 16. {
                    painter.text(
                        band.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("{}", index + 1),
                        egui::FontId::monospace(10.),
                        egui::Color32::WHITE,
                    );
                }
                #[cfg(test)]
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        ui.scope_id().with(("thumbnail-section", index)),
                        section_rect,
                    )
                });
                if hovered {
                    response.clone().on_hover_text(format!(
                        "{} · {:.1}–{:.1}s · energy {:.0}/10\n{}",
                        section.name,
                        section.start,
                        section.end,
                        1. + section.energy * 9.,
                        if allowed {
                            "Click: prepare here when paused; queue a timed jump when playing."
                        } else {
                            "Outside protected cue bounds. Edit cues in Library to enable."
                        }
                    ));
                    if allowed && response.clicked() {
                        choice = Some(section.start);
                    }
                }
            }
        }
        let n = self.peaks.len();
        if n > 0 {
            let graph = if profile.is_some() {
                egui::Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.right(), (rect.bottom() - 14.).max(rect.top() + 1.)),
                )
            } else {
                rect
            };
            let count = (rect.width().max(1.) as usize).min(n);
            for x in 0..count {
                let index = x * n / count;
                let peak = self.peaks[index];
                let px = rect.left() + x as f32 * rect.width() / count as f32;
                painter.line_segment(
                    [
                        egui::pos2(px, graph.center().y + -peak[0] * graph.height() * 0.45),
                        egui::pos2(px, graph.center().y - peak[1] * graph.height() * 0.45),
                    ],
                    egui::Stroke::new(1., accent),
                );
            }
            let duration = n as f64 / 50.;
            let x = rect.left() + (position / duration).clamp(0., 1.) as f32 * rect.width();
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1., egui::Color32::WHITE),
            );
        }
        if profile.is_none() && track.is_some() {
            painter.text(
                rect.left_bottom(),
                egui::Align2::LEFT_BOTTOM,
                "Analyze for sections",
                egui::FontId::proportional(10.),
                egui::Color32::GRAY,
            );
            response
                .on_hover_text("Analyze this song in Library to enable clickable energy sections.");
        }
        choice
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        root: &Path,
        track: Option<&amp_core::Track>,
        position: f64,
        span: f64,
        deck: usize,
    ) {
        self.prepare(ui, root, track);
        let color = self.tint.unwrap_or(if deck.is_multiple_of(2) {
            super::skin::ORANGE
        } else {
            super::skin::CYAN
        });
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), self.height.unwrap_or(80.)),
            if self.ignore_seek {
                egui::Sense::hover()
            } else {
                egui::Sense::click()
            },
        );
        #[cfg(test)]
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::unique(("waveform-lane", deck)), rect));
        let painter = ui.painter_at(rect);
        if !self.overlay {
            painter.rect_filled(rect, 3., Color32::from_rgb(5, 7, 10));
        }
        let graph = graph_rect(rect);
        let camera = if self.overview { span * 0.3 } else { position };
        let start = camera - span * 0.3;
        if !self.ignore_seek
            && response.clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            self.seek =
                Some((start + (at.x - rect.left()) as f64 / rect.width() as f64 * span).max(0.));
        }
        if let Some(t) = track {
            if let Some(profile) = amp_core::energy_profile::read(&t.preparation) {
                for (index, section) in profile.sections.iter().enumerate() {
                    let left = time_x(section.start, camera, span, rect).max(rect.left());
                    let right = time_x(section.end, camera, span, rect).min(rect.right());
                    if right > left {
                        let band = egui::Rect::from_min_max(
                            egui::pos2(left, rect.top()),
                            egui::pos2(right, rect.bottom()),
                        );
                        painter.rect_filled(
                            band,
                            0.,
                            color.gamma_multiply(
                                if position >= section.start && position < section.end {
                                    0.18
                                } else {
                                    0.07
                                },
                            ),
                        );
                        painter.vline(
                            left,
                            rect.y_range(),
                            Stroke::new(1., color.gamma_multiply(0.5)),
                        );
                        if right - left >= 28. {
                            painter.text(
                                egui::pos2(left + 3., rect.bottom()),
                                egui::Align2::LEFT_BOTTOM,
                                format!("{}", index + 1),
                                egui::FontId::monospace(9.),
                                color,
                            );
                        }
                    }
                }
            }
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
                        if response.clicked() {
                            self.seek = Some(*seconds);
                        }
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
        if !self.hide_caption {
            let name = track.map_or("Load a track", |t| t.title.as_str());
            painter.text(
                rect.left_top() + egui::vec2(5., 2.),
                egui::Align2::LEFT_TOP,
                format!(
                    "{} · {position:.2}s · {name}",
                    if deck.is_multiple_of(2) { "A" } else { "B" }
                ),
                egui::FontId::monospace(12.),
                color,
            );
        }
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
    fn strip_sections_click_their_start_and_protect_manual_cue_bounds() {
        for (section_index, expected) in [(0usize, None), (1, Some(10.))] {
            let ctx = egui::Context::default();
            let mut lane = Lane::default();
            let mut track = amp_core::Track {
                path: "missing.wav".into(),
                title: "Sections".into(),
                preparation: Default::default(),
            };
            track.preparation.cue_in = 5.;
            track.preparation.manual_override = true;
            track.preparation.extensions.insert("energy_profile".into(),serde_json::json!({"version":1,"duration":20.,"step":1.,"curve":[],"sections":[{"id":"intro","name":"Beginning","start":0.,"end":10.,"energy":0.3},{"id":"drop","name":"High energy","start":10.,"end":20.,"energy":0.9}]}));
            let mut point = egui::Pos2::ZERO;
            let mut chosen = None;
            for frame in 0..4 {
                let mut raw = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(300., 100.),
                    )),
                    ..Default::default()
                };
                raw.events.push(egui::Event::PointerMoved(point));
                if frame >= 2 {
                    raw.events.push(egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 2,
                        modifiers: Default::default(),
                    });
                }
                let mut output = ctx.run_ui(raw, |ui| {
                    if let Some(at) =
                        lane.thumbnail(ui, Path::new(""), Some(&track), 6., Color32::YELLOW, 60.)
                    {
                        chosen = Some(at);
                    }
                    point = ui
                        .ctx()
                        .data(|data| {
                            data.get_temp::<egui::Rect>(
                                ui.scope_id().with(("thumbnail-section", section_index)),
                            )
                        })
                        .unwrap()
                        .center();
                });
                output.textures_delta.clear();
            }
            assert_eq!(chosen, expected);
        }
    }
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
    #[ignore = "requires ffmpeg and AMP_WAVEFORM_FIXTURE"]
    fn concurrent_views_share_the_same_decoded_peak_buffer() {
        let path = PathBuf::from(std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap());
        let barrier = Arc::new(std::sync::Barrier::new(6));
        let workers: Vec<_> = (0..6)
            .map(|_| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    shared_decode(&path, &AtomicBool::new(false)).unwrap()
                })
            })
            .collect();
        let buffers: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        assert!(!buffers[0].is_empty());
        assert!(buffers.iter().all(|peaks| Arc::ptr_eq(&buffers[0], peaks)));
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
