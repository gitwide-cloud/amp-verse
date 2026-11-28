use super::*;
use crate::prepared_stem_cache::{PlaylistSnapshot, Progress};
pub(super) static PREPARING: AtomicBool = AtomicBool::new(false);
struct Choice {
    snapshot: PlaylistSnapshot,
    estimated_bytes: u64,
    unknown: usize,
}
struct Task {
    cancel: Arc<AtomicBool>,
    progress: mpsc::Receiver<Progress>,
    result: mpsc::Receiver<Result<String, String>>,
    name: String,
}
impl Drop for Task {
    fn drop(&mut self) {
        self.cancel.store(true, Relaxed);
    }
}
struct ProcessingLease<'a>(&'a AtomicBool);
impl Drop for ProcessingLease<'_> {
    fn drop(&mut self) {
        self.0.store(false, Relaxed);
    }
}
#[derive(Default)]
pub(super) struct State {
    choice: Option<Choice>,
    task: Option<Task>,
    status: String,
    progress: Option<Progress>,
    stats: Option<(usize, u64)>,
    stats_attempted: bool,
    stats_rx: Option<mpsc::Receiver<Result<(usize, u64), String>>>,
    clear_confirm: bool,
}
fn start_allowed(has_tracks: bool, engine_available: bool, live_enabled: bool) -> bool {
    has_tracks && engine_available && !live_enabled
}
impl State {
    pub(super) fn busy(&self) -> bool {
        self.task.is_some()
    }
}
impl Panel {
    pub(crate) fn clear_playlist_offer(&mut self) {
        self.playlist_prep.choice = None;
    }
    pub(crate) fn offer_playlist(
        &mut self,
        name: String,
        tracks: Vec<PathBuf>,
        estimated_bytes: u64,
        unknown: usize,
    ) {
        if self.playlist_prep.busy() {
            self.playlist_prep.status="A playlist preparation is already running. Cancel it before choosing another playlist.".into();
            return;
        }
        let mut seen = std::collections::HashSet::new();
        let tracks = tracks
            .into_iter()
            .filter(|p| seen.insert(p.clone()))
            .collect();
        self.playlist_prep.choice = Some(Choice {
            snapshot: PlaylistSnapshot { name, tracks },
            estimated_bytes,
            unknown,
        });
    }
    pub(crate) fn shutdown_playlist_preparation(&mut self) {
        if let Some(task) = self.playlist_prep.task.take() {
            task.cancel.store(true, Relaxed);
            if task.result.recv_timeout(Duration::from_secs(3)).is_err() {
                eprintln!(
                    "Playlist preparation cleanup did not acknowledge before application shutdown"
                );
            }
        }
    }
    pub(crate) fn playlist_preparation_ui(
        &mut self,
        ui: &mut egui::Ui,
        live_enabled: bool,
        cached_enabled: bool,
    ) {
        let state = &mut self.playlist_prep;
        if let Some(task) = &state.task {
            while let Ok(progress) = task.progress.try_recv() {
                state.progress = Some(progress);
            }
            if let Ok(result) = task.result.try_recv() {
                state.status = result.unwrap_or_else(|e| e);
                state.task = None;
                state.stats = None;
                state.stats_attempted = false;
                state.stats_rx = None;
            }
        }
        if let Some(rx) = &state.stats_rx
            && let Ok(result) = rx.try_recv()
        {
            match result {
                Ok(stats) => state.stats = Some(stats),
                Err(error) => state.status = error,
            }
            state.stats_rx = None;
        }
        if !state.stats_attempted
            && state.stats_rx.is_none()
            && state.task.is_none()
            && let Some(root) = crate::prepared_stem_playback::root()
        {
            let (tx, rx) = mpsc::channel();
            state.stats_rx = Some(rx);
            state.stats_attempted = true;
            std::thread::spawn(move || {
                let _ = tx.send(crate::prepared_stem_cache::stats(&root));
            });
        }
        ui.separator();
        ui.strong("Playlist preparation");
        ui.weak("Prepared songs are ready for the stem controls when you mix.");
        if let Some(task) = &state.task {
            ui.label(format!("Preparing {}", task.name));
            if let Some(progress) = &state.progress {
                ui.label(format!(
                    "{} / {} songs · {}",
                    progress.completed, progress.total, progress.phase
                ));
            } else {
                ui.spinner();
                ui.label("Starting selected playlist…");
            }
            if ui.button("Cancel preparation").clicked() {
                task.cancel.store(true, Relaxed);
                state.status = "Canceling preparation; completed songs stay cached.".into();
            }
            ui.ctx().request_repaint_after(Duration::from_millis(150));
        } else if let Some(choice) = &state.choice {
            ui.label(format!(
                "Selected playlist: {} · {} unique songs",
                choice.snapshot.name,
                choice.snapshot.tracks.len()
            ));
            ui.label(format!(
                "Estimated space needed: {:.1} MB",
                choice.estimated_bytes as f64 / 1000000.
            ));
            if choice.unknown > 0 {
                ui.weak(format!(
                    "{} song lengths are unknown; the space estimate allows four minutes each.",
                    choice.unknown
                ));
            }
            let mut start = false;
            let mut dismiss = false;
            ui.horizontal_wrapped(|ui| {
                start = ui
                    .add_enabled(
                        start_allowed(
                            !choice.snapshot.tracks.is_empty(),
                            crate::onnx_stem_worker::configured(),
                            live_enabled,
                        ),
                        egui::Button::new("Prepare stems"),
                    )
                    .clicked();
                dismiss = ui.button("Cancel selection").clicked();
            });
            if live_enabled {
                ui.weak("Finish using live stems before preparing another playlist. Your music can keep playing.");
            }
            if !crate::onnx_stem_worker::configured() {
                ui.weak("Set up stems before preparing this playlist. Songs you already prepared can still play.");
            }
            if start
                && let Some(choice) = state.choice.take()
                && let Some(root) = crate::prepared_stem_playback::root()
            {
                let cancel = Arc::new(AtomicBool::new(false));
                let stopped = cancel.clone();
                let (ptx, prx) = mpsc::sync_channel(8);
                let (tx, rx) = mpsc::channel();
                let name = choice.snapshot.name.clone();
                PREPARING.store(true, Relaxed);
                let started=std::thread::Builder::new().name("playlist-stem-preparation".into()).spawn(move || {
                    let _processing=ProcessingLease(&PREPARING);
                    let result=crate::prepared_stem_cache::prepare_selected(&root,choice.snapshot,stopped,|p|{let _=ptx.try_send(p);}).map(|songs|format!("Prepared {} songs locally. Pause a deck and enable prepared stems to use them.",songs.len()));
                    let _ = tx.send(result);
                });
                if let Err(error) = started {
                    PREPARING.store(false, Relaxed);
                    state.status = format!("Could not start playlist preparation: {error}");
                    return;
                }
                state.task = Some(Task {
                    cancel,
                    progress: prx,
                    result: rx,
                    name,
                });
                state.progress = None;
            } else if dismiss {
                state.choice = None;
            }
        } else {
            ui.weak("Choose a playlist, then press Prepare stems.");
        }
        if !state.status.is_empty() {
            ui.label(&state.status);
        }
        ui.collapsing("Prepared music storage", |ui| {
        if let Some((songs, bytes)) = state.stats {
            ui.label(format!(
                "Cache: {songs} songs · {:.1} MiB used · 10 GiB maximum",
                bytes as f64 / 1048576.
            ));
        }
        if state.task.is_none() {
            if ui.button("Clear prepared stem cache…").clicked() {
                state.clear_confirm = true;
            }
            if state.clear_confirm {
                ui.weak("Remove only generated prepared stem audio? Original songs and playlists stay unchanged.");
                if cached_enabled {
                    ui.weak("Switch prepared decks to original audio before clearing their cache.");
                }
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            !cached_enabled,
                            egui::Button::new("Clear local prepared stems"),
                        )
                        .clicked()
                        && let Some(root) = crate::prepared_stem_playback::root()
                    {
                        let cancel = Arc::new(AtomicBool::new(false));
                        let (tx, rx) = mpsc::channel();
                        let (_ptx, prx) = mpsc::channel();
                        std::thread::spawn(move || {
                            let _ = tx.send(
                                crate::prepared_stem_cache::clear(&root)
                                    .map(|()| "Prepared stem cache cleared.".into()),
                            );
                        });
                        state.task = Some(Task {
                            cancel,
                            progress: prx,
                            result: rx,
                            name: "cache cleanup".into(),
                        });
                        state.clear_confirm = false;
                    }
                    if ui.button("Keep cache").clicked() {
                        state.clear_confirm = false;
                    }
                });
            }
        }
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn completed_worker_releases_live_guard_without_any_ui_poll() {
        let busy = std::sync::Arc::new(super::AtomicBool::new(true));
        let worker_busy = busy.clone();
        std::thread::spawn(move || {
            let _lease = super::ProcessingLease(&worker_busy);
        })
        .join()
        .unwrap();
        assert!(!busy.load(super::Relaxed));
    }
    #[test]
    fn optional_preparation_never_starts_over_live_decks() {
        assert!(!super::start_allowed(true, true, true));
        assert!(super::start_allowed(true, true, false));
        assert!(!super::start_allowed(false, true, false));
        assert!(!super::start_allowed(true, false, false));
    }
}
