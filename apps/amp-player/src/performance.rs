//! Performance-only orchestration. Never writes preparation or saved show rules.
use crate::{
    Player,
    control::{Action, Deck, Execution},
};
use amp_core::show::{self, Style};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::atomic::Ordering::Relaxed,
    time::{Duration, Instant},
};
pub struct Run {
    pub choice: String,
    pub profile: Option<show::SmartPlaylist>,
    pub settings: show::Settings,
    pub playlist: Option<String>,
    pub style: Style,
    pub target: Option<f64>,
    pub auto: bool,
    pub crossfade: bool,
    pub locked: bool,
    pub progression: Option<show::Progression>,
    planned_progression: Option<show::EnergyPlan>,
    planned_force: bool,
    pub active: usize,
    pub preloaded: Option<(usize, usize)>,
    pending: Option<(usize, Instant)>,
    pending_at: Option<f64>,
    transition: Option<usize>,
    recent: VecDeque<String>,
    ducked: Option<f32>,
}
impl Default for Run {
    fn default() -> Self {
        Self {
            choice: String::new(),
            profile: None,
            settings: Default::default(),
            playlist: None,
            style: Style::Steady,
            target: None,
            auto: false,
            crossfade: true,
            locked: false,
            progression: None,
            planned_progression: None,
            planned_force: false,
            active: 0,
            preloaded: None,
            pending: None,
            pending_at: None,
            transition: None,
            recent: VecDeque::new(),
            ducked: None,
        }
    }
}
impl Run {
    pub fn cancel_automation(&mut self) {
        self.auto = false;
        self.pending = None;
        self.pending_at = None;
        self.transition = None;
        self.preloaded = None;
    }
}
impl Player {
    fn active_deck(&self) -> usize {
        if let Some(a) = &self.audio {
            let p = [a.playing(0), a.playing(1)];
            match p {
                [true, false] => 0,
                [false, true] => 1,
                [true, true] => usize::from(a.fader() >= 0.5),
                _ => self.run.active,
            }
        } else {
            self.run.active
        }
    }
    fn requested_deck(&self, deck: Deck, incoming: bool) -> usize {
        match deck {
            Deck::A => 0,
            Deck::B => 1,
            Deck::Auto => {
                let a = self.active_deck();
                if incoming && self.audio.as_ref().is_some_and(|e| e.playing(a)) {
                    1 - a
                } else {
                    a
                }
            }
        }
    }
    fn find_song(&self, song: &str) -> Result<usize, String> {
        let lib = self
            .library
            .as_ref()
            .ok_or("Choose and scan music in AMP Library first")?;
        let matches: Vec<_> = lib
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| show::song_id(t) == song || t.title.eq_ignore_ascii_case(song))
            .map(|(i, _)| i)
            .collect();
        match matches.as_slice() {
            [i] => Ok(*i),
            [] => Err("Song was not found".into()),
            _ => Err("Song title is ambiguous; use its song ID".into()),
        }
    }
    fn cue_seconds(&self, index: usize, cue: &str) -> Result<f64, String> {
        let p = &self.library.as_ref().ok_or("Library is not ready")?.tracks[index].preparation;
        let cues: Vec<_> = p.manual_cues.iter().chain(&p.cues).collect();
        if let Ok(number) = cue.parse::<u16>() {
            return p
                .manual_cues
                .iter()
                .chain(&p.cues)
                .find(|c| c.number == Some(number))
                .or_else(|| {
                    p.manual_cues
                        .get(number.saturating_sub(1) as usize)
                        .filter(|c| number > 0 && c.number.is_none())
                })
                .map(|c| c.seconds)
                .ok_or("Saved hot-cue number was not found".into());
        }
        let matches: Vec<_> = cues
            .iter()
            .filter(|c| c.id == cue || c.label.eq_ignore_ascii_case(cue))
            .collect();
        match matches.as_slice() {
            [c] => Ok(c.seconds),
            [] => Err("Cue ID or name was not found".into()),
            _ => Err("Cue name is ambiguous; use its ID".into()),
        }
    }
    fn call_track(
        &mut self,
        index: usize,
        cue: Option<&str>,
        deck: Deck,
        execution: Execution,
    ) -> Result<(), String> {
        let d = self.requested_deck(deck, true);
        let active = self.active_deck();
        if execution == Execution::LoadOnly && self.audio.as_ref().is_some_and(|a| a.playing(d)) {
            return Err("Load Only cannot replace a playing deck; select the idle deck".into());
        }
        if execution == Execution::AutoFade
            && self.audio.as_ref().is_some_and(|a| a.playing(active))
            && d != 1 - usize::from(self.audio.as_ref().unwrap().fader() >= 0.5)
        {
            return Err("Auto Fade requires the opposite deck".into());
        }
        let mut track = self
            .library
            .as_ref()
            .ok_or("Library unavailable")?
            .tracks
            .get(index)
            .ok_or("Unknown song")?
            .clone();
        if let Some(cue) = cue {
            track.preparation.cue_in = self.cue_seconds(index, cue)?;
        } else if self.run.style == Style::Hype
            && let Some(p) = self.run.profile.as_ref()
            && p.use_manual_cues
            && let Some(c) = track.preparation.manual_cues.iter().find(|c| {
                (c.label.to_lowercase().contains("drop")
                    || c.label.to_lowercase().contains("build"))
                    && track.preparation.cue_out.is_none_or(|out| c.seconds < out)
            })
        {
            track.preparation.cue_in = c.seconds;
        }
        track.preparation.validate()?;
        self.ensure_audio()?;
        let engine = self.audio.as_mut().unwrap();
        let rate = track
            .preparation
            .extensions
            .get("playback_rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.)
            .clamp(0.84, 1.16) as f32;
        self.pitch[d] = (rate - 1.) * 100.;
        engine.set_rate(d, rate);
        engine.load(d, &self.root, &track, execution == Execution::PlayNow)?;
        self.loaded[d] = Some(index);
        self.auto = false;
        match execution {
            Execution::LoadOnly => self.run.preloaded = Some((d, index)),
            Execution::PlayNow => {
                engine.set_fader(d as f32);
                if engine.playing(1 - d) {
                    engine.controls.play[1 - d].store(false, Relaxed);
                }
                self.run.active = d;
                self.record_song(index);
            }
            Execution::AutoFade => {
                self.run.preloaded = Some((d, index));
                self.run.pending = Some((d, Instant::now()));
            }
        }
        Ok(())
    }
    fn record_song(&mut self, index: usize) {
        if let Some(t) = self.library.as_ref().and_then(|l| l.tracks.get(index)) {
            self.run.recent.push_back(show::song_id(t));
            while self.run.recent.len() > 1000 {
                self.run.recent.pop_front();
            }
        }
    }
    fn preload_smart(&mut self) -> Result<(), String> {
        self.preload_variant(0)
    }
    fn preload_variant(&mut self, alternate: usize) -> Result<(), String> {
        if self.run.preloaded.is_some() {
            return Ok(());
        }
        if self.run.profile.is_none() {
            let incoming = self.requested_deck(Deck::Auto, true);
            if let Some(index) = self.loaded[incoming]
                && self.audio.as_ref().is_some_and(|a| {
                    a.load_current(incoming)
                        && a.loaded(incoming)
                        && !a.playing(incoming)
                        && !a.controls.finished[incoming].load(Relaxed)
                })
            {
                self.run.preloaded = Some((incoming, index));
                return Ok(());
            }
        }
        let p = self
            .run
            .profile
            .as_ref()
            .ok_or("Load a Smart Playlist first")?
            .clone();
        let current = self.loaded[self.active_deck()];
        let recent: Vec<_> = self.run.recent.iter().cloned().collect();
        let tracks = &self.library.as_ref().ok_or("Library unavailable")?.tracks;
        let target = self
            .run
            .progression
            .as_ref()
            .map(|p| p.next())
            .or(self.run.target);
        let index = show::choose(&p, tracks, current, &recent, target, alternate)
            .ok_or("No eligible next song in this Smart Playlist")?;
        self.call_track(index, None, Deck::Auto, Execution::LoadOnly)
    }
    pub(super) fn execute_control(&mut self, action: Action) -> Result<(), String> {
        if self.run.locked
            && !matches!(
                action,
                Action::ManualLock { .. }
                    | Action::Stop { .. }
                    | Action::Pause { .. }
                    | Action::Play { .. }
                    | Action::Sampler { .. }
                    | Action::SetCrossfader { .. }
            )
        {
            return Err("Manual DJ Lock prevents automated takeover".into());
        }
        match action {
            Action::CallSong {
                song,
                cue,
                deck,
                execution,
            } => {
                let i = self.find_song(&song)?;
                self.call_track(i, cue.as_deref(), deck, execution)?;
            }
            Action::PreloadNext => self.preload_smart()?,
            Action::NextSong { immediate } => {
                if self.run.profile.is_some() {
                    self.execute_control(Action::MixNext { immediate })?;
                } else {
                    let i = *self.queue.first().ok_or("No next song is queued")?;
                    self.call_track(i, None, Deck::Auto, Execution::AutoFade)?;
                    self.queue.remove(0);
                }
            }
            Action::SmartShuffle => {
                self.run.preloaded = None;
                self.preload_variant(1)?;
            }
            Action::Play { deck } => {
                let d = self.requested_deck(deck, false);
                let a = self.audio.as_ref().ok_or("Load a song first")?;
                if !a.load_current(d) || !a.loaded(d) {
                    return Err("Deck is not loaded yet".into());
                }
                a.controls.play[d].store(true, Relaxed);
            }
            Action::Pause { deck } => {
                let d = self.requested_deck(deck, false);
                if let Some(a) = &self.audio {
                    a.controls.play[d].store(false, Relaxed);
                }
            }
            Action::Stop { deck } => {
                let d = self.requested_deck(deck, false);
                self.run.cancel_automation();
                self.stop(d);
            }
            Action::RestartCue { deck, cue } => {
                let d = self.requested_deck(deck, false);
                let i = self.loaded[d].ok_or("Deck is not loaded")?;
                self.call_track(
                    i,
                    cue.as_deref(),
                    if d == 0 { Deck::A } else { Deck::B },
                    Execution::PlayNow,
                )?;
            }
            Action::Fade {
                deck,
                master,
                target,
                seconds,
                stop_after,
            } => {
                let d = if master {
                    2
                } else {
                    self.requested_deck(deck, false)
                };
                self.audio
                    .as_ref()
                    .ok_or("Audio engine is not running")?
                    .level_fade(d, target, seconds, stop_after)?;
                if stop_after {
                    self.run.cancel_automation();
                }
            }
            Action::Duck { level, seconds } => {
                let a = self.audio.as_ref().ok_or("Audio engine is not running")?;
                let prior = a.master_level();
                a.level_fade(2, level, seconds, false)?;
                if self.run.ducked.is_none() {
                    self.run.ducked = Some(prior);
                }
            }
            Action::Restore { seconds } => {
                let level = self.run.ducked.ok_or("No ducked level to restore")?;
                self.audio
                    .as_ref()
                    .ok_or("Audio engine is not running")?
                    .level_fade(2, level, seconds, false)?;
                self.run.ducked = None;
            }
            Action::AutoFade => {
                self.preload_smart()?;
                let d = self.run.preloaded.unwrap().0;
                self.run.pending_at = None;
                self.run.pending = Some((d, Instant::now()));
            }
            Action::MixNext { immediate } => {
                self.preload_smart()?;
                let d = self.run.preloaded.unwrap().0;
                let active = self.active_deck();
                self.run.pending_at = if immediate {
                    None
                } else {
                    let a = self.audio.as_ref().ok_or("Audio engine is not running")?;
                    let position = a.source_position(active);
                    let p = &self.library.as_ref().unwrap().tracks
                        [self.loaded[active].ok_or("No active song")?]
                    .preparation;
                    let last = p
                        .cue_out
                        .or(p.analysis.as_ref().map(|a| a.duration))
                        .map(|end| (end - self.fade_seconds as f64).max(position));
                    let cue = p
                        .manual_cues
                        .iter()
                        .chain(&p.cues)
                        .filter(|c| {
                            c.seconds > position + 0.5
                                && matches!(c.kind.as_str(), "grid" | "transition" | "mix_out")
                        })
                        .map(|c| c.seconds)
                        .min_by(f64::total_cmp);
                    Some(match (cue, last) {
                        (Some(a), Some(b)) => a.min(b),
                        (Some(a), None) => a,
                        (None, Some(b)) => b,
                        _ => position,
                    })
                };
                self.run.pending = Some((d, Instant::now()));
            }
            Action::AutoCrossfade { enabled } => self.run.crossfade = enabled,
            Action::LoadSmartPlaylist {
                playlist,
                execution,
                style,
                energy,
            } => {
                if energy.is_some_and(|e| !e.is_finite() || !(1. ..=10.).contains(&e)) {
                    return Err("Energy must be 1–10".into());
                }
                let p = self
                    .run
                    .settings
                    .playlists
                    .iter()
                    .find(|p| p.id == playlist || p.name.eq_ignore_ascii_case(&playlist))
                    .ok_or("Smart Playlist not found")?
                    .clone();
                self.run.choice = p.id.clone();
                self.run.planned_progression = p.energy_progression.clone();
                self.run.planned_force = false;
                self.run.progression = None;
                self.run.profile = Some(p.clone());
                self.run.playlist = Some(p.id);
                self.run.style = style.unwrap_or(p.style);
                self.run.target = energy.or(p.target_energy);
                self.run.preloaded = None;
                self.run.pending = None;
                if execution != Execution::LoadOnly {
                    self.preload_smart()?;
                    let (d, i) = self.run.preloaded.unwrap();
                    if execution == Execution::PlayNow {
                        self.call_track(
                            i,
                            None,
                            if d == 0 { Deck::A } else { Deck::B },
                            execution,
                        )?;
                    } else {
                        self.run.pending = Some((d, Instant::now()));
                    }
                    self.run.auto = true;
                }
            }
            Action::Recognition {
                song,
                cue,
                execution,
            } => {
                let assigned = song.or_else(|| {
                    self.run
                        .settings
                        .playlists
                        .iter()
                        .find(|p| p.id == "recognition")
                        .and_then(|p| p.recognition_song.clone())
                });
                self.execute_control(Action::LoadSmartPlaylist {
                    playlist: "recognition".into(),
                    execution: Execution::LoadOnly,
                    style: None,
                    energy: None,
                })?;
                if let Some(song) = assigned {
                    let i = self.find_song(&song)?;
                    let p = self
                        .run
                        .settings
                        .playlists
                        .iter()
                        .find(|p| p.id == "recognition")
                        .unwrap();
                    if !show::eligible(p, &self.library.as_ref().unwrap().tracks[i]) {
                        return Err("Assigned recognition song is excluded by Library rules".into());
                    }
                    self.call_track(i, cue.as_deref(), Deck::Auto, execution)?;
                } else {
                    self.preload_smart()?;
                    let (d, i) = self.run.preloaded.unwrap();
                    self.call_track(
                        i,
                        cue.as_deref(),
                        if d == 0 { Deck::A } else { Deck::B },
                        execution,
                    )?;
                }
            }
            Action::SetStyle { style } => {
                self.run.style = style;
                self.run.preloaded = None;
            }
            Action::SetEnergy { target } => {
                if !target.is_finite() || !(1. ..=10.).contains(&target) {
                    return Err("Energy must be 1–10".into());
                }
                if let Some(p) = self.run.progression.as_ref() {
                    self.run.progression = Some(show::Progression::new(
                        self.current_energy().ok_or("Current energy unavailable")?,
                        target,
                        p.remaining().max(1),
                        p.hold,
                    )?);
                } else {
                    self.run.target = Some(target);
                }
                self.run.preloaded = None;
                self.run.pending = None;
                self.run.pending_at = None;
            }
            Action::EnergyStep { delta } => {
                if !delta.is_finite() {
                    return Err("Invalid energy increment".into());
                }
                let target = (self.run.target.or(self.current_energy()).unwrap_or(5.) + delta)
                    .clamp(1., 10.);
                self.execute_control(Action::SetEnergy { target })?;
            }
            Action::Progression {
                target,
                transitions,
                hold,
            } => {
                let start = self
                    .current_energy()
                    .ok_or("Current track needs reviewed energy metadata")?;
                let plan = show::Progression::new(start, target, transitions, hold)?;
                self.run.planned_progression = None;
                self.run.planned_force = false;
                if self.run.transition.is_some() {
                    self.run.progression = None;
                    self.run.planned_progression = Some(show::EnergyPlan {
                        target,
                        transitions,
                        hold,
                    });
                    self.run.planned_force = true;
                } else {
                    self.run.progression = Some(plan);
                    self.run.pending = None;
                    self.run.pending_at = None;
                }
                self.run.preloaded = None;
            }
            Action::CancelProgression => {
                self.run.progression = None;
                self.run.planned_progression = None;
                self.run.planned_force = false;
            }
            Action::ReverseProgression => {
                let old = self
                    .run
                    .progression
                    .as_ref()
                    .cloned()
                    .ok_or("No active progression")?;
                self.run.progression = Some(show::Progression::new(
                    self.current_energy().ok_or("Current energy unavailable")?,
                    old.start,
                    old.remaining().max(1),
                    old.hold,
                )?);
                self.run.preloaded = None;
            }
            Action::HoldEnergy => {
                self.run.planned_progression = None;
                self.run.planned_force = false;
                self.run.target = Some(self.current_energy().ok_or("Current energy unavailable")?);
                self.run.progression = None;
                self.run.preloaded = None;
            }
            Action::AutoDj { enabled } => {
                if enabled && self.run.playlist.is_none() {
                    return Err("Load a Smart Playlist first".into());
                }
                self.run.auto = enabled;
                if enabled
                    && !self
                        .audio
                        .as_ref()
                        .is_some_and(|a| a.playing(self.active_deck()))
                {
                    self.preload_smart()?;
                    let (d, i) = self.run.preloaded.unwrap();
                    self.call_track(
                        i,
                        None,
                        if d == 0 { Deck::A } else { Deck::B },
                        Execution::PlayNow,
                    )?;
                }
            }
            Action::ManualLock { enabled } => {
                self.run.locked = enabled;
                if enabled {
                    self.run.cancel_automation();
                }
            }
            Action::SetCrossfader { value } => {
                if !value.is_finite() || !(0. ..=1.).contains(&value) {
                    return Err("Crossfader must be 0–1".into());
                }
                self.audio
                    .as_ref()
                    .ok_or("Audio engine is not running")?
                    .set_fader(value);
                self.run.transition = None;
            }
            Action::Sampler { pad } => {
                if !(1..=9).contains(&pad) {
                    return Err("Sampler pad must be 1–9".into());
                }
                self.audio
                    .as_ref()
                    .ok_or("Load sampler effects in DJ first")?
                    .sample_trigger(pad - 1)?;
            }
        }
        Ok(())
    }
    fn current_energy(&self) -> Option<f64> {
        let active = self.active_deck();
        if !self
            .audio
            .as_ref()
            .is_some_and(|a| a.load_current(active) && a.loaded(active))
        {
            return None;
        }
        self.loaded[active]
            .and_then(|i| self.library.as_ref()?.tracks.get(i))
            .and_then(|t| show::energy(&t.preparation))
    }
    pub(super) fn performance_tick(&mut self) {
        if (self.run.auto || self.run.planned_force)
            && self.run.transition.is_none()
            && self.run.progression.is_none()
            && let Some(start) = self.current_energy()
            && let Some(plan) = self.run.planned_progression.take()
        {
            self.run.progression =
                show::Progression::new(start, plan.target, plan.transitions, plan.hold).ok();
        }
        self.run.active = self.active_deck();
        if let Some((incoming, started)) = self.run.pending {
            let ready = self.audio.as_ref().is_some_and(|a| {
                a.load_current(incoming)
                    && a.loaded(incoming)
                    && a.controls.ready[incoming].load(Relaxed)
            });
            let due = self.run.pending_at.is_none_or(|due| {
                self.audio
                    .as_ref()
                    .is_some_and(|a| a.source_position(self.active_deck()) >= due)
            });
            if !ready && started.elapsed() > Duration::from_secs(15) {
                self.status = "Incoming deck did not become ready; outgoing audio retained".into();
                self.run.cancel_automation();
            } else if ready && due {
                let a = self.audio.as_ref().unwrap();
                if !a.playing(1 - incoming) {
                    a.controls.play[incoming].store(true, Relaxed);
                    a.set_fader(incoming as f32);
                    self.complete_change(incoming);
                } else if incoming != 1 - usize::from(a.fader() >= 0.5) {
                    self.status =
                        "Crossfader changed after preload; request the next song again".into();
                    self.run.cancel_automation();
                } else {
                    let seconds = match self.run.style {
                        Style::Hype => 3.,
                        Style::Steady => self.fade_seconds,
                        Style::Chill => 10.,
                    };
                    match a.crossfade(seconds) {
                        Ok(()) => self.run.transition = Some(incoming),
                        Err(e) => {
                            self.status = e;
                            self.run.auto = false;
                        }
                    }
                }
                self.run.pending = None;
                self.run.pending_at = None;
            }
        }
        if let Some(incoming) = self.run.transition
            && let Some(a) = &self.audio
            && !a.fading()
            && !a.playing(1 - incoming)
            && a.playing(incoming)
            && (a.fader() - incoming as f32).abs() < 0.001
        {
            self.complete_change(incoming);
        }

        if self.run.auto
            && !self.run.locked
            && self.run.pending.is_none()
            && self.run.transition.is_none()
        {
            let active = self.active_deck();
            let remaining = self.audio.as_ref().and_then(|a| a.remaining(active));
            let mix_seconds = match self.run.style {
                Style::Hype => 3.,
                Style::Steady => self.fade_seconds,
                Style::Chill => 10.,
            };
            if self.audio.as_ref().is_some_and(|a| a.playing(active))
                && remaining.is_some_and(|r| r < self.fade_seconds.max(10.) + 10.)
                && self.run.preloaded.is_none()
                && let Err(e) = self.preload_smart()
            {
                self.status = e;
                self.run.auto = false;
            }
            let finished = self
                .audio
                .as_ref()
                .is_some_and(|a| a.controls.finished[active].load(Relaxed));
            if (finished || (self.run.crossfade && remaining.is_some_and(|r| r <= mix_seconds)))
                && let Some((incoming, _)) = self.run.preloaded
            {
                self.run.pending = Some((incoming, Instant::now()));
            }
        }
    }
    fn complete_change(&mut self, incoming: usize) {
        self.run.active = incoming;
        self.run.transition = None;
        self.run.preloaded = None;
        if let Some(i) = self.loaded[incoming] {
            self.record_song(i);
        }
        if let Some(p) = &mut self.run.progression {
            p.completed += 1;
            if p.remaining() == 0 {
                if p.hold {
                    self.run.target = Some(p.target);
                }
                self.run.progression = None;
            }
        }
    }
    pub(super) fn control_catalog(&self) -> Value {
        json!({"songs":self.library.as_ref().map(|l|l.tracks.iter().map(|t|json!({"id":show::song_id(t),"title":t.title,"artist":t.preparation.extensions.get("artist"),"cues":t.preparation.manual_cues.iter().chain(&t.preparation.cues).map(|c|json!({"number":c.number,"id":c.id,"name":c.label,"seconds":c.seconds})).collect::<Vec<_>>()})).collect::<Vec<_>>()).unwrap_or_default(),"playlists":self.run.settings.playlists,"version":1})
    }
    pub(super) fn control_snapshot(&self) -> Value {
        let active = self.active_deck();
        let a = self.audio.as_ref();
        let tracks:Vec<_>=(0..2).map(|d|{
            let engine_loaded=a.is_some_and(|a|a.load_current(d)&&a.loaded(d));let t=self.loaded[d].and_then(|i|self.library.as_ref()?.tracks.get(i));
            let playing=engine_loaded&&a.is_some_and(|a|a.playing(d));
            json!({"deck":if d==0{"A"}else{"B"},"loaded":engine_loaded,"ready":engine_loaded&&a.is_some_and(|a|a.controls.ready[d].load(Relaxed)),"playing":playing,"state":if playing{"playing"}else if engine_loaded{"paused"}else{"stopped"},"song_id":if engine_loaded{t.map(show::song_id)}else{None},"title":if engine_loaded{t.map(|t|t.title.as_str())}else{None},"artist":if engine_loaded{t.and_then(|t|t.preparation.extensions.get("artist")).cloned()}else{None},"position":if engine_loaded{a.map(|a|a.source_position(d))}else{None},"remaining":if engine_loaded{a.and_then(|a|a.remaining(d))}else{None}})
        }).collect();
        let fading = a.is_some_and(|a| a.fading() || a.controls.level_fading.load(Relaxed));
        json!({"version":1,"amp":{"connected":true,"playback_state":tracks[active]["state"],"current_song":tracks[active]["title"],"current_artist":tracks[active]["artist"],"current_cue":if tracks[active]["loaded"]==true{self.loaded[active].and_then(|i|self.library.as_ref()?.tracks.get(i)).and_then(|t|a.and_then(|a|t.preparation.manual_cues.iter().chain(&t.preparation.cues).find(|c|(c.seconds-(a.source_position(active)-a.position(active))).abs()<0.001).map(|c|c.id.clone())))}else{None},"active_deck":if active==0{"A"}else{"B"},"next_song":self.run.preloaded.and_then(|(_,i)|self.library.as_ref()?.tracks.get(i)).map(|t|t.title.clone()),"remaining_time":tracks[active]["remaining"],"playlist":self.run.playlist,"mix_style":self.run.style,"target_energy":self.run.progression.as_ref().map(|p|p.target).or(self.run.planned_progression.as_ref().map(|p|p.target)).or(self.run.target),"current_energy":self.current_energy(),"progression_remaining":self.run.progression.as_ref().map_or(0,|p|p.remaining()),"auto_dj_enabled":self.run.auto,"auto_crossfade_enabled":self.run.crossfade,"fade_state":if fading{"running"}else{"idle"},"fade_progress":a.map_or(0.,|a|f32::from_bits(a.controls.fade_progress.load(Relaxed))),"master_level":a.map_or(self.volume as f32/100.,|a|a.master_level()),"manual_dj_locked":self.run.locked},"decks":tracks})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wait(player: &mut Player, check: impl Fn(&Player) -> bool) {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            player.poll();
            player.performance_tick();
            if check(player) {
                break;
            }
            assert!(Instant::now() < until, "Timed out: {}", player.status);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    #[ignore = "requires native audio and AMP_WAVEFORM_FIXTURE; muted by channel gains"]
    fn native_show_control_preserves_music_and_approved_metadata() {
        let fixture = std::env::var_os("AMP_WAVEFORM_FIXTURE").expect("fixture");
        let dir = tempfile::tempdir().unwrap();
        for n in 0..4 {
            std::fs::copy(&fixture, dir.path().join(format!("song{n}.wav"))).unwrap();
        }
        let mut lib = amp_core::scan(dir.path()).unwrap();
        for (i, t) in lib.tracks.iter_mut().enumerate() {
            t.preparation.cue_in = 1.;
            t.preparation.cue_out = Some(9.);
            t.preparation.bpm = Some(120.);
            t.preparation.harmonic_key = Some("8B".into());
            t.preparation.manual_override = true;
            t.preparation
                .extensions
                .insert("energy".into(), json!(([9., 7., 5., 3.][i] - 1.) / 9.));
            t.preparation.manual_cues.push(amp_core::preparation::Cue {
                id: "drop".into(),
                label: "Drop".into(),
                seconds: 2.5,
                kind: "hotcue".into(),
                ..Default::default()
            });
            amp_core::preparation::save(dir.path(), &t.path, &t.preparation).unwrap();
        }
        let before: Vec<_> = lib
            .tracks
            .iter()
            .map(|t| amp_core::preparation::revision(&dir.path().join(&t.path)))
            .collect();
        let mut player = Player {
            root: dir.path().into(),
            library: Some(lib),
            volume: 75,
            last_master: 75,
            channel_gain: [0.; 2],
            fade_seconds: 0.15,
            ..Default::default()
        };
        player
            .execute_control(Action::CallSong {
                song: "song0".into(),
                cue: None,
                deck: Deck::A,
                execution: Execution::PlayNow,
            })
            .unwrap();
        wait(&mut player, |p| {
            p.audio.as_ref().unwrap().load_current(0)
                && p.audio.as_ref().unwrap().controls.ready[0].load(Relaxed)
        });
        player
            .execute_control(Action::CallSong {
                song: "song1".into(),
                cue: Some("drop".into()),
                deck: Deck::B,
                execution: Execution::LoadOnly,
            })
            .unwrap();
        wait(&mut player, |p| {
            p.audio.as_ref().unwrap().load_current(1)
                && p.audio.as_ref().unwrap().controls.ready[1].load(Relaxed)
        });
        let a = player.audio.as_ref().unwrap();
        assert!(a.playing(0));
        assert!(!a.playing(1));
        assert!((a.source_position(1) - a.position(1) - 2.5).abs() < 0.001);
        player
            .execute_control(Action::CallSong {
                song: "song1".into(),
                cue: Some("drop".into()),
                deck: Deck::B,
                execution: Execution::AutoFade,
            })
            .unwrap();
        wait(&mut player, |p| {
            p.run.pending.is_none() && p.run.transition.is_none() && p.active_deck() == 1
        });
        assert!(player.audio.as_ref().unwrap().source_position(1) >= 2.5);
        player
            .execute_control(Action::CallSong {
                song: "song0".into(),
                cue: None,
                deck: Deck::A,
                execution: Execution::PlayNow,
            })
            .unwrap();
        wait(&mut player, |p| {
            p.audio.as_ref().unwrap().load_current(0)
                && p.audio.as_ref().unwrap().controls.ready[0].load(Relaxed)
        });
        player
            .execute_control(Action::ManualLock { enabled: true })
            .unwrap();
        assert!(player.execute_control(Action::AutoFade).is_err());
        assert!(player.audio.as_ref().unwrap().playing(0));
        player
            .execute_control(Action::ManualLock { enabled: false })
            .unwrap();
        player
            .execute_control(Action::Duck {
                level: 0.2,
                seconds: 0.05,
            })
            .unwrap();
        wait(&mut player, |p| {
            (p.audio.as_ref().unwrap().master_level() - 0.2).abs() < 0.001
        });
        player
            .execute_control(Action::Restore { seconds: 0.05 })
            .unwrap();
        wait(&mut player, |p| {
            (p.audio.as_ref().unwrap().master_level() - 0.75).abs() < 0.001
        });
        player
            .run
            .settings
            .playlists
            .iter_mut()
            .find(|p| p.id == "everyday")
            .unwrap()
            .recent_repeat = 0;
        player
            .execute_control(Action::LoadSmartPlaylist {
                playlist: "everyday".into(),
                execution: Execution::LoadOnly,
                style: Some(Style::Steady),
                energy: None,
            })
            .unwrap();
        player
            .execute_control(Action::Progression {
                target: 3.,
                transitions: 3,
                hold: true,
            })
            .unwrap();
        assert!(player.audio.as_ref().unwrap().playing(0));
        for expected in [7., 5., 3.] {
            player
                .execute_control(Action::MixNext { immediate: true })
                .unwrap();
            wait(&mut player, |p| {
                p.run.pending.is_none()
                    && p.run.transition.is_none()
                    && p.current_energy()
                        .is_some_and(|e| (e - expected).abs() < 0.001)
            });
        }
        assert!(player.run.progression.is_none());
        assert_eq!(player.run.target, Some(3.));
        for (i, t) in player.library.as_ref().unwrap().tracks.iter().enumerate() {
            assert_eq!(
                amp_core::preparation::revision(&dir.path().join(&t.path)),
                before[i]
            );
        }
        let active = player.active_deck();
        assert!(player.audio.as_ref().unwrap().playing(active));
        let c =
            crate::control::Control::start(&eframe::egui::Context::default(), "127.0.0.1:0", None)
                .unwrap();
        drop(c);
        assert!(player.audio.as_ref().unwrap().playing(active));
        assert!(player.audio.as_ref().unwrap().take_error().is_none());
    }
}
