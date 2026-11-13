//! One resource budget for all preparation; active playback always takes priority.
use eframe::egui;
use std::{
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    },
    time::Duration,
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    Quiet,
    #[default]
    Balanced,
    Fast,
}
impl Mode {
    pub fn id(self) -> usize {
        match self {
            Self::Quiet => 0,
            Self::Balanced => 1,
            Self::Fast => 2,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Quiet => "Quiet",
            Self::Balanced => "Balanced",
            Self::Fast => "Fast",
        }
    }
    pub fn workers(self, cores: usize) -> usize {
        match self {
            Self::Quiet => 1,
            Self::Balanced => (cores / 4).clamp(1, 4),
            Self::Fast => (cores / 2).clamp(1, 12),
        }
    }
    pub fn stem_threads(self, cores: usize) -> usize {
        match self {
            Self::Quiet => 1,
            Self::Balanced => (cores / 4).clamp(1, 4),
            Self::Fast => (cores / 3).clamp(1, 8),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaybackPolicy {
    #[default]
    Slow,
    Pause,
}
impl PlaybackPolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Slow => "Keep working slowly",
            Self::Pause => "Pause preparation",
        }
    }
    fn id(self) -> usize {
        usize::from(self == Self::Pause)
    }
}
struct Budget {
    mode: AtomicUsize,
    playing: AtomicBool,
    policy: AtomicUsize,
    marker: tempfile::TempDir,
}
static BUDGET: OnceLock<Budget> = OnceLock::new();
fn budget() -> &'static Budget {
    BUDGET.get_or_init(|| {
        let mode = crate::theme::settings_path()
            .and_then(|p| std::fs::read_to_string(p.with_file_name("preparation-speed.txt")).ok())
            .and_then(|s| s.trim().parse::<usize>().ok())
            .filter(|n| *n <= 2)
            .unwrap_or(1);
        Budget {
            mode: AtomicUsize::new(mode),
            playing: AtomicBool::new(false),
            policy: AtomicUsize::new(
                crate::theme::settings_path()
                    .and_then(|p| {
                        std::fs::read_to_string(p.with_file_name("playback-preparation.txt")).ok()
                    })
                    .is_some_and(|s| s.trim() == "pause") as usize,
            ),
            marker: tempfile::TempDir::new().expect("Preparation playback gate"),
        }
    })
}
pub fn mode() -> Mode {
    match budget().mode.load(Relaxed) {
        0 => Mode::Quiet,
        2 => Mode::Fast,
        _ => Mode::Balanced,
    }
}
pub fn playing() -> bool {
    budget().playing.load(Relaxed)
}
pub fn policy() -> PlaybackPolicy {
    if budget().policy.load(Relaxed) == 1 {
        PlaybackPolicy::Pause
    } else {
        PlaybackPolicy::Slow
    }
}
pub fn paused() -> bool {
    playing() && policy() == PlaybackPolicy::Pause
}
pub fn throttled() -> bool {
    playing() && policy() == PlaybackPolicy::Slow
}
fn worker_limit(selected: Mode, cores: usize, active: bool, choice: PlaybackPolicy) -> usize {
    if active && choice == PlaybackPolicy::Slow {
        1
    } else {
        selected.workers(cores)
    }
}
#[cfg(not(test))]
fn publish_pause(active: bool) {
    use std::io::Write;
    let state = budget();
    if let Ok(mut file) = tempfile::NamedTempFile::new_in(state.marker.path()) {
        let until = if active {
            amp_core::background::now_ms() + 2500
        } else {
            0
        };
        let choice = if policy() == PlaybackPolicy::Slow {
            "slow"
        } else {
            "pause"
        };
        if write!(file, "{until} {choice}").is_ok() {
            let _ = file.persist(state.marker.path().join("pause"));
        }
    }
}
#[cfg(test)]
pub fn set_playing(_: bool) {}
#[cfg(not(test))]
pub fn set_playing(active: bool) {
    static HEARTBEAT: OnceLock<()> = OnceLock::new();
    let previous = budget().playing.swap(active, Relaxed);
    HEARTBEAT.get_or_init(|| {
        std::thread::spawn(|| {
            loop {
                publish_pause(playing());
                std::thread::sleep(Duration::from_millis(250));
            }
        });
    });
    if previous != active {
        publish_pause(active);
    }
}
pub fn wait(cancel: Option<&AtomicBool>) -> bool {
    while paused() {
        if cancel.is_some_and(|c| c.load(Relaxed)) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if throttled() {
        std::thread::sleep(Duration::from_millis(2));
    }
    !cancel.is_some_and(|c| c.load(Relaxed))
}
static ANALYSIS_ACTIVE: AtomicUsize = AtomicUsize::new(0);
pub struct AnalysisPermit;
impl Drop for AnalysisPermit {
    fn drop(&mut self) {
        ANALYSIS_ACTIVE.fetch_sub(1, Relaxed);
    }
}
fn admit(active: &AtomicUsize, limit: usize) -> bool {
    let mut count = active.load(Relaxed);
    loop {
        if count >= limit {
            return false;
        }
        match active.compare_exchange_weak(count, count + 1, Relaxed, Relaxed) {
            Ok(_) => return true,
            Err(current) => count = current,
        }
    }
}
pub fn analysis_permit() -> AnalysisPermit {
    loop {
        wait(None);
        if admit(
            &ANALYSIS_ACTIVE,
            worker_limit(mode(), amp_core::batch::cpu_count(), playing(), policy()),
        ) {
            return AnalysisPermit;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
pub fn configure(command: &mut std::process::Command) {
    command.env(
        amp_core::background::PAUSE_ENV,
        budget().marker.path().join("pause"),
    );
    if let Some(cache) =
        crate::theme::settings_path().and_then(|p| p.parent().map(|p| p.join("work-cache")))
    {
        command.env("AMP_PREPARATION_CACHE_DIR", cache);
    }
    command.env(
        "AMP_STEM_THREADS",
        mode()
            .stem_threads(amp_core::batch::cpu_count())
            .to_string(),
    );
    command.env(
        "AMP_STEM_IDLE_MS",
        match mode() {
            Mode::Quiet => "150",
            Mode::Balanced => "30",
            Mode::Fast => "0",
        },
    );
    command
        .env("OMP_NUM_THREADS", "1")
        .env("MKL_NUM_THREADS", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000 | 0x00004000);
    }
}
pub fn controls(ui: &mut egui::Ui) {
    let mut selected = mode();
    ui.horizontal_wrapped(|ui| {
        ui.label("Preparation speed");
        for choice in [Mode::Quiet, Mode::Balanced, Mode::Fast] {
            ui.selectable_value(&mut selected, choice, choice.label());
        }
    });
    if selected != mode() {
        budget().mode.store(selected.id(), Relaxed);
        if let Some(path) =
            crate::theme::settings_path().map(|p| p.with_file_name("preparation-speed.txt"))
        {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(error) = std::fs::write(path, selected.id().to_string()) {
                ui.label(format!("Could not save speed: {error}"));
            }
        }
    }
    ui.weak(format!(
        "Up to {} analysis songs • {} CPU threads for stems • GPU automatic",
        selected.workers(amp_core::batch::cpu_count()),
        selected.stem_threads(amp_core::batch::cpu_count())
    ));
    let mut choice = policy();
    ui.horizontal_wrapped(|ui| {
        ui.label("During playback");
        for option in [PlaybackPolicy::Slow, PlaybackPolicy::Pause] {
            ui.selectable_value(&mut choice, option, option.label());
        }
    });
    if choice != policy() {
        budget().policy.store(choice.id(), Relaxed);
        if let Some(path) =
            crate::theme::settings_path().map(|p| p.with_file_name("playback-preparation.txt"))
        {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(error) = std::fs::write(
                path,
                if choice == PlaybackPolicy::Slow {
                    "slow"
                } else {
                    "pause"
                },
            ) {
                ui.label(format!("Could not save playback policy: {error}"));
            }
        }
        #[cfg(not(test))]
        publish_pause(playing());
    }
    if paused() {
        ui.colored_label(
            crate::GOLD,
            "Preparation paused for playback; it resumes automatically afterward.",
        );
    } else if throttled() {
        ui.colored_label(crate::GOLD,"Working slowly during playback: one analysis song, reduced stem CPU use and spaced GPU segments.");
    } else {
        ui.weak("Both choices are remembered. Playback changes apply at the next safe processing checkpoint.");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn budgets_reserve_resources_and_increase_with_mode() {
        for cores in [1, 2, 4, 8, 32] {
            assert!(Mode::Quiet.workers(cores) <= Mode::Balanced.workers(cores));
            assert!(Mode::Balanced.workers(cores) <= Mode::Fast.workers(cores));
            assert!(Mode::Fast.stem_threads(cores) <= cores);
        }
    }
    #[test]
    fn budget_changes_allow_more_songs_but_never_admit_above_new_limit() {
        let active = AtomicUsize::new(0);
        assert!(admit(&active, 1));
        assert!(!admit(&active, 1));
        assert!(admit(&active, 3));
        assert!(!admit(&active, 1));
        active.fetch_sub(1, Relaxed);
        assert!(!admit(&active, 1));
        active.fetch_sub(1, Relaxed);
        assert!(admit(&active, 1));
    }
    #[test]
    fn playback_slow_caps_new_analysis_but_pause_keeps_normal_resume_budget() {
        assert_eq!(worker_limit(Mode::Fast, 16, true, PlaybackPolicy::Slow), 1);
        assert_eq!(worker_limit(Mode::Fast, 16, false, PlaybackPolicy::Slow), 8);
        assert_eq!(
            worker_limit(Mode::Balanced, 16, true, PlaybackPolicy::Pause),
            4
        );
    }
    #[test]
    fn cancelled_wait_returns_without_starting_a_song() {
        assert!(!wait(Some(&AtomicBool::new(true))));
    }
}
