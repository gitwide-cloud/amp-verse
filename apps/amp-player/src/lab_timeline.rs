use super::*;
use std::{
    collections::{HashMap, VecDeque},
    process::Child,
};
type Peaks = Vec<[f32; 2]>;
type WaveJob = (PathBuf, mpsc::Receiver<Result<(f64, Peaks), String>>);
type Imported = Result<(PathBuf, f64, Peaks), String>;
#[derive(Default)]
pub(super) struct Timeline {
    selected: Option<usize>,
    zoom: f64,
    cursor: f64,
    lanes: usize,
    waves: HashMap<PathBuf, Peaks>,
    pending: Vec<mpsc::Receiver<Imported>>,
    placements: Vec<(usize, f64)>,
    queued: VecDeque<(PathBuf, usize, f64)>,
    wave_jobs: Vec<WaveJob>,
    source_lengths: HashMap<PathBuf, f64>,
    undo: Vec<Vec<Clip>>,
    drag: Option<(usize, Clip, u8)>,
    player: Option<Child>,
    preview: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    preview_file: Option<PathBuf>,
    #[cfg(test)]
    area: Option<egui::Rect>,
}
impl Timeline {
    pub fn reset(&mut self) {
        self.stop();
        self.selected = None;
        self.undo.clear();
        self.pending.clear();
        self.placements.clear();
        self.queued.clear();
        self.drag = None;
    }
    pub fn stop(&mut self) {
        if let Some(mut p) = self.player.take() {
            let _ = p.kill();
            let _ = p.wait();
        }
        self.preview = None;
        if let Some(p) = self.preview_file.take() {
            let _ = std::fs::remove_file(p);
        }
    }
    pub(super) fn remember(&mut self, clips: &[Clip]) {
        self.undo.push(clips.to_vec());
        if self.undo.len() > 40 {
            self.undo.remove(0);
        }
    }
    pub(super) fn import(&mut self, path: PathBuf, lane: usize, start: f64) {
        if amp_core::supported(&path) {
            self.queued.push_back((path, lane.min(63), start));
        }
    }
    fn dispatch(&mut self) {
        while self.pending.len() < 2 {
            let Some((path, lane, start)) = self.queued.pop_front() else {
                break;
            };
            let (tx, rx) = mpsc::channel();
            self.pending.push(rx);
            // Placement travels with the import; decoding never blocks the UI.
            std::thread::spawn(move || {
                let result = (|| {
                    let d = crate::lab_editor::duration(&path)?;
                    if !d.is_finite() || d <= 0. {
                        return Err("Invalid audio duration".into());
                    }
                    let wave = crate::lab_editor::read_waveform(&path, d)?;
                    Ok((path, d, wave))
                })();
                let _ = tx.send(result);
            });
            self.placements.push((lane, start));
        }
    }
}
impl Drop for Timeline {
    fn drop(&mut self) {
        self.stop();
    }
}
impl Arrangement {
    pub(super) fn timeline_ui(&mut self, ui: &mut egui::Ui, root: &Path) {
        self.timeline.dispatch();
        let mut completed = Vec::new();
        for (i, rx) in self.timeline.pending.iter().enumerate() {
            if let Ok(result) = rx.try_recv() {
                completed.push((i, result));
            }
        }
        for (i, result) in completed.into_iter().rev() {
            let (lane, start) = self.timeline.placements.remove(i);
            self.timeline.pending.remove(i);
            match result {
                Ok((path, d, wave)) => {
                    self.timeline.remember(&self.clips);
                    self.timeline.source_lengths.insert(path.clone(), d);
                    self.timeline.waves.insert(path.clone(), wave);
                    self.clips.push(Clip {
                        path,
                        start,
                        lane,
                        preparation: amp_core::Preparation {
                            cue_out: Some(d),
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                    self.timeline.selected = Some(self.clips.len() - 1);
                    self.status = "Audio added — original file unchanged".into();
                }
                Err(e) => self.status = e,
            }
        }
        // Library clips use the same asynchronous waveform cache as dropped files.
        for c in &self.clips {
            if !self.timeline.waves.contains_key(&c.path) && self.timeline.wave_jobs.len() < 4 {
                self.timeline.waves.insert(c.path.clone(), Vec::new());
                if let Ok(p) = amp_core::arrangement::source(root, &c.path) {
                    let key = c.path.clone();
                    let (tx, rx) = mpsc::channel();
                    self.timeline.wave_jobs.push((key, rx));
                    std::thread::spawn(move || {
                        let result = crate::lab_editor::duration(&p)
                            .and_then(|d| crate::lab_editor::read_waveform(&p, d).map(|w| (d, w)));
                        let _ = tx.send(result);
                    });
                }
            }
        }
        let mut done = Vec::new();
        for (i, (_, rx)) in self.timeline.wave_jobs.iter().enumerate() {
            if let Ok(r) = rx.try_recv() {
                done.push((i, r));
            }
        }
        for (i, r) in done.into_iter().rev() {
            let (key, _) = self.timeline.wave_jobs.remove(i);
            if let Ok((d, w)) = r {
                self.timeline.source_lengths.insert(key.clone(), d);
                self.timeline.waves.insert(key, w);
            }
        }
        if let Some(rx) = &self.timeline.preview
            && let Ok(result) = rx.try_recv()
        {
            self.timeline.preview = None;
            match result {
                Ok(p) => {
                    match amp_core::command("ffplay")
                        .args([
                            "-nodisp",
                            "-autoexit",
                            "-loglevel",
                            "error",
                            "-volume",
                            "65",
                        ])
                        .arg(&p)
                        .spawn()
                    {
                        Ok(child) => {
                            self.timeline.player = Some(child);
                            self.timeline.preview_file = Some(p);
                            self.status = "Playing 30-second timeline preview".into();
                        }
                        Err(e) => self.status = e.to_string(),
                    }
                }
                Err(e) => self.status = e,
            }
        }
        if !self.timeline.queued.is_empty()
            || !self.timeline.pending.is_empty()
            || !self.timeline.wave_jobs.is_empty()
            || self.timeline.preview.is_some()
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if !self.timeline.pending.is_empty() || !self.timeline.queued.is_empty() {
            ui.weak(format!(
                "Reading {} audio files in background…",
                self.timeline.pending.len() + self.timeline.queued.len()
            ));
        }
        let extent = self
            .clips
            .iter()
            .filter_map(|c| c.duration().ok().map(|d| c.start + d))
            .fold(60., f64::max);
        if self.timeline.zoom <= 0. {
            self.timeline.zoom = 2.;
        }
        self.timeline.lanes = self
            .timeline
            .lanes
            .max(3)
            .max(self.clips.iter().map(|c| c.lane + 1).max().unwrap_or(0));
        let fit_width = ui.available_width() - 110.;
        ui.horizontal_wrapped(|ui| {
            if ui.button("+ Add audio…").clicked()
                && let Some(files) = rfd::FileDialog::new()
                    .add_filter(
                        "Audio",
                        &["mp3", "wav", "flac", "m4a", "ogg", "opus", "aac"],
                    )
                    .pick_files()
            {
                for (p, path) in files.into_iter().enumerate() {
                    self.timeline.import(path, p, self.timeline.cursor);
                }
            }
            if ui.button("+ Lane").clicked() {
                self.timeline.lanes = (self.timeline.lanes + 1).min(64);
            }
            if ui
                .add_enabled(!self.timeline.undo.is_empty(), egui::Button::new("Undo"))
                .clicked()
            {
                self.clips = self.timeline.undo.pop().unwrap();
                self.timeline.selected = None;
            }
            if ui.button("−").clicked() {
                self.timeline.zoom = (self.timeline.zoom / 1.5).max(0.02);
            }
            if ui.button("+").clicked() {
                self.timeline.zoom = (self.timeline.zoom * 1.5).min(200.);
            }
            if ui.button("Fit mix").clicked() {
                self.timeline.zoom = (fit_width.max(100.) as f64 / extent).clamp(0.02, 200.);
            }
            ui.label("Playhead");
            ui.add(
                egui::DragValue::new(&mut self.timeline.cursor)
                    .range(0. ..=864000.)
                    .speed(0.1)
                    .suffix(" s"),
            );
            if ui
                .add_enabled(
                    !self.clips.is_empty() && self.timeline.preview.is_none(),
                    egui::Button::new("Preview 30 s"),
                )
                .clicked()
            {
                self.preview_timeline(root);
            }
            if ui.button("Stop preview").clicked() {
                self.timeline.stop();
            }
        });
        ui.weak("Drop MP3s onto a lane • drag a clip to move • drag its edges to trim • click the ruler to position the playhead");
        let scale = self.timeline.zoom as f32;
        let width = ((extent + 30.) as f32 * scale + 110.)
            .max(ui.available_width())
            .min(2_000_000.);
        let height = (self.timeline.lanes as f32 * 100. + 30.).min(100_000.);
        let mut remove = None;
        let mut split = None;
        egui::ScrollArea::both()
            .id_salt("lab-multitrack")
            .auto_shrink([false, false])
            .max_height(420.)
            .show(ui, |ui| {
                let (area, _) =
                    ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
                #[cfg(test)]
                {
                    self.timeline.area = Some(area);
                }
                let left = area.left() + 110.;
                let top = area.top() + 30.;
                let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(area));
                painter.rect_filled(area, 3., ui.visuals().extreme_bg_color);
                let visible = ui.clip_rect().intersect(area);
                let step = if scale > 30. {
                    1.
                } else if scale > 5. {
                    10.
                } else if scale > 1. {
                    60.
                } else {
                    300.
                };
                let first = (((visible.left() - left).max(0.) / scale) / step).floor() as usize;
                let last = (((visible.right() - left).max(0.) / scale) / step).ceil() as usize;
                for n in first..=last {
                    let sec = n as f32 * step;
                    let x = left + sec * scale;
                    painter.line_segment(
                        [egui::pos2(x, area.top()), egui::pos2(x, area.bottom())],
                        egui::Stroke::new(1., egui::Color32::from_gray(48)),
                    );
                    painter.text(
                        egui::pos2(x + 3., area.top() + 5.),
                        egui::Align2::LEFT_TOP,
                        format!("{}:{:02}", sec as u64 / 60, sec as u64 % 60),
                        egui::FontId::monospace(11.),
                        egui::Color32::LIGHT_GRAY,
                    );
                }
                let ruler = egui::Rect::from_min_max(
                    egui::pos2(left, area.top()),
                    egui::pos2(area.right(), top),
                );
                let r = ui.interact(
                    ruler,
                    ui.scope_id().with("mix-ruler"),
                    egui::Sense::click_and_drag(),
                );
                if (r.clicked() || r.dragged())
                    && let Some(p) = r.interact_pointer_pos()
                {
                    self.timeline.cursor = ((p.x - left) / scale).max(0.) as f64;
                }
                for lane in 0..self.timeline.lanes {
                    let y = top + lane as f32 * 100.;
                    painter.line_segment(
                        [egui::pos2(area.left(), y), egui::pos2(area.right(), y)],
                        egui::Stroke::new(1., egui::Color32::GRAY),
                    );
                    painter.text(
                        egui::pos2(area.left() + 10., y + 15.),
                        egui::Align2::LEFT_TOP,
                        format!("Track {}", lane + 1),
                        egui::FontId::proportional(14.),
                        egui::Color32::LIGHT_GRAY,
                    );
                }
                let pointer = ui.input(|i| i.pointer.hover_pos());
                if let Some(pos) = pointer
                    && area.contains(pos)
                {
                    let files = ui.ctx().input(|i| i.raw.dropped_files.clone());
                    for f in files {
                        {
                            let p = f.path().to_path_buf();
                            self.timeline.import(
                                p,
                                ((pos.y - top) / 100.).max(0.) as usize,
                                ((pos.x - left) / scale).max(0.) as f64,
                            );
                        }
                    }
                }
                for i in 0..self.clips.len() {
                    let c = &self.clips[i];
                    let Ok(d) = c.duration() else { continue };
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(
                            left + c.start as f32 * scale,
                            top + c.lane as f32 * 100. + 4.,
                        ),
                        egui::vec2((d as f32 * scale).max(12.), 90.),
                    );
                    if !rect.intersects(visible) {
                        continue;
                    }
                    let color = if c.lane.is_multiple_of(2) {
                        egui::Color32::from_rgb(215, 158, 36)
                    } else {
                        egui::Color32::from_rgb(32, 177, 203)
                    };
                    painter.rect_filled(
                        rect,
                        4.,
                        color.gamma_multiply(if self.timeline.selected == Some(i) {
                            0.55
                        } else {
                            0.3
                        }),
                    );
                    painter.rect_stroke(
                        rect,
                        4.,
                        egui::Stroke::new(
                            if self.timeline.selected == Some(i) {
                                2.
                            } else {
                                1.
                            },
                            color,
                        ),
                        egui::StrokeKind::Inside,
                    );
                    let cp = painter.with_clip_rect(rect.intersect(visible));
                    cp.text(
                        rect.left_top() + egui::vec2(7., 5.),
                        egui::Align2::LEFT_TOP,
                        c.path.file_name().unwrap_or_default().to_string_lossy(),
                        egui::FontId::proportional(13.),
                        egui::Color32::WHITE,
                    );
                    if let Some(w) = self.timeline.waves.get(&c.path)
                        && !w.is_empty()
                    {
                        let full = self
                            .timeline
                            .source_lengths
                            .get(&c.path)
                            .copied()
                            .unwrap_or(c.preparation.cue_out.unwrap_or(d));
                        let x0 = rect.left().max(visible.left());
                        let x1 = rect.right().min(visible.right());
                        for x in (x0 as i32)..=(x1 as i32) {
                            let time = c.preparation.cue_in
                                + (x as f32 - rect.left()) as f64 / self.timeline.zoom;
                            let idx = ((time / full) * w.len() as f64) as usize;
                            if let Some(v) = w.get(idx) {
                                let a = v[0].max(v[1]).min(1.) * 26.;
                                cp.line_segment(
                                    [
                                        egui::pos2(x as f32, rect.center().y - a + 8.),
                                        egui::pos2(x as f32, rect.center().y + a + 8.),
                                    ],
                                    egui::Stroke::new(1., color),
                                );
                            }
                        }
                    }
                    for (fade, incoming) in [(c.fade_in, true), (c.fade_out, false)] {
                        if fade > 0. {
                            let w = (fade as f32 * scale).min(rect.width());
                            let (a, b) = if incoming {
                                (
                                    rect.left_bottom(),
                                    egui::pos2(rect.left() + w, rect.top() + 23.),
                                )
                            } else {
                                (
                                    egui::pos2(rect.right() - w, rect.top() + 23.),
                                    rect.right_bottom(),
                                )
                            };
                            cp.line_segment([a, b], egui::Stroke::new(2., egui::Color32::WHITE));
                        }
                    }
                    let response = ui.interact(
                        rect,
                        ui.scope_id().with(("mix-clip", i)),
                        egui::Sense::click_and_drag(),
                    );
                    if response.clicked() {
                        self.timeline.selected = Some(i);
                    }
                    if response.drag_started() {
                        self.timeline.remember(&self.clips);
                        self.timeline.selected = Some(i);
                        let p = response.interact_pointer_pos().unwrap_or(rect.center())
                            - response.total_drag_delta().unwrap_or_default();
                        let mode = if p.x - rect.left() < 9. {
                            1
                        } else if rect.right() - p.x < 9. {
                            2
                        } else {
                            0
                        };
                        self.timeline.drag = Some((i, self.clips[i].clone(), mode));
                    }
                    if response.dragged()
                        && let Some((index, original, mode)) = &self.timeline.drag
                        && *index == i
                    {
                        let delta = response.total_drag_delta().unwrap_or_default();
                        let c = &mut self.clips[i];
                        let seconds = delta.x as f64 / self.timeline.zoom;
                        match mode {
                            1 => {
                                let delta = seconds
                                    .clamp(
                                        -original.preparation.cue_in,
                                        original.duration().unwrap_or(0.05) - 0.05,
                                    )
                                    .max(-original.start);
                                c.start = original.start + delta;
                                c.preparation.cue_in = original.preparation.cue_in + delta;
                            }
                            2 => {
                                let max = self
                                    .timeline
                                    .source_lengths
                                    .get(&c.path)
                                    .copied()
                                    .unwrap_or(original.preparation.cue_out.unwrap_or(d));
                                c.preparation.cue_out = Some(
                                    (original.preparation.cue_out.unwrap() + seconds)
                                        .clamp(original.preparation.cue_in + 0.05, max),
                                );
                            }
                            _ => {
                                c.start = (original.start + seconds).max(0.);
                                c.lane = (original.lane as f32 + delta.y / 100.).round().max(0.)
                                    as usize;
                                c.lane = c.lane.min(63);
                            }
                        }
                    }
                    if response.drag_stopped() {
                        self.timeline.drag = None;
                    }
                }
                let x = left + self.timeline.cursor as f32 * scale;
                painter.line_segment(
                    [egui::pos2(x, area.top()), egui::pos2(x, area.bottom())],
                    egui::Stroke::new(2., egui::Color32::WHITE),
                );
            });
        if let Some(i) = self.timeline.selected.filter(|i| *i < self.clips.len()) {
            let before = self.clips.clone();
            ui.horizontal_wrapped(|ui| {
                let c = &mut self.clips[i];
                ui.label(c.path.file_name().unwrap_or_default().to_string_lossy());
                ui.label("Start");
                ui.add(
                    egui::DragValue::new(&mut c.start)
                        .range(0. ..=864000.)
                        .speed(0.1)
                        .suffix(" s"),
                );
                ui.label("In");
                ui.add(
                    egui::DragValue::new(&mut c.preparation.cue_in)
                        .range(0. ..=c.preparation.cue_out.unwrap_or(864000.) - 0.05)
                        .speed(0.1),
                );
                ui.label("Out");
                let max = self
                    .timeline
                    .source_lengths
                    .get(&c.path)
                    .copied()
                    .unwrap_or(c.preparation.cue_out.unwrap_or(864000.));
                ui.add(
                    egui::DragValue::new(c.preparation.cue_out.get_or_insert(max))
                        .range(c.preparation.cue_in + 0.05..=max)
                        .speed(0.1),
                );
                ui.add(
                    egui::DragValue::new(&mut c.gain_db)
                        .range(-60. ..=12.)
                        .suffix(" dB"),
                );
                let d = c.duration().unwrap_or(0.);
                ui.label("Fade in");
                ui.add(
                    egui::DragValue::new(&mut c.fade_in)
                        .range(0. ..=d)
                        .speed(0.1)
                        .suffix(" s"),
                );
                ui.label("Fade out");
                ui.add(
                    egui::DragValue::new(&mut c.fade_out)
                        .range(0. ..=d)
                        .speed(0.1)
                        .suffix(" s"),
                );
                if ui.button("Split at playhead").clicked() {
                    split = Some(i);
                }
                if ui.button("Remove clip").clicked() {
                    remove = Some(i);
                }
            });
            if serde_json::to_vec(&before).ok() != serde_json::to_vec(&self.clips).ok() {
                self.timeline.remember(&before);
            }
        }
        if let Some(i) = split {
            let t = self.timeline.cursor - self.clips[i].start;
            if t > 0.05 && t < self.clips[i].duration().unwrap_or(0.) - 0.05 {
                self.timeline.remember(&self.clips);
                let mut right = self.clips[i].clone();
                right.start += t;
                right.preparation.cue_in += t;
                right.fade_in = 0.;
                self.clips[i].preparation.cue_out = Some(right.preparation.cue_in);
                self.clips[i].fade_out = 0.;
                self.clips.push(right);
            } else {
                self.status = "Place the playhead inside the selected clip to split it".into();
            }
        }
        if let Some(i) = remove {
            self.timeline.remember(&self.clips);
            self.clips.remove(i);
            self.timeline.selected = None;
        }
        ui.label(format!(
            "Mix length: {}:{:02} · {} clips",
            extent as u64 / 60,
            extent as u64 % 60,
            self.clips.len()
        ));
    }
    fn preview_timeline(&mut self, root: &Path) {
        self.timeline.stop();
        let cursor = self.timeline.cursor;
        let clips: Vec<_> = self
            .clips
            .iter()
            .filter_map(|c| {
                let end = c.start + c.duration().ok()?;
                if end <= cursor || c.start >= cursor + 30. {
                    return None;
                }
                let mut out = c.clone();
                let trim = (cursor - c.start).max(0.);
                out.preparation.cue_in += trim;
                out.start = (c.start - cursor).max(0.);
                out.preparation.cue_out = Some(
                    c.preparation
                        .cue_out?
                        .min(c.preparation.cue_in + trim + 30. - out.start),
                );
                out.fade_in = if trim > 0. { 0. } else { c.fade_in };
                out.fade_out = if end > cursor + 30. { 0. } else { c.fade_out };
                Some(out)
            })
            .collect();
        let root = root.to_path_buf();
        let (tx, rx) = mpsc::channel();
        self.timeline.preview = Some(rx);
        std::thread::spawn(move || {
            let path = std::env::temp_dir().join(format!(
                "amp-mix-preview-{}.mp3",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let result = render(&root, &clips, &path).map(|_| path);
            if let Err(e) = tx.send(result)
                && let Ok(path) = e.0
            {
                let _ = std::fs::remove_file(path);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(ctx: &egui::Context, a: &mut Arrangement, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000., 800.),
            )),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| a.timeline_ui(ui, Path::new(".")));
        output.textures_delta.clear();
    }
    #[test]
    fn timeline_drag_moves_and_trims_without_changing_source_duration() {
        let ctx = egui::Context::default();
        let mut a = Arrangement::default();
        let path = PathBuf::from("test.mp3");
        a.clips.push(Clip {
            path: path.clone(),
            start: 10.,
            preparation: amp_core::Preparation {
                cue_out: Some(120.),
                ..Default::default()
            },
            ..Default::default()
        });
        a.timeline.waves.insert(path.clone(), vec![[0.5, 0.5]; 100]);
        a.timeline.source_lengths.insert(path, 120.);
        frame(&ctx, &mut a, vec![]);
        let area = a.timeline.area.unwrap();
        let from = egui::pos2(area.left() + 110. + 20. + 60., area.top() + 30. + 45.);
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(
            &ctx,
            &mut a,
            vec![egui::Event::PointerMoved(from), press(from, true)],
        );
        let to = from + egui::vec2(40., 100.);
        frame(&ctx, &mut a, vec![egui::Event::PointerMoved(to)]);
        frame(&ctx, &mut a, vec![press(to, false)]);
        assert!(
            (a.clips[0].start - 30.).abs() < 0.1,
            "start {}",
            a.clips[0].start
        );
        assert_eq!(a.clips[0].lane, 1);
        assert_eq!(a.clips[0].preparation.cue_out, Some(120.));
        let edge = egui::pos2(area.left() + 110. + 60. + 2., area.top() + 30. + 100. + 45.);
        frame(
            &ctx,
            &mut a,
            vec![egui::Event::PointerMoved(edge), press(edge, true)],
        );
        let to = edge + egui::vec2(20., 0.);
        frame(&ctx, &mut a, vec![egui::Event::PointerMoved(to)]);
        frame(&ctx, &mut a, vec![press(to, false)]);
        assert!((a.clips[0].preparation.cue_in - 10.).abs() < 0.1);
        assert!((a.clips[0].start - 40.).abs() < 0.1);
        assert_eq!(a.clips[0].preparation.cue_out, Some(120.));
    }
    #[test]
    #[ignore = "requires supplied MP3 fixture folder and FFmpeg"]
    fn exports_layered_real_mp3s_with_trims_and_fades() {
        let root =
            PathBuf::from(std::env::var_os("AMP_ENERGY_FIXTURE_ROOT").expect("fixture root"));
        let lib = amp_core::scan(&root).unwrap();
        assert!(lib.tracks.len() >= 2);
        let clips: Vec<_> = lib
            .tracks
            .iter()
            .take(2)
            .enumerate()
            .map(|(i, t)| Clip {
                path: amp_core::music_sources::audio_path(&root, &t.path),
                start: i as f64 * 3.,
                lane: i,
                preparation: amp_core::Preparation {
                    cue_in: 2.,
                    cue_out: Some(8.),
                    ..Default::default()
                },
                fade_in: 1.,
                fade_out: 1.,
                gain_db: -6.,
            })
            .collect();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("test.ampmix");
        std::fs::write(&project, serde_json::to_vec(&clips).unwrap()).unwrap();
        let loaded: Vec<Clip> = serde_json::from_slice(&std::fs::read(project).unwrap()).unwrap();
        let output = dir.path().join("layered.mp3");
        render(&root, &loaded, &output).unwrap();
        let duration = crate::lab_editor::duration(&output).unwrap();
        assert!((duration - 9.).abs() < 0.1, "duration {duration}");
        assert!(
            crate::lab_editor::read_waveform(&output, duration)
                .unwrap()
                .iter()
                .any(|p| p[0] > 0.01)
        );
    }
}
