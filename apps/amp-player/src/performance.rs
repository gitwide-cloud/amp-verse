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
pub(super) fn regular_playlist_id(path: &std::path::Path) -> String {
    use sha2::Digest;
    format!(
        "regular_{:x}",
        sha2::Sha256::digest(path.to_string_lossy().as_bytes())
    )
}
/// Automatic target matching may choose an early section, never an outro.
/// Explicit cue/manual selection is handled separately by call_track.
fn automatic_energy_entry(p: &amp_core::Preparation, target: f64, minimum: f64) -> Option<f64> {
    amp_core::energy_profile::automatic_entry(p, target, minimum).map(|entry| entry.seconds)
}
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
#[cfg(test)]
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
#[cfg(test)]
fn blend_stems(progress: f32, original: [[f32; 3]; 2]) -> [[f32; 3]; 2] {
    let masks = amp_core::transition_plan::stem_handoff(
        (progress.clamp(0., 1.) * 10000.) as u64,
        10000,
        400,
    );
    std::array::from_fn(|d| std::array::from_fn(|stem| original[d][stem] * masks[d][stem]))
}

fn buildup_halves(elapsed: f32, capture: f32, duration: f32) -> u8 {
    if elapsed < capture || duration <= capture {
        return 0;
    }
    (1 + ((elapsed - capture) / (duration - capture) * 4.) as u8).min(4)
}
struct RegularReservation {
    deck: usize,
    slot: usize,
    index: usize,
    queue: Vec<usize>,
    started: Instant,
}
struct RegularRun {
    unstarted: bool,
    reservation: Option<RegularReservation>,
}
pub struct Run {
    pub failure: Option<String>,
    recognition_context: Option<String>,
    pub requests: VecDeque<String>,
    pub force_mashup: bool,
    pub transitions: crate::transition_styles::Settings,
    effect_bridge: Option<EffectBridge>,
    effect_done: Option<Instant>,
    pub choice: String,
    pub transition_trace: Option<amp_core::transition_plan::TransitionDecision>,
    pub profile: Option<show::SmartPlaylist>,
    regular: Option<RegularRun>,
    pub settings: show::Settings,
    pub playlist: Option<String>,
    pub style: Style,
    pub target: Option<f64>,
    pub auto: bool,
    pub paused: bool,
    pub crossfade: bool,
    pub scratch_assist: bool,
    scratch_bridge: Option<(usize, Instant)>,
    completed_handoffs: u32,
    pub future_candidates: VecDeque<usize>,
    failed_songs: Vec<String>,
    smart_cycle: amp_core::smart_cycle::SmartCycle,
    smart_audible: Option<(usize, usize, u64)>,
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
            failure: None,
            recognition_context: None,
            requests: VecDeque::new(),
            force_mashup: false,
            transitions,
            effect_bridge: None,
            effect_done: None,
            choice: String::new(),
            transition_trace: None,
            profile: None,
            regular: None,
            settings: Default::default(),
            playlist: None,
            style: Style::Steady,
            target: None,
            auto: false,
            paused: false,
            crossfade: true,
            scratch_assist: transitions.enabled,
            scratch_bridge: None,
            completed_handoffs: 0,
            future_candidates: VecDeque::new(),
            failed_songs: Vec::new(),
            smart_cycle: Default::default(),
            smart_audible: None,
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
    pub(super) fn has_regular_playlist(&self) -> bool {
        self.profile.is_none() && self.regular.is_some()
    }
    pub(super) fn load_regular_playlist(&mut self, name: String) {
        self.cancel_automation();
        self.profile = None;
        self.playlist = Some(name);
        self.requests.clear();
        self.failed_songs.clear();
        self.smart_cycle = Default::default();
        self.smart_audible = None;
        self.progression = None;
        self.planned_progression = None;
        self.regular = Some(RegularRun {
            unstarted: true,
            reservation: None,
        });
    }
    pub(super) fn regular_deck_replaced(&mut self, deck: usize) {
        if let Some(regular) = &mut self.regular
            && regular
                .reservation
                .as_ref()
                .is_some_and(|reservation| reservation.deck == deck)
        {
            regular.reservation = None;
            // An explicit source replacement is not an implicit start of the queued
            // first occurrence, even when it happens to name the same song.
            regular.unstarted = false;
        }
    }
    pub(super) fn recent_songs(&self) -> Vec<String> {
        self.recent.iter().cloned().collect()
    }
    fn tick_effect(&mut self, audio: &crate::mixer::Engine) -> Option<String> {
        let mut effect = self.effect_bridge.take()?;
        if !audio.fading() || !audio.playing(audio.controls.other(effect.deck)) {
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
        if matches!(
            effect.style,
            crate::transition_styles::Style::StemSwap | crate::transition_styles::Style::BassSwap
        ) {
            // Session envelopes are evaluated by Mixer from the audio frame clock.
            let label = effect.style.label().to_string();
            self.effect_bridge = Some(effect);
            return Some(label);
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
            } else if effect.style == crate::transition_styles::Style::Filter {
                audio.controls.cutoff[effect.deck].store(effect.original.to_bits(), Relaxed);
            } else {
                audio.scratch(effect.deck, false, 1.);
            }
        }
        if let Some((deck, _)) = self.scratch_bridge.take() {
            audio.scratch(deck, false, 1.);
        }
    }
    /// Prepare without consuming bridges: a full audio queue must leave the
    /// automatic performance recoverable and its owned effects identifiable.
    pub fn live_takeover_cleanup(&self) -> crate::mixer::LiveTakeoverCleanup {
        use crate::transition_styles::Style;
        let mut cleanup = crate::mixer::LiveTakeoverCleanup::default();
        if let Some(effect) = &self.effect_bridge
            && effect.engaged
        {
            match effect.style {
                Style::Stutter | Style::LoopBuild => cleanup.auto_loop_deck = Some(effect.deck),
                Style::Filter => cleanup.restore_cutoff = Some((effect.deck, effect.original)),
                _ => cleanup.scratch_decks |= 1 << effect.deck,
            }
        }
        if let Some((deck, _)) = self.scratch_bridge {
            cleanup.scratch_decks |= 1 << deck;
        }
        cleanup
    }
    /// Called only after the single takeover command was accepted.
    pub fn finish_live_takeover(&mut self) {
        self.effect_bridge = None;
        self.scratch_bridge = None;
        self.cancel_automation();
    }
    pub fn cancel_automation(&mut self) {
        self.paused = false;
        self.force_mashup = false;
        self.future_candidates.clear();
        self.auto = false;
        self.pending = None;
        self.pending_at = None;
        self.transition = None;
        self.preloaded = None;
        if let Some(regular) = &mut self.regular
            && !regular.unstarted
        {
            regular.reservation = None;
        }
    }
}
struct EffectBridge {
    deck: usize,
    style: crate::transition_styles::Style,
    seconds: f32,
    grain: f32,
    engaged: bool,
    original: f32,
    start_progress: f32,
    halves: u8,
}
impl Player {
    pub(super) fn active_deck(&self) -> usize {
        let pair = self.pair();
        if let Some(a) = &self.audio {
            match [a.playing(pair[0]), a.playing(pair[1])] {
                [true, false] => pair[0],
                [false, true] => pair[1],
                [true, true] => pair[usize::from(a.fader() >= 0.5)],
                _ => {
                    if pair.contains(&self.run.active) {
                        self.run.active
                    } else {
                        pair[0]
                    }
                }
            }
        } else {
            pair[0]
        }
    }
    pub(super) fn requested_deck(&self, deck: Deck, incoming: bool) -> usize {
        match deck {
            Deck::A => self.rack.live[0],
            Deck::B => self.rack.live[1],
            Deck::A1 => 0,
            Deck::B1 => 1,
            Deck::A2 => 2,
            Deck::B2 => 3,
            Deck::A3 => 4,
            Deck::B3 => 5,
            Deck::Auto => {
                if self.playback_owner == crate::playback_owner::Owner::Player {
                    return 0;
                }
                let a = self.active_deck();
                if incoming && self.audio.as_ref().is_some_and(|e| e.playing(a)) {
                    self.other_deck(a)
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
            .filter(|(_, t)| {
                show::song_id(t) == song
                    || crate::resources::stable_song_id(t) == song
                    || t.title.eq_ignore_ascii_case(song)
            })
            .map(|(i, _)| i)
            .collect();
        match matches.as_slice() {
            [i] => Ok(*i),
            [i, ..] if song.starts_with("audio_") => Ok(*i),
            [] => Err("Song was not found".into()),
            _ => Err("Song title is ambiguous; use its song ID".into()),
        }
    }
    fn cue_seconds(&self, index: usize, cue: &str) -> Result<f64, String> {
        let track = self.fresh_track(index)?;
        let p = &track.preparation;
        if let Some(id) = cue.strip_prefix("energy-section:")
            && let Some(profile) = amp_core::energy_profile::read(p)
            && let Some(section) = profile.sections.iter().find(|s| s.id == id)
        {
            return Ok(section.start);
        }
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
    fn match_auto_pair(&mut self, outgoing: usize, incoming: usize) {
        let matched = self
            .loaded_track(outgoing)
            .zip(self.loaded_track(incoming))
            .and_then(|(a, b)| {
                use amp_core::transition_plan::*;
                plan_transition(
                    timing_snapshot(&a.preparation, false),
                    timing_snapshot(&b.preparation, false),
                    RequestedTiming::Tempo,
                    Handoff::Fade,
                    self.deck_rate(outgoing),
                    0.16,
                    3.,
                    f64::NAN,
                )
                .tempo
            });
        if let Some(plan) = matched
            && let Some(audio) = &self.audio
            && audio.playing(outgoing)
            && !audio.playing(incoming)
        {
            audio.controls.synced[incoming].store(false, Relaxed);
            audio.set_rate(incoming, plan.rate as f32);
            if audio.controls.synced[outgoing].load(Relaxed) && plan.pulse_factor == 1. {
                let _ = audio.sync(incoming, true);
            }
            if audio.controls.key_lock_available[incoming].load(Relaxed) {
                audio.controls.key_lock[incoming].store(true, Relaxed);
            }
            self.pitch[incoming] = (plan.rate as f32 - 1.) * 100.;
        }
    }
    fn build_drop_pair_plan(
        &self,
        outgoing: usize,
        incoming: usize,
    ) -> Option<(crate::mix_schedule::Grid, f32)> {
        let audio = self.audio.as_ref()?;
        let out = &self.loaded_track(outgoing)?.preparation;
        let next = &self.loaded_track(incoming)?.preparation;
        if next
            .extensions
            .get("runtime_build_swap")
            .and_then(|v| v.as_bool())
            != Some(true)
        {
            return None;
        }
        let rate_out = self.deck_rate(outgoing);
        let rate_in = self.deck_rate(incoming);
        let locked = |deck: usize| {
            audio.controls.key_lock[deck].load(Relaxed)
                && audio.controls.key_lock_available[deck].load(Relaxed)
        };
        if !crate::mashup::compatible_with_keylock(
            out,
            next,
            rate_out,
            rate_in,
            [locked(outgoing), locked(incoming)],
        ) {
            return None;
        }
        let plan = amp_core::transition_plan::build_swap(
            out,
            next,
            audio.source_position(outgoing),
            audio.source_position(incoming),
            next.extensions.get("runtime_mix_landing")?.as_f64()?,
            [rate_out, rate_in],
        )?;
        let origin = audio.source_position(outgoing) - audio.position(outgoing);
        Some((
            crate::mix_schedule::Grid {
                period: plan.seconds * rate_out,
                anchor: plan.outgoing_start - origin,
            },
            plan.seconds as f32,
        ))
    }
    fn mashup_pair_plan(
        &self,
        outgoing: usize,
        incoming: usize,
    ) -> Option<(crate::mix_schedule::Grid, f32)> {
        let audio = self.audio.as_ref()?;
        if !audio
            .controls
            .pair_indices()
            .iter()
            .all(|d| audio.controls.stems_loaded[*d].load(Relaxed))
        {
            return None;
        }
        let out = &self.loaded_track(outgoing)?.preparation;
        let next = &self.loaded_track(incoming)?.preparation;
        let out_rate = self.deck_rate(outgoing);
        let in_rate = self.deck_rate(incoming);
        let locked = |deck: usize| {
            audio.controls.key_lock[deck].load(Relaxed)
                && audio.controls.key_lock_available[deck].load(Relaxed)
        };
        if !crate::mashup::compatible_with_keylock(
            out,
            next,
            out_rate,
            in_rate,
            [locked(outgoing), locked(incoming)],
        ) {
            return None;
        }
        let timing = if self.run.transitions.timing.phrase_beats().is_some() {
            self.run.transitions.timing
        } else {
            crate::mix_schedule::Timing::Phrase16
        };
        let grid = crate::mix_schedule::safe_grid_at_rates(
            out,
            next,
            audio.source_position(incoming),
            audio.source_position(outgoing) - audio.position(outgoing),
            timing,
            out_rate,
            in_rate,
        )?;
        let hold = next
            .extensions
            .get("runtime_mix_landing")
            .and_then(|v| v.as_f64())
            .map(|landing| {
                (landing - audio.source_position(incoming)) / in_rate
                    - 2. * self.fade_seconds as f64
            })
            .unwrap_or(self.run.transitions.mashup_beats as f64 * 60. / (out.bpm? * out_rate));
        let profile = next
            .extensions
            .get("vocal_profile")
            .and_then(|v| {
                serde_json::from_value::<amp_core::vocal_profile::VocalProfile>(v.clone()).ok()
            })
            .or_else(|| {
                amp_core::resolve(&self.root, &self.loaded_track(incoming)?.path)
                    .ok()
                    .and_then(|path| crate::stems::vocal_profile(&path))
            })?;
        if !profile.active(
            audio.source_position(incoming) + self.fade_seconds as f64 * in_rate,
            hold * in_rate,
            0.25,
        ) {
            return None;
        }
        (hold.is_finite() && (0.1..=120.).contains(&hold)).then_some((grid, hold as f32))
    }
    fn call_track(
        &mut self,
        index: usize,
        cue: Option<&str>,
        deck: Deck,
        execution: Execution,
    ) -> Result<(), String> {
        let d = if deck == Deck::Auto {
            let active = self.active_deck();
            if self.audio.as_ref().is_some_and(|a| a.playing(active)) {
                self.other_deck(active)
            } else {
                active
            }
        } else {
            self.requested_deck(deck, true)
        };
        let active = self.active_deck();
        let paired = self.pair().contains(&d);
        if !paired && execution == Execution::AutoFade {
            return Err("Transitions target the assigned crossfade pair; use Play Now for a background layer".into());
        }
        if execution == Execution::LoadOnly && self.audio.as_ref().is_some_and(|a| a.playing(d)) {
            return Err("Load Only cannot replace a playing deck; select the idle deck".into());
        }
        if execution == Execution::AutoFade
            && self.audio.as_ref().is_some_and(|a| a.playing(active))
            && d != self.pair()[1 - usize::from(self.audio.as_ref().unwrap().fader() >= 0.5)]
        {
            return Err("Auto Fade requires the opposite deck".into());
        }
        let mut track = self.fresh_track(index)?;
        let mut automatic_section = false;
        if let Some(cue) = cue {
            track.preparation.cue_in = self.cue_seconds(index, cue)?;
        } else if paired
            && self.run.style == Style::Hype
            && let Some(p) = self.run.profile.as_ref()
            && p.use_manual_cues
            && let Some(seconds) = amp_core::hype_sections::marked_entry(&track.preparation)
        {
            track.preparation.cue_in = seconds;
        } else if paired
            && self.run.style == Style::Hype
            && self.run.profile.is_some()
            && let Some(seconds) = amp_core::hype_sections::entry(&track.preparation)
        {
            track.preparation.cue_in = seconds;
            automatic_section = true;
        } else if paired
            && self.run.style != Style::Hype
            && self.run.profile.is_some()
            && let Some(target) = self
                .run
                .progression
                .as_ref()
                .map(|p| p.next())
                .or(self.run.target)
                .map(|e| ((e - 1.) / 9.).clamp(0., 1.))
                .or_else(|| self.exit_energy())
            && let Some(seconds) = automatic_energy_entry(
                &track.preparation,
                target,
                (self.fade_seconds as f64 * 2.).max(8.),
            )
        {
            track.preparation.cue_in = seconds;
            automatic_section = true;
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
        let matched_rate = if paired
            && (self.run.profile.is_some() || execution == Execution::AutoFade)
            && d != active
            && self.audio.as_ref().is_some_and(|a| a.playing(active))
        {
            self.loaded_track(active)
                .filter(|t| {
                    amp_core::mix_schedule::timing_reliable(&t.preparation)
                        && amp_core::mix_schedule::timing_reliable(&track.preparation)
                })
                .and_then(|t| t.preparation.bpm)
                .zip(track.preparation.bpm)
                .and_then(|(outgoing, incoming)| {
                    amp_core::transition_plan::tempo_match(
                        outgoing,
                        self.deck_rate(active),
                        incoming,
                        0.16,
                    )
                    .map(|m| m.rate)
                })
        } else {
            None
        };
        let rate = matched_rate.unwrap_or(saved_rate) as f32;
        let planned_type = self.loaded_track(active).and_then(|out| {
            self.run.transitions.pick_for_pair(
                self.run.completed_handoffs,
                &out.preparation,
                &track.preparation,
            )
        });
        let planned_mashup =
            self.run.force_mashup || planned_type == Some(crate::transition_styles::Style::Mashup);
        let planned_build = planned_type == Some(crate::transition_styles::Style::BassSwap);
        let preroll = if planned_mashup {
            self.loaded_track(active)
                .and_then(|out| out.preparation.bpm)
                .map(|bpm| {
                    2. * self.fade_seconds as f64
                        + self.run.transitions.mashup_beats as f64 * 60.
                            / (bpm * self.deck_rate(active))
                })
                .unwrap_or(self.fade_seconds as f64)
        } else if planned_build {
            self.loaded_track(active)
                .and_then(|out| out.preparation.bpm)
                .map(|bpm| 16. * 60. / (bpm * self.deck_rate(active)))
                .unwrap_or(self.fade_seconds as f64)
        } else {
            self.fade_seconds as f64
        };
        if automatic_section
            && cue.is_none()
            && paired
            && d != active
            && (self.run.auto || execution == Execution::AutoFade)
            && let Some(plan) = amp_core::transition_plan::landing(
                &track.preparation,
                track.preparation.cue_in,
                rate as f64,
                preroll,
            )
        {
            track.preparation.cue_in = plan.entry;
            if planned_build {
                track
                    .preparation
                    .extensions
                    .insert("runtime_build_swap".into(), json!(true));
            }
            track.preparation.extensions.insert(
                "runtime_mix_landing".into(),
                serde_json::json!(plan.landing),
            );
        }
        let engine = self.audio.as_mut().unwrap();
        if self.run.auto || self.auto {
            engine.load_automix(d, &self.root, &track, execution == Execution::PlayNow)?;
        } else {
            engine.load(d, &self.root, &track, execution == Execution::PlayNow)?;
        }
        self.run.regular_deck_replaced(d);
        self.audio.as_ref().unwrap().set_rate(d, rate);
        if execution != Execution::LoadOnly {
            self.claim_playback(
                crate::playback_owner::Owner::Dj,
                d,
                execution == Execution::AutoFade,
            );
        }
        let engine = self.audio.as_mut().unwrap();
        self.pitch[d] = (rate - 1.) * 100.;
        self.loaded_snapshot[d] = Some(track.clone());
        self.loaded_origin[d] = Some(self.root.clone());
        self.loaded[d] = Some(index);
        if !paired {
            return Ok(());
        }
        self.auto = false;
        match execution {
            Execution::LoadOnly => self.run.preloaded = Some((d, index)),
            Execution::PlayNow => {
                self.run.preloaded = None;
                self.run.pending = None;
                self.run.pending_at = None;
                engine.set_fader((d % 2) as f32);
                if engine.playing(engine.controls.other(d)) {
                    engine.controls.play[engine.controls.other(d)].store(false, Relaxed);
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
    fn commit_smart_audible(&mut self) {
        let Some(profile) = self.run.profile.as_ref() else {
            return;
        };
        if self.playback_owner != crate::playback_owner::Owner::Dj {
            return;
        }
        let deck = self.active_deck();
        let pair = self.pair();
        if self.audio.as_ref().is_some_and(|a| {
            (deck == pair[0] && a.fader() >= 1. - 1e-4) || (deck == pair[1] && a.fader() <= 1e-4)
        }) {
            return;
        }
        if self.run.preloaded.is_some_and(|(d, _)| d == deck) {
            return;
        }
        if !self.audio.as_ref().is_some_and(|a| {
            a.load_current(deck)
                && a.loaded(deck)
                && a.controls.ready[deck].load(Relaxed)
                && a.playing(deck)
                && a.controls.position[deck].load(Relaxed) > 0
                && !a.controls.phones_only[deck].load(Relaxed)
        }) {
            return;
        }
        if self.loaded_origin[deck].as_deref() != Some(self.root.as_path()) {
            return;
        }
        if let Some(index) = self.loaded[deck] {
            let generation =
                self.audio.as_ref().unwrap().controls.deck_generation[deck].load(Relaxed);
            let event = (deck, index, generation);
            if self.run.smart_audible == Some(event) {
                return;
            }
            if let Some(track) = self.library.as_ref().and_then(|l| l.tracks.get(index)) {
                self.run
                    .smart_cycle
                    .played(&self.root.to_string_lossy(), &profile.id, track);
                self.run.smart_audible = Some(event);
            }
        }
    }
    fn preload_smart(&mut self) -> Result<(), String> {
        self.preload_variant(0)
    }
    fn preload_variant(&mut self, alternate: usize) -> Result<(), String> {
        self.commit_smart_audible();
        if self.cancel_changed_regular_queue() {
            return Err(self.status.clone());
        }
        if self.run.preloaded.is_some() {
            return Ok(());
        }
        while let Some(id) = self.run.requests.front().cloned() {
            let index = self
                .library
                .as_ref()
                .and_then(|l| l.tracks.iter().position(|t| show::song_id(t) == id))
                .filter(|index| {
                    self.run.profile.as_ref().is_none_or(|p| {
                        let track = &self.library.as_ref().unwrap().tracks[*index];
                        !p.excluded.contains(&id)
                            && p.filters.iter().all(|filter| filter.matches(track))
                            && (p.genres.is_empty()
                                || amp_core::genres::matches(&p.genres, &track.preparation))
                    })
                });
            if let Some(index) = index {
                match self.call_track(index, None, Deck::Auto, Execution::LoadOnly) {
                    Ok(()) => return Ok(()),
                    Err(e) => self.status = format!("Requested song unavailable: {e}"),
                }
            }
            self.run.requests.pop_front();
        }
        if self.run.has_regular_playlist() {
            return self.preload_regular(alternate);
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
        p.style = self.run.style;
        p.excluded.extend(self.run.failed_songs.iter().cloned());
        let current = self.loaded[self.active_deck()];
        let tracks = &self.library.as_ref().ok_or("Library unavailable")?.tracks;
        let target = self
            .run
            .progression
            .as_ref()
            .map(|p| p.next())
            .or(self.run.target);
        let cycle_candidates = self.run.smart_cycle.ranked(
            &self.root.to_string_lossy(),
            &p,
            tracks,
            current,
            target,
            None,
        );
        while let Some(index) = self.run.future_candidates.pop_front() {
            if cycle_candidates.contains(&index) {
                return self.call_track(index, None, Deck::Auto, Execution::LoadOnly);
            }
        }
        loop {
            let tracks = &self.library.as_ref().ok_or("Library unavailable")?.tracks;
            let mut snapshot = self
                .loaded_track(self.active_deck())
                .map(|t| t.preparation.clone());
            if let (Some(p), Some(energy)) = (&mut snapshot, self.exit_energy()) {
                p.extensions
                    .insert("energy".into(), serde_json::json!(energy));
            }
            if let (Some(snapshot), Some(audio)) = (&mut snapshot, &self.audio) {
                let active = self.active_deck();
                let available = audio.controls.key_lock_available[active].load(Relaxed);
                snapshot
                    .extensions
                    .insert("runtime_rank_rate".into(), json!(self.deck_rate(active)));
                snapshot.extensions.insert(
                    "runtime_rank_key_lock".into(),
                    json!(available && audio.controls.key_lock[active].load(Relaxed)),
                );
                snapshot
                    .extensions
                    .insert("runtime_rank_next_key_lock".into(), json!(available));
            }
            let ranked = self.run.smart_cycle.ranked(
                &self.root.to_string_lossy(),
                &p,
                tracks,
                current,
                target,
                snapshot.as_ref(),
            );
            let mut index=*ranked.get(alternate.min(ranked.len().saturating_sub(1))).ok_or("Auto Mix stopped: no playable next song matches this playlist's source and repeat rules")?;
            let wants_mashup = self.run.force_mashup
                || self.run.transitions.pick(self.run.completed_handoffs)
                    == Some(crate::transition_styles::Style::Mashup);
            if wants_mashup && let Some(out) = self.loaded_track(self.active_deck()) {
                let rate = self.deck_rate(self.active_deck());
                // Prefer a prepared compatible pair among the top energy-ranked candidates;
                // do not change eligibility, requests, or force a distant-energy song.
                if let Some(best) = ranked.iter().take(12).copied().find(|i| {
                    let next = &tracks[*i];
                    let in_rate =
                        out.preparation
                            .bpm
                            .zip(next.preparation.bpm)
                            .map_or(1., |(a, b)| {
                                amp_core::transition_plan::tempo_match(a, rate, b, 0.16)
                                    .map_or(1., |m| m.rate)
                            });
                    crate::mashup::compatible_with_keylock(
                        &out.preparation,
                        &next.preparation,
                        rate,
                        in_rate,
                        self.audio.as_ref().map_or([false, false], |audio| {
                            [
                                audio.controls.key_lock[self.active_deck()].load(Relaxed)
                                    && audio.controls.key_lock_available[self.active_deck()]
                                        .load(Relaxed),
                                audio.controls.key_lock[self.other_deck(self.active_deck())]
                                    .load(Relaxed),
                            ]
                        }),
                    ) && crate::stems::ready(&amp_core::music_sources::audio_path(
                        &self.root, &next.path,
                    ))
                    .is_some()
                }) {
                    index = best;
                }
            }
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
    fn cancel_changed_regular_queue(&mut self) -> bool {
        let stale = self
            .run
            .regular
            .as_ref()
            .and_then(|r| r.reservation.as_ref())
            .is_some_and(|r| r.queue != self.queue);
        if !stale {
            return false;
        }
        if let Some((deck, _)) = self.run.preloaded
            && let Some(audio) = &mut self.audio
        {
            let outgoing = audio.controls.other(deck);
            if audio.playing(outgoing) {
                audio.set_fader((outgoing % 2) as f32);
                audio.stop(deck);
            } else if !audio.playing(deck) {
                audio.stop(deck);
            }
        }
        self.run.cancel_automation();
        if let Some(regular) = &mut self.run.regular {
            regular.reservation = None;
        }
        self.status =
            "Playlist queue changed; Auto Mix stopped. Turn it on again to use the updated order."
                .into();
        self.run.failure = Some(self.status.clone());
        true
    }
    fn start_reserved_regular(&mut self, deck: usize, index: usize) -> Result<(), String> {
        let reservation = self.run.regular.as_mut().and_then(|regular| {
            if regular
                .reservation
                .as_ref()
                .is_some_and(|r| r.deck == deck && r.index == index)
            {
                regular.reservation.take()
            } else {
                None
            }
        });
        let result = self.call_track(index, None, Deck::PHYSICAL[deck], Execution::PlayNow);
        if let Some(mut reservation) = reservation
            && let Some(regular) = &mut self.run.regular
        {
            if result.is_ok() {
                reservation.started = Instant::now();
            }
            regular.reservation = Some(reservation);
        }
        result
    }
    fn reserve_regular(&mut self, deck: usize, slot: usize, index: usize) {
        if let Some(regular) = &mut self.run.regular {
            regular.reservation = Some(RegularReservation {
                deck,
                slot,
                index,
                queue: self.queue.clone(),
                started: Instant::now(),
            });
        }
    }
    fn commit_regular(&mut self, deck: usize) {
        let Some(regular) = &mut self.run.regular else {
            return;
        };
        let Some(reservation) = regular.reservation.take() else {
            return;
        };
        if reservation.deck == deck
            && self.loaded[deck] == Some(reservation.index)
            && self.queue == reservation.queue
            && self.queue.get(reservation.slot) == Some(&reservation.index)
        {
            self.queue.remove(reservation.slot);
            self.queue_selected = None;
            regular.unstarted = false;
        }
    }
    // Initial Play Now is asynchronous: account for it only after its decoder is
    // current, ready and actually playing. A cancelled/failed preload consumes nothing.
    pub(super) fn commit_regular_audible_start(&mut self) {
        if self.cancel_changed_regular_queue() {
            return;
        }
        if !self.run.has_regular_playlist() {
            return;
        }
        let deck = if self.playback_owner == crate::playback_owner::Owner::Player {
            0
        } else {
            self.active_deck()
        };
        let audible = self.audio.as_ref().is_some_and(|a| {
            a.load_current(deck)
                && a.loaded(deck)
                && a.controls.ready[deck].load(Relaxed)
                && a.playing(deck)
        });
        if !audible {
            return;
        }
        let unstarted = self.run.regular.as_ref().is_some_and(|r| r.unstarted);
        if unstarted
            && self.run.regular.as_ref().unwrap().reservation.is_none()
            && self.loaded[deck] == self.queue.first().copied()
            && !self.queue.is_empty()
        {
            self.reserve_regular(deck, 0, self.queue[0]);
        }
        let starts_here = self
            .run
            .regular
            .as_ref()
            .and_then(|r| r.reservation.as_ref())
            .is_some_and(|r| {
                r.deck == deck
                    && (self.run.preloaded.is_none()
                        || self.playback_owner == crate::playback_owner::Owner::Player)
            });
        if starts_here {
            self.commit_regular(deck);
            if self.playback_owner == crate::playback_owner::Owner::Player {
                self.run.preloaded = None;
            }
        }
    }
    fn preload_regular(&mut self, alternate: usize) -> Result<(), String> {
        self.commit_regular_audible_start();
        let previous = self
            .run
            .regular
            .as_ref()
            .and_then(|r| r.reservation.as_ref())
            .map(|r| r.slot);
        if let Some(regular) = &mut self.run.regular {
            regular.reservation = None;
        }
        let tracks = &self.library.as_ref().ok_or("Library unavailable")?.tracks;
        let mut candidates: Vec<_> = self
            .queue
            .iter()
            .enumerate()
            .filter_map(|(slot, &index)| {
                tracks
                    .get(index)
                    .filter(|t| !self.run.failed_songs.contains(&show::song_id(t)))
                    .map(|_| (slot, index))
            })
            .collect();
        if alternate > 0 && candidates.len() > 1 {
            candidates.retain(|(slot, _)| Some(*slot) != previous);
        }
        if self.shuffle || alternate > 0 {
            let current = self.loaded[self.active_deck()];
            if candidates.iter().any(|(_, index)| Some(*index) != current) {
                candidates.retain(|(_, index)| Some(*index) != current);
            }
            if !candidates.is_empty() {
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() as usize;
                let len = candidates.len();
                candidates.rotate_left(seed % len);
            }
        }
        for (slot, index) in candidates {
            if self.run.failed_songs.contains(&show::song_id(
                &self.library.as_ref().unwrap().tracks[index],
            )) {
                continue;
            }
            match self.call_track(index, None, Deck::Auto, Execution::LoadOnly) {
                Ok(()) => {
                    let deck = self
                        .run
                        .preloaded
                        .ok_or("Incoming playlist deck is unavailable")?
                        .0;
                    self.reserve_regular(deck, slot, index);
                    return Ok(());
                }
                Err(error) => {
                    let track = &self.library.as_ref().unwrap().tracks[index];
                    let id = show::song_id(track);
                    if !self.run.failed_songs.contains(&id) {
                        self.run.failed_songs.push(id);
                    }
                    self.status = format!("Skipping {}: {error}", track.title);
                }
            }
        }
        Err(if self.queue.is_empty() {"Playlist finished"} else {"No playable songs remain in this playlist; retry Auto Mix after restoring the unavailable files"}.into())
    }
    pub(super) fn recover_failed_song(&mut self, deck: usize, error: &str) {
        if self.playback_owner == crate::playback_owner::Owner::Player && deck == 0 {
            if let Some(track) = self.loaded_track(0) {
                self.player_sequence.failed.push(show::song_id(track));
            }
            if let Some(audio) = &mut self.audio {
                audio.stop(0);
            }
            self.player_sequence.advance = true;
            self.status = format!("Skipping unavailable Player song: {error}");
            return;
        }
        if !self.pair().contains(&deck) {
            if let Some(a) = &mut self.audio {
                a.stop(deck);
            }
            self.status = format!("Background deck {}: {error}", crate::deck_rack::NAMES[deck]);
            return;
        }
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
        if let Some(regular) = &mut self.run.regular
            && regular.reservation.as_ref().is_some_and(|r| r.deck == deck)
        {
            regular.reservation = None;
        }
        if self.run.preloaded.is_some_and(|(d, _)| d == deck) {
            self.run.preloaded = None;
            self.run.pending = None;
            self.run.pending_at = None;
            self.run.transition = None;
            if let Some(regular) = &mut self.run.regular {
                regular.reservation = None;
            }
        } else if self.run.transition.is_some() {
            // An outgoing decoder failed: the prepared incoming deck can take over.
            self.run.pending = self.run.preloaded.map(|(d, _)| (d, Instant::now()));
            self.run.transition = None;
        }
    }
    pub(super) fn execute_control(&mut self, action: Action) -> Result<(), String> {
        if self.output_admission.leased()
            && matches!(
                &action,
                Action::Play { .. }
                    | Action::QuantizedLaunch { .. }
                    | Action::Sampler { .. }
                    | Action::SideSampler { .. }
                    | Action::AutoDj { enabled: true }
                    | Action::AutoDjPause { paused: false }
                    | Action::StartLiveBroadcast { .. }
                    | Action::LoadPlaylist { .. }
                    | Action::LoadSmartPlaylist { .. }
                    | Action::RecallMusicScene { .. }
                    | Action::CallSong { .. }
                    | Action::Recognition { .. }
                    | Action::NextSong { .. }
                    | Action::PlayerNext
                    | Action::Previous
                    | Action::MixNext { .. }
                    | Action::AutoFade
                    | Action::RestartCue { .. }
                    | Action::CueDeck {
                        operation: crate::side_deck::Operation::Play,
                        ..
                    }
            )
        {
            return self.output_admission.local_allowed();
        }
        match action {
            Action::SaveMusicScene { name } => self.save_music_scene(&name)?,
            Action::RecallMusicScene { scene, execution } => {
                self.recall_music_scene(&scene, execution)?
            }
            Action::HotCue { deck, slot } => {
                let d = self.requested_deck(deck, false);
                if !(1..=8).contains(&slot) {
                    return Err("Hot cue slot must be 1–8".into());
                }
                let cue = self
                    .loaded_track(d)
                    .and_then(|t| crate::hot_cues::slot(&t.preparation, slot))
                    .ok_or("Hot cue is not saved on this deck")?
                    .seconds;
                let position = self.audio.as_ref().map_or(0., |a| a.source_position(d));
                self.seek_deck_relative(d, cue - position);
            }
            Action::DeckFilter { deck, value } => {
                if !value.is_finite() || !(-1. ..=1.).contains(&value) {
                    return Err("Filter must be -1 to 1".into());
                }
                let d = self.requested_deck(deck, false);
                self.mixer_panel.set_filter(d, value);
                let audio = self.audio.as_ref().ok_or("Load a deck first")?;
                self.mixer_panel.apply(audio);
            }
            Action::StopSampler { side, pad } => {
                if side > 1 || !(1..=9).contains(&pad) {
                    return Err("Choose sampler side 0/1 and pad 1–9".into());
                }
                let audio = if side == 0 {
                    self.audio.as_ref()
                } else {
                    self.right_sample_audio.as_ref()
                }
                .ok_or("Sampler bank is not loaded")?;
                audio.sample_stop(Some(pad - 1))?;
            }
            Action::GoogleCast { operation } => self.cast.google.remote(operation)?,
            Action::DeckKeyLock { deck, enabled } => {
                let d = self.requested_deck(deck, false);
                let a = self.audio.as_ref().ok_or("Load a deck first")?;
                if enabled && !a.controls.key_lock_available[d].load(Relaxed) {
                    return Err(
                        "Key Lock backend unavailable; install the matching amp-keylock library"
                            .into(),
                    );
                }
                a.controls.key_lock[d].store(enabled, Relaxed);
            }
            Action::BroadcastLevel { level, muted } => self
                .live_mix
                .as_ref()
                .ok_or("Start Live Broadcast first")?
                .set_level(level, muted)?,

            Action::SelectLive { deck } => {
                let d = self.requested_deck(deck, false);
                self.rack.live[d % 2] = d;
            }
            Action::AssignCrossfadePair { a, b } => {
                if self.run.auto {
                    return Err("Stop Auto DJ before reassigning its pair".into());
                }
                let pair = [self.requested_deck(a, false), self.requested_deck(b, false)];
                self.ensure_audio()?;
                self.audio.as_ref().unwrap().assign_pair(pair)?;
            }
            Action::MasterClock { bpm, quantization } => {
                self.ensure_audio()?;
                self.audio.as_ref().unwrap().set_clock(bpm, quantization)?;
            }
            Action::DeckSync { deck, enabled } => {
                let d = self.requested_deck(deck, false);
                self.audio
                    .as_ref()
                    .ok_or("Load a deck first")?
                    .sync(d, enabled)?;
            }
            Action::QuantizedLaunch { deck } => {
                let d = self.requested_deck(deck, false);
                self.audio.as_ref().ok_or("Load a deck first")?.launch(d)?;
                self.claim_playback(self.remote_owner(deck), d, false);
            }
            Action::DeckLoop {
                deck,
                enabled,
                beats,
            } => {
                if !(1..=64).contains(&beats) {
                    return Err("Loop length must be 1–64 beats".into());
                }
                let d = self.requested_deck(deck, false);
                let a = self.audio.as_ref().ok_or("Load a deck first")?;
                if enabled {
                    if !a.controls.looping[d].load(Relaxed) {
                        let bpm = self
                            .loaded_track(d)
                            .and_then(|t| t.preparation.bpm)
                            .ok_or("Prepare BPM before a beat loop")?;
                        a.loop_toggle(d, (beats as f64 * 60. / bpm) as f32)?;
                    }
                } else {
                    a.loop_stop(d);
                }
            }
            Action::DeckGain { deck, value } => {
                if !value.is_finite() || !(0. ..=1.).contains(&value) {
                    return Err("Deck volume must be 0–1".into());
                }
                let d = self.requested_deck(deck, false);
                self.channel_gain[d] = value;
                self.audio
                    .as_ref()
                    .ok_or("Load a deck first")?
                    .set_gain(d, value);
            }
            Action::DeckStems { deck, levels } => {
                if !levels
                    .iter()
                    .all(|n| n.is_finite() && (0. ..=1.).contains(n))
                {
                    return Err("Stem levels must be 0–1".into());
                }
                let d = self.requested_deck(deck, false);
                let a = self.audio.as_ref().ok_or("Load prepared stems first")?;
                if !a.controls.stem_controls_available(d) {
                    return Err("Stem controls unavailable on this deck".into());
                }
                if a.controls.pair_indices().contains(&d) {
                    self.run.cancel_effects(a);
                }
                for (stem, level) in levels.into_iter().enumerate() {
                    a.controls.stem_level[d][stem].store(level.to_bits(), Relaxed);
                    a.controls.stem_knobs[d][stem].store((level - 1.).to_bits(), Relaxed);
                }
            }
            Action::CrossfadeDuration { seconds } => {
                if !seconds.is_finite() || !(0.5..=30.).contains(&seconds) {
                    return Err("Mix duration must be 0.5–30 seconds".into());
                }
                crate::crossfade_settings::save(seconds).map_err(|e| e.to_string())?;
                self.fade_seconds = seconds;
            }
            Action::StopPlayer => {
                self.stop_player();
            }
            Action::StopAllDecks => {
                self.run.auto = false;
                self.auto = false;
                self.run.cancel_automation();
                for d in 0..6 {
                    self.stop(d);
                }
            }
            Action::EmergencyMute => {
                self.volume = 0;
                if let Some(a) = &self.audio {
                    a.set_master(0.);
                }
                if let Some(a) = &self.right_sample_audio {
                    a.set_master(0.);
                }
            }
            Action::StopSamplers { side } => match side {
                Some(0) => {
                    if let Some(a) = &self.audio {
                        let _ = a.sample_stop(None);
                    }
                }
                Some(1) => {
                    if let Some(a) = &self.right_sample_audio {
                        let _ = a.sample_stop(None);
                    }
                }
                None => {
                    if let Some(a) = &self.audio {
                        let _ = a.sample_stop(None);
                    }
                    if let Some(a) = &self.right_sample_audio {
                        let _ = a.sample_stop(None);
                    }
                }
                _ => return Err("Sampler side must be 0 or 1".into()),
            },

            Action::StartLiveBroadcast {
                address,
                password,
                player_only,
            } => {
                let destination: std::net::SocketAddr = address
                    .parse()
                    .map_err(|_| "Use a LAN IP and port for the broadcast server")?;
                let lan = match destination.ip() {
                    std::net::IpAddr::V4(ip) => {
                        ip.is_private() || ip.is_loopback() || ip.is_link_local()
                    }
                    std::net::IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local(),
                };
                if !lan {
                    return Err("Live broadcast target must be on the local network".into());
                }
                self.ensure_audio()?;
                self.ensure_right_samples()?;
                self.live_mix = Some(crate::live_mix::Broadcast::start_source_normalized(
                    self.audio.as_ref().unwrap(),
                    self.right_sample_audio.as_ref().unwrap(),
                    address,
                    password,
                    player_only,
                    self.cast.normalize,
                )?);
            }
            Action::StopLiveBroadcast => {
                self.live_mix = None;
                if let Some(a) = &self.audio {
                    a.stop_broadcast_tap();
                }
                if let Some(a) = &self.right_sample_audio {
                    a.stop_broadcast_tap();
                }
            }
            Action::AdjustVolume { delta } => {
                if !delta.is_finite() || delta.abs() > 1. {
                    return Err("Volume adjustment must be between -1 and 1".into());
                }
                self.volume = (self.volume as f32 + delta * 100.).round().clamp(0., 100.) as u8;
                if let Some(audio) = &self.audio {
                    audio.set_master(self.volume as f32 / 100.);
                }
            }
            Action::SeekRelative { deck, seconds } => {
                if !seconds.is_finite() || seconds.abs() > 86400. {
                    return Err("Invalid seek".into());
                }
                let d = self.requested_deck(deck, false);
                if self.loaded_track(d).is_none() {
                    return Err("Load a song first".into());
                }
                self.seek_deck_relative(d, seconds);
            }
            Action::Previous => {
                if self.podcasts.previous_episode() {
                    return Ok(());
                }
                let index = self.loaded[0].ok_or("No current song")?;
                self.play(0, index.saturating_sub(1));
            }
            Action::PlayerNext => self.next(),
            Action::PlayerOptions { shuffle, repeat } => {
                if let Some(value) = shuffle {
                    self.shuffle = value;
                }
                if let Some(value) = repeat {
                    self.repeat_one = value;
                }
            }
            Action::LoadPlaylist { playlist } => {
                let (path, list) = self
                    .dj_browser
                    .playlists
                    .iter()
                    .find(|(path, _)| regular_playlist_id(path) == playlist)
                    .cloned()
                    .ok_or("Playlist unavailable; refresh the catalog")?;
                let library = self.library.as_ref().ok_or("Library unavailable")?;
                let indices: Vec<_> = list
                    .tracks
                    .iter()
                    .filter_map(|path| library.tracks.iter().position(|t| &t.path == path))
                    .collect();
                let first = *indices.first().ok_or("Playlist has no available songs")?;
                self.run.load_regular_playlist(list.name.clone());
                self.queue = indices;
                self.player_sequence.smart = None;
                self.player_sequence.explicit_regular = true;
                self.queue_selected = None;
                self.playlist_snapshot = Some((path, list));
                debug_assert_eq!(self.queue.first(), Some(&first));
                self.preload_smart()?;
            }
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
                    tempo_recovery: self.run.transitions.tempo_recovery,
                    enabled,
                    pace: self.run.transitions.pace,
                    timing: self.run.transitions.timing,
                    techniques,
                    every,
                    style: crate::transition_styles::Style::Variety,
                    loop_build: loop_build.unwrap_or(self.run.transitions.loop_build),
                    stem_swap: stem_swap.unwrap_or(self.run.transitions.stem_swap),
                    mashup: self.run.transitions.mashup,
                    mashup_beats: self.run.transitions.mashup_beats,
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
                if let Some(p) = &self.run.profile
                    && ((!p.genres.is_empty()
                        && !amp_core::genres::matches(&p.genres, &track.preparation))
                        || !p.filters.iter().all(|filter| filter.matches(track))
                        || p.excluded.contains(&id))
                {
                    return Err("That song does not match this Smart Playlist’s genre / analysis filters or is excluded".into());
                }
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
                if self.playback_owner == crate::playback_owner::Owner::Player {
                    self.next();
                    return Ok(());
                }
                if self.run.profile.is_none()
                    && self.run.regular.is_none()
                    && !self.queue.is_empty()
                {
                    self.run.regular = Some(RegularRun {
                        unstarted: false,
                        reservation: None,
                    });
                }
                self.execute_control(Action::MixNext { immediate })?;
            }
            Action::SmartShuffle => {
                self.run.preloaded = None;
                self.preload_variant(1)?;
            }
            Action::Play { deck } => {
                if deck == Deck::Auto
                    && self.playback_owner == crate::playback_owner::Owner::Dj
                    && self.run.paused
                {
                    return self.execute_control(Action::AutoDjPause { paused: false });
                }
                let d = self.requested_deck(deck, false);
                self.transport_play(self.remote_owner(deck), d)?;
            }
            Action::Pause { deck } => {
                if deck == Deck::Auto {
                    self.pause_playback();
                    return Ok(());
                }
                let d = self.requested_deck(deck, false);
                if self.remote_owner(deck) != self.playback_owner {
                    return Err("DJ is not the active playback owner".into());
                }
                self.cancel_deck_automation(d);
                if let Some(a) = &mut self.audio {
                    a.pause(d);
                }
            }
            Action::Stop { deck } => {
                if deck == Deck::Auto {
                    self.stop_playback();
                    return Ok(());
                }
                if self.playback_owner != crate::playback_owner::Owner::Dj {
                    return Err("DJ is not the active playback owner".into());
                }
                let d = self.requested_deck(deck, false);
                self.stop(d);
            }
            Action::RestartCue { deck, cue } => {
                let d = self.requested_deck(deck, false);
                let i = self.loaded[d].ok_or("Deck is not loaded")?;
                self.call_track(i, cue.as_deref(), Deck::PHYSICAL[d], Execution::PlayNow)?;
            }
            Action::Fade {
                deck,
                master,
                target,
                seconds,
                stop_after,
            } => {
                let d = self.requested_deck(deck, false);
                let audio = self.audio.as_ref().ok_or("Audio engine is not running")?;
                if master {
                    audio.level_fade(2, target, seconds, stop_after)?;
                } else {
                    audio.fade_deck(d, target, seconds, stop_after)?;
                }
                if stop_after && (master || self.pair().contains(&d)) {
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
                self.claim_playback(crate::playback_owner::Owner::Dj, d, true);
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
                    .find(|p| p.id == playlist)
                    .or_else(|| {
                        self.run
                            .settings
                            .playlists
                            .iter()
                            .find(|p| p.name.eq_ignore_ascii_case(&playlist))
                    })
                    .ok_or("Smart Playlist not found")?
                    .clone();
                if self.show_editor.playlist_unsaved(&p.id) {
                    return Err("This Smart Playlist has unsaved Library changes. Open it in Library and Save Playlist first.".into());
                }
                let matching_tracks = self.library.as_ref().map_or(0, |library| {
                    library
                        .tracks
                        .iter()
                        .filter(|track| show::eligible(&p, track))
                        .count()
                });
                if matching_tracks == 0 {
                    return Err("This saved Smart Playlist has no matching songs. Choose its Music Source in Library, then Save Playlist.".into());
                }
                self.run.failed_songs.clear();
                self.run.smart_cycle = Default::default();
                self.run.smart_audible = None;
                self.run.choice = p.id.clone();
                self.run.planned_progression = p.energy_progression.clone();
                self.run.planned_force = false;
                self.run.progression = None;
                self.run.regular = None;
                self.run.profile = Some(p.clone());
                self.run.playlist = Some(p.id);
                self.run.style = style.unwrap_or(p.style);
                self.run.target = energy.or(p.target_energy);
                self.run.preloaded = None;
                self.run.pending = None;
                if execution == Execution::LoadOnly {
                    self.status = if self.run.auto {
                        format!(
                            "{} selected for Auto Mix ({} matching track{}). Upcoming songs use this source; current decks keep their songs.",
                            p.name,
                            matching_tracks,
                            if matching_tracks == 1 { "" } else { "s" }
                        )
                    } else {
                        format!(
                            "{} selected for Auto Mix ({} matching track{}). Turn Auto Mix ON to load and play its songs.",
                            p.name,
                            matching_tracks,
                            if matching_tracks == 1 { "" } else { "s" }
                        )
                    };
                }
                if execution != Execution::LoadOnly {
                    self.run.failure = None;
                    self.run.crossfade = true;
                    self.preload_smart()?;
                    let (d, i) = self.run.preloaded.unwrap();
                    if execution == Execution::PlayNow {
                        self.call_track(i, None, Deck::PHYSICAL[d], execution)?;
                    } else {
                        self.claim_playback(crate::playback_owner::Owner::Dj, d, true);
                        self.run.pending = Some((d, Instant::now()));
                    }
                    self.run.auto = true;
                    self.auto_dj_controls = true;
                }
            }
            Action::Recognition {
                song,
                playlist,
                recipient,
                team,
                milestone,
                cue,
                execution,
            } => {
                let playlist = playlist
                    .or_else(|| {
                        self.run
                            .settings
                            .playlists
                            .iter()
                            .find(|p| p.id == "recognition")
                            .map(|p| p.id.clone())
                    })
                    .or_else(|| self.run.playlist.clone())
                    .ok_or("Choose a saved playlist for Recognition")?;
                let context: Vec<_> = [recipient, team, milestone]
                    .into_iter()
                    .flatten()
                    .map(|v| v.trim().to_owned())
                    .filter(|v| !v.is_empty())
                    .collect();
                if context
                    .iter()
                    .any(|v| v.len() > 120 || v.chars().any(char::is_control))
                {
                    return Err(
                        "Recognition labels must be 1–120 characters without control characters"
                            .into(),
                    );
                }
                let assigned = song.or_else(|| {
                    self.run
                        .settings
                        .playlists
                        .iter()
                        .find(|p| p.id == playlist)
                        .and_then(|p| p.recognition_song.clone())
                });
                self.execute_control(Action::LoadSmartPlaylist {
                    playlist: playlist.clone(),
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
                        .find(|p| p.id == playlist)
                        .unwrap();
                    if !show::eligible(p, &self.library.as_ref().unwrap().tracks[i]) {
                        return Err("Assigned recognition song is excluded by Library rules".into());
                    }
                    self.call_track(i, cue.as_deref(), Deck::Auto, execution)?;
                } else {
                    self.preload_smart()?;
                    let (d, i) = self.run.preloaded.unwrap();
                    self.call_track(i, cue.as_deref(), Deck::PHYSICAL[d], execution)?;
                }
                self.run.recognition_context = (!context.is_empty()).then(|| context.join(" · "));
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
                        arc: show::ArcShape::Linear,
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
            Action::AutoDjPause { paused } => {
                if (self.run.profile.is_none() && !self.run.has_regular_playlist())
                    || self.run.playlist.is_none()
                {
                    return Err("Load a playlist first".into());
                }
                if self.run.transition.is_some()
                    || self.run.pending.is_some()
                    || self.audio.as_ref().is_some_and(|a| a.fading())
                {
                    return Err(
                        "Finish the current handoff before pausing or resuming Auto DJ".into(),
                    );
                }
                let active = self.active_deck();
                let pair = self.pair();
                let audio = self.audio.as_mut().ok_or("Audio engine is not running")?;
                if !audio.loaded(active) {
                    return Err("The active deck is not loaded".into());
                }
                self.run.cancel_effects(audio);
                for d in pair {
                    if !paused && d == active {
                        audio.play(d)?;
                    } else {
                        audio.pause(d);
                    }
                }
                self.run.auto = !paused;
                self.run.paused = paused;
                if !paused {
                    self.claim_playback(crate::playback_owner::Owner::Dj, active, false);
                }
                self.status = if paused {
                    "Auto Mix paused; resume to continue the loaded playlist".into()
                } else {
                    "Auto Mix ON; continuing the loaded playlist".into()
                };
            }
            Action::AutoDj { enabled } => {
                self.commit_regular_audible_start();
                if !enabled {
                    self.auto_dj_controls = false;
                }
                self.run.paused = false;
                if enabled && self.run.playlist.is_none() {
                    return Err("Load a playlist first".into());
                }
                // An explicit restart retries songs skipped by the previous run.
                if enabled {
                    self.run.failure = None;
                    self.run.crossfade = true;
                    self.run.failed_songs.clear();
                }
                // Do not claim ON until loading/starting the first song succeeds.
                self.run.auto = false;
                if enabled
                    && !((self.playback_owner == crate::playback_owner::Owner::Dj
                        || (self.run.has_regular_playlist()
                            && self.run.regular.as_ref().is_some_and(|r| !r.unstarted)))
                        && self
                            .audio
                            .as_ref()
                            .is_some_and(|a| a.playing(self.active_deck())))
                {
                    self.preload_smart()?;
                    let (d, i) = self.run.preloaded.unwrap();
                    self.start_reserved_regular(d, i)?;
                }
                self.run.auto = enabled;
                if enabled {
                    self.claim_playback(crate::playback_owner::Owner::Dj, self.active_deck(), true);
                    let active = self.active_deck();
                    let other = self.other_deck(active);
                    if let Some(audio) = &self.audio
                        && audio.playing(active)
                        && audio.playing(other)
                        && !audio.fading()
                    {
                        if (audio.fader() - (active % 2) as f32).abs() < 0.001 {
                            audio.controls.play[other].store(false, Relaxed);
                        } else {
                            audio.settle_pair(active, self.fade_seconds)?;
                            self.run.transition = Some(active);
                        }
                    }
                }
                if !enabled {
                    if let Some(audio) = &self.audio {
                        self.run.cancel_effects(audio);
                        audio.set_fader(audio.fader());
                    }
                    self.run.cancel_automation();
                }
                self.status = if enabled {
                    format!(
                        "Auto Mix ON · {}",
                        self.run.playlist.as_deref().unwrap_or("loaded playlist")
                    )
                } else {
                    "Auto Mix OFF; manual deck playback is retained".into()
                };
            }
            Action::ManualLock { enabled } => {
                if enabled {
                    // Compatibility for older remotes: this now means Auto Mix OFF,
                    // never a second hidden mode that can prevent later handoffs.
                    self.execute_control(Action::AutoDj { enabled: false })?;
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
                self.claim_playback(crate::playback_owner::Owner::Dj, usize::MAX, false);
            }
            Action::Sampler { pad } => {
                if !(1..=9).contains(&pad) {
                    return Err("Sampler pad must be 1–9".into());
                }
                self.audio
                    .as_ref()
                    .ok_or("Load sampler effects in DJ first")?
                    .sample_trigger(pad - 1)?;
                self.claim_playback(crate::playback_owner::Owner::Dj, usize::MAX, false);
            }
        }
        Ok(())
    }
    fn automatic_exit(&self, deck: usize) -> Option<f64> {
        if !self.run.auto {
            return None;
        }
        let track = self.loaded_track(deck)?;
        let duration = track
            .preparation
            .analysis
            .as_ref()
            .map(|a| a.duration)
            .or_else(|| {
                let audio = self.audio.as_ref()?;
                Some(
                    audio.source_position(deck)
                        + audio.remaining(deck)? as f64 * self.deck_rate(deck),
                )
            })?;
        // Protect the whole planned transition, including a vocal hold or
        // buildup, rather than reserving only its final crossfade ramp.
        let incoming = self.other_deck(deck);
        let transition = match self.run.transitions.pick(self.run.completed_handoffs) {
            Some(crate::transition_styles::Style::Mashup) => self
                .mashup_pair_plan(deck, incoming)
                .map_or(self.fade_seconds, |(_, hold)| 2. * self.fade_seconds + hold),
            Some(crate::transition_styles::Style::BassSwap) => self
                .build_drop_pair_plan(deck, incoming)
                .map_or(self.fade_seconds, |(_, seconds)| seconds),
            Some(crate::transition_styles::Style::Scratch) => track
                .preparation
                .bpm
                .map(|b| (60. / (b * self.deck_rate(deck))) as f32)
                .map_or(self.fade_seconds, |beat| {
                    4. * beat + self.fade_seconds.min(beat)
                }),
            _ => self.fade_seconds,
        };
        Some(amp_core::dj_phrasing::exit(
            &track.preparation,
            self.run.style,
            self.run.transitions.pace,
            duration,
            transition as f64 * self.deck_rate(deck),
        ))
    }
    fn exit_energy(&self) -> Option<f64> {
        let active = self.active_deck();
        let t = self.loaded_track(active)?;
        let profile = amp_core::energy_profile::read(&t.preparation)?;
        let end = self
            .automatic_exit(active)
            .unwrap_or_else(|| t.preparation.cue_out.unwrap_or(profile.duration));
        profile
            .at((end - self.fade_seconds as f64 * (self.deck_rate(active)))
                .max(t.preparation.cue_in))
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
        self.loaded_track(active).and_then(|t| {
            amp_core::energy_profile::read(&t.preparation)
                .and_then(|p| {
                    self.audio
                        .as_ref()
                        .and_then(|a| p.at(a.source_position(active)))
                })
                .map(|e| 1. + e * 9.)
                .or_else(|| show::energy(&t.preparation))
        })
    }
    pub(super) fn performance_tick(&mut self) {
        self.enforce_playback_owner();
        if self.cancel_changed_regular_queue() {
            return;
        }
        self.commit_regular_audible_start();
        self.commit_smart_audible();
        if self.run.auto
            && self.run.has_regular_playlist()
            && self.run.preloaded.is_none()
            && let Some((deck, started)) = self
                .run
                .regular
                .as_ref()
                .and_then(|r| r.reservation.as_ref())
                .map(|r| (r.deck, r.started))
            && self
                .audio
                .as_ref()
                .is_some_and(|a| !a.load_current(deck) || !a.controls.ready[deck].load(Relaxed))
        {
            if started.elapsed() > Duration::from_secs(15) {
                self.recover_failed_song(
                    deck,
                    "Initial playlist song did not become ready; trying another song",
                );
            }
            return;
        }
        // Every temporary effect must release its transport when automation stops.
        if ((self.run.pending.is_none() && self.run.transition.is_none())
            || !self.run.scratch_assist)
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
                show::Progression::new(start, plan.target, plan.transitions, plan.hold)
                    .and_then(|p| p.with_arc(plan.arc))
                    .ok();
        }
        self.run.active = self.active_deck();
        if let Some((incoming, started)) = self.run.pending {
            let ready = self.audio.as_ref().is_some_and(|a| {
                a.load_current(incoming)
                    && a.loaded(incoming)
                    && a.controls.ready[incoming].load(Relaxed)
            });
            let due = self.run.pending_at.is_none_or(|due| {
                self.audio.as_ref().is_some_and(|a| {
                    a.controls.finished[self.other_deck(incoming)].load(Relaxed)
                        || a.source_position(self.other_deck(incoming)) >= due
                })
            });
            if !ready && started.elapsed() > Duration::from_secs(15) {
                self.recover_failed_song(
                    incoming,
                    "Incoming deck did not become ready; trying another song",
                );
            } else if ready && due {
                let outgoing = self.other_deck(incoming);
                if self.run.auto {
                    self.match_auto_pair(outgoing, incoming);
                }
                let chosen = if self.run.force_mashup {
                    Some(crate::transition_styles::Style::Mashup)
                } else if self.run.auto && self.run.effect_done != Some(started) {
                    self.loaded_track(outgoing)
                        .zip(self.loaded_track(incoming))
                        .and_then(|(a, b)| {
                            self.run.transitions.pick_for_pair(
                                self.run.completed_handoffs,
                                &a.preparation,
                                &b.preparation,
                            )
                        })
                } else {
                    None
                };
                let flourish =
                    !self.run.auto && chosen == Some(crate::transition_styles::Style::Scratch);
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
                    .map(|bpm| (60. / (bpm * (self.deck_rate(outgoing)))) as f32);
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
                        self.audio.as_ref().map(|a| {
                            let stems = a
                                .controls
                                .pair_indices()
                                .iter()
                                .all(|d| a.controls.stems_loaded[*d].load(Relaxed));
                            (
                                if stems {
                                    crate::transition_styles::Style::StemSwap
                                } else {
                                    crate::transition_styles::Style::BassSwap
                                },
                                0.,
                            )
                        })
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
                        self.loaded_track(outgoing)
                            .filter(|t| amp_core::mix_schedule::timing_reliable(&t.preparation))
                            .and_then(|t| t.preparation.bpm)
                            .map(|bpm| {
                                let bpm = bpm * self.deck_rate(outgoing);
                                (chosen.unwrap(), (60. / bpm / 4.).clamp(0.03, 0.25) as f32)
                            })
                    } else {
                        None
                    };
                let bridge_needed = !self.run.auto
                    && self.run.scratch_assist
                    && !(chosen.is_none()
                        && self
                            .run
                            .transitions
                            .pick(self.run.completed_handoffs)
                            .is_some())
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
                                    let ratio = a * (self.deck_rate(outgoing)) / b;
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
                if !a.playing(self.other_deck(incoming))
                    || a.remaining(self.other_deck(incoming))
                        .is_some_and(|remaining| remaining < 0.15)
                {
                    // The outgoing song may end between the UI tick and fade request.
                    // A ready next song can take over without disabling continuous mixing.
                    a.controls.play[self.other_deck(incoming)].store(false, Relaxed);
                    a.controls.play[incoming].store(true, Relaxed);
                    a.set_fader((incoming % 2) as f32);
                    self.complete_change(incoming);
                } else if incoming != self.pair()[1 - usize::from(a.fader() >= 0.5)] {
                    self.status =
                        "Crossfader changed after preload; request the next song again".into();
                    self.run.cancel_automation();
                } else {
                    let build_plan = if chosen == Some(crate::transition_styles::Style::BassSwap) {
                        self.build_drop_pair_plan(outgoing, incoming)
                    } else {
                        None
                    };
                    let remaining = a.remaining(outgoing).map(|tail| {
                        if !self.run.auto {
                            return tail;
                        }
                        self.automatic_exit(outgoing).map_or(tail, |end| {
                            tail.min(
                                ((end - a.source_position(outgoing)) / self.deck_rate(outgoing))
                                    .max(0.) as f32,
                            )
                        })
                    });
                    let decision = self
                        .loaded_track(outgoing)
                        .zip(self.loaded_track(incoming))
                        .map(|(out, next)| {
                            use amp_core::transition_plan::*;
                            plan_transition(
                                timing_snapshot(
                                    &out.preparation,
                                    a.controls.stems_loaded[outgoing].load(Relaxed),
                                ),
                                timing_snapshot(
                                    &next.preparation,
                                    a.controls.stems_loaded[incoming].load(Relaxed),
                                ),
                                requested_timing(self.run.transitions.timing),
                                match chosen {
                                    Some(crate::transition_styles::Style::StemSwap) => {
                                        Handoff::StemSwap
                                    }
                                    Some(crate::transition_styles::Style::BassSwap) => {
                                        Handoff::BassSwap
                                    }
                                    _ => Handoff::Fade,
                                },
                                self.deck_rate(outgoing),
                                0.16,
                                self.fade_seconds as f64,
                                remaining.map_or(f64::NAN, f64::from),
                            )
                        });
                    let seconds = build_plan.map_or_else(
                        || amp_core::transition_plan::tail_fade(self.fade_seconds, remaining),
                        |(_, seconds)| seconds,
                    );
                    let seconds = self
                        .loaded_track(outgoing)
                        .zip(self.loaded_track(incoming))
                        .map_or(seconds, |(out, next)| {
                            let confidence = |p: &amp_core::Preparation| {
                                if p.manual_override
                                    || p.analysis.as_ref().is_some_and(|a| a.reviewed)
                                {
                                    1.
                                } else {
                                    p.analysis.as_ref().map_or(0., |a| a.key_confidence)
                                }
                            };
                            let key = |p: &amp_core::Preparation| {
                                amp_core::transition_plan::key_code(
                                    p.harmonic_key.as_deref().unwrap_or(""),
                                )
                            };
                            let clash = amp_core::transition_plan::harmonic_cost(
                                key(&out.preparation),
                                key(&next.preparation),
                                confidence(&out.preparation),
                                confidence(&next.preparation),
                                self.deck_rate(outgoing),
                                self.deck_rate(incoming),
                                a.controls.key_lock[outgoing].load(Relaxed),
                                a.controls.key_lock[incoming].load(Relaxed),
                            ) >= 1.8;
                            if clash { seconds.min(4.) } else { seconds }
                        });
                    let dual_vocals = self
                        .loaded_track(outgoing)
                        .zip(self.loaded_track(incoming))
                        .is_some_and(|(out, next)| {
                            let active = |p: &amp_core::Preparation, start| {
                                p.extensions
                                    .get("vocal_profile")
                                    .and_then(|v| {
                                        serde_json::from_value::<
                                            amp_core::vocal_profile::VocalProfile,
                                        >(v.clone())
                                        .ok()
                                    })
                                    .is_some_and(|v| v.active(start, seconds as f64, 0.45))
                            };
                            !a.controls.stems_loaded[outgoing].load(Relaxed)
                                && active(&out.preparation, a.source_position(outgoing))
                                && active(&next.preparation, a.source_position(incoming))
                        });
                    let seconds = if dual_vocals {
                        seconds.min(4.)
                    } else {
                        seconds
                    };
                    let grid = self
                        .loaded_track(outgoing)
                        .zip(self.loaded_track(incoming))
                        .and_then(|(out, next)| {
                            crate::mix_schedule::safe_grid_at_rates(
                                &out.preparation,
                                &next.preparation,
                                a.source_position(incoming),
                                a.source_position(outgoing) - a.position(outgoing),
                                self.run.transitions.timing,
                                self.deck_rate(outgoing),
                                self.deck_rate(incoming),
                            )
                        });
                    let mashup_plan = if chosen == Some(crate::transition_styles::Style::Mashup) {
                        self.mashup_pair_plan(outgoing, incoming)
                    } else {
                        None
                    };
                    a.set_handoff_mode(effect_plan.map_or(0, |(style, _)| match style {
                        crate::transition_styles::Style::StemSwap => 2,
                        crate::transition_styles::Style::BassSwap => 1,
                        _ => 0,
                    }));
                    if let Some(decision) = &decision {
                        use amp_core::transition_plan::Handoff;
                        if matches!(
                            chosen,
                            Some(
                                crate::transition_styles::Style::StemSwap
                                    | crate::transition_styles::Style::BassSwap
                            )
                        ) {
                            a.set_handoff_mode(match decision.handoff {
                                Handoff::StemSwap => 2,
                                Handoff::BassSwap => 1,
                                Handoff::Fade => 0,
                            });
                        }
                    }
                    if dual_vocals {
                        a.set_handoff_mode(3);
                    }
                    let result = if let Some((build_grid, _)) = build_plan {
                        self.status =
                            "16-beat buildup handoff: incoming track owns the drop".into();
                        a.scheduled_crossfade(seconds, build_grid)
                    } else if let Some((mashup_grid, hold)) = mashup_plan {
                        match a.mashup(seconds, hold, mashup_grid) {
                            Ok(()) => {
                                self.status = format!(
                                    "Vocal / instrumental mashup · {}-beat hold",
                                    self.run.transitions.mashup_beats
                                );
                                Ok(())
                            }
                            Err(e) => {
                                self.status = format!("Mashup skipped: {e}; using normal blend");
                                a.crossfade(seconds)
                            }
                        }
                    } else if chosen == Some(crate::transition_styles::Style::Scratch)
                        && self.run.auto
                    {
                        match grid
                            .zip(beat)
                            .ok_or_else(|| "Reliable matched grids required".to_string())
                            .and_then(|(grid, beat)| a.scratch_crossfade(seconds, grid, beat))
                        {
                            Ok(()) => {
                                self.status =
                                    "Beat-locked incoming drum scratch, then phrase blend".into();
                                Ok(())
                            }
                            Err(error) => {
                                self.status =
                                    format!("Scratch-in skipped: {error}; using clean blend");
                                if let Some(grid) = grid {
                                    a.scheduled_crossfade(seconds, grid)
                                } else {
                                    a.crossfade(seconds)
                                }
                            }
                        }
                    } else if let Some(grid) = grid {
                        // Select once against the musical exit. A late UI tick must
                        // never roll the handoff into another phrase past that exit.
                        let exit = remaining.map(|tail| {
                            a.position(outgoing) + f64::from(tail) * self.deck_rate(outgoing)
                        });
                        let at = exit.and_then(|end| {
                            crate::mix_schedule::launch_before_exit(
                                grid,
                                a.position(outgoing),
                                end,
                                self.deck_rate(outgoing),
                                f64::from(seconds),
                            )
                        });
                        if self.run.auto {
                            if let Some(at) = at {
                                a.crossfade_at(seconds, grid, at)
                            } else {
                                a.crossfade(seconds)
                            }
                        } else if remaining.is_some_and(|r| {
                            r > seconds
                                + (grid.next(a.position(outgoing) + 0.1) - a.position(outgoing))
                                    as f32
                                    / self.deck_rate(outgoing) as f32
                                + 0.1
                        }) {
                            a.scheduled_crossfade(seconds, grid)
                        } else {
                            a.crossfade(seconds)
                        }
                    } else {
                        a.crossfade(seconds)
                    };
                    match result {
                        Ok(()) => {
                            self.run.transition_trace = decision.clone();
                            self.run.force_mashup = false;
                            if chosen == Some(crate::transition_styles::Style::Mashup)
                                && mashup_plan.is_none()
                            {
                                self.status="Mashup skipped: prepare both stem sets and use compatible tempo, tuning and grids; using normal blend".into();
                            }
                            self.run.transition = Some(incoming);
                            if let Some((style, grain)) = effect_plan {
                                self.run.effect_bridge = Some(EffectBridge {
                                    deck: outgoing,
                                    style,
                                    seconds,
                                    grain,
                                    engaged: false,
                                    original: 0.,
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
            && self.audio.as_ref().is_some_and(|a| {
                !a.fading()
                    && !a.playing(incoming)
                    && a.controls.finished[incoming].load(Relaxed)
                    && a.playing(self.other_deck(incoming))
            })
        {
            self.recover_failed_song(
                incoming,
                "Incoming audio ended before the mix completed; current deck retained",
            );
        }
        if let Some(incoming) = self.run.transition
            && let Some(a) = &self.audio
            && !a.fading()
            && !a.playing(self.other_deck(incoming))
            && a.playing(incoming)
            && (a.fader() - (incoming % 2) as f32).abs() < 0.001
        {
            self.complete_change(incoming);
        }

        if self.run.auto
            && !self.audio.as_ref().is_some_and(|a| {
                a.controls
                    .pair_indices()
                    .iter()
                    .any(|d| matches!(a.controls.section_state[*d].load(Relaxed), 1 | 2))
            })
            && self.run.pending.is_none()
            && self.run.transition.is_none()
        {
            let active = self.active_deck();
            let remaining = self
                .audio
                .as_ref()
                .and_then(|a| a.remaining(active))
                .map(|tail| {
                    self.automatic_exit(active)
                        .zip(self.audio.as_ref())
                        .map_or(tail, |(end, a)| {
                            tail.min(
                                ((end - a.source_position(active)) / self.deck_rate(active)).max(0.)
                                    as f32,
                            )
                        })
                });
            let next_mashup = self.run.transitions.pick(self.run.completed_handoffs)
                == Some(crate::transition_styles::Style::Mashup);
            let auto_mashup_plan = if next_mashup {
                self.run
                    .preloaded
                    .and_then(|(incoming, _)| self.mashup_pair_plan(active, incoming))
            } else {
                None
            };
            let next_scratch = self.run.transitions.pick(self.run.completed_handoffs)
                == Some(crate::transition_styles::Style::Scratch);
            let scratch_length = self
                .loaded_track(active)
                .and_then(|t| t.preparation.bpm)
                .map(|b| (60. / (b * self.deck_rate(active))) as f32)
                .map(|beat| 4. * beat + self.fade_seconds.min(beat));
            let build_length = if self.run.transitions.pick(self.run.completed_handoffs)
                == Some(crate::transition_styles::Style::BassSwap)
            {
                self.run
                    .preloaded
                    .and_then(|(incoming, _)| self.build_drop_pair_plan(active, incoming))
                    .map(|(_, seconds)| seconds)
            } else {
                None
            };
            let mix_seconds = if let Some(seconds) = build_length {
                seconds
            } else if next_scratch {
                scratch_length.unwrap_or(self.fade_seconds)
            } else {
                self.fade_seconds
                    + auto_mashup_plan.map_or(0., |(_, hold)| self.fade_seconds + hold)
            };
            let lead_timing = if auto_mashup_plan.is_some()
                && self.run.transitions.timing.phrase_beats().is_none()
            {
                crate::mix_schedule::Timing::Phrase16
            } else {
                self.run.transitions.timing
            };
            let grid_lead = if lead_timing != crate::mix_schedule::Timing::Immediate {
                self.loaded_track(active)
                    .and_then(|t| {
                        t.preparation.bpm.map(|b| {
                            let beats = if let Some(beats) = lead_timing.phrase_beats() {
                                beats
                            } else if self.run.transitions.timing
                                == crate::mix_schedule::Timing::Bar
                                && t.preparation.extensions.contains_key("mix_bar_anchor")
                            {
                                4.
                            } else {
                                1.
                            };
                            (beats * 60. / (b * (self.deck_rate(active)))) as f32 + 0.2
                        })
                    })
                    .unwrap_or(0.)
            } else {
                0.
            };
            if self
                .audio
                .as_ref()
                .is_some_and(|a| a.playing(active) || a.controls.finished[active].load(Relaxed))
                && self.run.preloaded.is_none()
                && let Err(e) = self.preload_smart()
            {
                self.status = e;
                self.run.failure = Some(self.status.clone());
                self.run.auto = false;
            }
            let finished = self
                .audio
                .as_ref()
                .is_some_and(|a| a.controls.finished[active].load(Relaxed));
            if (finished
                || (self.run.crossfade
                    && remaining.is_some_and(|r| {
                        r <= mix_seconds
                            + grid_lead
                            + if self.run.scratch_assist { 0.8 } else { 0. }
                    })))
                && let Some((incoming, _)) = self.run.preloaded
            {
                self.run.pending_at = None;
                self.run.pending = Some((incoming, Instant::now()));
            }
        }
    }
    fn complete_change(&mut self, incoming: usize) {
        self.commit_regular(incoming);
        if self.run.auto
            && self.run.transitions.tempo_recovery
            && let Some(audio) = &self.audio
        {
            audio.controls.tempo_recovery[incoming].store(true, Relaxed);
        }

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
                    self.run.target = Some(p.next());
                }
                self.run.progression = None;
            }
        }
    }
    pub(super) fn control_catalog_key(&self) -> Vec<u8> {
        serde_json::to_vec(&(
            &self.root,
            self.library_generation,
            self.preparer.revision,
            crate::music_scenes::revision(),
            &self.run.settings,
            &self.dj_browser.playlists,
        ))
        .unwrap_or_default()
    }
    pub(super) fn control_snapshot(&self) -> Value {
        let active = if self.playback_owner == crate::playback_owner::Owner::Player {
            0
        } else {
            self.active_deck()
        };
        let a = self.audio.as_ref();
        let tracks:Vec<_>=(0..6).map(|d|{
            let engine_loaded=a.is_some_and(|a|a.load_current(d)&&a.loaded(d));let t=self.loaded_track(d);
            let playing=engine_loaded&&a.is_some_and(|a|a.playing(d));
            json!({"transition_decision": self.run.transition_trace.as_ref(),"deck":crate::deck_rack::NAMES[d],"key_lock":a.is_some_and(|a|a.controls.key_lock[d].load(Relaxed)),"key_lock_available":a.is_some_and(|a|a.controls.key_lock_available[d].load(Relaxed)),"stem_levels":std::array::from_fn::<_,3,_>(|i|a.map_or(1.,|a|f32::from_bits(a.controls.stem_level[d][i].load(Relaxed)))),"stems_ready":a.is_some_and(|a|a.controls.stems_loaded[d].load(Relaxed)),"sync":a.is_some_and(|a|a.controls.synced[d].load(Relaxed)),"looping":a.is_some_and(|a|a.controls.looping[d].load(Relaxed)),"gain":a.map_or(0.,|a|a.channel_level(d)),"underruns":a.map_or(0,|a|a.controls.underruns[d].load(Relaxed)),"loaded":engine_loaded,"ready":engine_loaded&&a.is_some_and(|a|a.controls.ready[d].load(Relaxed)),"playing":playing,"state":if playing{"playing"}else if engine_loaded{"paused"}else{"stopped"},"song_id":if engine_loaded{t.map(show::song_id)}else{None},"title":if engine_loaded{t.map(|t|t.title.as_str())}else{None},"artist":if engine_loaded{t.and_then(|t|t.preparation.extensions.get("artist")).cloned()}else{None},"position":if engine_loaded{a.map(|a|a.source_position(d))}else{None},"remaining":if engine_loaded{a.and_then(|a|a.remaining(d)).or_else(||t.and_then(|t|t.preparation.extensions.get("duration_seconds")).and_then(|v|v.as_f64()).map(|duration|(duration-a.map_or(0.,|a|a.source_position(d))).max(0.) as f32))}else{None}})
        }).collect();
        let fading = a.is_some_and(|a| a.fading() || a.controls.level_fading.load(Relaxed));
        json!({"version":1,"amp":{"playback_owner":self.playback_owner,"preparation_paused":crate::preparation_speed::paused(),"preparation_throttled":crate::preparation_speed::throttled(),"preparation_playback_policy":crate::preparation_speed::policy().label(),"preparation_speed":crate::preparation_speed::mode().label(),"connected":true,"status":self.status,"playback_state":tracks[active]["state"],"recognition_context":self.run.recognition_context,"current_song":tracks[active]["title"],"current_artist":tracks[active]["artist"],"current_cue":if tracks[active]["loaded"]==true{self.loaded_track(active).and_then(|t|a.and_then(|a|t.preparation.manual_cues.iter().chain(&t.preparation.cues).find(|c|(c.seconds-(a.source_position(active)-a.position(active))).abs()<0.001).map(|c|c.id.clone())))}else{None},"active_deck":if active.is_multiple_of(2){"A"}else{"B"},"next_song":self.run.preloaded.and_then(|(_,i)|self.library.as_ref()?.tracks.get(i)).map(|t|t.title.clone()),"remaining_time":tracks[active]["remaining"],"playlist":self.run.playlist,"mix_style":self.run.style,"target_energy":self.run.progression.as_ref().map(|p|p.target).or(self.run.planned_progression.as_ref().map(|p|p.target)).or(self.run.target),"current_energy":self.current_energy(),"progression_remaining":self.run.progression.as_ref().map_or(0,|p|p.remaining()),"auto_dj_enabled":self.run.auto,"auto_dj_paused":self.run.paused,"auto_crossfade_enabled":self.run.crossfade,"fade_state":if fading{"running"}else{"idle"},"fade_progress":a.map_or(0.,|a|f32::from_bits(a.controls.fade_progress.load(Relaxed))),"position":tracks[active]["position"],"crossfader":a.map_or(0.5,|a|a.fader()),"master_level":a.map_or(self.volume as f32/100.,|a|a.master_level()),"shuffle":self.shuffle,"repeat":self.repeat_one,"manual_dj_locked":false},"samplers":[self.sampler.snapshot(a),self.right_sampler.snapshot(self.right_sample_audio.as_ref())],"google_cast":self.cast.get().map(|c|c.google.snapshot()).unwrap_or_else(||crate::google_cast::Output::default().snapshot()),"live_broadcast":self.live_mix.as_ref().map(|b|json!({"running":b.running(),"source":if b.player_only{"player"}else{"dj"},"levels":b.levels(),"status":b.status.lock().ok().map(|s|s.clone())})),"deck_layers":tracks,"selected_decks":self.rack.live.map(|d|crate::deck_rack::NAMES[d]),"crossfade_pair":self.pair().map(|d|crate::deck_rack::NAMES[d]),"clock":{"bpm":a.map_or(120.,|a|f32::from_bits(a.controls.master_bpm.load(Relaxed))),"quantization":a.map_or(4,|a|a.controls.quantization.load(Relaxed)),"sample_frames":a.map_or(0,|a|a.controls.rendered.load(Relaxed)),"sample_rate":a.map_or(48000,|a|a.sample_rate),"beat":a.map_or(0.,|a|a.controls.beat_at(a.controls.rendered.load(Relaxed),a.sample_rate))},"decks":self.rack.live.map(|d|{let mut v=tracks[d].clone();v["deck"]=json!(if d%2==0{"A"}else{"B"});v}),"cue_decks":self.sides.iter().map(|side|side.snapshot()).collect::<Vec<_>>()})
    }
}

#[cfg(test)]
mod tests {
    fn takeover_effect(style: crate::transition_styles::Style) -> EffectBridge {
        EffectBridge {
            deck: 2,
            style,
            seconds: 1.,
            grain: 0.1,
            engaged: true,
            original: 1234.,
            start_progress: 0.,
            halves: 0,
        }
    }
    #[test]
    fn live_takeover_cleanup_maps_owned_stutter_loop_brake_and_filter_only() {
        use crate::transition_styles::Style;
        for style in [
            Style::Stutter,
            Style::LoopBuild,
            Style::Brake,
            Style::Filter,
        ] {
            let run = Run {
                effect_bridge: Some(takeover_effect(style)),
                scratch_bridge: Some((1, Instant::now())),
                auto: true,
                ..Default::default()
            };
            let cleanup = run.live_takeover_cleanup();
            assert_eq!(
                cleanup.auto_loop_deck,
                matches!(style, Style::Stutter | Style::LoopBuild).then_some(2)
            );
            assert_eq!(
                cleanup.restore_cutoff,
                (style == Style::Filter).then_some((2, 1234.))
            );
            assert_eq!(
                cleanup.scratch_decks,
                (1 << 1) | if style == Style::Brake { 1 << 2 } else { 0 }
            );
            assert!(run.effect_bridge.is_some() && run.scratch_bridge.is_some() && run.auto);
        }
        let mut run = Run {
            effect_bridge: Some(takeover_effect(Style::Stutter)),
            ..Default::default()
        };
        run.effect_bridge.as_mut().unwrap().engaged = false;
        assert_eq!(
            run.live_takeover_cleanup(),
            crate::mixer::LiveTakeoverCleanup::default()
        );
        assert_eq!(
            Run::default().live_takeover_cleanup(),
            crate::mixer::LiveTakeoverCleanup::default(),
            "Unowned manual effects must not be requested for cleanup"
        );
    }
    #[test]
    fn live_takeover_queue_failure_retains_bridges_until_accepted_for_retry() {
        use crate::transition_styles::Style;
        let mut run = Run {
            effect_bridge: Some(takeover_effect(Style::Stutter)),
            scratch_bridge: Some((1, Instant::now())),
            auto: true,
            preloaded: Some((2, 3)),
            ..Default::default()
        };
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        sender
            .try_send(crate::mixer::LiveTakeoverCleanup::default())
            .unwrap();
        let cleanup = run.live_takeover_cleanup();
        let result = sender.try_send(cleanup);
        assert!(matches!(
            result,
            Err(std::sync::mpsc::TrySendError::Full(_))
        ));
        // Preparation consumes nothing; the UI must call finish only after
        // Engine's single try_send succeeds, so the same ownership can retry.
        assert!(run.effect_bridge.is_some() && run.scratch_bridge.is_some() && run.auto);
        assert_eq!(run.preloaded, Some((2, 3)));
        receiver.recv().unwrap();
        sender.try_send(run.live_takeover_cleanup()).unwrap();
        run.finish_live_takeover();
        assert_eq!(receiver.recv().unwrap(), cleanup);
        assert!(run.effect_bridge.is_none() && run.scratch_bridge.is_none());
        assert!(!run.auto && run.preloaded.is_none());
    }
    #[test]
    fn automatic_energy_entry_and_feature_snap_stay_early_without_restricting_manual_cues() {
        use amp_core::energy_profile::{FIELD, Profile, Section};
        use amp_core::preparation::Cue;
        let profile = Profile {
            version: 1,
            duration: 150.,
            step: 1.,
            curve: vec![0.2; 150],
            sections: vec![
                Section {
                    id: "intro".into(),
                    name: "Intro".into(),
                    start: 0.,
                    end: 30.,
                    energy: 0.2,
                },
                Section {
                    id: "early".into(),
                    name: "Early".into(),
                    start: 30.,
                    end: 80.,
                    energy: 0.6,
                },
                Section {
                    id: "outro".into(),
                    name: "Late".into(),
                    start: 132.,
                    end: 150.,
                    energy: 0.9,
                },
            ],
        };
        let mut p = amp_core::Preparation {
            cue_out: Some(150.),
            ..Default::default()
        };
        p.extensions
            .insert(FIELD.into(), serde_json::to_value(profile).unwrap());
        p.cues.push(Cue {
            id: "late".into(),
            kind: "drop".into(),
            seconds: 132.,
            ..Default::default()
        });
        assert_eq!(automatic_energy_entry(&p, 0.9, 8.), Some(30.));
        p.cues.push(Cue {
            id: "early-drop".into(),
            kind: "drop".into(),
            seconds: 42.,
            ..Default::default()
        });
        assert_eq!(automatic_energy_entry(&p, 0.9, 8.), Some(42.));
        p.manual_override = true;
        p.cue_in = 132.;
        assert_eq!(automatic_energy_entry(&p, 0.9, 8.), None);
        assert_eq!(p.cue_in, 132.);
        assert_eq!(p.cues[0].seconds, 132.);
        p.manual_override = false;
        p.manual_cues.push(p.cues[0].clone());
        assert_eq!(automatic_energy_entry(&p, 0.9, 8.), None);
    }
    #[test]
    fn auto_mix_off_replaces_stale_start_instructions_without_an_audio_device() {
        let root = tempfile::tempdir().unwrap();
        let mut player = normal_test_player(root.path());
        player.run.auto = true;
        player.run.playlist = Some("edm".into());
        player.status = "edm selected for Auto Mix (497 matching tracks). Turn Auto Mix ON to load and play its songs.".into();
        player
            .execute_control(Action::AutoDj { enabled: false })
            .unwrap();
        assert!(!player.run.auto);
        assert!(
            player.status.starts_with("Auto Mix OFF"),
            "stale status: {}",
            player.status
        );
        assert!(!player.status.contains("Turn Auto Mix ON"));
    }
    fn normal_test_player(root: &std::path::Path) -> Player {
        Player {
            root: root.into(),
            playback_owner: crate::playback_owner::Owner::Dj,
            library: Some(amp_core::Library {
                tracks: (0..3)
                    .map(|i| amp_core::Track {
                        path: format!("source-{i}/song.wav").into(),
                        title: format!("Song {i}"),
                        preparation: Default::default(),
                    })
                    .collect(),
                tree: amp_core::Folder {
                    name: "Music".into(),
                    folders: vec![],
                    tracks: vec![],
                },
            }),
            ..Default::default()
        }
    }
    #[test]
    #[ignore = "muted generated native audio; requires a paced null sink"]
    fn smart_cycle_only_consumes_audible_loads_not_cancelled_preloads() {
        fn ready(player: &mut Player, deck: usize) {
            let end = Instant::now() + Duration::from_secs(8);
            loop {
                player.performance_tick();
                if player.audio.as_ref().is_some_and(|a| {
                    a.load_current(deck)
                        && a.controls.ready[deck].load(Relaxed)
                        && (!a.playing(deck) || a.controls.position[deck].load(Relaxed) > 0)
                }) {
                    break;
                }
                assert!(Instant::now() < end);
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let root = tempfile::tempdir().unwrap();
        let fixture = root.path().join("fixture.wav");
        assert!(
            amp_core::command("ffmpeg")
                .args([
                    "-v",
                    "fatal",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=8"
                ])
                .arg(&fixture)
                .status()
                .unwrap()
                .success()
        );
        for i in 0..3 {
            let folder = root.path().join(format!("source-{i}"));
            std::fs::create_dir(&folder).unwrap();
            std::fs::copy(&fixture, folder.join("song.wav")).unwrap();
        }
        let mut player = normal_test_player(root.path());
        player.channel_gain = [0.; 6];
        player.run.profile = Some(show::Settings::default().playlists.remove(0));
        player.run.profile.as_mut().unwrap().recent_repeat = 1;
        player.run.playlist = Some("demo".into());
        player.run.transitions.enabled = false;
        player.preload_smart().unwrap();
        let (deck, first) = player.run.preloaded.unwrap();
        ready(&mut player, deck);
        player.commit_smart_audible();
        let profile = player.run.profile.clone().unwrap();
        assert_eq!(
            player
                .run
                .smart_cycle
                .ranked(
                    &root.path().to_string_lossy(),
                    &profile,
                    &player.library.as_ref().unwrap().tracks,
                    None,
                    None,
                    None
                )
                .len(),
            3,
            "prefetch is not playback"
        );
        player.run.preloaded = None;
        // set_fader enqueues a mixer command. Confirm the rendered crossfader
        // acknowledged closure before starting a headphone-only transport.
        let pair = player.pair();
        let side = pair.iter().position(|d| *d == deck).unwrap();
        let closed = if side == 0 { 1. } else { 0. };
        player.audio.as_ref().unwrap().set_fader(closed);
        let end = Instant::now() + Duration::from_secs(5);
        while (player.audio.as_ref().unwrap().fader() - closed).abs() > 1e-6 {
            assert!(
                Instant::now() < end,
                "crossfader command was not acknowledged"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(player.pair(), pair);
        assert_eq!(player.audio.as_ref().unwrap().fader(), closed);
        player.audio.as_ref().unwrap().controls.cue[deck].store(true, Relaxed);
        player.audio.as_ref().unwrap().play(deck).unwrap();
        while player.audio.as_ref().unwrap().controls.position[deck].load(Relaxed) == 0 {
            assert!(Instant::now() < end, "cue transport did not render");
            std::thread::sleep(Duration::from_millis(5));
        }
        player.commit_smart_audible();
        assert_eq!(
            player
                .run
                .smart_cycle
                .ranked(
                    &root.path().to_string_lossy(),
                    &profile,
                    &player.library.as_ref().unwrap().tracks,
                    None,
                    None,
                    None
                )
                .len(),
            3,
            "rendered headphone cue must not consume a Smart cycle member"
        );
        // Phones-only routing remains private even with the crossfader open.
        player.audio.as_ref().unwrap().controls.phones_only[deck].store(true, Relaxed);
        player.audio.as_ref().unwrap().set_fader(1. - closed);
        while (player.audio.as_ref().unwrap().fader() - (1. - closed)).abs() > 1e-6 {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        player.commit_smart_audible();
        assert_eq!(
            player
                .run
                .smart_cycle
                .ranked(
                    &root.path().to_string_lossy(),
                    &profile,
                    &player.library.as_ref().unwrap().tracks,
                    None,
                    None,
                    None
                )
                .len(),
            3,
            "phones-only routing must not consume a Smart cycle member"
        );
        player.audio.as_ref().unwrap().controls.phones_only[deck].store(false, Relaxed);
        player.audio.as_ref().unwrap().controls.cue[deck].store(false, Relaxed);
        player.audio.as_mut().unwrap().stop(deck);
        let mut heard = std::collections::HashSet::new();
        let mut active_side = side;
        for _ in 0..3 {
            let active = pair[active_side];
            let current = player.loaded[player.active_deck()];
            let index = player.run.smart_cycle.ranked(
                &root.path().to_string_lossy(),
                &profile,
                &player.library.as_ref().unwrap().tracks,
                current,
                None,
                None,
            )[0];
            assert!(heard.insert(index));
            player
                .call_track(index, None, Deck::PHYSICAL[active], Execution::PlayNow)
                .unwrap();
            ready(&mut player, active);
            player.commit_smart_audible();
            active_side = 1 - active_side;
        }
        assert_eq!(heard.len(), 3);
        assert!(heard.contains(&first));
        let last = player.loaded[player.active_deck()].unwrap();
        player.preload_smart().unwrap();
        let (_, next) = player.run.preloaded.unwrap();
        assert_ne!(next, last);
        player.run.preloaded = None;
        for _ in 0..5 {
            player.commit_smart_audible();
        }
        player
            .call_track(
                next,
                None,
                Deck::PHYSICAL[pair[active_side]],
                Execution::PlayNow,
            )
            .unwrap();
        ready(&mut player, pair[active_side]);
        player.commit_smart_audible();
        let remaining = player.run.smart_cycle.ranked(
            &root.path().to_string_lossy(),
            &profile,
            &player.library.as_ref().unwrap().tracks,
            Some(next),
            None,
            None,
        );
        assert!(
            remaining.contains(&last),
            "old boundary playback must not be consumed a second time by UI ticks"
        );
        if let Some(audio) = &mut player.audio {
            for d in 0..6 {
                audio.stop(d);
            }
        }
    }
    #[test]
    fn regular_playlist_failed_preload_keeps_order_duplicates_and_saved_sources() {
        let root = tempfile::tempdir().unwrap();
        let mut player = normal_test_player(root.path());
        let path = root.path().join("ordered.json");
        let list = amp_core::Playlist {
            version: 1,
            name: "Ordered".into(),
            tracks: vec![
                "source-1/song.wav".into(),
                "source-1/song.wav".into(),
                "source-0/song.wav".into(),
            ],
            sync: None,
            companion_available: false,
        };
        let original = serde_json::to_value(&list).unwrap();
        player.dj_browser.playlists.push((path.clone(), list));
        let error = player
            .execute_control(Action::LoadPlaylist {
                playlist: regular_playlist_id(&path),
            })
            .unwrap_err();
        assert!(error.contains("No playable songs"), "{error}");
        assert!(!error.contains("Smart"));
        assert!(player.run.has_regular_playlist());
        assert_eq!(player.queue, [1, 1, 0]);
        assert_eq!(player.run.failed_songs.len(), 2);
        assert_eq!(
            serde_json::to_value(&player.playlist_snapshot.as_ref().unwrap().1).unwrap(),
            original
        );
        assert!(player.run.preloaded.is_none());
        assert!(
            player
                .execute_control(Action::AutoDj { enabled: true })
                .is_err()
        );
        assert!(!player.run.auto);
        player
            .execute_control(Action::ManualLock { enabled: true })
            .unwrap();
        assert_eq!(player.queue, [1, 1, 0]);
    }
    #[test]
    fn regular_playlist_queue_commits_one_reserved_occurrence_only_after_handoff() {
        let root = tempfile::tempdir().unwrap();
        let mut player = normal_test_player(root.path());
        player.run.load_regular_playlist("Duplicates".into());
        player.queue = vec![0, 1, 0];
        player.loaded[1] = Some(0);
        player.reserve_regular(1, 2, 0);
        player.run.preloaded = Some((1, 0));
        assert_eq!(player.queue, [0, 1, 0]);
        player.complete_change(1);
        assert_eq!(
            player.queue,
            [0, 1],
            "remove the reserved duplicate, not the first matching song"
        );
        player.complete_change(1);
        assert_eq!(player.queue, [0, 1], "handoff commitment is idempotent");
        player.loaded[0] = Some(1);
        player.reserve_regular(0, 1, 1);
        player.run.preloaded = Some((0, 1));
        player
            .execute_control(Action::ManualLock { enabled: true })
            .unwrap();
        player.complete_change(0);
        assert_eq!(
            player.queue,
            [0, 1],
            "a cancelled preload never drains the queue"
        );
        player.reserve_regular(0, 1, 1);
        player.run.preloaded = Some((0, 1));
        player.recover_failed_song(0, "injected decode failure");
        assert_eq!(player.queue, [0, 1]);
        assert!(player.run.regular.as_ref().unwrap().reservation.is_none());
        player.reserve_regular(0, 1, 1);
        player.run.preloaded = Some((0, 1));
        player.run.auto = true;
        player.queue.push(2);
        assert!(
            player
                .execute_control(Action::MixNext { immediate: true })
                .unwrap_err()
                .contains("queue changed")
        );
        assert!(!player.run.auto && player.run.preloaded.is_none() && player.run.pending.is_none());
        player.complete_change(0);
        assert_eq!(
            player.queue,
            [0, 1, 2],
            "queue edits invalidate an older position reservation"
        );
    }
    #[test]
    fn regular_manual_replacement_releases_same_song_occurrence_without_consuming_it() {
        let root = tempfile::tempdir().unwrap();
        let mut player = normal_test_player(root.path());
        player
            .run
            .load_regular_playlist("Reserved duplicates".into());
        player.queue = vec![0, 1, 0];
        player.loaded[1] = Some(0);
        player.reserve_regular(1, 2, 0);
        player.run.preloaded = Some((1, 0));
        player.run.regular_deck_replaced(0);
        assert!(player.run.regular.as_ref().unwrap().reservation.is_some());
        player.run.regular_deck_replaced(1);
        assert!(!player.run.regular.as_ref().unwrap().unstarted);
        player.complete_change(1);
        assert_eq!(player.queue, [0, 1, 0]);
    }
    #[test]
    fn regular_shuffle_selects_remaining_members_without_consuming_failed_attempts() {
        let root = tempfile::tempdir().unwrap();
        let mut player = normal_test_player(root.path());
        player.run.load_regular_playlist("Shuffle".into());
        player.queue = vec![0, 1, 2, 0];
        player.loaded[0] = Some(0);
        player.reserve_regular(1, 1, 1);
        player.run.preloaded = Some((1, 1));
        let error = player.execute_control(Action::SmartShuffle).unwrap_err();
        assert!(error.contains("No playable songs"), "{error}");
        assert_eq!(player.queue, [0, 1, 2, 0]);
        let tracks = &player.library.as_ref().unwrap().tracks;
        assert_eq!(
            player.run.failed_songs,
            [show::song_id(&tracks[2])],
            "shuffle tries another remaining member, not sounding/currently preloaded songs"
        );
        assert!(player.run.preloaded.is_none());
    }
    #[test]
    #[ignore = "muted native audio and FFmpeg; generated playlist fixtures only"]
    fn regular_playlist_automatic_preload_shuffle_cancel_and_handoff() {
        let root = tempfile::tempdir().unwrap();
        let fixture = root.path().join("fixture.wav");
        let generated = amp_core::command("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=8",
            ])
            .arg(&fixture)
            .output()
            .unwrap();
        assert!(generated.status.success());
        for i in 0..3 {
            std::fs::create_dir(root.path().join(format!("source-{i}"))).unwrap();
            std::fs::copy(&fixture, root.path().join(format!("source-{i}/song.wav"))).unwrap();
        }
        // Both Player instances need the same explicit supported fade. A fresh
        // instance otherwise inherits the eight-second device preference,
        // which correctly exceeds this fixture's five-second handoff wait.
        const FIXTURE_FADE_SECONDS: f32 = 0.5;
        let mut player = normal_test_player(root.path());
        player.channel_gain = [0.; 6];
        player.fade_seconds = FIXTURE_FADE_SECONDS;
        player.run.transitions.enabled = false;
        let path = root.path().join("playlist.json");
        let list = amp_core::Playlist {
            version: 1,
            name: "Ordered normal".into(),
            tracks: vec![
                "source-1/song.wav".into(),
                "source-0/song.wav".into(),
                "source-2/song.wav".into(),
                "source-1/song.wav".into(),
            ],
            sync: None,
            companion_available: false,
        };
        let saved = serde_json::to_value(&list).unwrap();
        player.dj_browser.playlists.push((path.clone(), list));
        player
            .execute_control(Action::LoadPlaylist {
                playlist: regular_playlist_id(&path),
            })
            .unwrap();
        assert_eq!(player.queue, [1, 0, 2, 1]);
        assert_eq!(player.run.preloaded.map(|(_, i)| i), Some(1));
        wait(&mut player, |p| {
            p.audio.as_ref().unwrap().controls.ready[p.run.preloaded.unwrap().0].load(Relaxed)
        });
        player
            .execute_control(Action::AutoDj { enabled: true })
            .unwrap();
        wait(&mut player, |p| {
            p.queue.len() == 3 && p.run.preloaded.is_some()
        });
        assert!(
            player.status.starts_with("Auto Mix ON"),
            "{}",
            player.status
        );
        assert_eq!(player.loaded[player.active_deck()], Some(1));
        assert_eq!(player.queue, [0, 2, 1]);
        assert_eq!(
            player.run.preloaded.map(|(_, i)| i),
            Some(0),
            "normal automatic next follows saved order without Smart rules"
        );
        player
            .execute_control(Action::AutoDjPause { paused: true })
            .unwrap();
        assert!(player.status.starts_with("Auto Mix paused"));
        assert_eq!(player.queue, [0, 2, 1]);
        player
            .execute_control(Action::AutoDjPause { paused: false })
            .unwrap();
        assert!(player.status.starts_with("Auto Mix ON"));
        player.execute_control(Action::SmartShuffle).unwrap();
        let selected = player.run.preloaded.unwrap().1;
        assert_eq!(
            selected, 2,
            "shuffle must not repeat current or already-preloaded member when another remains"
        );
        assert_eq!(player.queue, [0, 2, 1]);
        player
            .execute_control(Action::ManualLock { enabled: true })
            .unwrap();
        assert_eq!(player.queue, [0, 2, 1]);
        player
            .execute_control(Action::AutoDj { enabled: true })
            .unwrap();
        wait(&mut player, |p| p.run.preloaded.is_some());
        assert_eq!(player.run.preloaded.unwrap().1, 0);
        player
            .execute_control(Action::MixNext { immediate: true })
            .unwrap();
        wait(&mut player, |p| {
            p.loaded[p.active_deck()] == Some(0) && p.run.transition.is_none() && p.queue.len() == 2
        });
        assert_eq!(player.queue, [2, 1]);
        assert_eq!(player.fade_seconds, FIXTURE_FADE_SECONDS);
        assert_eq!(
            serde_json::to_value(&player.playlist_snapshot.as_ref().unwrap().1).unwrap(),
            saved
        );
        player
            .execute_control(Action::AutoDj { enabled: false })
            .unwrap();
        if let Some(audio) = &mut player.audio {
            for deck in 0..6 {
                audio.stop(deck);
            }
        }
        let mut transferred = normal_test_player(root.path());
        transferred.channel_gain = [0.; 6];
        transferred.fade_seconds = FIXTURE_FADE_SECONDS;
        transferred.run.transitions.enabled = false;
        transferred.playback_owner = crate::playback_owner::Owner::Player;
        transferred.shuffle = true;
        let list = player.playlist_snapshot.as_ref().unwrap().1.clone();
        transferred.dj_browser.playlists.push((path.clone(), list));
        transferred
            .execute_control(Action::LoadPlaylist {
                playlist: regular_playlist_id(&path),
            })
            .unwrap();
        let (deck, selected) = transferred.run.preloaded.unwrap();
        assert_eq!(deck, 0);
        let slot = transferred
            .run
            .regular
            .as_ref()
            .unwrap()
            .reservation
            .as_ref()
            .unwrap()
            .slot;
        let mut remaining = transferred.queue.clone();
        remaining.remove(slot);
        transferred
            .transport_play(crate::playback_owner::Owner::Player, 0)
            .unwrap();
        wait(&mut transferred, |p| p.queue.len() == 3);
        assert_eq!(
            transferred.queue, remaining,
            "shuffled Player start consumes exactly its selected occurrence"
        );
        transferred
            .execute_control(Action::AutoDj { enabled: true })
            .unwrap();
        assert_eq!(
            transferred.loaded[transferred.active_deck()],
            Some(selected),
            "owner transfer must not replay or skip the sounding song"
        );
        assert_eq!(transferred.queue, remaining);
        assert_eq!(
            transferred.fade_seconds, FIXTURE_FADE_SECONDS,
            "Owner transfer preserves the explicit device fade"
        );
        transferred.preload_smart().unwrap();
        let (manual_deck, manual_index) = transferred.run.preloaded.unwrap();
        let manual_id = show::song_id(&transferred.library.as_ref().unwrap().tracks[manual_index]);
        assert!(
            transferred
                .execute_control(Action::CallSong {
                    song: manual_id.clone(),
                    cue: Some("missing-explicit-cue".into()),
                    deck: Deck::PHYSICAL[manual_deck],
                    execution: Execution::LoadOnly,
                })
                .is_err()
        );
        assert!(
            transferred
                .run
                .regular
                .as_ref()
                .unwrap()
                .reservation
                .is_some(),
            "failed replacement keeps the original valid preload"
        );
        transferred
            .execute_control(Action::CallSong {
                song: manual_id,
                cue: None,
                deck: Deck::PHYSICAL[manual_deck],
                execution: Execution::LoadOnly,
            })
            .unwrap();
        assert!(
            transferred
                .run
                .regular
                .as_ref()
                .unwrap()
                .reservation
                .is_none(),
            "same-song manual replacement is not the old queue transaction"
        );
        transferred
            .execute_control(Action::MixNext { immediate: true })
            .unwrap();
        wait(&mut transferred, |p| {
            p.active_deck() == manual_deck && p.run.pending.is_none() && p.run.transition.is_none()
        });
        assert_eq!(
            transferred.queue, remaining,
            "an explicit replacement must not consume an old reserved occurrence"
        );
        assert_eq!(transferred.fade_seconds, FIXTURE_FADE_SECONDS);
        transferred
            .execute_control(Action::ManualLock { enabled: true })
            .unwrap();
        assert_eq!(transferred.queue, remaining);
        if let Some(audio) = &mut transferred.audio {
            for deck in 0..6 {
                audio.stop(deck);
            }
        }
    }
    #[test]
    #[ignore = "muted native audio; requires AMP_WAVEFORM_FIXTURE"]
    fn automatic_match_uses_sounding_deck_not_unrelated_master_or_toggle() {
        let fixture = std::path::PathBuf::from(std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap());
        let root = fixture.parent().unwrap();
        let mut player = Player::default();
        player.ensure_audio().unwrap();
        player.audio.as_ref().unwrap().set_master(0.);
        for (deck, bpm) in [120., 125.].into_iter().enumerate() {
            let track = amp_core::Track {
                path: fixture.file_name().unwrap().into(),
                title: "Timing fixture".into(),
                preparation: amp_core::Preparation {
                    bpm: Some(bpm),
                    beat_offset: Some(0.),
                    ..Default::default()
                },
            };
            player
                .audio
                .as_mut()
                .unwrap()
                .load(deck, root, &track, deck == 0)
                .unwrap();
            player.loaded_snapshot[deck] = Some(track);
        }
        wait(&mut player, |p| {
            p.audio.as_ref().unwrap().controls.ready[1].load(Relaxed)
        });
        let audio = player.audio.as_ref().unwrap();
        audio.set_rate(0, 1.05);
        audio.controls.master_bpm.store(95f32.to_bits(), Relaxed);
        audio.controls.synced[1].store(true, Relaxed);
        player.match_auto_pair(0, 1);
        let audio = player.audio.as_ref().unwrap();
        assert!((audio.controls.effective_rate(1) - 126. / 125.).abs() < 1e-6);
        assert!(!audio.controls.synced[1].load(Relaxed));
        assert_eq!(f32::from_bits(audio.controls.master_bpm.load(Relaxed)), 95.);
        // Repeated automatic matching is idempotent, never a Sync button toggle.
        player.match_auto_pair(0, 1);
        assert!(
            (player.audio.as_ref().unwrap().controls.effective_rate(1) - 126. / 125.).abs() < 1e-6
        );
    }
    #[test]
    #[ignore = "native audio, muted; simulated playhead; requires AMP_WAVEFORM_FIXTURE"]
    fn auto_dj_hands_off_early_while_the_audio_tail_is_still_long() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap();
        for name in ["a.wav", "b.wav"] {
            std::fs::copy(&fixture, dir.path().join(name)).unwrap();
            amp_core::preparation::save(
                dir.path(),
                std::path::Path::new(name),
                &amp_core::Preparation {
                    bpm: Some(120.),
                    beat_offset: Some(0.),
                    cue_out: Some(300.),
                    analysis: Some(amp_core::preparation::AnalysisInfo {
                        engine: "fixture".into(),
                        duration: 300.,
                        tempo_confidence: 0.9,
                        key_confidence: 0.9,
                        reviewed: false,
                    }),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let mut player = Player {
            root: dir.path().into(),
            library: Some(amp_core::scan(dir.path()).unwrap()),
            channel_gain: [0.; 6],
            fade_seconds: 0.2,
            ..Default::default()
        };
        player.run.transitions.enabled = false;
        player.run.scratch_assist = false;
        player.run.transitions.timing = crate::mix_schedule::Timing::Immediate;
        player
            .execute_control(Action::LoadSmartPlaylist {
                playlist: "demo".into(),
                execution: Execution::PlayNow,
                style: None,
                energy: None,
            })
            .unwrap();
        wait(&mut player, |p| {
            p.run.preloaded.is_some()
                && p.audio.as_ref().is_some_and(|a| {
                    a.playing(p.active_deck()) && a.source_position(p.active_deck()) > 0.1
                })
        });
        let active = player.active_deck();
        let audio = player.audio.as_ref().unwrap();
        assert!(audio.remaining(active).unwrap() > 10.);
        // Simulate reaching the planned song section without a two-minute test wait.
        audio.simulate_source_origin(active, 130.);
        wait(&mut player, |p| p.run.completed_handoffs == 1);
        assert!(player.run.auto);
        assert_ne!(player.active_deck(), active);
        player
            .execute_control(Action::AutoDj { enabled: false })
            .unwrap();
    }
    #[test]
    #[ignore = "native audio, muted; requires AMP_WAVEFORM_FIXTURE"]
    fn native_drop_preroll_preserves_saved_cues_and_three_second_fade() {
        let root = tempfile::tempdir().unwrap();
        for name in ["a.wav", "b.wav"] {
            std::fs::copy(
                std::env::var_os("AMP_WAVEFORM_FIXTURE").unwrap(),
                root.path().join(name),
            )
            .unwrap();
            let mut prep = amp_core::Preparation {
                bpm: Some(120.),
                beat_offset: Some(0.),
                cue_out: Some(16.),
                ..Default::default()
            };
            prep.cues.push(amp_core::preparation::Cue {
                id: "auto-in".into(),
                seconds: 0.,
                ..Default::default()
            });
            prep.extensions.insert("energy_profile".into(), serde_json::json!({
                "version":1,"duration":16.,"step":1.,"curve":[0.2,0.8],
                "sections":[{"id":"drop","name":"High energy","start":8.,"end":16.,"energy":0.8}]
            }));
            amp_core::preparation::save(root.path(), std::path::Path::new(name), &prep).unwrap();
        }
        let library = amp_core::scan(root.path()).unwrap();
        let saved: Vec<_> = library
            .tracks
            .iter()
            .map(|t| amp_core::preparation::load(&root.path().join(&t.path)).unwrap())
            .collect();
        let mut player = Player {
            root: root.path().into(),
            library: Some(library),
            channel_gain: [0.; 6],
            fade_seconds: 3.,
            ..Default::default()
        };
        player.run.profile = Some(player.run.settings.playlists[0].clone());
        player.run.target = Some(8.2);
        player.run.transitions.enabled = false;
        player.run.transitions.timing = crate::mix_schedule::Timing::Immediate;
        let songs: Vec<_> = player
            .library
            .as_ref()
            .unwrap()
            .tracks
            .iter()
            .map(show::song_id)
            .collect();
        player
            .execute_control(Action::CallSong {
                song: songs[0].clone(),
                cue: None,
                deck: Deck::A,
                execution: Execution::PlayNow,
            })
            .unwrap();
        wait(&mut player, |p| {
            p.audio.as_ref().is_some_and(|a| a.position(0) > 0.1)
        });
        player.playback_owner = crate::playback_owner::Owner::Dj;
        player.run.auto = true;
        player
            .execute_control(Action::CallSong {
                song: songs[1].clone(),
                cue: None,
                deck: Deck::B,
                execution: Execution::AutoFade,
            })
            .unwrap();
        let prepared = &player.loaded_track(1).unwrap().preparation;
        assert_eq!(prepared.cue_in, 5.);
        assert_eq!(prepared.extensions["runtime_mix_landing"], 8.);
        wait(&mut player, |p| p.run.completed_handoffs == 1);
        assert!(player.run.auto);
        assert_eq!(player.fade_seconds, 3.);
        for (track, expected) in player.library.as_ref().unwrap().tracks.iter().zip(saved) {
            assert_eq!(
                amp_core::preparation::load(&root.path().join(&track.path)).unwrap(),
                expected
            );
        }
        player
            .execute_control(Action::AutoDj { enabled: false })
            .unwrap();
    }
    #[test]
    #[ignore = "uses AMP_ENERGY_FIXTURE_ROOT and native audio; both channels muted"]
    fn real_energy_profile_two_deck_handoff_preserves_preparation_and_keeps_auto_running() {
        let root = std::path::PathBuf::from(std::env::var_os("AMP_ENERGY_FIXTURE_ROOT").unwrap());
        let mut library = amp_core::scan(&root).unwrap();
        library.tracks.retain(|t| t.preparation.bpm.is_some());
        assert!(library.tracks.len() >= 2);
        let before: Vec<_> = library
            .tracks
            .iter()
            .map(|t| amp_core::preparation::revision(&root.join(&t.path)))
            .collect();
        let mut player = Player {
            root: root.clone(),
            library: Some(library),
            channel_gain: [0.; 6],
            fade_seconds: 0.75,
            ..Default::default()
        };
        player.run.profile = Some(player.run.settings.playlists[0].clone());
        player.run.target = Some(7.);
        let first = show::song_id(&player.library.as_ref().unwrap().tracks[0]);
        let second = show::song_id(&player.library.as_ref().unwrap().tracks[1]);
        player
            .execute_control(Action::CallSong {
                song: first,
                cue: None,
                deck: Deck::A,
                execution: Execution::PlayNow,
            })
            .unwrap();
        wait(&mut player, |p| {
            p.audio.as_ref().is_some_and(|a| a.position(0) > 0.1)
        });
        player.playback_owner = crate::playback_owner::Owner::Dj;
        player.run.auto = true;
        player
            .execute_control(Action::CallSong {
                song: second,
                cue: None,
                deck: Deck::B,
                execution: Execution::AutoFade,
            })
            .unwrap();
        let t = &player.library.as_ref().unwrap().tracks[1];
        let profile = amp_core::energy_profile::read(&t.preparation)
            .expect("analyze fixture with engine v4 first");
        let expected = profile
            .entry(
                (7. - 1.) / 9.,
                amp_core::energy_profile::entry_bound(&t.preparation),
                t.preparation.cue_out.unwrap_or(profile.duration),
                8.,
            )
            .unwrap()
            .start;
        let loaded = &player.loaded_track(1).unwrap().preparation;
        let rate = 1. + player.pitch[1] as f64 / 100.;
        let entry = amp_core::transition_plan::landing(&t.preparation, expected, rate, 0.75)
            .map_or(expected, |plan| plan.entry);
        assert!((loaded.cue_in - entry).abs() < 0.001);
        let outgoing = player.loaded_track(0).unwrap().preparation.bpm.unwrap();
        let incoming = player.loaded_track(1).unwrap().preparation.bpm.unwrap();
        assert!((incoming * (1. + player.pitch[1] as f64 / 100.) - outgoing).abs() < 0.01);
        wait(&mut player, |p| p.run.completed_handoffs == 1);
        assert!(player.run.auto);
        assert!(player.audio.as_ref().unwrap().playing(1));
        assert_eq!(player.fade_seconds, 0.75);
        for (i, t) in player.library.as_ref().unwrap().tracks.iter().enumerate() {
            assert_eq!(
                amp_core::preparation::revision(&root.join(&t.path)),
                before[i]
            );
        }
        println!(
            "Energy section handoff completed on two decks; entry {expected:.2}s, Auto DJ retained, metadata unchanged"
        );
    }
    #[test]
    fn stem_handoff_preserves_muted_parts_and_brings_in_vocals_before_backing() {
        let original = [[0., 0.7, 1.], [0.8, 0., 0.6]];
        assert_eq!(super::blend_stems(0., original)[0], original[0]);
        assert_eq!(super::blend_stems(0., original)[1], [0.; 3]);
        let middle = super::blend_stems(0.35, original);
        assert_eq!(
            middle[1],
            [0., 0., 0.],
            "Both vocals must be silent in the guard gap"
        );
        let arrived = super::blend_stems(0.70, original);
        assert_eq!(arrived[1], [0.8, 0., 0.6]);
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
        player.run.profile.as_mut().unwrap().genres = vec!["Rock".into()];
        assert!(
            player
                .execute_control(Action::RequestSong {
                    song: "Song 0".into(),
                    within: 5
                })
                .unwrap_err()
                .contains("genre")
        );
        assert!(player.run.requests.is_empty());
        player.run.profile.as_mut().unwrap().genres.clear();
        player.run.profile.as_mut().unwrap().filters =
            vec![amp_core::smart_filters::Filter::Number {
                field: amp_core::smart_filters::Field::Bpm,
                min: 90.,
                max: 110.,
            }];
        assert!(
            player
                .execute_control(Action::RequestSong {
                    song: "Song 0".into(),
                    within: 5
                })
                .is_err()
        );
        assert!(player.run.requests.is_empty());
        player.run.profile.as_mut().unwrap().filters.clear();

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
                channel_gain: [0.; 6],
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
    fn unprepared_scratch_pair_falls_back_and_keeps_auto_dj_running() {
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
            channel_gain: [0.; 6],
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
    fn smart_playlist_id_wins_over_another_playlists_name() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("song.mp3"), b"fixture").unwrap();
        let mut player = Player {
            root: root.path().into(),
            library: Some(amp_core::scan(root.path()).unwrap()),
            ..Default::default()
        };
        let mut target = player.run.settings.playlists[0].clone();
        target.id = "target-id".into();
        target.name = "Intended playlist".into();
        target.allowed.clear();
        target.restrict_allowed = false;
        target.folders.clear();
        target.restrict_folders = false;
        let mut collision = target.clone();
        collision.id = "wrong-id".into();
        collision.name = target.id.clone();
        player.run.settings.playlists = vec![collision, target];
        player
            .execute_control(Action::LoadSmartPlaylist {
                playlist: "target-id".into(),
                execution: Execution::LoadOnly,
                style: None,
                energy: None,
            })
            .unwrap();
        assert_eq!(player.run.playlist.as_deref(), Some("target-id"));
        assert_eq!(
            player.run.profile.as_ref().unwrap().name,
            "Intended playlist"
        );
        player.run.settings.playlists[1].name = "Renamed saved playlist".into();
        assert_eq!(
            player.run.profile.as_ref().unwrap().name,
            "Intended playlist"
        );
        assert!(!player.run.auto);
        assert!(player.audio.is_none());
        assert_eq!(
            player.status,
            "Intended playlist selected for Auto Mix (1 matching track). Turn Auto Mix ON to load and play its songs."
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
        player.playback_owner = crate::playback_owner::Owner::Dj;
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
        assert!(
            player
                .execute_control(Action::AutoDj { enabled: true })
                .is_err()
        );
        assert!(!player.run.auto);
        player.playback_owner = crate::playback_owner::Owner::Dj;
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
    fn large_tempo_gap_uses_clean_transition_and_can_cancel() {
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
            channel_gain: [0.; 6],
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
        wait(&mut player, |p| p.run.transition.is_some());
        assert!(!player.audio.as_ref().unwrap().controls.scratching[0].load(Relaxed));
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
            channel_gain: [0.; 6],
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
        // A phrase boundary beyond EOF must never strand a prepared next deck.
        assert!(player.run.pending.is_some());
        player.run.pending_at = Some(1000.);
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
            channel_gain: [0.; 6],
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
            channel_gain: [0.; 6],
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
            channel_gain: [0.; 6],
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
        assert!(!player.run.auto);
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
