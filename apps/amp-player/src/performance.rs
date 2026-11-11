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
fn scratch_phrase(seconds: f32, flourish: bool) -> f32 {
    if !flourish {
        return if seconds < 0.09 { -0.65 } else { 0.8 };
    }
    // Two short back/forward strokes, ending forward before the handoff.
    match (seconds / 0.09) as usize {
        0 => -0.75,
        1 => 1.1,
        2 => -0.6,
        _ => 1.,
    }
}
// Relative cuts preserve the user's saved EQ rather than resetting it to noon.
// Bass crosses once at the midpoint; highs enter first, then the mids.
fn blend_eq(progress: f32, original: [[f32; 3]; 2]) -> [[f32; 3]; 2] {
    let p = progress.clamp(0., 1.);
    let ramp = |start: f32, end: f32| ((p - start) / (end - start)).clamp(0., 1.);
    let bass = ramp(0.4, 0.6);
    let highs = ramp(0., 0.4);
    let mids = ramp(0.4, 0.9);
    let cuts = [
        [24. * bass, 12. * mids, 9. * highs],
        [24. * (1. - bass), 9. * (1. - mids), 6. * (1. - highs)],
    ];
    std::array::from_fn(|d| std::array::from_fn(|b| (original[d][b] - cuts[d][b]).max(-24.)))
}
fn blend_stems(progress: f32, original: [[f32; 3]; 2]) -> [[f32; 3]; 2] {
    let p = progress.clamp(0., 1.);
    let ramp = |a: f32, b: f32| ((p - a) / (b - a)).clamp(0., 1.);
    let drums = ramp(0.4, 0.6);
    let weights = [
        [1. - ramp(0., 0.25), 1. - ramp(0.5, 1.), 1. - drums],
        [ramp(0.1, 0.35), ramp(0.45, 0.85), drums],
    ];
    std::array::from_fn(|d| std::array::from_fn(|s| original[d][s] * weights[d][s]))
}
fn buildup_halves(elapsed: f32, capture: f32, duration: f32) -> u8 {
    if elapsed < capture || duration <= capture {
        return 0;
    }
    (1 + ((elapsed - capture) / (duration - capture) * 4.) as u8).min(4)
}
pub struct Run {
    pub requests: VecDeque<String>,
    pub transitions: crate::transition_styles::Settings,
    effect_bridge: Option<EffectBridge>,
    effect_done: Option<Instant>,
    pub choice: String,
    pub profile: Option<show::SmartPlaylist>,
    pub settings: show::Settings,
    pub playlist: Option<String>,
    pub style: Style,
    pub target: Option<f64>,
    pub auto: bool,
    pub crossfade: bool,
    pub scratch_assist: bool,
    scratch_bridge: Option<(usize, Instant)>,
    completed_handoffs: u32,
    pub future_candidates: VecDeque<usize>,
    failed_songs: Vec<String>,
    pub locked: bool,
    pub progression: Option<show::Progression>,
    planned_progression: Option<show::EnergyPlan>,
    planned_force: bool,
    pub active: usize,
    pub preloaded: Option<(usize, usize)>,
    pub(super) pending: Option<(usize, Instant)>,
    pub(super) pending_at: Option<f64>,
    transition: Option<usize>,
    recent: VecDeque<String>,
    ducked: Option<f32>,
}
impl Default for Run {
    fn default() -> Self {
        let transitions = crate::transition_styles::load();
        Self {
            requests: VecDeque::new(),
            transitions,
            effect_bridge: None,
            effect_done: None,
            choice: String::new(),
            profile: None,
            settings: Default::default(),
            playlist: None,
            style: Style::Steady,
            target: None,
            auto: false,
            crossfade: true,
            scratch_assist: transitions.enabled,
            scratch_bridge: None,
            completed_handoffs: 0,
            future_candidates: VecDeque::new(),
            failed_songs: Vec::new(),
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
    fn tick_effect(&mut self, audio: &crate::mixer::Engine) -> Option<String> {
        let mut effect = self.effect_bridge.take()?;
        if !audio.fading() {
            self.effect_bridge = Some(effect);
            return None;
        }
        let progress = f32::from_bits(audio.controls.fade_progress.load(Relaxed)).clamp(0., 1.);
        if effect.style == crate::transition_styles::Style::LoopBuild {
            if !effect.engaged {
                if audio.controls.looping[effect.deck].load(Relaxed)
                    || audio.buildup_start(effect.deck, effect.grain).is_err()
                {
                    return None;
                }
                effect.start_progress = progress;
                effect.engaged = true;
            }
            let elapsed = (progress - effect.start_progress) * effect.seconds;
            let target = buildup_halves(elapsed, effect.grain + 0.05, effect.seconds);
            while effect.halves < target {
                if audio.loop_halve(effect.deck).is_err() {
                    break;
                }
                effect.halves += 1;
            }
            self.effect_bridge = Some(effect);
            return Some("Loop buildup transition".into());
        }
        if effect.style == crate::transition_styles::Style::StemSwap {
            if !effect.engaged {
                if !audio.controls.stems_loaded.iter().all(|v| v.load(Relaxed)) {
                    return None;
                }
                effect.original_stems = std::array::from_fn(|side| {
                    std::array::from_fn(|s| {
                        f32::from_bits(
                            audio.controls.stem_level[if side == 0 {
                                effect.deck
                            } else {
                                1 - effect.deck
                            }][s]
                                .load(Relaxed),
                        )
                    })
                });
                effect.engaged = true;
            }
            for (side, levels) in blend_stems(progress, effect.original_stems)
                .iter()
                .enumerate()
            {
                let deck = if side == 0 {
                    effect.deck
                } else {
                    1 - effect.deck
                };
                for (stem, value) in levels.iter().enumerate() {
                    audio.controls.stem_level[deck][stem].store(value.to_bits(), Relaxed);
                }
            }
            self.effect_bridge = Some(effect);
            return Some("Stem handoff transition".into());
        }
        if effect.style == crate::transition_styles::Style::BassSwap {
            if !effect.engaged {
                effect.original_eq = std::array::from_fn(|d| {
                    std::array::from_fn(|b| {
                        f32::from_bits(
                            audio.controls.eq[if d == 0 { effect.deck } else { 1 - effect.deck }]
                                [b]
                                .load(Relaxed),
                        )
                    })
                });
                effect.engaged = true;
            }
            for (side, bands) in blend_eq(progress, effect.original_eq).iter().enumerate() {
                let deck = if side == 0 {
                    effect.deck
                } else {
                    1 - effect.deck
                };
                for (band, value) in bands.iter().enumerate() {
                    audio.set_eq(deck, band, *value);
                }
            }
            self.effect_bridge = Some(effect);
            return Some("EQ blend / bass swap transition".into());
        }
        let window = effect.seconds.min(0.8);
        let elapsed = progress * effect.seconds - (effect.seconds - window);
        if elapsed >= 0. {
            if !effect.engaged {
                if audio.controls.looping[effect.deck].load(Relaxed) {
                    return None;
                }
                if effect.style == crate::transition_styles::Style::Stutter
                    && audio.stutter_start(effect.deck, effect.grain).is_err()
                {
                    return None;
                }
                effect.original = match effect.style {
                    crate::transition_styles::Style::Filter => {
                        f32::from_bits(audio.controls.cutoff[effect.deck].load(Relaxed))
                    }
                    _ => 0.,
                };
                effect.engaged = true;
            }
            let amount = (elapsed / window).clamp(0., 1.);
            if effect.style == crate::transition_styles::Style::Filter {
                audio.controls.cutoff[effect.deck].store(
                    (effect.original * (1. - amount) + 180. * amount).to_bits(),
                    Relaxed,
                );
            }
            if effect.style == crate::transition_styles::Style::Brake {
                let rate = f32::from_bits(audio.controls.rate[effect.deck].load(Relaxed));
                audio.scratch(effect.deck, true, rate * (1. - elapsed / window).max(0.04));
            }
        }
        let label = effect
            .engaged
            .then(|| format!("{} transition", effect.style.label()));
        self.effect_bridge = Some(effect);
        label
    }
    pub fn cancel_effects(&mut self, audio: &crate::mixer::Engine) {
        if let Some(effect) = self.effect_bridge.take()
            && effect.engaged
        {
            if matches!(
                effect.style,
                crate::transition_styles::Style::Stutter
                    | crate::transition_styles::Style::LoopBuild
            ) {
                audio.loop_stop(effect.deck);
            } else if effect.style == crate::transition_styles::Style::StemSwap {
                for (side, levels) in effect.original_stems.iter().enumerate() {
                    let deck = if side == 0 {
                        effect.deck
                    } else {
                        1 - effect.deck
                    };
                    for (stem, value) in levels.iter().enumerate() {
                        audio.controls.stem_level[deck][stem].store(value.to_bits(), Relaxed);
                    }
                }
            } else if effect.style == crate::transition_styles::Style::Filter {
                audio.controls.cutoff[effect.deck].store(effect.original.to_bits(), Relaxed);
            } else if effect.style == crate::transition_styles::Style::BassSwap {
                for (side, bands) in effect.original_eq.iter().enumerate() {
                    let deck = if side == 0 {
                        effect.deck
                    } else {
                        1 - effect.deck
                    };
                    for (band, value) in bands.iter().enumerate() {
                        audio.set_eq(deck, band, *value);
                    }
                }
            } else {
                audio.scratch(effect.deck, false, 1.);
            }
        }
        if let Some((deck, _)) = self.scratch_bridge.take() {
            audio.scratch(deck, false, 1.);
        }
    }
    pub fn cancel_automation(&mut self) {
        self.future_candidates.clear();
        self.auto = false;
        self.pending = None;
        self.pending_at = None;
        self.transition = None;
        self.preloaded = None;
    }
}
struct EffectBridge {
    deck: usize,
    style: crate::transition_styles::Style,
    seconds: f32,
    grain: f32,
    engaged: bool,
    original: f32,
    original_eq: [[f32; 3]; 2],
    original_stems: [[f32; 3]; 2],
    start_progress: f32,
    halves: u8,
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
    pub(super) fn find_song(&self, song: &str) -> Result<usize, String> {
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
        let track = self.fresh_track(index)?;
        let p = &track.preparation;
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
        let mut track = self.fresh_track(index)?;
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
        let saved_rate = track
            .preparation
            .extensions
            .get("playback_rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.)
            .clamp(0.84, 1.16);
        // Match only the incoming voice. Never change the sounding deck or saved metadata.
        let matched_rate = if self.run.profile.is_some()
            && d != active
            && self.audio.as_ref().is_some_and(|a| a.playing(active))
        {
            self.loaded_track(active)
                .and_then(|t| t.preparation.bpm)
                .zip(track.preparation.bpm)
                .map(|(outgoing, incoming)| {
                    outgoing * (1. + self.pitch[active] as f64 / 100.) / incoming
                })
                .filter(|ratio| ratio.is_finite() && (0.84..=1.16).contains(ratio))
        } else {
            None
        };
        let rate = matched_rate.unwrap_or(saved_rate) as f32;
        self.pitch[d] = (rate - 1.) * 100.;
        let engine = self.audio.as_mut().unwrap();
        engine.set_rate(d, rate);
        engine.load(d, &self.root, &track, execution == Execution::PlayNow)?;
        self.loaded_snapshot[d] = Some(track.clone());
        self.loaded_origin[d] = Some(self.root.clone());
        self.loaded[d] = Some(index);
        self.auto = false;
        match execution {
            Execution::LoadOnly => self.run.preloaded = Some((d, index)),
            Execution::PlayNow => {
                self.run.preloaded = None;
                self.run.pending = None;
                self.run.pending_at = None;
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
            let id = show::song_id(t);
            self.run.requests.retain(|s| *s != id);
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
        while let Some(id) = self.run.requests.front().cloned() {
            let index = self
                .library
                .as_ref()
                .and_then(|l| l.tracks.iter().position(|t| show::song_id(t) == id));
            if let Some(index) = index {
                match self.call_track(index, None, Deck::Auto, Execution::LoadOnly) {
                    Ok(()) => return Ok(()),
                    Err(e) => self.status = format!("Requested song unavailable: {e}"),
                }
            }
            self.run.requests.pop_front();
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
        let mut p = self
            .run
            .profile
            .as_ref()
            .ok_or("Load a Smart Playlist first")?
            .clone();
        p.excluded.extend(self.run.failed_songs.iter().cloned());
        let current = self.loaded[self.active_deck()];
        let recent: Vec<_> = self.run.recent.iter().cloned().collect();
        let tracks = &self.library.as_ref().ok_or("Library unavailable")?.tracks;
        let target = self
            .run
            .progression
            .as_ref()
            .map(|p| p.next())
            .or(self.run.target);
        while let Some(index) = self.run.future_candidates.pop_front() {
            if current != Some(index)
                && let Some(track) = tracks.get(index)
                && show::eligible(&p, track)
                && !recent
                    .iter()
                    .rev()
                    .take(p.recent_repeat)
                    .any(|id| *id == show::song_id(track))
            {
                return self.call_track(index, None, Deck::Auto, Execution::LoadOnly);
            }
        }
        loop {
            let tracks = &self.library.as_ref().ok_or("Library unavailable")?.tracks;
            let index = show::choose_with_snapshot(
                &p, tracks, current, &recent, target, alternate,
                self.loaded_track(self.active_deck()).map(|t| &t.preparation),
            ).ok_or("Auto Mix stopped: no playable next song matches this playlist's source and repeat rules")?;
            match self.call_track(index, None, Deck::Auto, Execution::LoadOnly) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    let track = &self.library.as_ref().unwrap().tracks[index];
                    let id = show::song_id(track);
                    self.status = format!("Skipping {}: {error}", track.title);
                    p.excluded.push(id.clone());
                    self.run.failed_songs.push(id);
                }
            }
        }
    }
    pub(super) fn recover_failed_song(&mut self, deck: usize, error: &str) {
        if let Some(track) = self.loaded_track(deck) {
            let id = show::song_id(track);
            self.status = format!("Skipping {}: {error}", track.title);
            if !self.run.failed_songs.contains(&id) {
                self.run.failed_songs.push(id);
            }
        } else {
            self.status = error.into();
        }
        if let Some(audio) = &mut self.audio {
            audio.stop(deck);
        }
        if self.run.preloaded.is_some_and(|(d, _)| d == deck) {
            self.run.preloaded = None;
            self.run.pending = None;
            self.run.pending_at = None;
            self.run.transition = None;
        } else if self.run.transition.is_some() {
            // An outgoing decoder failed: the prepared incoming deck can take over.
            self.run.pending = self.run.preloaded.map(|(d, _)| (d, Instant::now()));
            self.run.transition = None;
        }
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
                    | Action::CueDeck { .. }
                    | Action::SideSampler { .. }
                    | Action::SetCrossfader { .. }
                    | Action::AutoDj { enabled: false }
            )
        {
            return Err("Manual DJ Lock prevents automated takeover".into());
        }
        match action {
            Action::TransitionSettings {
                enabled,
                techniques,
                every,
                loop_build,
                stem_swap,
            } => {
                if !(2..=12).contains(&every) {
                    return Err("Effect frequency must be 2–12 mixes".into());
                }
                let settings = crate::transition_styles::Settings {
                    enabled,
                    techniques,
                    every,
                    style: crate::transition_styles::Style::Variety,
                    loop_build: loop_build.unwrap_or(self.run.transitions.loop_build),
                    stem_swap: stem_swap.unwrap_or(self.run.transitions.stem_swap),
                };
                crate::transition_styles::save(settings)?;
                self.run.transitions = settings;
                self.run.scratch_assist = enabled;
            }
            Action::RequestSong { song, within } => {
                if !(1..=5).contains(&within) {
                    return Err("Request window must be 1–5 songs".into());
                }
                if self.run.profile.is_none() {
                    return Err("Load a Smart Playlist before accepting requests".into());
                }
                let index = self.find_song(&song)?;
                let track = &self.library.as_ref().unwrap().tracks[index];
                let id = show::song_id(track);
                if self.run.requests.contains(&id) {
                    return Err("That song is already requested".into());
                }
                let slot = self.run.requests.len()
                    + 1
                    + usize::from(
                        self.run.preloaded.is_some()
                            || self.run.pending.is_some()
                            || self.run.transition.is_some(),
                    );
                if slot > within as usize {
                    return Err(
                        "That request window is full; choose a later slot or wait for the queue"
                            .into(),
                    );
                }
                self.run.requests.push_back(id.clone());
                if let Some(p) = &mut self.run.profile
                    && !p.allowed.contains(&id)
                {
                    p.allowed.push(id);
                }
                self.status = format!(
                    "Requested {} · plays within {slot} songs while Auto DJ runs",
                    track.title
                );
            }
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
                self.execute_control(Action::MixNext { immediate: true })?;
            }
            Action::MixNext { immediate } => {
                // Repeated requests must not restart or cancel the in-flight handoff.
                // Keep its incoming track until completion, then refill the freed deck.
                if self.run.transition.is_some() || self.audio.as_ref().is_some_and(|a| a.fading())
                {
                    self.status =
                        "Mix in progress; the next song will load after the handoff".into();
                    return Ok(());
                }
                self.preload_smart()?;
                let d = self.run.preloaded.unwrap().0;
                let active = self.active_deck();
                self.run.pending_at = if immediate {
                    None
                } else {
                    let a = self.audio.as_ref().ok_or("Audio engine is not running")?;
                    let position = a.source_position(active);
                    let p = &self
                        .loaded_track(active)
                        .ok_or("No active song")?
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
                if self.scan.is_some() && self.library.is_none() {
                    return Err("Your music library is still scanning. Wait for the tracks to appear before starting Auto DJ.".into());
                }

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
                if self.show_editor.playlist_unsaved(&p.id) {
                    return Err("This Smart Playlist has unsaved Library changes. Open it in Library and Save Playlist first.".into());
                }
                if !self.library.as_ref().is_some_and(|library| {
                    library.tracks.iter().any(|track| show::eligible(&p, track))
                }) {
                    return Err("This saved Smart Playlist has no matching songs. Choose its Music Source in Library, then Save Playlist.".into());
                }
                self.run.failed_songs.clear();
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
                // An explicit restart retries songs skipped by the previous run.
                if enabled {
                    self.run.failed_songs.clear();
                }
                // Do not claim ON until loading/starting the first song succeeds.
                self.run.auto = false;
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
                self.run.auto = enabled;
                if !enabled {
                    self.run.cancel_automation();
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
            Action::CueDeck { side, operation } => self.cue_command(side, operation)?,
            Action::SideSampler { side, pad } => {
                if side > 1 || !(1..=9).contains(&pad) {
                    return Err("Sampler side is 0/1 and pad is 1–9".into());
                }
                if side == 0 {
                    self.audio
                        .as_ref()
                        .ok_or("Load the left sampler first")?
                        .sample_trigger(pad - 1)?;
                } else {
                    self.right_sample_audio
                        .as_ref()
                        .ok_or("Load the right sampler first")?
                        .sample_trigger(pad - 1)?;
                }
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
        self.loaded_track(active)
            .and_then(|t| show::energy(&t.preparation))
    }
    pub(super) fn performance_tick(&mut self) {
        // Every temporary effect must release its transport when automation stops.
        if ((self.run.pending.is_none() && self.run.transition.is_none())
            || !self.run.scratch_assist
            || self.run.locked)
            && let Some(audio) = &self.audio
        {
            self.run.cancel_effects(audio);
        }

        if let Some(audio) = &self.audio
            && let Some(label) = self.run.tick_effect(audio)
        {
            self.status = label;
        }

        if self.run.auto
            && let Some((deck, _)) = self.run.preloaded
            && self.audio.as_ref().is_some_and(|a| {
                a.load_current(deck) && a.controls.finished[deck].load(Relaxed) && !a.playing(deck)
            })
        {
            self.recover_failed_song(
                deck,
                "No playable audio at the saved cue; trying another song",
            );
        }

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
                self.recover_failed_song(
                    incoming,
                    "Incoming deck did not become ready; trying another song",
                );
            } else if ready && due {
                let outgoing = 1 - incoming;
                let chosen =
                    if self.run.auto && !self.run.locked && self.run.effect_done != Some(started) {
                        self.run.transitions.pick(self.run.completed_handoffs)
                    } else {
                        None
                    };
                if chosen == Some(crate::transition_styles::Style::Beatmatch) {
                    self.sync_deck(incoming);
                }
                let flourish = chosen == Some(crate::transition_styles::Style::Scratch);
                let safe = self.audio.as_ref().is_some_and(|a| {
                    a.playing(outgoing)
                        && a.position(outgoing) > 0.3
                        && !a.controls.looping[outgoing].load(Relaxed)
                        && a.remaining(outgoing).is_none_or(|r| r > 0.9)
                });
                let beat = self
                    .loaded_track(outgoing)
                    .and_then(|t| t.preparation.bpm)
                    .filter(|bpm| bpm.is_finite() && (40. ..=240.).contains(bpm))
                    .map(|bpm| (60. / (bpm * (1. + self.pitch[outgoing] as f64 / 100.))) as f32);
                let effect_plan =
                    if safe && chosen == Some(crate::transition_styles::Style::LoopBuild) {
                        beat.filter(|beat| {
                            self.fade_seconds >= beat * 6.
                                && self.audio.as_ref().is_some_and(|a| {
                                    a.remaining(outgoing).is_some_and(|r| r > beat * 4. + 0.3)
                                })
                        })
                        .map(|beat| (crate::transition_styles::Style::LoopBuild, beat * 4.))
                    } else if safe && chosen == Some(crate::transition_styles::Style::StemSwap) {
                        self.audio
                            .as_ref()
                            .filter(|a| a.controls.stems_loaded.iter().all(|v| v.load(Relaxed)))
                            .map(|_| (crate::transition_styles::Style::StemSwap, 0.))
                    } else if safe
                        && matches!(
                            chosen,
                            Some(
                                crate::transition_styles::Style::Stutter
                                    | crate::transition_styles::Style::Brake
                                    | crate::transition_styles::Style::Filter
                                    | crate::transition_styles::Style::BassSwap
                            )
                        )
                    {
                        let bpm = self
                            .loaded_track(outgoing)
                            .and_then(|t| t.preparation.bpm)
                            .unwrap_or(120.)
                            * (1. + self.pitch[outgoing] as f64 / 100.);
                        Some((chosen.unwrap(), (60. / bpm / 4.).clamp(0.03, 0.25) as f32))
                    } else {
                        None
                    };
                let bridge_needed = self.run.scratch_assist
                    && !self.run.locked
                    && self.run.effect_done != Some(started)
                    && self.audio.as_ref().is_some_and(|a| {
                        a.playing(outgoing)
                            && a.position(outgoing) > 0.25
                            && !a.controls.looping[outgoing].load(Relaxed)
                    })
                    && (flourish
                        || (matches!(
                            self.run.transitions.style,
                            crate::transition_styles::Style::Variety
                                | crate::transition_styles::Style::Scratch
                        ) && self.run.transitions.techniques[0]
                            && chosen.is_none()
                            && self
                                .loaded_track(outgoing)
                                .and_then(|t| t.preparation.bpm)
                                .zip(self.loaded_track(incoming).and_then(|t| t.preparation.bpm))
                                .is_some_and(|(a, b)| {
                                    let ratio = a * (1. + self.pitch[outgoing] as f64 / 100.) / b;
                                    !(0.84..=1.16).contains(&ratio)
                                })));
                if bridge_needed || self.run.scratch_bridge.is_some() {
                    let (_, started) = *self
                        .run
                        .scratch_bridge
                        .get_or_insert((outgoing, Instant::now()));
                    let elapsed = started.elapsed();
                    let audio = self.audio.as_ref().unwrap();
                    let duration = if flourish { 360 } else { 180 };
                    if elapsed < Duration::from_millis(duration) {
                        audio.scratch(
                            outgoing,
                            true,
                            scratch_phrase(elapsed.as_secs_f32(), flourish),
                        );
                        self.status = "Scratch-assisted transition".into();
                        return;
                    }
                    audio.scratch(outgoing, false, 1.);
                    self.run.scratch_bridge = None;
                    self.run.effect_done = Some(started);
                }

                let a = self.audio.as_ref().unwrap();
                if !a.playing(1 - incoming)
                    || a.remaining(1 - incoming)
                        .is_some_and(|remaining| remaining < 0.15)
                {
                    // The outgoing song may end between the UI tick and fade request.
                    // A ready next song can take over without disabling continuous mixing.
                    a.controls.play[1 - incoming].store(false, Relaxed);
                    a.controls.play[incoming].store(true, Relaxed);
                    a.set_fader(incoming as f32);
                    self.complete_change(incoming);
                } else if incoming != 1 - usize::from(a.fader() >= 0.5) {
                    self.status =
                        "Crossfader changed after preload; request the next song again".into();
                    self.run.cancel_automation();
                } else {
                    let seconds = self.fade_seconds;
                    match a.crossfade(seconds) {
                        Ok(()) => {
                            self.run.transition = Some(incoming);
                            if let Some((style, grain)) = effect_plan {
                                self.run.effect_bridge = Some(EffectBridge {
                                    deck: outgoing,
                                    style,
                                    seconds,
                                    grain,
                                    engaged: false,
                                    original: 0.,
                                    original_eq: [[0.; 3]; 2],
                                    original_stems: [[1.; 3]; 2],
                                    start_progress: 0.,
                                    halves: 0,
                                });
                            }
                        }
                        Err(e) => {
                            self.status = format!("Waiting for a safe handoff: {e}");
                            // Keep the incoming track and retry next tick, including
                            // a direct handoff if the outgoing song finishes meanwhile.
                            return;
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
            let mix_seconds = self.fade_seconds;
            if self
                .audio
                .as_ref()
                .is_some_and(|a| a.playing(active) || a.controls.finished[active].load(Relaxed))
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
            if (finished
                || (self.run.crossfade
                    && remaining.is_some_and(|r| {
                        r <= mix_seconds + if self.run.scratch_assist { 0.8 } else { 0. }
                    })))
                && let Some((incoming, _)) = self.run.preloaded
            {
                self.run.pending_at = None;
                self.run.pending = Some((incoming, Instant::now()));
            }
        }
    }
    fn complete_change(&mut self, incoming: usize) {
        if let Some(audio) = &self.audio {
            self.run.cancel_effects(audio);
        }
        self.run.completed_handoffs = self.run.completed_handoffs.wrapping_add(1);
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
            let engine_loaded=a.is_some_and(|a|a.load_current(d)&&a.loaded(d));let t=self.loaded_track(d);
            let playing=engine_loaded&&a.is_some_and(|a|a.playing(d));
            json!({"deck":if d==0{"A"}else{"B"},"loaded":engine_loaded,"ready":engine_loaded&&a.is_some_and(|a|a.controls.ready[d].load(Relaxed)),"playing":playing,"state":if playing{"playing"}else if engine_loaded{"paused"}else{"stopped"},"song_id":if engine_loaded{t.map(show::song_id)}else{None},"title":if engine_loaded{t.map(|t|t.title.as_str())}else{None},"artist":if engine_loaded{t.and_then(|t|t.preparation.extensions.get("artist")).cloned()}else{None},"position":if engine_loaded{a.map(|a|a.source_position(d))}else{None},"remaining":if engine_loaded{a.and_then(|a|a.remaining(d))}else{None}})
        }).collect();
        let fading = a.is_some_and(|a| a.fading() || a.controls.level_fading.load(Relaxed));
        json!({"version":1,"amp":{"connected":true,"status":self.status,"playback_state":tracks[active]["state"],"current_song":tracks[active]["title"],"current_artist":tracks[active]["artist"],"current_cue":if tracks[active]["loaded"]==true{self.loaded_track(active).and_then(|t|a.and_then(|a|t.preparation.manual_cues.iter().chain(&t.preparation.cues).find(|c|(c.seconds-(a.source_position(active)-a.position(active))).abs()<0.001).map(|c|c.id.clone())))}else{None},"active_deck":if active==0{"A"}else{"B"},"next_song":self.run.preloaded.and_then(|(_,i)|self.library.as_ref()?.tracks.get(i)).map(|t|t.title.clone()),"remaining_time":tracks[active]["remaining"],"playlist":self.run.playlist,"mix_style":self.run.style,"target_energy":self.run.progression.as_ref().map(|p|p.target).or(self.run.planned_progression.as_ref().map(|p|p.target)).or(self.run.target),"current_energy":self.current_energy(),"progression_remaining":self.run.progression.as_ref().map_or(0,|p|p.remaining()),"auto_dj_enabled":self.run.auto,"auto_crossfade_enabled":self.run.crossfade,"fade_state":if fading{"running"}else{"idle"},"fade_progress":a.map_or(0.,|a|f32::from_bits(a.controls.fade_progress.load(Relaxed))),"master_level":a.map_or(self.volume as f32/100.,|a|a.master_level()),"manual_dj_locked":self.run.locked},"decks":tracks,"cue_decks":self.sides.iter().map(|side|side.snapshot()).collect::<Vec<_>>()})
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn stem_handoff_preserves_muted_parts_and_brings_in_vocals_before_backing() {
        let original = [[0., 0.7, 1.], [0.8, 0., 0.6]];
        assert_eq!(super::blend_stems(0., original)[0], original[0]);
        assert_eq!(super::blend_stems(0., original)[1], [0.; 3]);
        let middle = super::blend_stems(0.35, original);
        assert_eq!(middle[1], [0.8, 0., 0.]);
        assert_eq!(super::blend_stems(1., original), [[0.; 3], original[1]]);
        assert_eq!(super::buildup_halves(1.9, 2., 3.), 0);
        assert_eq!(super::buildup_halves(2., 2., 3.), 1);
        assert_eq!(super::buildup_halves(2.5, 2., 3.), 3);
        assert_eq!(super::buildup_halves(3., 2., 3.), 4);
    }
    #[test]
    fn eq_blend_swaps_bass_and_stages_the_other_bands_relative_to_user_eq() {
        let original = [[-2., 1., 3.], [2., -1., -3.]];
        let begin = super::blend_eq(0., original);
        assert_eq!(begin[0], original[0]);
        assert_eq!(begin[1][0], -22.);
        let middle = super::blend_eq(0.5, original);
        assert!((middle[0][0] + 14.).abs() < 0.0001);
        assert!((middle[1][0] + 10.).abs() < 0.0001);
        assert_eq!(middle[1][2], original[1][2]);
        assert!(middle[1][1] < original[1][1]);
        let end = super::blend_eq(1., original);
        assert_eq!(end[1], original[1]);
        assert_eq!(end[0][0], -24.);
        assert_eq!(super::blend_eq(-1., original), begin);
        assert_eq!(super::blend_eq(2., original), end);
    }
    #[test]
    fn requests_reserve_a_bounded_live_window_without_rewriting_saved_rules() {
        let mut player = Player {
            library: Some(amp_core::Library {
                tracks: (0..6)
                    .map(|i| amp_core::Track {
                        path: format!("{i}.wav").into(),
                        title: format!("Song {i}"),
                        preparation: Default::default(),
                    })
                    .collect(),
                tree: amp_core::Folder {
                    name: "music".into(),
                    folders: vec![],
                    tracks: vec![],
                },
            }),
            ..Default::default()
        };
        player.run.profile = Some(player.run.settings.playlists[0].clone());
        let saved = serde_json::to_value(&player.run.settings).unwrap();
        assert!(
            player
                .execute_control(Action::RequestSong {
                    song: "Song 0".into(),
                    within: 0
                })
                .is_err()
        );
        for i in 0..5 {
            player
                .execute_control(Action::RequestSong {
                    song: format!("Song {i}"),
                    within: 5,
                })
                .unwrap();
        }
        assert!(
            player
                .execute_control(Action::RequestSong {
                    song: "Song 5".into(),
                    within: 5
                })
                .is_err()
        );
        assert_eq!(player.run.requests.len(), 5);
        assert_eq!(serde_json::to_value(&player.run.settings).unwrap(), saved);
        player.record_song(0);
        assert_eq!(player.run.requests.len(), 4);
    }
    #[test]
    #[ignore = "native audio and AMP_WAVEFORM_FIXTURE; output muted"]
    fn stutter_and_brake_complete_and_release_all_temporary_controls() {
        use crate::transition_styles::Style as Fx;
        let dir = tempfile::tempdir().unwrap();
        let fixture = std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap();
        for i in 0..2 {
            let path = std::path::PathBuf::from(format!("song{i}.wav"));
            std::fs::copy(&fixture, dir.path().join(&path)).unwrap();
            amp_core::preparation::save(
                dir.path(),
                &path,
                &amp_core::Preparation {
                    cue_in: 0.25,
                    cue_out: Some(8.),
                    bpm: Some(120.),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        for style in [
            Fx::Stutter,
            Fx::Brake,
            Fx::Filter,
            Fx::BassSwap,
            Fx::LoopBuild,
        ] {
            let fade_seconds = if style == Fx::LoopBuild { 3.5 } else { 0.8 };
            let mut player = Player {
                root: dir.path().into(),
                library: Some(amp_core::scan(dir.path()).unwrap()),
                channel_gain: [0.; 2],
                fade_seconds,
                ..Default::default()
            };
            player
                .execute_control(Action::LoadSmartPlaylist {
                    playlist: "demo".into(),
                    execution: Execution::PlayNow,
                    style: Some(Style::Steady),
                    energy: None,
                })
                .unwrap();
            wait(&mut player, |p| {
                p.audio
                    .as_ref()
                    .is_some_and(|a| a.position(p.active_deck()) > 0.3)
            });
            player.run.transitions.style = style;
            player.run.transitions.loop_build = true;
            player.run.transitions.every = 2;
            player.run.completed_handoffs = 1;
            let original_eq = [[-2., 1., 3.], [2., -1., -3.]];
            for (deck, bands) in original_eq.iter().enumerate() {
                for (band, value) in bands.iter().enumerate() {
                    player.audio.as_ref().unwrap().set_eq(deck, band, *value);
                }
            }
            player
                .execute_control(Action::MixNext { immediate: true })
                .unwrap();
            wait(&mut player, |p| {
                p.run.effect_bridge.as_ref().is_some_and(|fx| fx.engaged)
            });
            wait(&mut player, |p| p.run.completed_handoffs == 2);
            assert!(player.run.auto);
            assert_eq!(player.fade_seconds, fade_seconds);
            assert!(player.run.effect_bridge.is_none());
            let audio = player.audio.as_ref().unwrap();
            for (deck, bands) in original_eq.iter().enumerate() {
                for (band, value) in bands.iter().enumerate() {
                    assert_eq!(
                        f32::from_bits(audio.controls.eq[deck][band].load(Relaxed)),
                        *value
                    );
                }
            }
            assert!(audio.controls.scratching.iter().all(|v| !v.load(Relaxed)));
            // Loop stop is an audio-thread command; allow its acknowledgement.
            wait(&mut player, |p| {
                p.audio
                    .as_ref()
                    .unwrap()
                    .controls
                    .looping
                    .iter()
                    .all(|v| !v.load(Relaxed))
            });
        }
    }
    #[test]
    #[ignore = "requires native audio and AMP_WAVEFORM_FIXTURE; output muted"]
    fn occasional_scratch_runs_at_matched_tempo_and_keeps_auto_dj_running() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap();
        for i in 0..2 {
            let path = std::path::PathBuf::from(format!("song{i}.wav"));
            std::fs::copy(&fixture, dir.path().join(&path)).unwrap();
            amp_core::preparation::save(
                dir.path(),
                &path,
                &amp_core::Preparation {
                    cue_in: 0.25,
                    cue_out: Some(4.),
                    bpm: Some(120.),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let mut player = Player {
            root: dir.path().into(),
            library: Some(amp_core::scan(dir.path()).unwrap()),
            channel_gain: [0.; 2],
            fade_seconds: 0.5,
            ..Default::default()
        };
        player
            .execute_control(Action::LoadSmartPlaylist {
                playlist: "demo".into(),
                execution: Execution::PlayNow,
                style: Some(Style::Steady),
                energy: None,
            })
            .unwrap();
        wait(&mut player, |p| {
            p.audio
                .as_ref()
                .is_some_and(|a| a.position(p.active_deck()) > 0.3)
        });
        player.run.completed_handoffs = 3;
        player
            .execute_control(Action::MixNext { immediate: true })
            .unwrap();
        wait(&mut player, |p| p.run.scratch_bridge.is_some());
        wait(&mut player, |p| p.run.completed_handoffs == 4);
        assert!(player.run.auto);
        assert_eq!(player.fade_seconds, 0.5);
        assert!(player.run.scratch_bridge.is_none());
        assert!(
            player
                .audio
                .as_ref()
                .unwrap()
                .controls
                .scratching
                .iter()
                .all(|v| !v.load(Relaxed))
        );
    }
    #[test]
    fn loading_empty_smart_source_preserves_the_current_session() {
        let mut player = Player {
            library: Some(amp_core::Library {
                tracks: Vec::new(),
                tree: amp_core::Folder {
                    name: String::new(),
                    folders: Vec::new(),
                    tracks: Vec::new(),
                },
            }),
            ..Default::default()
        };
        player.run.profile = Some(player.run.settings.playlists[0].clone());
        player.run.playlist = Some("walk-in".into());
        player.run.auto = true;
        let result = player.execute_control(Action::LoadSmartPlaylist {
            playlist: "demo".into(),
            execution: Execution::PlayNow,
            style: None,
            energy: None,
        });
        assert!(result.err().unwrap().contains("no matching songs"));
        assert_eq!(player.run.playlist.as_deref(), Some("walk-in"));
        assert!(player.run.auto);
        assert!(player.audio.is_none());
    }
    #[test]
    fn smart_load_reports_scan_in_progress_without_changing_the_session() {
        let (_sender, receiver) = std::sync::mpsc::channel();
        let mut player = Player {
            scan: Some(receiver),
            ..Default::default()
        };
        let result = player.execute_control(Action::LoadSmartPlaylist {
            playlist: "demo".into(),
            execution: Execution::PlayNow,
            style: None,
            energy: None,
        });
        assert!(result.unwrap_err().contains("still scanning"));
        assert!(!player.run.auto);
        assert!(player.run.profile.is_none());
        // A saved catalog remains usable while the background receiver is pending.
        player.library = Some(amp_core::Library {
            tracks: vec![],
            tree: amp_core::Folder {
                name: "cached".into(),
                folders: vec![],
                tracks: vec![],
            },
        });
        let error = player
            .execute_control(Action::LoadSmartPlaylist {
                playlist: "demo".into(),
                execution: Execution::PlayNow,
                style: None,
                energy: None,
            })
            .unwrap_err();
        assert!(!error.contains("still scanning"), "{error}");
        assert!(error.contains("no matching songs"), "{error}");
    }
    #[test]
    fn auto_mix_failed_start_stays_off_and_locked_stop_remains_available() {
        let mut player = Player::default();
        player.run.playlist = Some("test".into());
        player.run.profile = Some(show::Settings::default().playlists.remove(0));
        assert!(
            player
                .execute_control(Action::AutoDj { enabled: true })
                .is_err()
        );
        assert!(
            !player.run.auto,
            "A failed start must not display Auto Mix ON"
        );
        player.run.locked = true;
        assert!(
            player
                .execute_control(Action::AutoDj { enabled: true })
                .is_err()
        );
        assert!(!player.run.auto);
        player.run.auto = true;
        player
            .execute_control(Action::AutoDj { enabled: false })
            .unwrap();
        assert!(
            !player.run.auto,
            "Stopping automation remains available under manual lock"
        );
    }
    use super::*;
    #[test]
    #[ignore = "requires native audio and AMP_WAVEFORM_FIXTURE; output muted"]
    fn scratch_assist_bridges_large_tempo_gap_and_releases_on_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap();
        for (i, bpm) in [120., 174.].into_iter().enumerate() {
            let path = std::path::PathBuf::from(format!("song{i}.wav"));
            std::fs::copy(&fixture, dir.path().join(&path)).unwrap();
            amp_core::preparation::save(
                dir.path(),
                &path,
                &amp_core::Preparation {
                    cue_in: 0.25,
                    cue_out: Some(4.),
                    bpm: Some(bpm),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let mut player = Player {
            root: dir.path().into(),
            library: Some(amp_core::scan(dir.path()).unwrap()),
            channel_gain: [0.; 2],
            fade_seconds: 0.25,
            ..Default::default()
        };
        player
            .execute_control(Action::LoadSmartPlaylist {
                playlist: "demo".into(),
                execution: Execution::PlayNow,
                style: Some(Style::Steady),
                energy: None,
            })
            .unwrap();
        wait(&mut player, |p| p.run.scratch_bridge.is_some());
        assert!(player.audio.as_ref().unwrap().controls.scratching[0].load(Relaxed));
        player
            .execute_control(Action::AutoDj { enabled: false })
            .unwrap();
        player.performance_tick();
        assert!(!player.audio.as_ref().unwrap().controls.scratching[0].load(Relaxed));
        player
            .execute_control(Action::AutoDj { enabled: true })
            .unwrap();
        wait(&mut player, |p| {
            p.loaded[p.active_deck()] == Some(1) && p.run.transition.is_none()
        });
        assert!(player.run.auto);
        assert!(player.run.scratch_bridge.is_none());
        assert!(!player.audio.as_ref().unwrap().controls.scratching[0].load(Relaxed));
        assert!(player.audio.as_ref().unwrap().take_error().is_none());
    }
    #[test]
    #[ignore = "requires native audio and AMP_WAVEFORM_FIXTURE; output muted"]
    fn auto_mix_skips_broken_song_and_survives_late_handoff() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap();
        for i in 0..4 {
            let path = dir.path().join(format!("song{i}.wav"));
            if i == 1 {
                std::fs::write(&path, b"broken audio").unwrap();
            } else {
                std::fs::copy(&fixture, &path).unwrap();
            }
            amp_core::preparation::save(
                dir.path(),
                std::path::Path::new(&format!("song{i}.wav")),
                &amp_core::Preparation {
                    cue_in: 0.25,
                    cue_out: Some(2.),
                    bpm: Some(120.),
                    manual_override: true,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let mut player = Player {
            root: dir.path().into(),
            library: Some(amp_core::scan(dir.path()).unwrap()),
            channel_gain: [0.; 2],
            fade_seconds: 0.2,
            ..Default::default()
        };
        let before: Vec<_> = player
            .library
            .as_ref()
            .unwrap()
            .tracks
            .iter()
            .map(|t| amp_core::preparation::revision(&dir.path().join(&t.path)))
            .collect();
        player.run.crossfade = false;
        player
            .execute_control(Action::LoadSmartPlaylist {
                playlist: "demo".into(),
                execution: Execution::PlayNow,
                style: Some(Style::Steady),
                energy: None,
            })
            .unwrap();
        wait(&mut player, |p| {
            p.run.preloaded.is_some_and(|(d, i)| {
                i == 2 && p.audio.as_ref().unwrap().controls.ready[d].load(Relaxed)
            })
        });
        assert!(
            player.run.auto,
            "Bad incoming audio must not stop Auto Mix: {}",
            player.status
        );
        assert_eq!(player.run.failed_songs.len(), 1);
        // Let the UI request arrive too late for the old 100ms fade safety check.
        let active = player.active_deck();
        let until = Instant::now() + Duration::from_secs(5);
        while player.audio.as_ref().unwrap().remaining(active).unwrap() > 0.07 {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        }
        player.auto_crossfade();
        wait(&mut player, |p| {
            p.loaded[p.active_deck()] == Some(2)
                && p.run.pending.is_none()
                && p.run.transition.is_none()
        });
        assert!(
            player.run.auto,
            "Late handoff must not turn Auto Mix off: {}",
            player.status
        );
        player.run.crossfade = true;
        for _ in 0..5 {
            let previous = player.loaded[player.active_deck()];
            wait(&mut player, |p| {
                p.loaded[p.active_deck()] != previous && p.run.transition.is_none()
            });
            assert_ne!(player.loaded[player.active_deck()], Some(1));
            assert!(
                player.run.auto,
                "Stopped during continuous playback: {}",
                player.status
            );
        }
        for (i, t) in player.library.as_ref().unwrap().tracks.iter().enumerate() {
            assert_eq!(
                amp_core::preparation::revision(&dir.path().join(&t.path)),
                before[i]
            );
        }
    }
    #[test]
    #[ignore = "requires native audio and AMP_WAVEFORM_FIXTURE; output muted"]
    fn smart_auto_mix_advances_reorders_and_refills_without_commands() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap();
        for (i, energy) in [8., 8.1, 8.2, 3.].into_iter().enumerate() {
            let path = dir.path().join(format!("song{i}.wav"));
            std::fs::copy(&fixture, &path).unwrap();
            let mut prep = amp_core::Preparation {
                cue_in: 0.25,
                cue_out: Some(2.25),
                bpm: Some(120. + i as f64),
                harmonic_key: Some("8A".into()),
                manual_override: true,
                ..Default::default()
            };
            prep.extensions
                .insert("energy".into(), json!((energy - 1.) / 9.));
            amp_core::preparation::save(
                dir.path(),
                std::path::Path::new(&format!("song{i}.wav")),
                &prep,
            )
            .unwrap();
        }
        let library = amp_core::scan(dir.path()).unwrap();
        let before: Vec<_> = library
            .tracks
            .iter()
            .map(|t| amp_core::preparation::revision(&dir.path().join(&t.path)))
            .collect();
        let mut player = Player {
            root: dir.path().into(),
            library: Some(library),
            channel_gain: [0.; 2],
            fade_seconds: 0.2,
            ..Default::default()
        };
        player
            .execute_control(Action::LoadSmartPlaylist {
                playlist: "demo".into(),
                execution: Execution::PlayNow,
                style: Some(Style::Steady),
                energy: None,
            })
            .unwrap();
        assert!(
            player.run.preloaded.is_none(),
            "The starting voice is not the next voice"
        );
        wait(&mut player, |p| p.run.preloaded.is_some());
        assert_eq!(player.loaded[player.active_deck()], Some(0));
        let (incoming, next) = player.run.preloaded.unwrap();
        assert_ne!(incoming, player.active_deck());
        assert_eq!(
            next, 1,
            "Choose the nearest energy rather than playlist order"
        );
        assert!((player.pitch[incoming] - ((120. / 121. - 1.) * 100.) as f32).abs() < 0.01);
        wait(&mut player, |p| {
            p.loaded[p.active_deck()] == Some(1) && p.run.transition.is_none()
        });
        wait(&mut player, |p| p.run.preloaded.is_some());
        assert!(player.run.auto);
        assert_eq!(player.run.preloaded.unwrap().1, 2);
        // Changing the requested energy invalidates the old next choice, without stopping music.
        player
            .execute_control(Action::SetEnergy { target: 3. })
            .unwrap();
        wait(&mut player, |p| p.run.preloaded.is_some());
        assert_eq!(player.run.preloaded.unwrap().1, 3);
        wait(&mut player, |p| {
            p.loaded[p.active_deck()] == Some(3) && p.run.transition.is_none()
        });
        wait(&mut player, |p| p.run.preloaded.is_some());
        assert_eq!(
            player.run.preloaded.unwrap().1,
            2,
            "Refill the freed deck after another automatic transition"
        );
        assert!(player.run.auto);
        // The centre button uses the Smart Playlist handoff too, then keeps running.
        let next_deck = player.run.preloaded.unwrap().0;
        assert_eq!(
            player.loaded_track(next_deck).unwrap().preparation.cue_in,
            0.25
        );
        player.auto_crossfade();
        wait(&mut player, |p| p.run.transition.is_some());
        // Pressing the centre button again during the fade used to cancel Auto Mix.
        player.auto_crossfade();
        assert!(player.run.auto);
        assert!(player.run.transition.is_some());
        player
            .execute_control(Action::MixNext { immediate: true })
            .unwrap();
        assert!(
            player.run.pending.is_none(),
            "Do not schedule the same incoming voice twice"
        );
        wait(&mut player, |p| {
            p.loaded[p.active_deck()] == Some(2) && p.run.transition.is_none()
        });
        wait(&mut player, |p| p.run.preloaded.is_some());
        assert_eq!(player.run.preloaded.unwrap().0, 1 - player.active_deck());
        assert_ne!(player.run.preloaded.unwrap().1, 2);
        assert!(
            player.run.auto,
            "Mix Now must not disable continuous Auto Mix"
        );
        player
            .execute_control(Action::AutoDj { enabled: false })
            .unwrap();
        for (i, track) in player.library.as_ref().unwrap().tracks.iter().enumerate() {
            assert_eq!(
                amp_core::preparation::revision(&dir.path().join(&track.path)),
                before[i]
            );
        }
    }
    #[test]
    #[ignore = "requires native audio and AMP_WAVEFORM_FIXTURE; muted"]
    fn playing_deck_keeps_snapshot_until_explicit_reload() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.wav");
        std::fs::copy(std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap(), &audio).unwrap();
        let prep = amp_core::Preparation {
            cue_in: 1.,
            cue_out: Some(9.),
            bpm: Some(120.),
            manual_override: true,
            ..Default::default()
        };
        amp_core::preparation::save(dir.path(), std::path::Path::new("song.wav"), &prep).unwrap();
        let mut player = Player {
            root: dir.path().into(),
            library: Some(amp_core::scan(dir.path()).unwrap()),
            channel_gain: [0.; 2],
            ..Default::default()
        };
        player
            .execute_control(Action::CallSong {
                song: "song".into(),
                cue: None,
                deck: Deck::A,
                execution: Execution::PlayNow,
            })
            .unwrap();
        wait(&mut player, |p| {
            p.audio.as_ref().unwrap().load_current(0)
                && p.audio.as_ref().unwrap().controls.ready[0].load(Relaxed)
        });
        let mut correction = amp_core::preparation::load(&audio).unwrap();
        correction.cue_in = 3.;
        correction.bpm = Some(126.);
        amp_core::preparation::save(dir.path(), std::path::Path::new("song.wav"), &correction)
            .unwrap();
        player.accept_library_scan(amp_core::scan(dir.path()).unwrap());
        player.performance_tick();
        assert_eq!(player.loaded_track(0).unwrap().preparation.bpm, Some(120.));
        let a = player.audio.as_ref().unwrap();
        assert!(a.playing(0));
        assert!((a.source_position(0) - a.position(0) - 1.).abs() < 0.001);
        player.load_deck(0, 0, false);
        wait(&mut player, |p| {
            p.audio.as_ref().unwrap().load_current(0)
                && p.audio.as_ref().unwrap().controls.ready[0].load(Relaxed)
        });
        assert_eq!(player.loaded_track(0).unwrap().preparation.bpm, Some(126.));
        let a = player.audio.as_ref().unwrap();
        assert!((a.source_position(0) - a.position(0) - 3.).abs() < 0.001);
    }
    fn wait(player: &mut Player, check: impl Fn(&Player) -> bool) {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            player.poll();
            player.performance_tick();
            if check(player) {
                break;
            }
            assert!(
                Instant::now() < until,
                "Timed out: {} {}",
                player.status,
                player.control_snapshot()
            );
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
            .find(|p| p.id == "demo")
            .unwrap()
            .recent_repeat = 0;
        player
            .execute_control(Action::LoadSmartPlaylist {
                playlist: "demo".into(),
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
