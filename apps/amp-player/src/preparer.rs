//! Desktop preparation review and background library analysis.
use amp_core::{
    Library, Preparation, Track,
    preparation::{self, Cue},
};
use eframe::egui;
use std::{path::PathBuf, sync::mpsc};
struct Update {
    root: PathBuf,
    path: PathBuf,
    result: Result<Preparation, String>,
    selected: bool,
    skipped: bool,
}
#[derive(Default)]
pub struct Preparer {
    wave: crate::waveform::Lane,
    position: f64,
    zoom: Option<f64>,
    section: Option<String>,
    pub mix_section: Option<(PathBuf, String)>,
    compact_job: Option<mpsc::Receiver<Result<usize, String>>>,
    stem_panel: crate::stems::Panel,
    stem_preview: Option<crate::mixer::Engine>,
    pub preview_blocked: bool,
    pub open: bool,
    pub preview: Option<(PathBuf, f64)>,
    review_selected: std::collections::HashSet<String>,
    editing_revision: Option<Vec<u8>>,
    library_id: String,
    root: PathBuf,
    path: PathBuf,
    selection_root: PathBuf,
    selection_path: PathBuf,
    title: String,
    draft: Preparation,
    job: Option<mpsc::Receiver<Update>>,
    queue: Option<amp_core::analysis_queue::Queue>,
    queue_init: Option<
        mpsc::Receiver<
            Result<
                (
                    amp_core::analysis_queue::Queue,
                    amp_core::analysis_queue::Stats,
                ),
                String,
            >,
        >,
    >,
    queue_discovery: Option<mpsc::Receiver<Result<amp_core::analysis_queue::Stats, String>>>,
    queue_root: PathBuf,
    queue_retry_at: Option<std::time::Instant>,
    pub queue_enabled: bool,
    stop_queue: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub status: String,
    active_workers: usize,
    total: usize,
    done: usize,
    failures: usize,
    skipped: usize,
}
impl Preparer {
    fn dirty(&self) -> bool {
        self.editing_revision
            .as_deref()
            .and_then(|b| serde_json::from_slice::<Preparation>(b).ok())
            .unwrap_or_default()
            != self.draft
    }
    fn selected_track(&self, root: &std::path::Path, track: &Track) -> bool {
        !self.title.is_empty() && self.selection_root == root && self.selection_path == track.path
    }
    fn published_track_changed(&self, track: &Track) -> bool {
        if self.draft.extensions.contains_key("_amp_sync")
            || track.preparation.extensions.contains_key("_amp_sync")
        {
            return !crate::library_cache::keep_current_preparation(
                &self.draft,
                &track.preparation,
            ) && self.draft.extensions.get("_amp_sync")
                != track.preparation.extensions.get("_amp_sync");
        }
        let clean = |p: &Preparation| {
            let mut p = p.clone();
            for field in ["embedded_tags", "embedded_genres", "audio_properties"] {
                p.extensions.remove(field);
            }
            p
        };
        clean(&self.draft) != clean(&track.preparation)
    }
    pub fn prepare_panel(
        &mut self,
        ui: &mut egui::Ui,
        root: &std::path::Path,
        track: Option<&Track>,
        library: Option<&mut Library>,
    ) {
        let Some(track) = track else {
            ui.heading("Prepare music");
            ui.weak("Select a song below to prepare its waveform and energy sections.");
            return;
        };
        if !self.busy()
            && !self.title.is_empty()
            && !self.selected_track(root, track)
            && self.dirty()
        {
            ui.heading(format!("Unsaved preparation: {}", self.title));
            ui.weak("Save or discard this draft before preparing another song. Select the original song below to keep editing.");
            ui.horizontal(|ui| {
                if ui.button("Save current draft").clicked() {
                    let same = self.selection_root == root;
                    self.save_inline(if same { library } else { None });
                }
                if ui.button("Discard draft").clicked() {
                    self.title.clear();
                }
            });
            return;
        }
        if !self.busy() && (!self.selected_track(root, track) || self.title.is_empty()) {
            self.edit(root, track);
            self.open = false;
            self.position = 0.;
            self.section = None;
        }
        if !self.busy()
            && self.selected_track(root, track)
            && !self.dirty()
            && self.published_track_changed(track)
        {
            self.edit(root, track);
            self.open = false;
        }
        if !self.selected_track(root, track) {
            ui.weak(if self.busy() {
                "Finish the current analysis before changing the prepared track."
            } else if self.status.is_empty() {
                "Could not prepare this song. Check that its music source is connected."
            } else {
                &self.status
            });
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.heading(&self.title);
            ui.label(format!(
                "BPM {} · Key {}",
                self.draft.bpm.map_or("—".into(), |b| format!("{b:.1}")),
                self.draft.harmonic_key.as_deref().unwrap_or("—")
            ));
            if ui
                .add_enabled(!self.busy(), egui::Button::new("Analyze track"))
                .clicked()
            {
                self.total = 1;
                self.done = 0;
                self.failures = 0;
                self.start(
                    self.root.clone(),
                    vec![Track {
                        path: self.path.clone(),
                        title: self.title.clone(),
                        preparation: self.draft.clone(),
                    }],
                    true,
                );
            }
            if ui
                .add_enabled(!self.busy(), egui::Button::new("Save preparation"))
                .clicked()
            {
                self.save_inline(library);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Content type:");
            let selected = amp_core::media_kind::kind(&self.path, &self.draft);
            for (label, kind) in [
                ("Music", amp_core::media_kind::Kind::Music),
                ("Audiobook", amp_core::media_kind::Kind::Audiobook),
                ("Speech", amp_core::media_kind::Kind::Speech),
                ("Podcast", amp_core::media_kind::Kind::Podcast),
            ] {
                if ui.selectable_label(selected == kind, label).clicked() {
                    self.draft
                        .extensions
                        .insert("media_kind".into(), serde_json::json!(kind.id()));
                }
            }
            ui.weak("Save preparation to keep this choice. Files stay in place.");
        });
        if amp_core::review::proposal(&self.draft)
            .is_some_and(|p| amp_core::energy_profile::read(&p).is_some())
            && ui
                .button("Use suggested energy profile (keep my other corrections)")
                .clicked()
        {
            match amp_core::review::decide(
                &self.draft,
                &[amp_core::energy_profile::FIELD.into()],
                true,
            ) {
                Ok(p) => self.draft = p,
                Err(e) => self.status = e,
            }
        }
        let profile = amp_core::energy_profile::read(&self.draft);
        let duration = profile
            .as_ref()
            .map(|p| p.duration)
            .or(self.draft.analysis.as_ref().map(|a| a.duration))
            .or(self.draft.cue_out)
            .or_else(|| {
                track
                    .preparation
                    .extensions
                    .get("audio_properties")?
                    .get("duration")?
                    .as_f64()
                    .filter(|v| v.is_finite() && *v > 0.)
            })
            .unwrap_or(32.)
            .max(0.1);
        ui.horizontal_wrapped(|ui| {
            if self.draft.bpm.is_some() && self.draft.beat_offset.is_none() {
                ui.weak("Tempo estimated; align the beatgrid before beat-sensitive mixing.");
            }
            if ui.button("Full track").clicked() {
                self.zoom = None;
            }
            if ui.button("Mark phrase start at cursor").clicked() {
                self.draft
                    .extensions
                    .insert("mix_phrase_anchor".into(), serde_json::json!(self.position));
                self.draft
                    .extensions
                    .insert("mix_bar_anchor".into(), serde_json::json!(self.position));
                self.draft.beat_offset = self.draft.bpm.map(|b| self.position.rem_euclid(60. / b));
                self.draft.manual_override = true;
            }
            if ui
                .button("Mark bar 1 at cursor")
                .on_hover_text("Use this reviewed musical downbeat for next-bar scheduling")
                .clicked()
            {
                self.draft
                    .extensions
                    .insert("mix_bar_anchor".into(), serde_json::json!(self.position));
                self.draft.beat_offset = self.draft.bpm.map(|b| self.position.rem_euclid(60. / b));
                self.draft.manual_override = true;
            }
            if ui.button("Zoom to cursor").clicked() {
                self.zoom = Some(16.);
            }
            if ui
                .add_enabled(
                    self.draft.bpm.is_some(),
                    egui::Button::new("Align beat to cursor"),
                )
                .clicked()
                && let Some(bpm) = self.draft.bpm
            {
                self.draft.beat_offset = Some(self.position.rem_euclid(60. / bpm));
            }
        });
        self.wave.overview = self.zoom.is_none();
        self.wave.height = Some(100.);
        self.wave.show(
            ui,
            root,
            Some(&Track {
                path: self.path.clone(),
                title: self.title.clone(),
                preparation: self.draft.clone(),
            }),
            self.position,
            self.zoom.unwrap_or(duration),
            0,
        );
        if let Some(at) = self.wave.seek.take() {
            self.position = at.min(duration);
        }
        if let Some(mut p) = profile {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 38.), egui::Sense::click());
            for section in &p.sections {
                let r = egui::Rect::from_min_max(
                    egui::pos2(
                        rect.left() + rect.width() * (section.start / duration) as f32,
                        rect.top(),
                    ),
                    egui::pos2(
                        rect.left() + rect.width() * (section.end / duration) as f32,
                        rect.bottom(),
                    ),
                );
                let color = egui::Color32::from_rgb(
                    (70. + 180. * section.energy) as u8,
                    (90. + 70. * section.energy) as u8,
                    45,
                );
                ui.painter().rect_filled(r.shrink(1.), 3., color);
                if r.width() > 25. {
                    ui.painter().text(
                        r.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("{:.0}", 1. + section.energy * 9.),
                        egui::FontId::proportional(13.),
                        egui::Color32::WHITE,
                    );
                }
                if self.section.as_deref() == Some(&section.id) {
                    ui.painter().rect_stroke(
                        r.shrink(1.),
                        3.,
                        egui::Stroke::new(2., crate::GOLD),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.clicked()
                    && response
                        .interact_pointer_pos()
                        .is_some_and(|point| r.contains(point))
                {
                    self.section = Some(section.id.clone());
                    self.position = section.start;
                }
            }
            let mut changed = false;
            if let Some(section) = p
                .sections
                .iter_mut()
                .find(|s| Some(&s.id) == self.section.as_ref())
            {
                ui.horizontal_wrapped(|ui| {
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut section.name).desired_width(140.))
                        .changed();
                    ui.label(format!("{:.1}–{:.1}s", section.start, section.end));
                    let mut energy = 1. + section.energy * 9.;
                    if ui
                        .add(
                            egui::DragValue::new(&mut energy)
                                .range(1. ..=10.)
                                .speed(0.1)
                                .prefix("Energy "),
                        )
                        .changed()
                    {
                        section.energy = (energy - 1.) / 9.;
                        changed = true;
                    }
                    if ui
                        .add_enabled(!self.preview_blocked, egui::Button::new("Preview section"))
                        .clicked()
                    {
                        self.preview = Some((self.path.clone(), section.start));
                    }
                    let dirty = self.dirty() || changed;
                    if ui
                        .add_enabled(!dirty, egui::Button::new("Mix this section into DJ"))
                        .clicked()
                    {
                        self.mix_section = Some((
                            self.selection_path.clone(),
                            format!("energy-section:{}", section.id),
                        ));
                    }
                });
            } else {
                ui.weak("Click a section to select its entry point. Numbers show local energy from 1–10.");
            }
            let split = p
                .sections
                .iter()
                .position(|s| self.position > s.start + 0.25 && self.position < s.end - 0.25);
            if ui
                .add_enabled(
                    split.is_some(),
                    egui::Button::new("Split section at cursor"),
                )
                .clicked()
                && let Some(index) = split
            {
                let mut next = p.sections[index].clone();
                next.id = format!("section-{}", amp_core::coordination::token());
                next.name = "New section".into();
                next.start = self.position;
                p.sections[index].end = self.position;
                self.section = Some(next.id.clone());
                p.sections.insert(index + 1, next);
                changed = true;
            }
            if changed {
                self.draft.extensions.insert(
                    amp_core::energy_profile::FIELD.into(),
                    serde_json::to_value(p).unwrap(),
                );
            }
        } else {
            ui.weak("Analyze this song to build its energy curve and editable sections. Existing cues remain protected.");
        }
        ui.horizontal_wrapped(|ui| {
            let mut allowed = amp_core::energy_profile::mixing_allowed(&self.draft);
            if ui
                .checkbox(&mut allowed, "Auto DJ: use energy sections")
                .changed()
            {
                self.draft
                    .extensions
                    .insert("energy_section_mixing".into(), serde_json::json!(allowed));
            }
            let mut bpm = self.draft.bpm.unwrap_or(0.);
            if let Some(values) = self
                .draft
                .extensions
                .get("tempo_alternatives")
                .and_then(|v| v.as_array())
                .cloned()
            {
                ui.label("BPM pulse:");
                for value in values.iter().filter_map(|v| v.as_f64()) {
                    if ui.small_button(format!("{value:.1}")).clicked() {
                        self.draft.bpm = Some(value);
                        bpm = value;
                    }
                }
            }
            if ui
                .add(
                    egui::DragValue::new(&mut bpm)
                        .range(0. ..=300.)
                        .speed(0.1)
                        .prefix("BPM "),
                )
                .changed()
            {
                self.draft.bpm = (bpm > 0.).then_some(bpm);
            }
            let mut offset = self.draft.beat_offset.unwrap_or(0.);
            if ui
                .add(
                    egui::DragValue::new(&mut offset)
                        .range(0. ..=duration)
                        .speed(0.01)
                        .prefix("Beat offset ")
                        .suffix(" s"),
                )
                .changed()
            {
                self.draft.beat_offset = Some(offset);
            }
            if ui.button("More cue / stem edits…").clicked() {
                self.open = true;
            }
            ui.label(&self.status);
        });
    }
    fn save_inline(&mut self, library: Option<&mut Library>) {
        let mut proposed = self.draft.clone();
        let allowed = amp_core::energy_profile::mixing_allowed(&proposed);
        proposed
            .extensions
            .insert("energy_section_mixing".into(), serde_json::json!(allowed));
        proposed.manual_override = true;
        proposed.schema_version = Some(1);
        if let Some(a) = &mut proposed.analysis {
            a.reviewed = true;
        }
        match preparation::save_if_unchanged(
            &self.root,
            &self.path,
            &proposed,
            self.editing_revision.as_deref(),
        ) {
            Ok(()) => match preparation::load_with_revision(&self.root.join(&self.path)) {
                Ok((p, revision)) => {
                    self.draft = p;
                    self.editing_revision = revision;
                    if let Some(t) = library
                        .and_then(|l| l.tracks.iter_mut().find(|t| t.path == self.selection_path))
                    {
                        t.preparation = self.draft.clone();
                    }
                    self.status =
                        "Preparation saved; playing decks keep their loaded snapshot.".into();
                }
                Err(e) => self.status = e,
            },
            Err(e) => {
                self.status =
                    format!("{e}. Draft retained here; reload/review before replacing shared data.")
            }
        }
    }
    pub fn guard_preview(&mut self, blocked: bool) {
        self.preview_blocked = blocked;
        if blocked {
            self.stem_preview = None;
        }
    }
    pub fn failed(&self) -> usize {
        self.failures
    }
    pub fn progress(&self) -> (usize, usize) {
        (self.done, self.total)
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
            || self.compact_job.is_some()
            || self.queue_init.is_some()
            || self.queue_discovery.is_some()
    }
    pub fn catalog_ui(&mut self, ui: &mut egui::Ui, root: &std::path::Path, blocked: bool) {
        if let Some(rx) = &self.compact_job
            && let Ok(result) = rx.try_recv()
        {
            self.status=result.map_or_else(|e|e,|n|format!("Consolidated {n} records. Catalog database ready; recovery values retained inside the database."));
            self.compact_job = None;
        }
        ui.heading("Portable catalog");
        ui.weak("A shared index points to 500-song catalog files. More files are created as needed, with no fixed library limit. No database server needed.");
        if amp_core::catalog_db::path(root).is_file() {
            ui.label("Index: ampdj/catalog.ampdb. Starting the analysis queue organizes song catalogs in ampdj/catalogs and keeps a recovery copy.");
        } else if ui
            .add_enabled(
                !blocked && !self.busy(),
                egui::Button::new("Create portable catalog storage"),
            )
            .clicked()
        {
            let root = root.to_path_buf();
            let (tx, rx) = mpsc::channel();
            self.compact_job = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(amp_core::catalog_db::compact(&root));
            });
        }
        if self.compact_job.is_some() {
            ui.spinner();
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if blocked {
            ui.weak("Stop playback and stem preparation before converting.");
        }
        ui.label(&self.status);
    }
    pub fn edit(&mut self, root: &std::path::Path, track: &Track) {
        self.stem_preview = None;
        if self.busy() {
            return;
        }
        let (owner, relative) = match amp_core::music_sources::owner(root, &track.path) {
            Ok(value) => value,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        self.root = owner;
        self.path = relative;
        self.title = track.title.clone();
        match preparation::load_with_revision(&amp_core::music_sources::audio_path(
            root,
            &track.path,
        )) {
            Ok((p, revision)) => {
                self.draft = p;
                self.editing_revision = revision;
            }
            Err(e) => {
                self.status = e;
                return;
            }
        }
        if let Some(title) = self.draft.extensions.get("title").and_then(|v| v.as_str()) {
            self.title = title.into();
        }
        self.library_id = amp_core::coordination::library_id(&self.root).unwrap_or_default();
        self.selection_root = root.into();
        self.selection_path = track.path.clone();
        self.review_selected.clear();
        self.open = true;
    }
    pub fn batch_ref(&mut self, root: PathBuf, tracks: &[Track]) {
        self.queue_inputs(root, tracks, true);
    }
    pub fn resume_cached(&mut self, root: PathBuf, tracks: &[Track]) {
        self.queue_inputs(root, tracks, false);
    }
    fn queue_inputs(&mut self, root: PathBuf, tracks: &[Track], verify: bool) {
        if self.queue_init.is_some() || self.queue_discovery.is_some() {
            return;
        }
        let queue = if self.queue_root == root {
            self.queue.clone()
        } else {
            None
        };
        self.queue_retry_at = None;
        self.queue_root = root.clone();
        self.queue_enabled = true;
        if self.total == 0 {
            self.total = tracks.len();
        }
        let inputs: Vec<_> = tracks
            .iter()
            .filter(|t| amp_core::media_kind::is_music(&t.path, &t.preparation))
            .map(|t| {
                let mut preparation = Preparation {
                    analysis: t.preparation.analysis.clone(),
                    source_sha256: t.preparation.source_sha256.clone(),
                    ..Default::default()
                };
                if let Some(kind) = t.preparation.extensions.get("media_kind") {
                    preparation
                        .extensions
                        .insert("media_kind".into(), kind.clone());
                }
                if let Some(stat) = t.preparation.extensions.get("scan_source_stat") {
                    preparation
                        .extensions
                        .insert("scan_source_stat".into(), stat.clone());
                }
                if let Some(stat) = t.preparation.extensions.get("analysis_source_stat") {
                    preparation
                        .extensions
                        .insert("analysis_source_stat".into(), stat.clone());
                }
                Track {
                    path: t.path.clone(),
                    title: String::new(),
                    preparation,
                }
            })
            .collect();
        let (tx, rx) = mpsc::sync_channel(1);
        self.queue_init = Some(rx);
        let (discovered, discovery) = mpsc::sync_channel(1);
        self.queue_discovery = Some(discovery);
        self.status = "Opening the saved queue; folder discovery runs separately.".into();
        std::thread::spawn(move || {
            let opened = (|| {
                let q = match queue {
                    Some(q) => q,
                    None => amp_core::analysis_queue::Queue::for_library(&root, "desktop")?,
                };
                q.seed_known(&inputs)?;
                let stats = q.stats()?;
                Ok::<_, String>((q, stats))
            })();
            match opened {
                Ok((q, stats)) => {
                    if tx.send(Ok((q.clone(), stats))).is_err() {
                        return;
                    }
                    let result = if verify {
                        q.enqueue_tracks(&inputs).and_then(|_| q.stats())
                    } else {
                        q.stats()
                    };
                    let _ = discovered.send(result);
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            }
        });
    }
    pub fn retry_failed(&mut self) {
        if self.queue_init.is_some() {
            return;
        }
        let Some(q) = self.queue.clone() else { return };
        let (tx, rx) = mpsc::sync_channel(1);
        self.queue_init = Some(rx);
        self.queue_enabled = true;
        std::thread::spawn(move || {
            let result = q
                .retry_failed()
                .and_then(|_| q.stats())
                .map(|stats| (q, stats));
            let _ = tx.send(result);
        });
    }
    pub fn stop_queue(&mut self) {
        self.queue_retry_at = None;
        self.queue_enabled = false;
        self.stop_queue
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.status = "Stopping after active songs; pending work is saved.".into();
    }
    fn start_queue(&mut self, root: PathBuf) {
        self.queue_retry_at = None;
        let Some(q) = self.queue.clone() else { return };
        if crate::engine::path().is_none() {
            self.status =
                "Install the optional analysis engine to continue the saved queue.".into();
            return;
        }
        let (tx, rx) = mpsc::sync_channel(8);
        self.job = Some(rx);
        self.stop_queue = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = self.stop_queue.clone();
        let workers = crate::preparation_speed::Mode::Fast
            .workers(amp_core::batch::cpu_count())
            .max(1);
        self.status = "Processing saved analysis queue; new songs append at the end.".into();
        std::thread::spawn(move || {
            std::thread::scope(|scope| {
                for _ in 0..workers {
                    let q = &q;
                    let stop = &stop;
                    let tx = &tx;
                    let root = &root;
                    scope.spawn(move || {
                        loop {
                            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                                break;
                            }
                            let job = match q.next() {
                                Ok(Some(job)) => job,
                                Ok(None) => break,
                                Err(e) => {
                                    let _ = tx.send(Update {
                                        root: root.clone(),
                                        path: PathBuf::new(),
                                        result: Err(e),
                                        selected: false,
                                        skipped: false,
                                    });
                                    for _ in 0..20 {
                                        if stop.load(std::sync::atomic::Ordering::Relaxed) {
                                            break;
                                        }
                                        std::thread::sleep(std::time::Duration::from_millis(100));
                                    }
                                    continue;
                                }
                            };
                            let _heartbeat = q.keep_alive(&job);
                            let _permit = crate::preparation_speed::analysis_permit();
                            let mut skipped = false;
                            let result = match crate::engine::analyze(root, &job.path, None) {
                                Ok(Some(p)) => Ok(p),
                                Ok(None) => {
                                    skipped = true;
                                    preparation::load(&amp_core::music_sources::audio_path(
                                        root, &job.path,
                                    ))
                                }
                                Err(e) => Err(e),
                            };
                            let waiting = skipped
                                && result.as_ref().is_ok_and(|p| {
                                    !amp_core::analysis_queue::Queue::cached_complete(
                                        root,
                                        &Track {
                                            path: job.path.clone(),
                                            title: String::new(),
                                            preparation: p.clone(),
                                        },
                                    )
                                });
                            let recorded = if waiting {
                                q.defer(&job)
                            } else {
                                q.finish(&job, result.as_ref().map(|_| ()).map_err(String::as_str))
                            };
                            let queue_failed = recorded.is_err();
                            let result = match recorded {
                                Ok(()) => result,
                                Err(e) => Err(e),
                            };
                            if tx
                                .send(Update {
                                    root: root.clone(),
                                    path: if queue_failed {
                                        PathBuf::new()
                                    } else {
                                        job.path
                                    },
                                    result,
                                    selected: false,
                                    skipped,
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                    });
                }
            })
        });
    }
    fn start(&mut self, root: PathBuf, tracks: Vec<Track>, selected: bool) {
        if crate::engine::path().is_none() {
            self.status = "Analysis add-on is not installed. Open Plugin Manager; playback and manual edits remain available.".into();
            return;
        }

        let (tx, rx) = mpsc::sync_channel(8);
        self.job = Some(rx);
        self.active_workers = amp_core::batch::workers(
            crate::preparation_speed::mode().workers(amp_core::batch::cpu_count()),
            tracks.len(),
        );
        let workers = crate::preparation_speed::Mode::Fast
            .workers(amp_core::batch::cpu_count())
            .min(tracks.len());
        self.status = format!(
            "Preparing {} tracks with up to {} parallel workers",
            tracks.len(),
            self.active_workers
        );
        std::thread::spawn(move || {
            amp_core::batch::for_each(&tracks, workers, |track| {
                let _permit = crate::preparation_speed::analysis_permit();
                let mut skipped = false;
                let result = match crate::engine::analyze(
                    &root,
                    &track.path,
                    selected.then_some(&track.preparation),
                ) {
                    Ok(Some(p)) => Ok(p),
                    Ok(None) => {
                        skipped = true;
                        preparation::load(&amp_core::music_sources::audio_path(&root, &track.path))
                    }
                    Err(e) => Err(e),
                };
                tx.send(Update {
                    root: root.clone(),
                    path: track.path.clone(),
                    result,
                    selected,
                    skipped,
                })
                .is_ok()
            });
        });
    }
    pub fn poll(&mut self, root: &std::path::Path, library: Option<&mut Library>) {
        let initialized = self.queue_init.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(_) => Some(Err(
                "Queue initialization interrupted; completed metadata was kept".into(),
            )),
        });
        if let Some(result) = initialized {
            self.queue_init = None;
            match result {
                Ok((q, stats)) => {
                    self.queue = Some(q);
                    self.total = stats.total;
                    self.done = stats.done + stats.failed;
                    self.failures = stats.failed;
                    if self.queue_enabled && self.job.is_none() && stats.pending > 0 {
                        self.start_queue(root.into());
                    }
                }
                Err(e) => {
                    self.status = e;
                    self.queue_discovery = None;
                }
            }
        }
        let discovered = self
            .queue_discovery
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(value) => Some(value),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(_) => Some(Err(
                    "Folder discovery interrupted; saved queue was retained".into(),
                )),
            });
        if let Some(result) = discovered {
            self.queue_discovery = None;
            match result {
                Ok(stats) => {
                    self.total = stats.total;
                    if self.job.is_none() {
                        self.done = stats.done + stats.failed;
                        self.failures = stats.failed;
                        if self.queue_enabled && stats.pending > 0 {
                            self.start_queue(root.into());
                        }
                    }
                }
                Err(e) => self.status = e,
            }
        }
        self.active_workers = crate::preparation_speed::mode()
            .workers(amp_core::batch::cpu_count())
            .min(self.total);
        let mut updates = Vec::new();
        let mut finished = false;
        if let Some(rx) = &self.job {
            for _ in 0..8 {
                match rx.try_recv() {
                    Ok(update) => updates.push(update),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
        }
        let mut library = library;
        for update in updates {
            if update.path.as_os_str().is_empty() {
                if let Err(error) = update.result {
                    self.status = format!(
                        "Shared queue unavailable: {error}. Waiting and retrying; completed work is safe."
                    );
                }
                continue;
            }
            if update
                .result
                .as_ref()
                .is_err_and(|e| amp_core::analysis_queue::Queue::temporary_failure(e))
            {
                self.status = format!(
                    "Shared metadata busy for {}. Song remains queued for retry.",
                    update.path.display()
                );
                continue;
            }
            self.done += 1;
            self.skipped += usize::from(update.skipped);
            match update.result {
                Ok(p) => {
                    if update.selected {
                        self.draft = p;
                        self.status =
                            "Analysis ready for review. Save to publish metadata beside the song."
                                .into();
                    } else {
                        if update.root == root
                            && let Some(lib) = library.as_mut()
                            && let Some(t) = lib.tracks.iter_mut().find(|t| t.path == update.path)
                        {
                            t.preparation = p;
                        }
                        self.status = format!(
                            "{} {}/{} ({} workers): {}",
                            if update.skipped {
                                "Reused / claimed elsewhere"
                            } else {
                                "Prepared"
                            },
                            self.done,
                            self.total,
                            self.active_workers,
                            update.path.display()
                        );
                    }
                }
                Err(e) => {
                    self.failures += 1;
                    self.status = format!("{}: {e}", update.path.display());
                }
            }
        }
        if finished {
            if self.total > 1 {
                self.status = format!(
                    "Finished: {} prepared, {} reused/claimed, {} failed ({} tracks)",
                    self.done.saturating_sub(self.failures + self.skipped),
                    self.skipped,
                    self.failures,
                    self.total
                );
            }
            self.job = None;
            if let Some(q) = &self.queue
                && let Ok(stats) = q.stats()
            {
                self.total = stats.total;
                self.done = stats.done + stats.failed;
                self.failures = stats.failed;
                if self.queue_enabled && stats.pending > 0 {
                    self.queue_retry_at =
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
                } else if self.queue_enabled {
                    self.status = format!(
                        "Queue complete: {} done, {} failed. Watching folders for new songs.",
                        stats.done, stats.failed
                    );
                }
            }
        }
        if self.job.is_none()
            && self.queue_enabled
            && self
                .queue_retry_at
                .is_some_and(|at| std::time::Instant::now() >= at)
        {
            self.queue_retry_at = None;
            self.start_queue(root.into());
        }
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        root: &std::path::Path,
        library: Option<&mut Library>,
    ) {
        if !self.open {
            self.stem_preview = None;
            return;
        }
        let mut open = true;
        let mut library = library;
        egui::Window::new("AMP Preparation / key, tempo and cues").open(&mut open).default_width(700.).default_height(650.).vscroll(true).show(ctx,|ui|{
        ui.strong(&self.title);ui.label("Independent heuristic analyzer: review estimates before using them in a DJ set.");
        ui.add_enabled_ui(!self.busy(),|ui|{ui.horizontal(|ui|{
            if ui.button("Analyze track").clicked(){self.total=1;self.done=0;self.failures=0;let track=Track{path:self.path.clone(),title:self.title.clone(),preparation:self.draft.clone()};self.status="Analyzing decoded audio...".into();self.start(self.root.clone(),vec![track],true);}
            if ui.button("Save approved metadata").clicked(){
self.draft.schema_version=Some(1);self.draft.manual_override=true;if let Some(a)=&mut self.draft.analysis{a.reviewed=true;}match preparation::save_if_unchanged(&self.root,&self.path,&self.draft,self.editing_revision.as_deref()){Ok(())=>{if let Ok(fresh)=preparation::load(&self.root.join(&self.path)){self.draft=fresh;}
if let Some(lib)=library.as_mut()&&let Some(t)=lib.tracks.iter_mut().find(|t|self.selection_root==root && t.path==self.selection_path){t.preparation=self.draft.clone();if let Some(title)=self.draft.extensions.get("title").and_then(|v|v.as_str()){t.title=title.into();}}self.editing_revision=preparation::revision(&self.root.join(&self.path));self.status="Approved metadata saved in ampdj. Original audio unchanged.".into();},Err(e)=>{
let queued=(||{let base=if let Some(bytes)=&self.editing_revision{serde_json::from_slice(bytes).map_err(|e:serde_json::Error|e.to_string())?}else{Preparation::default()};amp_core::offline::Edit::new(self.library_id.clone(),self.path.clone(),base,self.draft.clone())?.queue()})();
self.status=match queued{Ok(path)=>format!("{e}. Draft kept locally at {}. Reconnection will not publish it automatically.",path.display()),Err(_)=>format!("{e}. Unsaved draft remains open; local queue unavailable.")};}}}
        });});
        let changes=amp_core::review::changes(&self.draft);
        if !changes.is_empty() {
            ui.separator();ui.strong("Analysis recommendations — review before applying");
            egui::ScrollArea::vertical().id_salt("review-changes").max_height(150.).show(ui,|ui|{
                for (field,before,after) in &changes {
                    let mut selected=self.review_selected.contains(field);
                    ui.horizontal(|ui|{if ui.checkbox(&mut selected,field).changed(){if selected{self.review_selected.insert(field.clone());}else{self.review_selected.remove(field);}}
                    ui.label(format!("{} → {}",before.to_string().chars().take(60).collect::<String>(),after.to_string().chars().take(60).collect::<String>()));});
                    egui::CollapsingHeader::new(format!("Exact {field} values")).show(ui,|ui|{ui.label("Existing");ui.code(serde_json::to_string_pretty(before).unwrap_or_default());ui.label("Suggested");ui.code(serde_json::to_string_pretty(after).unwrap_or_default());});
                }
            });
            ui.horizontal_wrapped(|ui|{
                for (label,accept,all) in [("Accept selected",true,false),("Accept all",true,true),("Reject selected",false,false),("Keep existing / dismiss all",false,true)] {
                    if ui.button(label).clicked(){let fields=if all {changes.iter().map(|c|c.0.clone()).collect()}else{self.review_selected.iter().cloned().collect::<Vec<_>>()};match amp_core::review::decide(&self.draft,&fields,accept){Ok(p)=>{self.draft=p;self.review_selected.clear();self.status="Review decision is staged. Save approved metadata to publish it.".into();},Err(e)=>self.status=e}}
                }
                if ui.button("Preview suggested IN").clicked()&&let Some(p)=amp_core::review::proposal(&self.draft){self.preview=Some((self.path.clone(),p.cue_in));}
            });
        }
        ui.add_enabled_ui(!self.busy(),|ui|{
        egui::CollapsingHeader::new("Library tags (portable metadata)").show(ui,|ui| {
            for field in ["title","artist","album","genre","comment"] {
                ui.horizontal(|ui| {
                    ui.label(field);
                    let mut value=self.draft.extensions.get(field).and_then(|v|v.as_str()).unwrap_or(if field=="title" {&self.title} else {""}).to_owned();
                    if ui.text_edit_singleline(&mut value).changed() {
                        if value.trim().is_empty() {self.draft.extensions.remove(field);} else {self.draft.extensions.insert(field.into(),serde_json::json!(value));}
                    }
                });
            }
        });
        self.stem_editor(ui);
        ui.weak("Saved and manually corrected values are protected. Use review decisions to approve replacements.");
        ui.horizontal(|ui|{ui.label("Cue IN (s)");if ui.add(egui::DragValue::new(&mut self.draft.cue_in).speed(0.01).range(0. ..=21600.)).changed(){self.draft.manual_override=true;}ui.label("Cue OUT (s)");let mut end=self.draft.cue_out.unwrap_or(0.);if ui.add(egui::DragValue::new(&mut end).speed(0.01).range(0. ..=21600.)).changed(){self.draft.cue_out=if end>0.{Some(end)}else{None};self.draft.manual_override=true;}});
        ui.horizontal(|ui|{ui.label("BPM");let mut bpm=self.draft.bpm.unwrap_or(0.);if ui.add(egui::DragValue::new(&mut bpm).speed(0.1).range(0. ..=300.)).changed(){self.draft.bpm=if bpm>0.{Some(bpm)}else{None};self.draft.manual_override=true;}ui.label("Grid offset (s)");let mut offset=self.draft.beat_offset.unwrap_or(0.);if ui.add(egui::DragValue::new(&mut offset).speed(0.01).range(0. ..=21600.)).changed(){self.draft.beat_offset=Some(offset);self.draft.manual_override=true;}});
        ui.horizontal(|ui|{ui.label("Key");let mut key=self.draft.key.clone().unwrap_or_default();if ui.add(egui::TextEdit::singleline(&mut key).desired_width(140.)).changed(){self.draft.key=if key.is_empty(){None}else{Some(key)};self.draft.manual_override=true;}ui.label("Harmonic code");let mut code=self.draft.harmonic_key.clone().unwrap_or_default();if ui.add(egui::TextEdit::singleline(&mut code).desired_width(70.)).changed(){self.draft.harmonic_key=if code.is_empty(){None}else{Some(code)};self.draft.manual_override=true;}});
        ui.horizontal(|ui| {
            ui.label("Rating");
            let mut rating = self.draft.extensions.get("rating").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            if ui.add(egui::Slider::new(&mut rating, 0..=5)).changed() { self.draft.extensions.insert("rating".into(), serde_json::json!(rating)); }
            let mut known = amp_core::smart::energy(&self.draft).is_some();
            if ui.checkbox(&mut known, "Energy known").changed() {
                if known { self.draft.extensions.insert("energy".into(), serde_json::json!(0.5)); }
                else { self.draft.extensions.remove("energy"); }
            }
            if known {
                let mut energy = amp_core::smart::energy(&self.draft).unwrap_or(0.5);
                if ui.add(egui::Slider::new(&mut energy, 0. ..=1.).text("Energy")).changed() { self.draft.extensions.insert("energy".into(), serde_json::json!(energy)); }
            }
        });
        ui.horizontal(|ui|{
            let mut rate=self.draft.extensions.get("playback_rate").and_then(|v|v.as_f64()).unwrap_or(1.);
            if ui.add(egui::Slider::new(&mut rate,0.84..=1.16).text("Playback rate")).changed() {self.draft.extensions.insert("playback_rate".into(),serde_json::json!(rate));}
            ui.weak("Applied when loaded; changing speed also changes audible pitch (no key lock).");
        });
        ui.weak("Key edits correct saved metadata; they do not transpose the audio. Energy is a relative level estimate or your manual value.");
        if let Some(a)=&self.draft.analysis{ui.label(format!("{:.2}s / tempo score {:.2} / key margin {:.2} / {}",a.duration,a.tempo_confidence,a.key_confidence,a.engine));}
        ui.separator();ui.strong("Manual cues (preserved across re-analysis)");
        let mut remove=None;egui::ScrollArea::vertical().id_salt("manual-cues").max_height(160.).show(ui,|ui|{for(i,cue)in self.draft.manual_cues.iter_mut().enumerate(){ui.push_id(i,|ui|{ui.horizontal(|ui|{let mut number=cue.number.unwrap_or((i+1) as u16);if ui.add(egui::DragValue::new(&mut number).range(1..=128).prefix("Cue ")).changed(){cue.number=Some(number);}
ui.add(egui::TextEdit::singleline(&mut cue.label).desired_width(220.));ui.add(egui::DragValue::new(&mut cue.seconds).speed(0.01).range(0. ..=21600.).suffix(" s"));if ui.button("Remove").clicked(){remove=Some(i);}});});}});if let Some(i)=remove{self.draft.manual_cues.remove(i);}
        if ui.button("Add manual cue").clicked(){self.draft.manual_cues.push(Cue{id:format!("manual-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()),label:"Cue".into(),seconds:self.draft.cue_in,kind:"hotcue".into(),..Default::default()});}
        amp_core::preparation::assign_numbers(&mut self.draft);
        egui::CollapsingHeader::new(format!("Generated cue candidates ({})",self.draft.cues.len())).show(ui,|ui|{egui::ScrollArea::vertical().max_height(120.).show(ui,|ui|{for cue in &self.draft.cues{ui.label(format!("{:.2}s   {}",cue.seconds,cue.label));}});});
        });
        ui.label(&self.status);ui.weak(format!("Metadata: {}",amp_core::preparation::portable(&self.root.join(&self.path)).display()));
    });
        self.open = open;
        if !open {
            self.stem_preview = None;
        }
    }
    fn stem_editor(&mut self, ui: &mut egui::Ui) {
        if self.preview_blocked {
            self.stem_preview = None;
        }
        ui.separator();
        ui.strong("Default stem mix for this song");
        ui.weak("Left removes a part; center keeps it; right isolates it. Save approved metadata to use this mix whenever DJ loads this song.");
        let source = self.root.join(&self.path);
        let prepared = self.stem_panel.prepare_song(ui, &source);
        let mut knobs = amp_core::stem_mix::knobs(&self.draft).unwrap_or([0.; 3]);
        let previous = knobs;
        ui.horizontal_wrapped(|ui| {
            for (index, name) in ["Vocals", "Instruments", "Beat"].iter().enumerate() {
                ui.vertical(|ui| {
                    crate::dj_mixer::dial(ui, name, &mut knobs[index], -1. ..=1., crate::ORANGE);
                    ui.label(*name);
                });
            }
            if ui.button("Original mix").clicked() {
                knobs = [0.; 3];
            }
        });
        if knobs != previous {
            self.draft
                .extensions
                .insert("default_stem_mix".into(), serde_json::json!(knobs));
            self.draft.manual_override = true;
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    prepared && !self.preview_blocked,
                    egui::Button::new("Preview stem mix"),
                )
                .clicked()
            {
                let result = (|| {
                    let mut audio = crate::mixer::Engine::new()?;
                    audio.load(
                        0,
                        &self.root,
                        &Track {
                            path: self.path.clone(),
                            title: self.title.clone(),
                            preparation: self.draft.clone(),
                        },
                        true,
                    )?;
                    audio.set_fader(0.);
                    self.stem_preview = Some(audio);
                    Ok::<(), String>(())
                })();
                if let Err(e) = result {
                    self.status = e;
                }
            }
            if ui.button("Stop preview").clicked() {
                self.stem_preview = None;
            }
        });
        if self.preview_blocked {
            ui.weak("Stop deck playback before previewing in Library.");
        }
        if let Some(audio) = &self.stem_preview {
            for (i, gain) in amp_core::stem_mix::gains(knobs).iter().enumerate() {
                audio.controls.stem_level[0][i]
                    .store(gain.to_bits(), std::sync::atomic::Ordering::Relaxed);
            }
        }
        ui.weak("Saved defaults apply on the next load; playing decks keep their current mix. Other computers need prepared stems too.");
    }
}

#[cfg(test)]
mod workflow_tests {
    use super::*;
    #[test]
    fn stop_analysis_preserves_completed_work_selection_and_saved_folder_options() {
        let dir = tempfile::tempdir().unwrap();
        let index = dir.path().join("index.ampdb");
        std::fs::write(dir.path().join("done.wav"), b"completed").unwrap();
        std::fs::write(dir.path().join("pending.wav"), b"pending").unwrap();
        let queue = amp_core::analysis_queue::Queue::open(&index, dir.path()).unwrap();
        queue
            .enqueue([
                (PathBuf::from("done.wav"), true),
                (PathBuf::from("pending.wav"), false),
            ])
            .unwrap();
        let stats = queue.stats().unwrap();
        assert_eq!((stats.done, stats.pending), (1, 1));
        let saved = serde_json::json!({"main_path":"music","folders":["extra"],"completed":7,"cues":[12.5]});
        amp_core::catalog_db::save_local_record(&index, "saved-library", &saved).unwrap();
        let mut preparer = Preparer {
            queue_enabled: true,
            queue: Some(queue.clone()),
            queue_root: dir.path().into(),
            selection_root: dir.path().into(),
            selection_path: "song.mp3".into(),
            done: 7,
            total: 8,
            ..Default::default()
        };
        preparer.draft.bpm = Some(128.);
        let before = preparer.draft.clone();
        preparer.stop_queue();
        assert!(!preparer.queue_enabled);
        assert!(
            preparer
                .stop_queue
                .load(std::sync::atomic::Ordering::Relaxed)
        );
        assert_eq!(preparer.queue_root, dir.path());
        assert_eq!(preparer.selection_path, PathBuf::from("song.mp3"));
        assert_eq!(preparer.done, 7);
        assert_eq!(preparer.total, 8);
        assert_eq!(
            serde_json::to_value(&preparer.draft).unwrap(),
            serde_json::to_value(before).unwrap()
        );
        assert_eq!(
            amp_core::catalog_db::local_record(&index, "saved-library").unwrap(),
            Some(saved)
        );
        drop(preparer);
        drop(queue);
        let reopened = amp_core::analysis_queue::Queue::open(&index, dir.path()).unwrap();
        let stats = reopened.stats().unwrap();
        assert_eq!((stats.total, stats.done, stats.pending), (2, 1, 1));
    }
    #[test]
    fn preparation_uses_display_selection_and_saves_to_added_source_without_false_busy_or_reload() {
        let main = tempfile::tempdir().unwrap();
        let extra = tempfile::tempdir().unwrap();
        std::fs::write(extra.path().join("song.mp3"), b"fixture").unwrap();
        amp_core::music_sources::add(main.path(), extra.path()).unwrap();
        let mut library = amp_core::scan(main.path()).unwrap();
        library.tracks[0].preparation.extensions.insert(
            "embedded_tags".into(),
            serde_json::json!({"Artist":["Display tags"]}),
        );
        let track = library.tracks[0].clone();
        let mut editor = Preparer::default();
        editor.edit(main.path(), &track);
        assert!(editor.selected_track(main.path(), &track));
        assert!(!editor.busy());
        assert!(
            !editor.published_track_changed(&track),
            "derived tags must not reopen metadata every frame"
        );
        editor.draft.bpm = Some(127.);
        editor.save_inline(Some(&mut library));
        assert_eq!(library.tracks[0].preparation.bpm, Some(127.));
        assert_eq!(
            preparation::load(&extra.path().join("song.mp3"))
                .unwrap()
                .bpm,
            Some(127.)
        );
        assert!(editor.selected_track(main.path(), &track));
    }
}
#[cfg(test)]
mod queue_progress_tests {
    use super::*;
    fn check_waiting(path: PathBuf, error: &str) {
        let mut preparer = Preparer {
            total: 10,
            done: 3,
            failures: 1,
            ..Default::default()
        };
        let (tx, rx) = mpsc::channel();
        preparer.job = Some(rx);
        tx.send(Update {
            root: PathBuf::from("unused"),
            path,
            result: Err(error.into()),
            selected: false,
            skipped: false,
        })
        .unwrap();
        preparer.poll(std::path::Path::new("unused"), None);
        assert_eq!(preparer.done, 3);
        assert_eq!(preparer.failures, 1);
        assert!(!preparer.status.starts_with(':'));
        drop(tx);
    }
    #[test]
    fn queue_access_errors_are_not_failed_or_completed_songs() {
        check_waiting(PathBuf::new(), "Access is denied. (os error 5)");
    }
    #[test]
    fn metadata_contention_does_not_advance_song_progress() {
        check_waiting(
            PathBuf::from("song.mp3"),
            "Shared record busy; retry after reloading",
        );
    }
}
