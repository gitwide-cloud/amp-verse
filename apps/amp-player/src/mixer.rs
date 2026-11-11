//! Shared two-deck device stream. FFmpeg decoding stays off the audio callback.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, PushError, RingBuffer};
use std::{
    collections::VecDeque,
    io::Read,
    path::Path,
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed},
        mpsc,
    },
    time::Duration,
};
type Frame = [f32; 2];
type DeckFrame = [f32; 6];
fn atom(v: f32) -> AtomicU32 {
    AtomicU32::new(v.to_bits())
}
fn get(v: &AtomicU32) -> f32 {
    f32::from_bits(v.load(Relaxed))
}
fn set(v: &AtomicU32, x: f32) {
    v.store(x.to_bits(), Relaxed);
}
pub struct Controls {
    pub sample_playing: [AtomicBool; 9],
    pub sample_looping: [AtomicBool; 9],
    sample_generation: [AtomicU64; 9],
    sample_gain: [AtomicU32; 9],
    sample_master: AtomicU32,
    pub deck_generation: [AtomicU64; 2],
    pub level_fading: AtomicBool,
    pub fade_progress: AtomicU32,
    pub play: [AtomicBool; 2],
    pub loaded: [AtomicBool; 2],
    pub ready: [AtomicBool; 2],
    pub finished: [AtomicBool; 2],
    pub gain: [AtomicU32; 2],
    pub rate: [AtomicU32; 2],
    pub scratching: [AtomicBool; 2],
    pub scratch_rate: [AtomicU32; 2],
    vinyl_target: [AtomicU64; 2],
    vinyl_follow: [AtomicBool; 2],
    pub section_state: [AtomicU32; 2],
    source_origin: [AtomicU64; 2],
    section_epoch: [AtomicU64; 2],
    pub eq: [[AtomicU32; 3]; 2],
    pub trim_db: [AtomicU32; 2],
    pub stem_level: [[AtomicU32; 3]; 2],
    pub stem_knobs: [[AtomicU32; 3]; 2],
    pub stems_loaded: [AtomicBool; 2],
    pub eq_kill: [[AtomicBool; 3]; 2],
    pub sweep: [AtomicU32; 2],
    pub cue: [AtomicBool; 2],
    pub cue_blend: AtomicU32,
    pub cue_volume: AtomicU32,
    pub cutoff: [AtomicU32; 2],
    pub position: [AtomicU64; 2],
    pub duration_frames: [AtomicU64; 2],
    pub peak: [AtomicU32; 2],
    pub scope: [[AtomicU32; 96]; 2],
    pub looping: [AtomicBool; 2],
    pub crossfader: AtomicU32,
    pub master: AtomicU32,
    pub fading: AtomicBool,
    pub mashup_active: AtomicBool,
    pub underruns: [AtomicU64; 2],
    pub rendered: AtomicU64,
    pub error: Mutex<Option<String>>,
    pub monitor_error: Mutex<Option<String>>,
    pub decode_errors: Mutex<Vec<(usize, u64, String)>>,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            sample_playing: std::array::from_fn(|_| AtomicBool::new(false)),
            sample_looping: std::array::from_fn(|_| AtomicBool::new(false)),
            sample_generation: std::array::from_fn(|_| AtomicU64::new(0)),
            sample_gain: std::array::from_fn(|_| atom(1.)),
            sample_master: atom(0.7),
            deck_generation: std::array::from_fn(|_| AtomicU64::new(0)),
            level_fading: AtomicBool::new(false),
            fade_progress: atom(0.),
            play: std::array::from_fn(|_| AtomicBool::new(false)),
            loaded: std::array::from_fn(|_| AtomicBool::new(false)),
            ready: std::array::from_fn(|_| AtomicBool::new(false)),
            finished: std::array::from_fn(|_| AtomicBool::new(false)),
            gain: std::array::from_fn(|_| atom(1.)),
            rate: std::array::from_fn(|_| atom(1.)),
            scratching: std::array::from_fn(|_| AtomicBool::new(false)),
            scratch_rate: std::array::from_fn(|_| atom(0.)),
            vinyl_target: std::array::from_fn(|_| AtomicU64::new(0)),
            vinyl_follow: std::array::from_fn(|_| AtomicBool::new(false)),
            section_state: std::array::from_fn(|_| AtomicU32::new(0)),
            source_origin: std::array::from_fn(|_| AtomicU64::new(0)),
            section_epoch: std::array::from_fn(|_| AtomicU64::new(0)),
            eq: std::array::from_fn(|_| std::array::from_fn(|_| atom(0.))),
            trim_db: std::array::from_fn(|_| atom(0.)),
            stem_level: std::array::from_fn(|_| std::array::from_fn(|_| atom(1.))),
            stem_knobs: std::array::from_fn(|_| std::array::from_fn(|_| atom(0.))),
            stems_loaded: std::array::from_fn(|_| AtomicBool::new(false)),
            eq_kill: std::array::from_fn(|_| std::array::from_fn(|_| AtomicBool::new(false))),
            sweep: std::array::from_fn(|_| atom(0.)),
            cue: std::array::from_fn(|_| AtomicBool::new(false)),
            cue_blend: atom(0.),
            cue_volume: atom(0.5),
            cutoff: std::array::from_fn(|_| atom(22000.)),
            position: std::array::from_fn(|_| AtomicU64::new(0)),
            duration_frames: std::array::from_fn(|_| AtomicU64::new(0)),
            peak: std::array::from_fn(|_| atom(0.)),
            scope: std::array::from_fn(|_| std::array::from_fn(|_| atom(0.))),
            looping: std::array::from_fn(|_| AtomicBool::new(false)),
            crossfader: atom(0.),
            master: atom(0.75),
            fading: AtomicBool::new(false),
            mashup_active: AtomicBool::new(false),
            underruns: std::array::from_fn(|_| AtomicU64::new(0)),
            rendered: AtomicU64::new(0),
            error: Mutex::new(None),
            monitor_error: Mutex::new(None),
            decode_errors: Mutex::new(Vec::new()),
        }
    }
}
struct Source {
    ring: Consumer<DeckFrame>,
    duration: Arc<AtomicU64>,
    done: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    history: VecDeque<DeckFrame>,
    history_start: u64,
    position: f64,
    primed: bool,
    separated: bool,
    stem_knobs: [f32; 3],
}
impl Source {
    fn buffered(&self, rate: u32) -> bool {
        self.ring.slots() >= rate as usize / 4
            || (self.done.load(std::sync::atomic::Ordering::Acquire) && self.ring.slots() > 1)
    }
    fn next(&mut self, rate: f32) -> Option<DeckFrame> {
        if rate.abs() < 0.0001 {
            return Some([0.; 6]);
        }
        self.position = self.position.max(self.history_start as f64);
        let index = self.position.floor() as u64;
        while self.history_start + self.history.len() as u64 <= index + 1 {
            let frame = self.ring.pop().ok()?;
            // Capacity is reserved on the decoder/UI thread, never in the callback.
            if self.history.len() == self.history.capacity() {
                self.history.pop_front();
                self.history_start += 1;
            }
            self.history.push_back(frame);
        }
        let offset = (index - self.history_start) as usize;
        let a = *self.history.get(offset)?;
        let b = *self.history.get(offset + 1)?;
        let fraction = self.position.fract() as f32;
        let frame =
            std::array::from_fn(|channel| a[channel] + (b[channel] - a[channel]) * fraction);
        self.position = (self.position + rate as f64).max(self.history_start as f64);
        Some(frame)
    }
}
struct Loop {
    frames: Vec<DeckFrame>,
    length: usize,
    phase: f64,
    origin: f64,
    rate: f32,
    capturing: bool,
}
#[derive(Default)]
struct Voice {
    source: Option<Source>,
    low: Frame,
    high: Frame,
    filter: Frame,
    sweep_state: Frame,
    trim: f32,
    stem_mix: [f32; 3],
    gain: f32,
    looped: Option<Loop>,
}
struct LevelFade {
    from: f32,
    to: f32,
    total: u64,
    elapsed: u64,
    stop: bool,
}
struct Fade {
    from: f32,
    to: f32,
    total: u64,
    elapsed: u64,
    incoming: usize,
    started: bool,
    grid: Option<crate::mix_schedule::Grid>,
    boundary: Option<f64>,
    mashup: Option<(u64, u64)>,
}
#[derive(Default)]
struct SampleVoice {
    frames: Option<Arc<[Frame]>>,
    position: usize,
    playing: bool,
}
enum Command {
    Monitor(Option<rtrb::Producer<Frame>>),
    Broadcast(Option<rtrb::Producer<Frame>>),
    SampleLoad(usize, u64, Option<Arc<[Frame]>>),
    SampleTrigger(usize),
    SampleStop(Option<usize>),
    Load(usize, Source, bool, u64),
    Level(usize, f32, f32, bool),
    LevelSet(usize, f32),
    Stop(usize),
    Fader(f32),
    Fade {
        to: f32,
        seconds: f32,
    },
    ScheduledFade {
        to: f32,
        seconds: f32,
        grid: crate::mix_schedule::Grid,
    },
    Mashup {
        to: f32,
        fade: f32,
        hold: f32,
        grid: crate::mix_schedule::Grid,
    },
    Section(usize, SectionChange),
    CancelSection(usize),
    Cancel,
    Loop(usize, Option<Vec<DeckFrame>>),
    LoopHalve(usize),
}
struct SectionChange {
    source: Option<Source>,
    origin: f64,
    previous_origin: f64,
    grid: Option<crate::mix_schedule::Grid>,
    at: Option<f64>,
    expected: u64,
    total: u64,
    elapsed: u64,
    old: Option<Source>,
}
struct Mixer {
    sections: [Option<SectionChange>; 2],
    voices: [Voice; 2],
    samples: [SampleVoice; 9],
    commands: mpsc::Receiver<Command>,
    pub controls: Arc<Controls>,
    rate: u32,
    fade: Option<Fade>,
    levels: [Option<LevelFade>; 3],
    monitor: Option<rtrb::Producer<Frame>>,
    broadcast: Option<rtrb::Producer<Frame>>,
}
impl Mixer {
    fn new(rate: u32, commands: mpsc::Receiver<Command>, controls: Arc<Controls>) -> Self {
        Self {
            sections: Default::default(),
            voices: Default::default(),
            samples: Default::default(),
            commands,
            controls,
            rate,
            fade: None,
            levels: Default::default(),
            monitor: None,
            broadcast: None,
        }
    }
    fn process_commands(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                Command::Monitor(producer) => self.monitor = producer,
                Command::Broadcast(producer) => self.broadcast = producer,
                Command::SampleLoad(pad, generation, frames) => {
                    self.samples[pad] = SampleVoice {
                        frames,
                        ..Default::default()
                    };
                    self.controls.sample_playing[pad].store(false, Relaxed);
                    // Old buffers remain owned by Engine until this acknowledgement.
                    self.controls.sample_generation[pad]
                        .store(generation, std::sync::atomic::Ordering::Release);
                }
                Command::SampleTrigger(pad) => {
                    let v = &mut self.samples[pad];
                    v.position = 0;
                    v.playing = v.frames.as_ref().is_some_and(|s| !s.is_empty());
                    self.controls.sample_playing[pad].store(v.playing, Relaxed);
                }
                Command::SampleStop(pad) => {
                    for (i, v) in self.samples.iter_mut().enumerate() {
                        if pad.is_none_or(|p| p == i) {
                            v.playing = false;
                            v.position = 0;
                            self.controls.sample_playing[i].store(false, Relaxed);
                        }
                    }
                }

                Command::Level(slot, to, seconds, stop) => {
                    let from = if slot == 2 {
                        get(&self.controls.master)
                    } else {
                        get(&self.controls.gain[slot])
                    };
                    self.levels[slot] = Some(LevelFade {
                        from,
                        to,
                        total: (seconds * self.rate as f32).max(1.) as u64,
                        elapsed: 0,
                        stop,
                    });
                }
                Command::LevelSet(slot, value) => {
                    self.levels[slot] = None;
                    if slot == 2 {
                        set(&self.controls.master, value);
                    } else {
                        set(&self.controls.gain[slot], value);
                    }
                }
                Command::CancelSection(d) => {
                    self.sections[d] = None;
                    self.controls.section_state[d].store(0, Relaxed);
                }
                Command::Section(d, s) => {
                    self.sections[d] = Some(s);
                    self.controls.section_state[d].store(1, Relaxed);
                }
                Command::Mashup {
                    to,
                    fade,
                    hold,
                    grid,
                } => {
                    let fade = (fade * self.rate as f32).max(1.) as u64;
                    let hold = (hold * self.rate as f32).max(1.) as u64;
                    self.fade = Some(Fade {
                        from: get(&self.controls.crossfader),
                        to,
                        total: fade * 2 + hold,
                        elapsed: 0,
                        incoming: usize::from(to > 0.5),
                        started: false,
                        grid: Some(grid),
                        boundary: None,
                        mashup: Some((fade, hold)),
                    });
                    set(&self.controls.fade_progress, 0.);
                    self.controls.fading.store(true, Relaxed);
                    self.controls.mashup_active.store(true, Relaxed);
                }
                Command::ScheduledFade { to, seconds, grid } => {
                    set(&self.controls.fade_progress, 0.);
                    self.fade = Some(Fade {
                        from: get(&self.controls.crossfader),
                        to,
                        total: (seconds * self.rate as f32).max(1.) as u64,
                        elapsed: 0,
                        incoming: usize::from(to > 0.5),
                        started: false,
                        grid: Some(grid),
                        boundary: None,
                        mashup: None,
                    });
                    self.controls.fading.store(true, Relaxed);
                }
                Command::Load(d, s, play, generation) => {
                    self.sections[d] = None;
                    self.controls.section_state[d].store(0, Relaxed);
                    self.levels[d] = None;
                    self.controls.scratching[d].store(false, Relaxed);
                    self.controls.play[d].store(play, Relaxed);
                    self.controls.stems_loaded[d].store(s.separated, Relaxed);
                    let gains = amp_core::stem_mix::gains(s.stem_knobs);
                    for (i, gain) in gains.iter().enumerate() {
                        set(&self.controls.stem_knobs[d][i], s.stem_knobs[i]);
                        set(&self.controls.stem_level[d][i], *gain);
                    }
                    self.voices[d] = Voice {
                        source: Some(s),
                        stem_mix: gains,
                        trim: 10f32.powf(get(&self.controls.trim_db[d]).clamp(-12., 12.) / 20.),
                        ..Default::default()
                    };
                    self.controls.loaded[d].store(true, Relaxed);
                    self.controls.finished[d].store(false, Relaxed);
                    self.controls.ready[d].store(false, Relaxed);
                    self.controls.looping[d].store(false, Relaxed);
                    self.controls.position[d].store(0, Relaxed);
                    self.controls.deck_generation[d]
                        .store(generation, std::sync::atomic::Ordering::Release);
                    self.fade = None;
                    self.controls.mashup_active.store(false, Relaxed);
                    self.controls.fading.store(false, Relaxed);
                }
                Command::Stop(d) => {
                    self.sections[d] = None;
                    self.controls.section_state[d].store(0, Relaxed);
                    self.levels[d] = None;
                    self.levels[2] = None;
                    self.voices[d] = Voice::default();
                    self.controls.play[d].store(false, Relaxed);
                    self.controls.loaded[d].store(false, Relaxed);
                    self.controls.ready[d].store(false, Relaxed);
                    self.controls.looping[d].store(false, Relaxed);
                    self.fade = None;
                    self.controls.mashup_active.store(false, Relaxed);
                    self.controls.fading.store(false, Relaxed);
                }
                Command::Fader(x) => {
                    set(&self.controls.crossfader, x.clamp(0., 1.));
                    self.fade = None;
                    self.controls.mashup_active.store(false, Relaxed);
                    self.controls.fading.store(false, Relaxed);
                }
                Command::Fade { to, seconds } => {
                    set(&self.controls.fade_progress, 0.);
                    self.fade = Some(Fade {
                        from: get(&self.controls.crossfader),
                        to,
                        total: (seconds * self.rate as f32).max(1.) as u64,
                        elapsed: 0,
                        incoming: usize::from(to > 0.5),
                        started: false,
                        grid: None,
                        boundary: None,
                        mashup: None,
                    });
                    self.controls.fading.store(true, Relaxed);
                }
                Command::Cancel => {
                    self.sections = Default::default();
                    for d in 0..2 {
                        self.controls.section_state[d].store(0, Relaxed);
                    }
                    self.fade = None;
                    self.controls.mashup_active.store(false, Relaxed);
                    self.controls.fading.store(false, Relaxed);
                }
                Command::LoopHalve(d) => {
                    if let Some(l) = &mut self.voices[d].looped
                        && l.length >= 4
                    {
                        l.length /= 2;
                        if l.frames.len() >= l.length {
                            l.frames.truncate(l.length);
                            l.phase %= l.length as f64;
                            l.capturing = false;
                        }
                    }
                }
                Command::Loop(d, frames) => {
                    if frames.is_none()
                        && let Some(old) = self.voices[d].looped.take()
                        && let Some(source) = self.voices[d].source.as_mut()
                        && !old.capturing
                    {
                        source.position = old.origin + old.frames.len() as f64 * old.rate as f64;
                    }
                    self.voices[d].looped = frames.map(|frames| Loop {
                        length: frames.capacity(),
                        frames,
                        phase: 0.,
                        origin: self.voices[d].source.as_ref().map_or(0., |s| s.position),
                        rate: get(&self.controls.rate[d]),
                        capturing: true,
                    });
                    self.controls.looping[d].store(self.voices[d].looped.is_some(), Relaxed);
                }
            }
        }
    }
    fn render(&mut self, frames: usize, mut output: impl FnMut(usize, Frame)) {
        self.process_commands();
        let c = &self.controls;
        let rates = std::array::from_fn::<_, 2, _>(|d| {
            if c.scratching[d].load(Relaxed) {
                get(&c.scratch_rate[d]).clamp(-4., 4.)
            } else {
                get(&c.rate[d]).clamp(0.5, 2.)
            }
        });

        let eq = std::array::from_fn::<_, 2, _>(|d| {
            std::array::from_fn::<_, 3, _>(|b| {
                if c.eq_kill[d][b].load(Relaxed) {
                    0.
                } else {
                    10f32.powf(get(&c.eq[d][b]).clamp(-24., 12.) / 20.)
                }
            })
        });
        let trims = std::array::from_fn::<_, 2, _>(|d| {
            10f32.powf(get(&c.trim_db[d]).clamp(-12., 12.) / 20.)
        });
        let sweeps = std::array::from_fn::<_, 2, _>(|d| get(&c.sweep[d]).clamp(-1., 1.));
        let sweep_alpha = std::array::from_fn::<_, 2, _>(|d| {
            let hz = if sweeps[d] < 0. {
                20000. * (30_f32 / 20000.).powf(-sweeps[d])
            } else {
                30. * (15000_f32 / 30.).powf(sweeps[d])
            };
            1. - (-std::f32::consts::TAU * hz.min(self.rate as f32 * 0.45) / self.rate as f32).exp()
        });
        let alpha_low = 1. - (-std::f32::consts::TAU * 200. / self.rate as f32).exp();
        let alpha_high = 1. - (-std::f32::consts::TAU * 2000. / self.rate as f32).exp();
        let filter = std::array::from_fn::<_, 2, _>(|d| {
            1. - (-std::f32::consts::TAU * get(&c.cutoff[d]).min(self.rate as f32 * 0.45)
                / self.rate as f32)
                .exp()
        });

        let mut peaks = [0f32; 2];
        let mut waves = [[0f32; 96]; 2];
        for d in 0..2 {
            if let Some(source) = &mut self.voices[d].source {
                c.duration_frames[d].store(source.duration.load(Relaxed), Relaxed);
                if !source.primed && source.buffered(self.rate) {
                    source.primed = true;
                }
                if !source.primed
                    && source.done.load(std::sync::atomic::Ordering::Acquire)
                    && source.ring.slots() < 2
                {
                    c.play[d].store(false, Relaxed);
                    c.finished[d].store(true, Relaxed);
                }
                c.ready[d].store(source.primed && !c.finished[d].load(Relaxed), Relaxed);
            }
        }
        for i in 0..frames {
            for slot in 0..3 {
                if let Some(fade) = &mut self.levels[slot] {
                    fade.elapsed = (fade.elapsed + 1).min(fade.total);
                    let progress = fade.elapsed as f32 / fade.total as f32;
                    let value = fade.from + (fade.to - fade.from) * progress;
                    if slot == 2 {
                        set(&c.master, value);
                    } else {
                        set(&c.gain[slot], value);
                    }
                    set(&c.fade_progress, progress);
                    if fade.elapsed == fade.total {
                        if fade.stop {
                            for d in 0..2 {
                                if slot == 2 || slot == d {
                                    c.play[d].store(false, Relaxed);
                                    c.loaded[d].store(false, Relaxed);
                                    c.ready[d].store(false, Relaxed);
                                    c.looping[d].store(false, Relaxed);
                                    c.finished[d].store(true, Relaxed);
                                    self.voices[d] = Voice::default();
                                }
                            }
                            if slot == 2 {
                                for (pad, v) in self.samples.iter_mut().enumerate() {
                                    v.playing = false;
                                    c.sample_playing[pad].store(false, Relaxed);
                                }
                            }
                        }
                        self.levels[slot] = None;
                    }
                }
            }
            c.level_fading
                .store(self.levels.iter().any(Option::is_some), Relaxed);

            if self
                .fade
                .as_ref()
                .is_some_and(|fade| c.finished[fade.incoming].load(Relaxed))
            {
                if let Some(fade) = &self.fade {
                    set(&c.crossfader, (1 - fade.incoming) as f32);
                }
                self.fade = None;
                c.fading.store(false, Relaxed);
            }
            let mut mashup_mix = None;
            if let Some(fade) = &mut self.fade {
                let outgoing = 1 - fade.incoming;
                let position = self.voices[outgoing]
                    .source
                    .as_ref()
                    .map_or(0., |s| s.position / self.rate as f64);
                let due = if let Some(grid) = fade.grid {
                    if fade.boundary.is_none() && c.ready[fade.incoming].load(Relaxed) {
                        fade.boundary = Some(grid.next(position + 32. / self.rate as f64));
                    }
                    fade.boundary.is_some_and(|at| position >= at)
                } else {
                    true
                };
                if !fade.started && due && c.ready[fade.incoming].load(Relaxed) {
                    c.play[fade.incoming].store(true, Relaxed);
                    fade.started = true;
                }
                if fade.started {
                    if let Some((ramp, _)) = fade.mashup
                        && c.finished[outgoing].load(Relaxed)
                    {
                        fade.elapsed = fade.elapsed.max(fade.total.saturating_sub(ramp));
                    }
                    if let Some((ramp, hold)) = fade.mashup {
                        mashup_mix = Some(crate::mashup::envelope(
                            fade.elapsed,
                            ramp,
                            hold,
                            fade.incoming,
                        ));
                    }
                    let t = (fade.elapsed as f32 / fade.total as f32).clamp(0., 1.);
                    set(&c.fade_progress, t);
                    set(&c.crossfader, fade.from + (fade.to - fade.from) * t);
                    fade.elapsed += 1;
                    if fade.elapsed > fade.total {
                        set(&c.crossfader, fade.to);
                        c.play[1 - fade.incoming].store(false, Relaxed);
                        c.fading.store(false, Relaxed);
                        self.fade = None;
                        self.controls.mashup_active.store(false, Relaxed);
                    }
                }
            }
            for d in 0..2 {
                let Some(pending) = &mut self.sections[d] else {
                    continue;
                };
                if pending.expected != c.deck_generation[d].load(Relaxed)
                    || c.scratching[d].load(Relaxed)
                    || c.looping[d].load(Relaxed)
                {
                    self.sections[d] = None;
                    c.section_state[d].store(0, Relaxed);
                    continue;
                }
                let pos = self.voices[d]
                    .source
                    .as_ref()
                    .map_or(0., |s| s.position / self.rate as f64);
                if pending.old.is_none() {
                    let Some(source) = &mut pending.source else {
                        continue;
                    };
                    if source.failed.load(std::sync::atomic::Ordering::Acquire) {
                        self.sections[d] = None;
                        c.section_state[d].store(3, Relaxed);
                        continue;
                    }
                    if !source.primed && source.buffered(self.rate) {
                        source.primed = true;
                    }
                    if !source.primed && source.done.load(std::sync::atomic::Ordering::Acquire) {
                        self.sections[d] = None;
                        c.section_state[d].store(3, Relaxed);
                        continue;
                    }
                    if pending.at.is_none() && source.primed {
                        pending.at = Some(
                            pending
                                .grid
                                .map_or(pos, |g| g.next(pos + 32. / self.rate as f64)),
                        );
                    }
                    if (c.play[d].load(Relaxed) && pending.at.is_some_and(|at| pos >= at))
                        || (source.primed && c.finished[d].load(Relaxed))
                    {
                        c.play[d].store(true, Relaxed);
                        c.finished[d].store(false, Relaxed);
                        c.ready[d].store(true, Relaxed);
                        pending.previous_origin = f64::from_bits(c.source_origin[d].load(Relaxed));
                        pending.old =
                            std::mem::replace(&mut self.voices[d].source, pending.source.take());
                        c.source_origin[d].store(pending.origin.to_bits(), Relaxed);
                        c.section_epoch[d].fetch_add(1, Relaxed);
                        c.section_state[d].store(2, Relaxed);
                    }
                }
            }
            let x = get(&c.crossfader);
            let mut weights = amp_core::show::crossfade_weights(x);
            let mut stem_masks = [[1.; 3]; 2];
            if let Some((buses, masks)) = mashup_mix {
                weights = buses;
                stem_masks = masks;
            }
            c.mashup_active.store(
                self.fade.as_ref().is_some_and(|f| f.mashup.is_some()),
                Relaxed,
            );
            let mut mix = [0.; 2];
            let mut cue_mix = [0.; 2];
            for d in 0..2 {
                let v = &mut self.voices[d];
                if !c.play[d].load(Relaxed) {
                    continue;
                }
                let Some(source) = &mut v.source else {
                    continue;
                };
                if !source.primed {
                    continue;
                }
                let repeating = v.looped.as_ref().is_some_and(|l| !l.capturing);
                let mut frame = if repeating {
                    let l = v.looped.as_mut().unwrap();
                    let a = l.phase as usize % l.frames.len();
                    let b = (a + 1) % l.frames.len();
                    let f = l.phase.fract() as f32;
                    let frame = std::array::from_fn(|ch| {
                        l.frames[a][ch] + (l.frames[b][ch] - l.frames[a][ch]) * f
                    });
                    l.phase = (l.phase + (rates[d] / l.rate) as f64) % l.frames.len() as f64;
                    source.position = l.origin + l.phase * l.rate as f64;
                    Some(frame)
                } else {
                    let rate = if c.vinyl_follow[d].load(Relaxed) && c.scratching[d].load(Relaxed) {
                        let target = (f64::from_bits(c.vinyl_target[d].load(Relaxed)))
                            .max(source.history_start as f64);
                        ((target - source.position) * 0.025).clamp(-4., 4.) as f32
                    } else {
                        rates[d]
                    };
                    source.next(rate)
                };
                if frame.is_none()
                    && let Some(pending) = &mut self.sections[d]
                    && let Some(old) = pending.old.take()
                {
                    *source = old;
                    frame = source.next(rates[d]);
                    c.source_origin[d].store(pending.previous_origin.to_bits(), Relaxed);
                    c.section_epoch[d].fetch_sub(1, Relaxed);
                    c.section_state[d].store(3, Relaxed);
                    self.sections[d] = None;
                }
                let Some(mut frame) = frame else {
                    if source.done.load(std::sync::atomic::Ordering::Acquire) {
                        c.play[d].store(false, Relaxed);
                        c.finished[d].store(true, Relaxed);
                        c.ready[d].store(false, Relaxed);
                    } else {
                        c.underruns[d].fetch_add(1, Relaxed);
                    }
                    continue;
                };
                if let Some(pending) = &mut self.sections[d]
                    && let Some(old) = &mut pending.old
                {
                    if let Some(previous) = old.next(rates[d]) {
                        let t = (pending.elapsed as f32 / pending.total.max(1) as f32).min(1.);
                        for ch in 0..6 {
                            frame[ch] = previous[ch] * (1. - t) + frame[ch] * t;
                        }
                    }
                    pending.elapsed += 1;
                    if pending.elapsed >= pending.total {
                        self.sections[d] = None;
                        c.section_state[d].store(0, Relaxed);
                    }
                }
                if let Some(l) = &mut v.looped
                    && l.capturing
                {
                    l.frames.push(frame);
                    if l.frames.len() >= l.length {
                        l.capturing = false;
                    }
                }
                for (stem, mask) in stem_masks[d].iter().enumerate() {
                    v.stem_mix[stem] += (get(&c.stem_level[d][stem]).clamp(0., 1.) * mask
                        - v.stem_mix[stem])
                        * 0.004;
                }
                let mut frame: Frame = if source.separated {
                    std::array::from_fn(|ch| {
                        frame[ch] * v.stem_mix[0]
                            + frame[ch + 2] * v.stem_mix[1]
                            + frame[ch + 4] * v.stem_mix[2]
                    })
                } else {
                    [frame[0], frame[1]]
                };
                v.gain += (get(&c.gain[d]).clamp(0., 1.) - v.gain) * 0.004;
                v.trim += (trims[d] - v.trim) * 0.004;
                for ch in 0..2 {
                    frame[ch] *= v.trim;
                    v.low[ch] += alpha_low * (frame[ch] - v.low[ch]);
                    v.high[ch] += alpha_high * (frame[ch] - v.high[ch]);
                    let value = v.low[ch] * eq[d][0]
                        + (v.high[ch] - v.low[ch]) * eq[d][1]
                        + (frame[ch] - v.high[ch]) * eq[d][2];
                    v.filter[ch] += filter[d] * (value - v.filter[ch]);
                    frame[ch] = if get(&c.cutoff[d]) >= 20000. {
                        value
                    } else {
                        v.filter[ch]
                    };
                    v.sweep_state[ch] += sweep_alpha[d] * (frame[ch] - v.sweep_state[ch]);
                    if sweeps[d] < -0.001 {
                        frame[ch] = v.sweep_state[ch];
                    } else if sweeps[d] > 0.001 {
                        frame[ch] -= v.sweep_state[ch];
                    }
                    if c.cue[d].load(Relaxed) {
                        cue_mix[ch] += frame[ch] * std::f32::consts::FRAC_1_SQRT_2;
                    }
                    frame[ch] *= v.gain;
                    peaks[d] = peaks[d].max(frame[ch].abs());
                    mix[ch] += frame[ch] * weights[d] * std::f32::consts::FRAC_1_SQRT_2;
                }
                let bin = i * 96 / frames.max(1);
                waves[d][bin] = waves[d][bin].max((frame[0] + frame[1]).abs() * 0.5);
            }
            for (pad, v) in self.samples.iter_mut().enumerate() {
                if !v.playing {
                    continue;
                }
                let frames = v.frames.as_ref().unwrap();
                let fade = (self.rate as usize / 200).max(1); // 5 ms attack/release for file edges.
                let envelope = (v.position as f32 / fade as f32)
                    .min(1.)
                    .min((frames.len() - v.position) as f32 / fade as f32);
                let gain = get(&c.sample_gain[pad])
                    * get(&c.sample_master)
                    * envelope
                    * std::f32::consts::FRAC_1_SQRT_2;
                for (ch, value) in mix.iter_mut().enumerate() {
                    *value += frames[v.position][ch] * gain;
                }
                v.position += 1;
                if v.position >= frames.len() {
                    if c.sample_looping[pad].load(Relaxed) {
                        v.position = 0;
                    } else {
                        v.playing = false;
                        c.sample_playing[pad].store(false, Relaxed);
                    }
                }
            }
            if let Some(monitor) = &mut self.monitor {
                let blend = get(&c.cue_blend).clamp(0., 1.);
                let volume = get(&c.cue_volume).clamp(0., 1.);
                let frame = std::array::from_fn(|ch| {
                    ((cue_mix[ch] * (1. - blend) + mix[ch] * get(&c.master) * blend) * volume)
                        .clamp(-0.98, 0.98)
                });
                let _ = monitor.push(frame);
            }
            let master = [
                (mix[0] * get(&c.master)).clamp(-0.98, 0.98),
                (mix[1] * get(&c.master)).clamp(-0.98, 0.98),
            ];
            if let Some(tap) = &mut self.broadcast {
                let _ = tap.push(master);
            }
            output(i, master);
        }
        for d in 0..2 {
            set(&c.peak[d], peaks[d]);
            for (i, value) in waves[d].iter().enumerate() {
                set(&c.scope[d][i], *value);
            }
            if let Some(s) = &self.voices[d].source {
                c.position[d].store(s.position as u64, Relaxed);
            }
        }
        c.rendered.fetch_add(frames as u64, Relaxed);
    }
}
pub struct Engine {
    _stream: cpal::Stream,
    monitor_stream: Option<cpal::Stream>,
    commands: mpsc::SyncSender<Command>,
    pub controls: Arc<Controls>,
    pub sample_rate: u32,
    pub device: String,
    cancel: [Option<Arc<AtomicBool>>; 2],
    section_cancel: [Option<Arc<AtomicBool>>; 2],
    observed_epoch: [u64; 2],
    requested_generation: [u64; 2],
    sample_buffers: [Option<Arc<[Frame]>>; 9],
    sample_retired: Vec<(usize, u64, Arc<[Frame]>)>,
    sample_generation: u64,
}
impl Engine {
    pub fn new() -> Result<Self, String> {
        Self::with_output(None)
    }
    pub fn output_names() -> Vec<String> {
        cpal::default_host()
            .output_devices()
            .map(|devices| {
                devices
                    .filter_map(|device| {
                        device
                            .description()
                            .ok()
                            .map(|description| description.name().to_owned())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn with_output(output: Option<&str>) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = if let Some(name) = output {
            host.output_devices()
                .map_err(|error| error.to_string())?
                .find(|device| {
                    device
                        .description()
                        .is_ok_and(|description| description.name() == name)
                })
                .ok_or("Selected cue output is unavailable; choose another device")?
        } else {
            host.default_output_device()
                .ok_or("No audio output device is available")?
        };
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let rate = config.sample_rate();
        let channels = config.channels() as usize;
        let name = device
            .description()
            .map(|d| d.name().to_owned())
            .unwrap_or_else(|_| "Default output".into());
        let controls = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(64);
        let mixer = Mixer::new(rate, rx, controls.clone());
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => stream::<f32>(&device, config.config(), channels, mixer),
            cpal::SampleFormat::I16 => stream::<i16>(&device, config.config(), channels, mixer),
            cpal::SampleFormat::U16 => stream::<u16>(&device, config.config(), channels, mixer),
            other => return Err(format!("Unsupported output sample format {other:?}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            _stream: stream,
            monitor_stream: None,
            commands: tx,
            controls,
            sample_rate: rate,
            device: name,
            cancel: Default::default(),
            section_cancel: Default::default(),
            observed_epoch: [0; 2],
            requested_generation: [0; 2],
            sample_buffers: Default::default(),
            sample_retired: Vec::new(),
            sample_generation: 0,
        })
    }
    pub fn configure_monitor(&mut self, name: Option<&str>) -> Result<(), String> {
        if let Some(name) = name {
            let (producer, consumer) = RingBuffer::new(self.sample_rate as usize / 2);
            let stream =
                crate::monitor::open(name, self.sample_rate, consumer, self.controls.clone())?;
            self.send(Command::Monitor(Some(producer)))?;
            self.monitor_stream = Some(stream);
        } else {
            self.send(Command::Monitor(None))?;
            self.monitor_stream = None;
        }
        Ok(())
    }
    pub fn broadcast_tap(&self) -> Result<rtrb::Consumer<Frame>, String> {
        let (producer, consumer) = RingBuffer::new(self.sample_rate as usize);
        self.send(Command::Broadcast(Some(producer)))?;
        Ok(consumer)
    }
    pub fn stop_broadcast_tap(&self) {
        let _ = self.send(Command::Broadcast(None));
    }
    pub fn load(
        &mut self,
        deck: usize,
        root: &Path,
        track: &amp_core::Track,
        play: bool,
    ) -> Result<(), String> {
        let path = amp_core::resolve(root, &track.path).map_err(|e| e.to_string())?;
        self.load_with_stems(deck, root, track, play, crate::stems::ready(&path))
    }
    fn load_with_stems(
        &mut self,
        deck: usize,
        root: &Path,
        track: &amp_core::Track,
        play: bool,
        stems: Option<[std::path::PathBuf; 3]>,
    ) -> Result<(), String> {
        self.load_decoded(deck, root, track, play, stems, None)
    }
    fn load_decoded(
        &mut self,
        deck: usize,
        root: &Path,
        track: &amp_core::Track,
        play: bool,
        stems: Option<[std::path::PathBuf; 3]>,
        section: Option<(Option<crate::mix_schedule::Grid>, f32)>,
    ) -> Result<(), String> {
        track.preparation.validate()?;
        let stem_knobs = amp_core::stem_mix::knobs(&track.preparation)?;
        if amp_core::stem_mix::changed(stem_knobs) && stems.is_none() {
            return Err(
                "This song has a saved stem mix. Prepare its stems in Library before playback."
                    .into(),
            );
        }
        if section.is_none() {
            self.cancel_section(deck);
            self.controls.source_origin[deck].store(track.preparation.cue_in.to_bits(), Relaxed);
        }
        let path = amp_core::resolve(root, &track.path).map_err(|e| e.to_string())?;
        let separated = stems.is_some();
        if section.is_none()
            && let Some(cancel) = self.cancel[deck].take()
        {
            cancel.store(true, Relaxed);
        }
        if section.is_some() {
            self.cancel_section(deck);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let (mut producer, consumer) = RingBuffer::<DeckFrame>::new(self.sample_rate as usize * 4);
        let rate = self.sample_rate;
        let prep = track.preparation.clone();
        let duration = Arc::new(AtomicU64::new(
            prep.cue_out
                .or(prep.analysis.as_ref().map(|a| a.duration))
                .map_or(0, |end| ((end - prep.cue_in).max(0.) * rate as f64) as u64),
        ));
        let duration_worker = duration.clone();
        let errors = self.controls.clone();
        let stopped = cancel.clone();
        let eof = done.clone();
        let generation = if section.is_some() {
            0
        } else {
            self.requested_generation[deck] + 1
        };
        let failed = Arc::new(AtomicBool::new(false));
        let worker_failed = failed.clone();
        std::thread::spawn(move || {
            let result = (|| {
                if duration_worker.load(Relaxed) == 0
                    && let Ok(probe) = amp_core::command("ffprobe")
                        .args([
                            "-v",
                            "error",
                            "-show_entries",
                            "format=duration",
                            "-of",
                            "default=nw=1:nk=1",
                        ])
                        .arg(&path)
                        .output()
                    && probe.status.success()
                    && let Ok(seconds) =
                        String::from_utf8_lossy(&probe.stdout).trim().parse::<f64>()
                    && !stopped.load(Relaxed)
                {
                    duration_worker.store(
                        ((seconds - prep.cue_in).max(0.) * rate as f64) as u64,
                        Relaxed,
                    );
                }
                let mut command = amp_core::command("ffmpeg");
                command.args(["-v", "error"]);
                if let Some([vocals, instruments, drums]) = stems {
                    command.args(amp_core::cue_args(&prep)?).arg("-i").arg(vocals)
                        .args(amp_core::cue_args(&prep)?).arg("-i").arg(instruments)
                        .args(amp_core::cue_args(&prep)?).arg("-i").arg(drums)
                        .args(["-filter_complex", "[0:a][1:a][2:a]join=inputs=3:channel_layout=5.1:map=0.0-FL|0.1-FR|1.0-FC|1.1-LFE|2.0-BL|2.1-BR[out]", "-map", "[out]"]);
                } else {
                    command.args(amp_core::cue_args(&prep)?).arg("-i").arg(path);
                }
                let mut child = command
                    .args([
                        "-vn",
                        "-ac",
                        if separated { "6" } else { "2" },
                        "-ar",
                        &rate.to_string(),
                        "-f",
                        "f32le",
                        "pipe:1",
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| e.to_string())?;
                let decoded: Result<(), String> = (|| {
                    let mut input = std::io::BufReader::new(
                        child.stdout.take().ok_or("Decoder output missing")?,
                    );
                    let mut bytes = [0; 8160];
                    loop {
                        if stopped.load(Relaxed) || producer.is_abandoned() {
                            return Ok(());
                        }
                        let mut n = 0;
                        while n < bytes.len() {
                            let got = input.read(&mut bytes[n..]).map_err(|e| e.to_string())?;
                            if got == 0 {
                                break;
                            }
                            n += got;
                        }
                        if n == 0 {
                            break;
                        }
                        let frame_bytes = if separated { 24 } else { 8 };
                        if n % frame_bytes != 0 {
                            return Err("Incomplete PCM frame".into());
                        }
                        for b in bytes[..n].chunks_exact(frame_bytes) {
                            let mut frame: DeckFrame = std::array::from_fn(|ch| {
                                if ch < frame_bytes / 4 {
                                    f32::from_le_bytes(b[ch * 4..ch * 4 + 4].try_into().unwrap())
                                } else {
                                    0.
                                }
                            });
                            if frame.iter().any(|s| !s.is_finite()) {
                                return Err("Invalid decoded PCM".into());
                            }
                            loop {
                                match producer.push(frame) {
                                    Ok(()) => break,
                                    Err(PushError::Full(value)) => {
                                        frame = value;
                                        if stopped.load(Relaxed) || producer.is_abandoned() {
                                            return Ok(());
                                        }
                                        std::thread::sleep(Duration::from_millis(2));
                                    }
                                }
                            }
                        }
                        if n < bytes.len() {
                            break;
                        }
                    }
                    Ok(())
                })();
                if stopped.load(Relaxed) || producer.is_abandoned() || decoded.is_err() {
                    let _ = child.kill();
                }
                let status = child.wait().map_err(|e| e.to_string())?;
                decoded?;
                if !status.success() && !stopped.load(Relaxed) && !producer.is_abandoned() {
                    return Err("Audio decoder failed".into());
                }
                Ok(())
            })();
            if result.is_err() {
                worker_failed.store(true, std::sync::atomic::Ordering::Release);
            }
            if let Err(error) = result
                && !stopped.load(Relaxed)
                && let Ok(mut messages) = errors.decode_errors.lock()
            {
                messages.push((deck, generation, error));
            }
            eof.store(true, std::sync::atomic::Ordering::Release);
        });
        let source = Source {
            ring: consumer,
            duration,
            done,
            failed,
            history: VecDeque::with_capacity(self.sample_rate as usize * 8),
            history_start: 0,
            position: 0.,
            primed: false,
            separated,
            stem_knobs,
        };
        if let Some((grid, seconds)) = section {
            self.send(Command::Section(
                deck,
                SectionChange {
                    source: Some(source),
                    origin: track.preparation.cue_in,
                    previous_origin: 0.,
                    grid,
                    at: None,
                    expected: self.requested_generation[deck],
                    total: (seconds * self.sample_rate as f32).max(1.) as u64,
                    elapsed: 0,
                    old: None,
                },
            ))?;
            self.section_cancel[deck] = Some(cancel);
            return Ok(());
        }
        self.cancel[deck] = Some(cancel);
        self.send(Command::Load(deck, source, play, generation))?;
        self.requested_generation[deck] = generation;
        self.controls.play[deck].store(play, Relaxed);
        Ok(())
    }
    pub fn load_current(&self, d: usize) -> bool {
        self.controls.deck_generation[d].load(std::sync::atomic::Ordering::Acquire)
            == self.requested_generation[d]
    }
    pub fn master_level(&self) -> f32 {
        get(&self.controls.master)
    }
    pub fn channel_level(&self, d: usize) -> f32 {
        get(&self.controls.gain[d])
    }
    pub fn level_fade(
        &self,
        slot: usize,
        target: f32,
        seconds: f32,
        stop: bool,
    ) -> Result<(), String> {
        if slot > 2
            || !target.is_finite()
            || !(0. ..=1.).contains(&target)
            || !seconds.is_finite()
            || !(0.01..=120.).contains(&seconds)
        {
            return Err("Invalid fade target or duration".into());
        }
        self.send(Command::Level(slot, target, seconds, stop))
    }
    pub fn sample_maintenance(&mut self) {
        self.sample_retired.retain(|(pad, generation, _)| {
            self.controls.sample_generation[*pad].load(std::sync::atomic::Ordering::Acquire)
                < *generation
        });
    }
    pub fn sample_load(
        &mut self,
        pad: usize,
        frames: Option<Arc<[[f32; 2]]>>,
    ) -> Result<(), String> {
        if pad >= 9 {
            return Err("Unknown sampler pad".into());
        }
        self.sample_maintenance();
        let generation = self.sample_generation + 1;
        self.send(Command::SampleLoad(pad, generation, frames.clone()))?;
        if let Some(old) = std::mem::replace(&mut self.sample_buffers[pad], frames) {
            self.sample_retired.push((pad, generation, old));
        }
        self.sample_generation = generation;
        Ok(())
    }
    pub fn sample_trigger(&self, pad: usize) -> Result<(), String> {
        if pad >= 9 || self.sample_buffers[pad].is_none() {
            return Err("Load a sound effect into this pad first".into());
        }
        self.send(Command::SampleTrigger(pad))
    }
    pub fn sample_stop(&self, pad: Option<usize>) -> Result<(), String> {
        if pad.is_some_and(|p| p >= 9) {
            return Err("Unknown sampler pad".into());
        }
        self.send(Command::SampleStop(pad))
    }
    pub fn sample_levels(&self, master: f32, gains: [f32; 9]) {
        set(&self.controls.sample_master, master.clamp(0., 1.));
        for (pad, gain) in gains.iter().enumerate() {
            set(&self.controls.sample_gain[pad], gain.clamp(0., 1.));
        }
    }
    fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .try_send(command)
            .map_err(|_| "Audio command queue is busy".into())
    }
    pub fn stop(&mut self, deck: usize) {
        self.cancel_section(deck);
        if let Some(c) = self.cancel[deck].take() {
            c.store(true, Relaxed);
        }
        let _ = self.send(Command::Stop(deck));
    }
    pub fn playing(&self, deck: usize) -> bool {
        self.controls.play[deck].load(Relaxed)
    }
    pub fn loaded(&self, deck: usize) -> bool {
        self.controls.loaded[deck].load(Relaxed)
    }
    pub fn toggle(&self, deck: usize) {
        let c = &self.controls.play[deck];
        c.store(!c.load(Relaxed), Relaxed);
    }
    pub fn source_position(&self, deck: usize) -> f64 {
        f64::from_bits(self.controls.source_origin[deck].load(Relaxed)) + self.position(deck)
    }
    pub fn position(&self, deck: usize) -> f64 {
        self.controls.position[deck].load(Relaxed) as f64 / self.sample_rate as f64
    }
    pub fn fader(&self) -> f32 {
        get(&self.controls.crossfader)
    }
    pub fn set_fader(&self, x: f32) {
        let _ = self.send(Command::Fader(x));
    }
    pub fn fading(&self) -> bool {
        self.controls.fading.load(Relaxed)
    }
    pub fn remaining(&self, deck: usize) -> Option<f32> {
        let frames = self.controls.duration_frames[deck].load(Relaxed);
        if frames == 0 {
            return None;
        }
        Some(
            (frames.saturating_sub(self.controls.position[deck].load(Relaxed)) as f32
                / self.sample_rate as f32
                / get(&self.controls.rate[deck]))
            .max(0.),
        )
    }
    pub fn crossfade(&self, seconds: f32) -> Result<(), String> {
        if self.fading() {
            return self.send(Command::Cancel);
        }
        let incoming = if self.fader() < 0.5 { 1 } else { 0 };
        if !self.loaded(incoming) || self.controls.finished[incoming].load(Relaxed) {
            return Err("Load or cue the incoming deck before crossfading".into());
        }
        if !self.playing(1 - incoming) {
            return Err("Start the outgoing deck before Auto Crossfade".into());
        }
        let mut duration = seconds.clamp(0.05, 30.);
        for deck in 0..2 {
            if self.controls.looping[deck].load(Relaxed) {
                continue;
            }
            let remaining = self
                .remaining(deck)
                .ok_or("Track is still loading its duration; try again shortly")?;
            if remaining < 0.1 {
                return Err("Not enough audio remains; cue or reload the deck".into());
            }
            duration = duration.min(remaining - 0.05);
        }
        self.send(Command::Fade {
            to: incoming as f32,
            seconds: duration,
        })
    }
    pub fn loop_toggle(&self, deck: usize, seconds: f32) -> Result<(), String> {
        let frames = if self.controls.looping[deck].load(Relaxed) {
            None
        } else {
            Some(Vec::with_capacity(
                (seconds.clamp(0.1, 30.) * self.sample_rate as f32) as usize,
            ))
        };
        self.send(Command::Loop(deck, frames))
    }
    pub fn loop_stop(&self, deck: usize) {
        let _ = self.send(Command::Loop(deck, None));
    }
    pub fn buildup_start(&self, deck: usize, seconds: f32) -> Result<(), String> {
        self.send(Command::Loop(
            deck,
            Some(Vec::with_capacity(
                (seconds.clamp(0.1, 8.) * self.sample_rate as f32) as usize,
            )),
        ))
    }
    pub fn loop_halve(&self, deck: usize) -> Result<(), String> {
        self.send(Command::LoopHalve(deck))
    }
    pub fn stutter_start(&self, deck: usize, seconds: f32) -> Result<(), String> {
        self.send(Command::Loop(
            deck,
            Some(Vec::with_capacity(
                (seconds.clamp(0.03, 0.25) * self.sample_rate as f32) as usize,
            )),
        ))
    }
    pub fn set_gain(&self, deck: usize, value: f32) {
        let _ = self.send(Command::LevelSet(deck, value.clamp(0., 1.)));
    }
    pub fn scratch(&self, deck: usize, active: bool, rate: f32) {
        self.controls.vinyl_follow[deck].store(false, Relaxed);
        set(
            &self.controls.scratch_rate[deck],
            if rate.is_finite() {
                rate.clamp(-4., 4.)
            } else {
                0.
            },
        );
        self.controls.scratching[deck].store(active, Relaxed);
    }
    pub fn vinyl_target(&self, deck: usize, seconds: f64) {
        if !seconds.is_finite() {
            return;
        }
        self.controls.vinyl_target[deck].store(
            (seconds.max(0.) * self.sample_rate as f64).to_bits(),
            Relaxed,
        );
        self.controls.vinyl_follow[deck].store(true, Relaxed);
        self.controls.scratching[deck].store(true, Relaxed);
    }
    pub fn schedule_section(
        &mut self,
        deck: usize,
        root: &Path,
        track: &amp_core::Track,
        grid: Option<crate::mix_schedule::Grid>,
        seconds: f32,
    ) -> Result<(), String> {
        if !self.loaded(deck) || !seconds.is_finite() || !(0.05..=30.).contains(&seconds) {
            return Err("Load a deck and choose a fade between 0.05 and 30 seconds".into());
        }
        let path = amp_core::resolve(root, &track.path).map_err(|e| e.to_string())?;
        let stems = crate::stems::ready(&path);
        if self.controls.stems_loaded[deck].load(Relaxed) != stems.is_some() {
            return Err(
                "Stem sources changed; reload this track before selecting a section".into(),
            );
        }
        self.load_decoded(deck, root, track, false, stems, Some((grid, seconds)))
    }
    pub fn cancel_section(&mut self, deck: usize) {
        // A committed section's decoder is now the current voice; retain it until load/stop.
        let epoch = self.controls.section_epoch[deck].load(Relaxed);
        if epoch != self.observed_epoch[deck] {
            self.observed_epoch[deck] = epoch;
            if let Some(old) = self.cancel[deck].take() {
                old.store(true, Relaxed);
            }
            self.cancel[deck] = self.section_cancel[deck].take();
        } else if let Some(cancel) = self.section_cancel[deck].take() {
            cancel.store(true, Relaxed);
        }
        let _ = self.send(Command::CancelSection(deck));
    }
    pub fn mashup(
        &self,
        seconds: f32,
        hold: f32,
        grid: crate::mix_schedule::Grid,
    ) -> Result<(), String> {
        let incoming = usize::from(self.fader() < 0.5);
        if !seconds.is_finite()
            || !hold.is_finite()
            || !(0.05..=30.).contains(&seconds)
            || !(0.1..=120.).contains(&hold)
        {
            return Err("Invalid mashup timing".into());
        }
        if !self.controls.stems_loaded.iter().all(|v| v.load(Relaxed))
            || !self.playing(1 - incoming)
            || self.playing(incoming)
            || self.fading()
        {
            return Err(
                "Mashup needs two prepared stem decks, with the incoming deck paused".into(),
            );
        }
        if self.controls.looping.iter().any(|v| v.load(Relaxed))
            || self.controls.scratching.iter().any(|v| v.load(Relaxed))
        {
            return Err("Release loops and scratching before a mashup".into());
        }
        let delay = (grid.next(self.position(1 - incoming) + 0.1) - self.position(1 - incoming))
            as f32
            / get(&self.controls.rate[1 - incoming]);
        if self
            .remaining(1 - incoming)
            .is_none_or(|r| r < seconds * 2. + hold + delay + 0.2)
            || self
                .remaining(incoming)
                .is_none_or(|r| r < seconds * 2. + hold + 0.2)
        {
            return Err("Not enough prepared audio remains for this mashup".into());
        }
        if get(&self.controls.stem_level[1 - incoming][0]) < 0.05
            || get(&self.controls.stem_level[incoming][1])
                + get(&self.controls.stem_level[incoming][2])
                < 0.05
        {
            return Err("The selected vocal or backing is muted by your stem settings".into());
        }
        self.send(Command::Mashup {
            to: incoming as f32,
            fade: seconds,
            hold,
            grid,
        })
    }
    pub fn scheduled_crossfade(
        &self,
        seconds: f32,
        grid: crate::mix_schedule::Grid,
    ) -> Result<(), String> {
        let incoming = usize::from(self.fader() < 0.5);
        if !self.loaded(incoming) || !self.playing(1 - incoming) {
            return Err("Prepare the incoming deck and start the outgoing deck".into());
        }
        if self.controls.looping[1 - incoming].load(Relaxed) || self.playing(incoming) {
            return self.crossfade(seconds);
        }
        self.send(Command::ScheduledFade {
            to: incoming as f32,
            seconds: seconds.clamp(0.05, 30.),
            grid,
        })
    }
    pub fn set_rate(&self, deck: usize, value: f32) {
        set(&self.controls.rate[deck], value);
    }
    pub fn set_master(&self, value: f32) {
        let _ = self.send(Command::LevelSet(2, value.clamp(0., 1.)));
    }
    pub fn set_eq(&self, deck: usize, band: usize, db: f32) {
        set(&self.controls.eq[deck][band], db);
    }
    pub fn set_filter(&self, deck: usize, hz: f32) {
        set(&self.controls.cutoff[deck], hz);
    }
    pub fn peak(&self, deck: usize) -> f32 {
        get(&self.controls.peak[deck])
    }
    pub fn take_decode_errors(&self) -> Vec<(usize, String)> {
        self.controls
            .decode_errors
            .lock()
            .map(|mut errors| {
                errors
                    .drain(..)
                    .filter(|(deck, generation, _)| self.requested_generation[*deck] == *generation)
                    .map(|(deck, _, error)| (deck, error))
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn take_error(&self) -> Option<String> {
        self.controls.error.lock().ok()?.take()
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        for c in self
            .cancel
            .iter()
            .chain(self.section_cancel.iter())
            .flatten()
        {
            c.store(true, Relaxed);
        }
    }
}
fn stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    mut mixer: Mixer,
) -> Result<cpal::Stream, String> {
    let errors = mixer.controls.clone();
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let frames = data.len() / channels;
                mixer.render(frames, |i, frame| {
                    for ch in 0..channels {
                        let value = if channels == 1 {
                            (frame[0] + frame[1]) * 0.5
                        } else if ch < 2 {
                            frame[ch]
                        } else {
                            0.
                        };
                        data[i * channels + ch] = T::from_sample(value);
                    }
                });
            },
            move |error| {
                for p in &errors.play {
                    p.store(false, Relaxed);
                }
                if let Ok(mut e) = errors.error.lock() {
                    *e = Some(format!("Audio output: {error}"));
                }
            },
            None,
        )
        .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn source(value: f32, frames: usize) -> Source {
        let (mut p, c) = RingBuffer::new(frames + 1);
        for _ in 0..frames {
            p.push([value, value, 0., 0., 0., 0.]).unwrap();
        }
        Source {
            ring: c,
            duration: Arc::new(AtomicU64::new(frames as u64)),
            done: Arc::new(AtomicBool::new(true)),
            failed: Arc::new(AtomicBool::new(false)),
            history: VecDeque::with_capacity(frames.max(2)),
            history_start: 0,
            position: 0.,
            primed: false,
            separated: false,
            stem_knobs: [0.; 3],
        }
    }

    fn layered_source(values: [f32; 3], frames: usize) -> Source {
        let (mut producer, consumer) = RingBuffer::new(frames + 1);
        for _ in 0..frames {
            producer
                .push([
                    values[0], values[0], values[1], values[1], values[2], values[2],
                ])
                .unwrap();
        }
        Source {
            ring: consumer,
            duration: Arc::new(AtomicU64::new(frames as u64)),
            done: Arc::new(AtomicBool::new(true)),
            failed: Arc::new(AtomicBool::new(false)),
            history: VecDeque::with_capacity(frames + 1),
            history_start: 0,
            position: 0.,
            primed: false,
            separated: true,
            stem_knobs: [0.; 3],
        }
    }

    #[test]
    #[ignore = "requires FFmpeg and native output; generated stem fixtures, master muted"]
    fn native_phrase_mashup_launches_two_stem_decoders_and_completes() {
        let dir = tempfile::tempdir().unwrap();
        let mut sets = Vec::new();
        for deck in 0..2 {
            let paths = std::array::from_fn::<_, 3, _>(|stem| {
                dir.path().join(format!("d{deck}-s{stem}.wav"))
            });
            for (stem, path) in paths.iter().enumerate() {
                assert!(
                    amp_core::command("ffmpeg")
                        .args(["-v", "error", "-f", "lavfi", "-i"])
                        .arg(format!(
                            "sine=frequency={}:duration=12",
                            220 + deck * 110 + stem * 55
                        ))
                        .args(["-ac", "2", "-ar", "48000"])
                        .arg(path)
                        .status()
                        .unwrap()
                        .success()
                );
            }
            sets.push(paths);
        }
        let mut engine = Engine::new().unwrap();
        engine.set_master(0.);
        let p = amp_core::Preparation {
            bpm: Some(240.),
            beat_offset: Some(0.),
            cue_out: Some(12.),
            harmonic_key: Some("8A".into()),
            ..Default::default()
        };
        for (deck, paths) in sets.iter().enumerate() {
            let t = amp_core::Track {
                path: paths[0].file_name().unwrap().into(),
                title: format!("Fixture {deck}"),
                preparation: p.clone(),
            };
            engine
                .load_with_stems(deck, dir.path(), &t, false, Some(paths.clone()))
                .unwrap();
        }
        let wait = |e: &Engine, f: &dyn Fn(&Engine) -> bool| {
            let end = std::time::Instant::now() + Duration::from_secs(10);
            while !f(e) {
                assert!(
                    std::time::Instant::now() < end,
                    "{:?}",
                    e.take_decode_errors()
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        wait(&engine, &|e| {
            e.controls.ready.iter().all(|v| v.load(Relaxed))
        });
        set(&engine.controls.stem_level[0][1], 0.4);
        let saved: [[u32; 3]; 2] = std::array::from_fn(|d| {
            std::array::from_fn(|s| engine.controls.stem_level[d][s].load(Relaxed))
        });
        engine.controls.play[0].store(true, Relaxed);
        let grid = crate::mix_schedule::grid(&p, &p, 0., 0., crate::mix_schedule::Timing::Phrase8)
            .unwrap();
        engine.mashup(0.1, 2., grid).unwrap();
        wait(&engine, &|e| {
            e.controls.mashup_active.load(Relaxed) && e.playing(1)
        });
        wait(&engine, &|e| !e.fading() && !e.playing(0) && e.playing(1));
        assert_eq!(engine.fader(), 1.);
        assert!(engine.take_decode_errors().is_empty());
        assert_eq!(
            saved,
            std::array::from_fn(|d| std::array::from_fn(
                |s| engine.controls.stem_level[d][s].load(Relaxed)
            ))
        );
        engine.stop(0);
        engine.stop(1);
    }
    #[test]
    fn failed_mashup_incoming_returns_to_outgoing_bus() {
        let c = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(32);
        let mut m = Mixer::new(1000, rx, c.clone());
        tx.send(Command::Load(
            0,
            layered_source([0.1, 0.1, 0.1], 12000),
            true,
            1,
        ))
        .unwrap();
        tx.send(Command::Load(
            1,
            layered_source([0.1, 0.1, 0.1], 600),
            false,
            1,
        ))
        .unwrap();
        tx.send(Command::Mashup {
            to: 1.,
            fade: 0.1,
            hold: 2.,
            grid: crate::mix_schedule::Grid {
                period: 0.5,
                anchor: 0.,
            },
        })
        .unwrap();
        m.render(2500, |_, _| {});
        assert!(c.play[0].load(Relaxed));
        assert!(!c.play[1].load(Relaxed));
        assert_eq!(get(&c.crossfader), 0.);
        assert!(!c.fading.load(Relaxed));
        assert!(!c.mashup_active.load(Relaxed));
    }
    #[test]
    fn mashup_hold_is_one_vocal_with_other_backing_and_does_not_write_live_stem_defaults() {
        let c = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(32);
        let mut m = Mixer::new(1000, rx, c.clone());
        // A: 0.10 vocal, 0.20 music, 0.30 drums; B: 0.40 vocal, 0.05 music, 0.02 drums.
        tx.send(Command::Load(
            0,
            layered_source([0.10, 0.20, 0.30], 12000),
            true,
            1,
        ))
        .unwrap();
        tx.send(Command::Load(
            1,
            layered_source([0.40, 0.05, 0.02], 12000),
            false,
            1,
        ))
        .unwrap();
        m.render(1000, |_, _| {});
        let saved: [[u32; 3]; 2] =
            std::array::from_fn(|d| std::array::from_fn(|s| c.stem_level[d][s].load(Relaxed)));
        tx.send(Command::Mashup {
            to: 1.,
            fade: 0.1,
            hold: 2.,
            grid: crate::mix_schedule::Grid {
                period: 0.5,
                anchor: 0.,
            },
        })
        .unwrap();
        m.render(2050, |_, _| {});
        let mut sum = 0.;
        m.render(100, |_, f| sum += f[0]);
        let expected = (0.10 + 0.05 + 0.02) * std::f32::consts::FRAC_1_SQRT_2 * 0.75;
        assert!(
            (sum / 100. - expected).abs() < 0.002,
            "hold {} expected {}",
            sum / 100.,
            expected
        );
        assert!(c.mashup_active.load(Relaxed));
        assert_eq!(
            saved,
            std::array::from_fn(|d| std::array::from_fn(|s| c.stem_level[d][s].load(Relaxed)))
        );
        m.render(2200, |_, _| {});
        assert!(!c.mashup_active.load(Relaxed));
        assert!(!c.play[0].load(Relaxed));
        assert!(c.play[1].load(Relaxed));
        assert_eq!(get(&c.crossfader), 1.);
        assert_eq!(
            saved,
            std::array::from_fn(|d| std::array::from_fn(|s| c.stem_level[d][s].load(Relaxed)))
        );
    }
    #[test]
    fn canceling_mashup_restores_masks_and_preserves_manual_stem_cuts() {
        let c = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(32);
        let mut m = Mixer::new(1000, rx, c.clone());
        tx.send(Command::Load(
            0,
            layered_source([0.1, 0.1, 0.1], 8000),
            true,
            1,
        ))
        .unwrap();
        tx.send(Command::Load(
            1,
            layered_source([0.1, 0.1, 0.1], 8000),
            false,
            1,
        ))
        .unwrap();
        m.render(500, |_, _| {});
        set(&c.stem_level[1][0], 0.);
        tx.send(Command::Mashup {
            to: 1.,
            fade: 0.1,
            hold: 2.,
            grid: crate::mix_schedule::Grid {
                period: 0.5,
                anchor: 0.,
            },
        })
        .unwrap();
        m.render(1000, |_, _| {});
        tx.send(Command::Fader(1.)).unwrap();
        m.render(2000, |_, _| {});
        assert!(!c.mashup_active.load(Relaxed));
        assert_eq!(get(&c.stem_level[1][0]), 0.);
        assert!(m.voices[1].stem_mix[0] < 0.001);
        assert!((m.voices[1].stem_mix[1] - 1.).abs() < 0.001);
    }
    #[test]
    #[ignore = "requires real supplied songs, FFmpeg and an audio output device; muted"]
    fn real_sections_and_vinyl_follow_keep_other_deck_and_metadata_intact() {
        let root = std::path::PathBuf::from(std::env::var_os("AMP_ENERGY_FIXTURE_ROOT").unwrap());
        let lib = amp_core::scan(&root).unwrap();
        let mut track = lib
            .tracks
            .iter()
            .find(|t| t.preparation.bpm.is_some() && t.preparation.beat_offset.is_some())
            .unwrap()
            .clone();
        let before = amp_core::preparation::revision(&root.join(&track.path));
        let original = track.preparation.clone();
        track.preparation.cue_in = 0.;
        let mut engine = Engine::new().unwrap();
        engine.set_master(0.);
        engine.set_gain(0, 0.);
        engine.set_gain(1, 0.);
        engine.load(0, &root, &track, true).unwrap();
        engine.load(1, &root, &track, false).unwrap();
        let wait = |engine: &mut Engine, predicate: &dyn Fn(&Engine) -> bool| {
            let end = std::time::Instant::now() + Duration::from_secs(12);
            while !predicate(engine) {
                assert!(std::time::Instant::now() < end, "audio operation timed out");
                engine.sample_maintenance();
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        wait(&mut engine, &|e| {
            e.controls.ready[0].load(Relaxed)
                && e.controls.ready[1].load(Relaxed)
                && e.position(0) > 0.5
        });
        let other = engine.controls.deck_generation[1].load(Relaxed);
        let mut section_track = track.clone();
        section_track.preparation.cue_in = 10.;
        let grid = crate::mix_schedule::grid(
            &original,
            &original,
            10.,
            0.,
            crate::mix_schedule::Timing::Beat,
        );
        engine
            .schedule_section(0, &root, &section_track, grid, 0.1)
            .unwrap();
        wait(&mut engine, &|e| {
            e.controls.section_epoch[0].load(Relaxed) > 0
                && e.controls.section_state[0].load(Relaxed) == 0
        });
        assert!(engine.source_position(0) >= 10.);
        assert_eq!(engine.controls.deck_generation[1].load(Relaxed), other);
        assert!(!engine.playing(1));
        assert_eq!(engine.position(1), 0.);
        wait(&mut engine, &|e| e.position(0) > 0.4);
        let target = engine.position(0) - 0.1;
        engine.vinyl_target(0, target);
        std::thread::sleep(Duration::from_millis(180));
        assert!((engine.position(0) - target).abs() < 0.01);
        let held = engine.position(0);
        std::thread::sleep(Duration::from_millis(80));
        assert!((engine.position(0) - held).abs() < 0.002);
        engine.scratch(0, false, 1.);
        std::thread::sleep(Duration::from_millis(100));
        assert!(engine.position(0) > held + 0.05);
        engine.cancel_section(0);
        std::thread::sleep(Duration::from_millis(100));
        assert!(engine.playing(0));
        assert_eq!(
            before,
            amp_core::preparation::revision(&root.join(&track.path))
        );
        engine.stop(0);
        engine.stop(1);
    }
    #[test]
    fn scheduled_fade_starts_on_exact_audio_grid_and_retains_seconds() {
        let controls = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(32);
        let mut mixer = Mixer::new(1000, rx, controls.clone());
        tx.send(Command::Load(0, source(0.2, 6000), true, 1))
            .unwrap();
        tx.send(Command::Load(1, source(0.3, 6000), false, 1))
            .unwrap();
        tx.send(Command::ScheduledFade {
            to: 1.,
            seconds: 0.3,
            grid: crate::mix_schedule::Grid {
                period: 0.5,
                anchor: 0.,
            },
        })
        .unwrap();
        mixer.render(500, |_, _| {});
        assert!(!controls.play[1].load(Relaxed));
        assert_eq!(get(&controls.crossfader), 0.);
        mixer.render(1, |_, _| {});
        assert!(controls.play[1].load(Relaxed));
        assert_eq!(controls.position[1].load(Relaxed), 1);
        mixer.render(300, |_, _| {});
        assert!(!controls.fading.load(Relaxed));
        assert_eq!(get(&controls.crossfader), 1.);
        assert!(!controls.play[0].load(Relaxed));
    }
    fn section(source: Source, expected: u64) -> SectionChange {
        SectionChange {
            source: Some(source),
            origin: 10.,
            previous_origin: 0.,
            grid: Some(crate::mix_schedule::Grid {
                period: 0.5,
                anchor: 0.,
            }),
            at: None,
            expected,
            total: 100,
            elapsed: 0,
            old: None,
        }
    }
    #[test]
    fn section_change_is_buffered_sample_scheduled_and_does_not_touch_other_deck() {
        let c = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(32);
        let mut m = Mixer::new(1000, rx, c.clone());
        tx.send(Command::Load(0, source(0.2, 6000), true, 1))
            .unwrap();
        tx.send(Command::Load(1, source(0.3, 6000), true, 1))
            .unwrap();
        tx.send(Command::Section(0, section(source(0.4, 6000), 1)))
            .unwrap();
        m.render(500, |_, _| {});
        assert_eq!(c.position[0].load(Relaxed), 500);
        assert_eq!(c.section_epoch[0].load(Relaxed), 0);
        let mut output = Vec::new();
        m.render(101, |_, f| output.push(f));
        assert_eq!(c.section_epoch[0].load(Relaxed), 1);
        assert_eq!(c.position[0].load(Relaxed), 101);
        assert_eq!(c.position[1].load(Relaxed), 601);
        assert_eq!(f64::from_bits(c.source_origin[0].load(Relaxed)), 10.);
        assert_eq!(c.section_state[0].load(Relaxed), 0);
        assert!(output.windows(2).all(|w| (w[1][0] - w[0][0]).abs() < 0.01));
    }
    #[test]
    fn canceled_replaced_stale_and_failed_section_requests_keep_current_audio() {
        let c = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(32);
        let mut m = Mixer::new(1000, rx, c.clone());
        tx.send(Command::Load(0, source(0.2, 6000), true, 1))
            .unwrap();
        tx.send(Command::Section(0, section(source(0.4, 6000), 1)))
            .unwrap();
        tx.send(Command::CancelSection(0)).unwrap();
        m.render(600, |_, _| {});
        assert_eq!(c.section_epoch[0].load(Relaxed), 0);
        assert_eq!(c.position[0].load(Relaxed), 600);
        tx.send(Command::Section(0, section(source(0.4, 6000), 0)))
            .unwrap();
        m.render(10, |_, _| {});
        assert_eq!(c.section_epoch[0].load(Relaxed), 0);
        tx.send(Command::Section(0, section(source(0.4, 0), 1)))
            .unwrap();
        m.render(10, |_, _| {});
        assert_eq!(c.section_state[0].load(Relaxed), 3);
        assert!(c.play[0].load(Relaxed));
        tx.send(Command::Section(0, section(source(0.4, 6000), 1)))
            .unwrap();
        tx.send(Command::Section(0, section(source(0.5, 6000), 1)))
            .unwrap();
        m.render(500, |_, _| {});
        assert_eq!(c.section_epoch[0].load(Relaxed), 1);
    }
    #[test]
    fn section_buffer_exhaustion_rolls_back_without_stopping_current_deck() {
        let c = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(32);
        let mut m = Mixer::new(1000, rx, c.clone());
        tx.send(Command::Load(0, source(0.2, 6000), true, 1))
            .unwrap();
        let mut change = section(source(0.4, 60), 1);
        change.total = 300;
        tx.send(Command::Section(0, change)).unwrap();
        m.render(650, |_, _| {});
        assert!(c.play[0].load(Relaxed));
        assert_eq!(c.section_state[0].load(Relaxed), 3);
        assert_eq!(c.section_epoch[0].load(Relaxed), 0);
        assert_eq!(f64::from_bits(c.source_origin[0].load(Relaxed)), 0.);
        assert!(c.position[0].load(Relaxed) >= 649);
    }
    #[test]
    fn vinyl_position_follows_hand_holds_and_releases_forward() {
        let c = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(32);
        let mut m = Mixer::new(1000, rx, c.clone());
        tx.send(Command::Load(0, source(0.2, 6000), true, 1))
            .unwrap();
        m.render(1000, |_, _| {});
        c.vinyl_target[0].store(800f64.to_bits(), Relaxed);
        c.vinyl_follow[0].store(true, Relaxed);
        c.scratching[0].store(true, Relaxed);
        m.render(1000, |_, _| {});
        assert!((c.position[0].load(Relaxed) as i64 - 800).abs() <= 1);
        m.render(300, |_, _| {});
        assert!((c.position[0].load(Relaxed) as i64 - 800).abs() <= 1);
        c.vinyl_target[0].store(1100f64.to_bits(), Relaxed);
        m.render(1000, |_, _| {});
        assert!((c.position[0].load(Relaxed) as i64 - 1100).abs() <= 1);
        c.scratching[0].store(false, Relaxed);
        m.render(100, |_, _| {});
        assert!((c.position[0].load(Relaxed) as i64 - 1200).abs() <= 1);
    }
    #[test]
    fn looping_pad_survives_end_and_stops_without_interrupting_main() {
        let controls = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(16);
        let mut mixer = Mixer::new(1000, rx, controls.clone());
        tx.send(Command::Load(0, source(0.2, 4000), true, 1))
            .unwrap();
        tx.send(Command::SampleLoad(
            0,
            1,
            Some(Arc::from(vec![[0.1, 0.1]; 10])),
        ))
        .unwrap();
        controls.sample_looping[0].store(true, Relaxed);
        tx.send(Command::SampleTrigger(0)).unwrap();
        mixer.render(100, |_, _| {});
        assert!(controls.sample_playing[0].load(Relaxed));
        tx.send(Command::SampleStop(Some(0))).unwrap();
        mixer.render(10, |_, _| {});
        assert!(!controls.sample_playing[0].load(Relaxed));
        assert!(controls.play[0].load(Relaxed));
    }
    #[test]
    fn scratch_reads_audio_in_both_directions_without_growing_history() {
        let (mut producer, consumer) = RingBuffer::new(65);
        for frame in 0..64 {
            producer
                .push([frame as f32, -(frame as f32), 0., 0., 0., 0.])
                .unwrap();
        }
        let mut source = Source {
            ring: consumer,
            duration: Arc::new(AtomicU64::new(64)),
            done: Arc::new(AtomicBool::new(true)),
            failed: Arc::new(AtomicBool::new(false)),
            history: VecDeque::with_capacity(16),
            history_start: 0,
            position: 0.,
            primed: true,
            separated: false,
            stem_knobs: [0.; 3],
        };
        for expected in 0..12 {
            assert_eq!(source.next(1.).unwrap()[0], expected as f32);
        }
        assert_eq!(source.next(-1.).unwrap(), [12., -12., 0., 0., 0., 0.]);
        assert_eq!(source.next(-1.).unwrap(), [11., -11., 0., 0., 0., 0.]);
        assert_eq!(source.next(-1.).unwrap(), [10., -10., 0., 0., 0., 0.]);
        assert_eq!(source.next(0.), Some([0.; 6]));
        assert_eq!(source.position, 9.);
        assert_eq!(source.next(0.5).unwrap()[0], 9.);
        assert_eq!(source.next(0.5).unwrap()[0], 9.5);
        for _ in 0..25 {
            source.next(1.).unwrap();
        }
        let capacity = source.history.capacity();
        assert_eq!(capacity, 16);
        for _ in 0..100 {
            source.next(-4.).unwrap();
        }
        assert_eq!(source.position, source.history_start as f64);
        assert_eq!(source.history.capacity(), capacity);
    }
    #[test]
    fn scratching_reverses_one_deck_and_restores_forward_playback() {
        let controls = Arc::new(Controls::default());
        let (sender, receiver) = mpsc::sync_channel(16);
        let mut mixer = Mixer::new(1000, receiver, controls.clone());
        sender
            .send(Command::Load(0, source(0.2, 4000), true, 1))
            .unwrap();
        sender
            .send(Command::Load(1, source(0.3, 4000), true, 1))
            .unwrap();
        mixer.render(1000, |_, _| {});
        controls.scratching[0].store(true, Relaxed);
        set(&controls.scratch_rate[0], -1.);
        mixer.render(100, |_, frame| {
            assert!(frame.iter().all(|sample| sample.is_finite()))
        });
        assert_eq!(controls.position[0].load(Relaxed), 900);
        assert_eq!(controls.position[1].load(Relaxed), 1100);
        set(&controls.scratch_rate[0], 0.);
        mixer.render(100, |_, _| {});
        assert_eq!(controls.position[0].load(Relaxed), 900);
        controls.scratching[0].store(false, Relaxed);
        mixer.render(100, |_, _| {});
        assert_eq!(controls.position[0].load(Relaxed), 1000);
        assert_eq!(controls.position[1].load(Relaxed), 1300);
    }
    #[test]
    fn level_fades_are_smooth_stop_separately_and_can_restore() {
        let controls = Arc::new(Controls::default());
        set(&controls.master, 0.75);
        let (tx, rx) = mpsc::sync_channel(16);
        let mut m = Mixer::new(1000, rx, controls.clone());
        tx.send(Command::Load(0, source(0.2, 4000), true, 1))
            .unwrap();
        m.render(1000, |_, _| {});
        tx.send(Command::Level(2, 0.2, 0.1, false)).unwrap();
        let mut values = Vec::new();
        m.render(100, |_, f| values.push(f[0]));
        assert!(values.windows(2).all(|w| w[1] <= w[0] + 0.0001));
        assert!((get(&controls.master) - 0.2).abs() < 0.001);
        assert!(controls.play[0].load(Relaxed));
        tx.send(Command::Level(2, 0.75, 0.1, false)).unwrap();
        m.render(100, |_, _| {});
        assert!((get(&controls.master) - 0.75).abs() < 0.001);
        tx.send(Command::Level(2, 0., 0.1, true)).unwrap();
        m.render(100, |_, _| {});
        assert!(!controls.play[0].load(Relaxed));
        assert!(!controls.loaded[0].load(Relaxed));
        assert_eq!(get(&controls.master), 0.);
    }
    #[test]
    fn nine_sampler_voices_mix_retrigger_stop_and_follow_master() {
        let controls = Arc::new(Controls::default());
        set(&controls.master, 1.);
        let (tx, rx) = mpsc::sync_channel(64);
        let mut mixer = Mixer::new(1000, rx, controls.clone());
        let sample: Arc<[Frame]> = Arc::from(vec![[0.1, 0.1]; 100]);
        for pad in 0..9 {
            tx.send(Command::SampleLoad(
                pad,
                pad as u64 + 1,
                Some(sample.clone()),
            ))
            .unwrap();
            tx.send(Command::SampleTrigger(pad)).unwrap();
        }
        let mut output = Vec::new();
        mixer.render(20, |_, f| output.push(f));
        assert!(controls.sample_playing.iter().all(|p| p.load(Relaxed)));
        assert!((output[19][0] - 9. * 0.1 * 0.7 * std::f32::consts::FRAC_1_SQRT_2).abs() < 0.001);
        tx.send(Command::Fader(1.)).unwrap();
        let mut after = 0.;
        mixer.render(1, |_, f| after = f[0]);
        assert!((after - output[19][0]).abs() < 0.001);
        tx.send(Command::SampleStop(Some(8))).unwrap();
        mixer.render(1, |_, _| {});
        assert!(!controls.sample_playing[8].load(Relaxed));
        assert!(controls.sample_playing[0].load(Relaxed));
        tx.send(Command::SampleStop(None)).unwrap();
        tx.send(Command::SampleTrigger(0)).unwrap();
        let mut restarted = Vec::new();
        mixer.render(2, |_, f| restarted.push(f));
        assert_eq!(restarted[0], [0., 0.]);
        assert!(restarted[1][0] > 0.);
        set(&controls.master, 0.);
        mixer.render(10, |_, f| assert_eq!(f, [0., 0.]));
        set(&controls.master, 1.);
        mixer.render(120, |_, _| {});
        assert!(controls.sample_playing.iter().all(|p| !p.load(Relaxed)));
        mixer.render(10, |_, f| assert_eq!(f, [0., 0.]));
    }
    #[test]
    fn sampler_layers_over_both_decks_without_stopping_them() {
        let controls = Arc::new(Controls::default());
        set(&controls.master, 1.);
        set(&controls.sample_master, 1.);
        let (tx, rx) = mpsc::sync_channel(64);
        let mut mixer = Mixer::new(1000, rx, controls.clone());
        for d in 0..2 {
            tx.send(Command::Load(d, source(0.1, 1000), true, 0))
                .unwrap();
        }
        tx.send(Command::Fader(0.5)).unwrap();
        let frames: Arc<[Frame]> = Arc::from(vec![[0.01, 0.01]; 1000]);
        for pad in 0..9 {
            tx.send(Command::SampleLoad(
                pad,
                pad as u64 + 1,
                Some(frames.clone()),
            ))
            .unwrap();
            tx.send(Command::SampleTrigger(pad)).unwrap();
        }
        let mut with = 0.;
        mixer.render(200, |_, f| with = f[0]);
        assert!(controls.play.iter().all(|p| p.load(Relaxed)));
        assert!(controls.sample_playing.iter().all(|p| p.load(Relaxed)));
        tx.send(Command::SampleStop(None)).unwrap();
        let mut without = 0.;
        mixer.render(1, |_, f| without = f[0]);
        assert!(without > 0.);
        assert!(with - without > 0.06);
        assert!(controls.play.iter().all(|p| p.load(Relaxed)));
    }
    #[test]
    fn replacement_acknowledges_after_callback_releases_old_buffer() {
        let controls = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(8);
        let mut mixer = Mixer::new(1000, rx, controls.clone());
        let old: Arc<[Frame]> = Arc::from(vec![[0.1, 0.1]; 10]);
        let weak = Arc::downgrade(&old);
        tx.send(Command::SampleLoad(0, 1, Some(old.clone())))
            .unwrap();
        mixer.render(1, |_, _| {});
        assert_eq!(Arc::strong_count(&old), 2);
        tx.send(Command::SampleLoad(0, 2, None)).unwrap();
        mixer.render(1, |_, _| {});
        assert_eq!(
            controls.sample_generation[0].load(std::sync::atomic::Ordering::Acquire),
            2
        );
        assert_eq!(Arc::strong_count(&old), 1);
        drop(old);
        assert!(weak.upgrade().is_none());
    }
    #[test]
    #[ignore = "requires a native output device and ffmpeg on PATH; output muted"]
    fn native_device_crossfade_with_real_decoders() {
        let dir = tempfile::tempdir().unwrap();
        for (name, hz) in [("a.wav", 330), ("b.wav", 660)] {
            assert!(
                amp_core::command("ffmpeg")
                    .args([
                        "-v",
                        "error",
                        "-f",
                        "lavfi",
                        "-i",
                        &format!("sine=frequency={hz}:duration=6")
                    ])
                    .arg(dir.path().join(name))
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let track = |name: &str| amp_core::Track {
            path: name.into(),
            title: name.into(),
            preparation: amp_core::Preparation {
                cue_in: 0.25,
                cue_out: Some(5.5),
                ..Default::default()
            },
        };
        let mut engine = Engine::new().unwrap();
        engine.set_master(0.);
        engine.load(0, dir.path(), &track("a.wav"), true).unwrap();
        engine.load(1, dir.path(), &track("b.wav"), false).unwrap();
        let wait = |predicate: &dyn Fn() -> bool| {
            let start = std::time::Instant::now();
            while !predicate() {
                assert!(
                    start.elapsed() < Duration::from_secs(8),
                    "native audio timeout"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        wait(&|| engine.controls.ready[1].load(Relaxed) && engine.position(0) > 0.3);
        engine.crossfade(0.5).unwrap();
        wait(&|| engine.fader() > 0.999 && !engine.fading());
        assert!(!engine.playing(0));
        assert!(engine.playing(1));
        assert!(engine.position(1) > 0.4);
        engine.crossfade(0.5).unwrap();
        wait(&|| engine.fader() < 0.001 && !engine.fading());
        assert!(engine.playing(0));
        assert!(!engine.playing(1));
        engine.toggle(0);
        std::thread::sleep(Duration::from_millis(60));
        let before = engine.position(0);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(engine.position(0), before);
        assert!(engine.controls.rendered.load(Relaxed) > engine.sample_rate as u64);
        assert_eq!(
            engine
                .controls
                .underruns
                .iter()
                .map(|x| x.load(Relaxed))
                .sum::<u64>(),
            0
        );
        assert!(engine.take_error().is_none());
    }
    #[test]
    fn song_stem_defaults_apply_at_load_and_reset_for_the_next_song() {
        let controls = Arc::new(Controls::default());
        set(&controls.master, 1.);
        let (tx, rx) = mpsc::sync_channel(8);
        let (mut producer, consumer) = RingBuffer::new(3000);
        for _ in 0..2500 {
            producer.push([0.2, 0.2, 0.1, 0.1, 0.05, 0.05]).unwrap();
        }
        let mut separated = source(0., 2);
        separated.ring = consumer;
        separated.separated = true;
        separated.stem_knobs = [-1., 0., 0.];
        tx.send(Command::Load(0, separated, true, 1)).unwrap();
        let mut mixer = Mixer::new(1000, rx, controls.clone());
        mixer.render(1500, |_, _| {});
        let mut frame = [0.; 2];
        mixer.render(1, |_, f| frame = f);
        assert!((frame[0] / std::f32::consts::FRAC_1_SQRT_2 - 0.15).abs() < 0.001);
        assert_eq!(get(&controls.stem_level[0][0]), 0.);
        assert_eq!(get(&controls.stem_knobs[0][0]), -1.);
        tx.send(Command::Load(0, source(0.2, 2), true, 2)).unwrap();
        mixer.render(1, |_, _| {});
        assert_eq!(get(&controls.stem_level[0][0]), 1.);
        assert_eq!(get(&controls.stem_knobs[0][0]), 0.);
    }
    #[test]
    fn stem_levels_share_transport_and_sum_without_touching_original_mode() {
        let c = Arc::new(Controls::default());
        set(&c.master, 1.);
        let (tx, rx) = mpsc::sync_channel(8);
        let (mut producer, consumer) = RingBuffer::new(12000);
        for _ in 0..11000 {
            producer.push([0.2, 0.1, -0.05, 0.1, 0.03, -0.02]).unwrap();
        }
        let mut voice = source(0., 2);
        voice.ring = consumer;
        voice.separated = true;
        tx.send(Command::Load(0, voice, true, 1)).unwrap();
        let mut mixer = Mixer::new(1000, rx, c.clone());
        mixer.render(2000, |_, _| {});
        let mut sum = [0.; 2];
        mixer.render(1, |_, f| sum = f);
        assert!((sum[0] / std::f32::consts::FRAC_1_SQRT_2 - 0.18).abs() < 0.001);
        assert!((sum[1] / std::f32::consts::FRAC_1_SQRT_2 - 0.18).abs() < 0.001);
        set(&c.stem_level[0][0], 0.);
        mixer.render(2000, |_, _| {});
        let mut instrumental = [0.; 2];
        mixer.render(1, |_, f| instrumental = f);
        assert!((instrumental[0] / std::f32::consts::FRAC_1_SQRT_2 + 0.02).abs() < 0.001);
        set(&c.stem_level[0][0], 1.);
        set(&c.stem_level[0][1], 0.);
        set(&c.stem_level[0][2], 0.);
        mixer.render(2000, |_, _| {});
        let mut vocals = [0.; 2];
        mixer.render(1, |_, f| vocals = f);
        assert!((vocals[0] / std::f32::consts::FRAC_1_SQRT_2 - 0.2).abs() < 0.001);
        set(&c.stem_level[0][0], 0.);
        set(&c.stem_level[0][2], 1.);
        mixer.render(2000, |_, _| {});
        let mut drums = [0.; 2];
        mixer.render(1, |_, f| drums = f);
        assert!((drums[0] / std::f32::consts::FRAC_1_SQRT_2 - 0.03).abs() < 0.001);
        assert!(c.play[0].load(Relaxed));
        assert!(c.stems_loaded[0].load(Relaxed));
        assert_eq!(c.position[0].load(Relaxed), 8004);
    }
    #[test]
    #[ignore = "requires FFmpeg and native audio; output muted"]
    fn native_prepared_stems_decode_in_lockstep_at_saved_cue() {
        let dir = tempfile::tempdir().unwrap();
        let paths = [
            dir.path().join("vocals.wav"),
            dir.path().join("instrumental.wav"),
            dir.path().join("drums.wav"),
        ];
        for (index, path) in paths.iter().enumerate() {
            assert!(
                amp_core::command("ffmpeg")
                    .args([
                        "-v",
                        "error",
                        "-f",
                        "lavfi",
                        "-i",
                        if index == 0 {
                            "sine=frequency=440:duration=3"
                        } else {
                            "sine=frequency=220:duration=3"
                        },
                        "-ac",
                        "2",
                        "-c:a",
                        "pcm_f32le"
                    ])
                    .arg(path)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let track = amp_core::Track {
            title: "Prepared test".into(),
            path: "vocals.wav".into(),
            preparation: amp_core::Preparation {
                cue_in: 0.5,
                cue_out: Some(2.5),
                ..Default::default()
            },
        };
        let mut engine = Engine::new().unwrap();
        engine.set_master(0.);
        engine
            .load_with_stems(0, dir.path(), &track, false, Some(paths))
            .unwrap();
        let until = std::time::Instant::now() + Duration::from_secs(5);
        while !engine.controls.ready[0].load(Relaxed) {
            assert!(
                std::time::Instant::now() < until,
                "{:?}",
                engine.take_decode_errors()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        // Drawing the knobs must not undo an automated stem level.
        let mut panel = crate::stems::Panel::default();
        set(&engine.controls.stem_level[0][0], 0.25);
        let ctx = eframe::egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            panel.show(ui, 0, None, Some(&engine), false);
        });
        output.textures_delta.clear();
        assert_eq!(get(&engine.controls.stem_level[0][0]), 0.25);
        assert!(panel.manual_change.is_none());
        assert!(engine.controls.stems_loaded[0].load(Relaxed));
        assert!((engine.source_position(0) - 0.5).abs() < 0.001);
        engine.controls.play[0].store(true, Relaxed);
        std::thread::sleep(Duration::from_millis(150));
        assert!(engine.position(0) > 0.05);
        assert!(engine.take_decode_errors().is_empty());
        engine.stop(0);
    }
    #[test]
    fn headphone_cue_is_pre_fader_and_trim_and_kills_affect_both_buses() {
        let controls = Arc::new(Controls::default());
        set(&controls.master, 0.);
        set(&controls.gain[0], 0.);
        set(&controls.crossfader, 1.);
        controls.cue[0].store(true, Relaxed);
        set(&controls.cue_volume, 1.);
        let (tx, rx) = mpsc::sync_channel(8);
        let (producer, mut consumer) = RingBuffer::new(20000);
        let (tap, mut broadcast) = RingBuffer::new(20000);
        tx.send(Command::Broadcast(Some(tap))).unwrap();
        tx.send(Command::Monitor(Some(producer))).unwrap();
        tx.send(Command::Load(0, source(0.2, 20000), true, 0))
            .unwrap();
        let mut mixer = Mixer::new(1000, rx, controls.clone());
        let mut speaker_peak = 0_f32;
        mixer.render(2000, |_, frame| {
            speaker_peak = speaker_peak.max(frame[0].abs())
        });
        let mut baseline = 0.;
        while let Ok(frame) = consumer.pop() {
            baseline = frame[0];
        }
        assert!(
            baseline > 0.13,
            "Cue must work with master and channel faders down"
        );
        assert_eq!(speaker_peak, 0., "Headphones must not leak into the master");
        assert!(
            std::iter::from_fn(|| broadcast.pop().ok()).all(|frame| frame == [0.; 2]),
            "Private headphone cue must not leak into live broadcast"
        );
        set(&controls.trim_db[0], 6.);
        mixer.render(2000, |_, _| {});
        let mut boosted = 0.;
        while let Ok(frame) = consumer.pop() {
            boosted = frame[0];
        }
        assert!((boosted / baseline - 10_f32.powf(6. / 20.)).abs() < 0.01);
        for kill in &controls.eq_kill[0] {
            kill.store(true, Relaxed);
        }
        mixer.render(1000, |_, _| {});
        while let Ok(frame) = consumer.pop() {
            assert_eq!(frame, [0.; 2]);
        }
        for kill in &controls.eq_kill[0] {
            kill.store(false, Relaxed);
        }
        controls.cue[0].store(false, Relaxed);
        mixer.render(1000, |_, _| {});
        while let Ok(frame) = consumer.pop() {
            assert_eq!(frame, [0.; 2]);
        }
        // Cue/master blend follows the actual post-master mix when fully right.
        set(&controls.cue_blend, 1.);
        set(&controls.master, 1.);
        set(&controls.gain[0], 1.);
        set(&controls.crossfader, 0.);
        let mut speaker = Vec::new();
        mixer.render(1000, |_, frame| speaker.push(frame));
        for frame in speaker {
            assert_eq!(consumer.pop().unwrap(), frame);
        }
    }
    #[test]
    fn filter_sweep_attenuates_the_correct_side_of_the_spectrum() {
        let measure = |frequency: f32, sweep: f32| {
            let controls = Arc::new(Controls::default());
            set(&controls.master, 1.);
            set(&controls.sweep[0], sweep);
            let (tx, rx) = mpsc::sync_channel(8);
            let (mut producer, consumer) = RingBuffer::new(10000);
            for i in 0..8000 {
                let value = (std::f32::consts::TAU * frequency * i as f32 / 48000.).sin() * 0.2;
                producer.push([value, value, 0., 0., 0., 0.]).unwrap();
            }
            let mut voice = source(0., 2);
            voice.ring = consumer;
            tx.send(Command::Load(0, voice, true, 0)).unwrap();
            let mut mixer = Mixer::new(48000, rx, controls);
            mixer.render(2000, |_, _| {});
            let mut sum = 0.;
            mixer.render(2000, |_, frame| sum += frame[0] * frame[0]);
            sum
        };
        assert!(measure(8000., -1.) < measure(8000., 0.) * 0.01);
        assert!(measure(80., 1.) < measure(80., 0.) * 0.01);
        assert!(measure(80., -0.2) > measure(80., 0.) * 0.9);
    }
    #[test]
    fn eq_flat_gain_and_filter_process_pcm() {
        let measure = |db: f32, cutoff: f32| {
            let c = Arc::new(Controls::default());
            set(&c.master, 1.);
            for band in &c.eq[0] {
                set(band, db);
            }
            set(&c.cutoff[0], cutoff);
            let (tx, rx) = mpsc::sync_channel(8);
            let (mut producer, consumer) = RingBuffer::new(10000);
            for i in 0..8000 {
                let value = (std::f32::consts::TAU * 8000. * i as f32 / 48000.).sin() * 0.2;
                producer.push([value, value, 0., 0., 0., 0.]).unwrap();
            }
            let mut voice = source(0., 2);
            voice.ring = consumer;
            tx.send(Command::Load(0, voice, true, 0)).unwrap();
            let mut m = Mixer::new(48000, rx, c);
            m.render(2000, |_, _| {});
            let mut energy = 0.;
            m.render(4000, |_, frame| energy += frame[0] * frame[0]);
            (energy / 4000.).sqrt()
        };
        let flat = measure(0., 22000.);
        assert!((flat - 0.1).abs() < 0.005);
        let boosted = measure(6., 22000.);
        assert!((boosted / flat - 10f32.powf(6. / 20.)).abs() < 0.02);
        assert!(measure(0., 200.) < flat * 0.05);
    }
    #[test]
    fn automatic_fade_is_continuous_and_stops_outgoing() {
        let controls = Arc::new(Controls::default());
        set(&controls.master, 1.);
        let (tx, rx) = mpsc::sync_channel(16);
        let mut m = Mixer::new(1000, rx, controls.clone());
        tx.send(Command::Load(0, source(0.2, 4000), true, 0))
            .unwrap();
        tx.send(Command::Load(1, source(0.3, 4000), false, 0))
            .unwrap();
        controls.play[0].store(true, Relaxed);
        m.render(500, |_, _| {});
        tx.send(Command::Fade {
            to: 1.,
            seconds: 1.,
        })
        .unwrap();
        let mut out = Vec::new();
        m.render(1200, |_, f| out.push(f[0]));
        assert!(out.iter().all(|s| *s > 0.1 && *s < 0.4));
        assert!(out.windows(2).all(|w| (w[1] - w[0]).abs() < 0.003));
        assert!(!controls.play[0].load(Relaxed));
        assert!(controls.play[1].load(Relaxed));
        assert_eq!(get(&controls.crossfader), 1.);
        assert_eq!(
            controls.underruns[0].load(Relaxed) + controls.underruns[1].load(Relaxed),
            0
        );
    }
    #[test]
    fn pause_loop_pitch_and_manual_cancel() {
        let c = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(16);
        let mut m = Mixer::new(1000, rx, c.clone());
        tx.send(Command::Load(0, source(0.2, 5000), true, 0))
            .unwrap();
        c.play[0].store(true, Relaxed);
        set(&c.rate[0], 2.);
        m.render(100, |_, _| {});
        assert_eq!(c.position[0].load(Relaxed), 200);
        c.play[0].store(false, Relaxed);
        m.render(100, |_, _| {});
        assert_eq!(c.position[0].load(Relaxed), 200);
        c.play[0].store(true, Relaxed);
        tx.send(Command::Loop(0, Some(Vec::with_capacity(100))))
            .unwrap();
        m.render(400, |_, _| {});
        assert!(c.position[0].load(Relaxed) <= 400);
        tx.send(Command::Fader(0.4)).unwrap();
        m.render(10, |_, _| {});
        assert_eq!(get(&c.crossfader), 0.4);
        assert!(!c.fading.load(Relaxed));
    }
    #[test]
    fn buildup_halves_captured_audio_without_new_allocations_or_empty_loops() {
        let c = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(16);
        let mut m = Mixer::new(1000, rx, c.clone());
        tx.send(Command::Load(0, source(0.2, 5000), true, 0))
            .unwrap();
        tx.send(Command::Loop(0, Some(Vec::with_capacity(400))))
            .unwrap();
        m.render(600, |_, _| {});
        let capacity = m.voices[0].looped.as_ref().unwrap().frames.capacity();
        for expected in [200, 100, 50, 25] {
            tx.send(Command::LoopHalve(0)).unwrap();
            m.render(80, |_, _| {});
            let l = m.voices[0].looped.as_ref().unwrap();
            assert_eq!(l.frames.len(), expected);
            assert_eq!(l.frames.capacity(), capacity);
            assert!(l.phase < expected as f64);
        }
        tx.send(Command::Loop(0, None)).unwrap();
        m.render(20, |_, _| {});
        assert!(!c.looping[0].load(Relaxed));
        assert!(c.play[0].load(Relaxed));
    }
    #[test]
    fn fade_waits_for_incoming_pcm() {
        let c = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(16);
        let mut m = Mixer::new(1000, rx, c.clone());
        tx.send(Command::Load(0, source(0.2, 5000), true, 0))
            .unwrap();
        let (_p, r) = RingBuffer::new(1000);
        tx.send(Command::Load(
            1,
            Source {
                ring: r,
                duration: Arc::new(AtomicU64::new(0)),
                done: Arc::new(AtomicBool::new(false)),
                failed: Arc::new(AtomicBool::new(false)),
                history: VecDeque::with_capacity(8000),
                history_start: 0,
                position: 0.,
                primed: false,
                separated: false,
                stem_knobs: [0.; 3],
            },
            false,
            0,
        ))
        .unwrap();
        c.play[0].store(true, Relaxed);
        tx.send(Command::Fade {
            to: 1.,
            seconds: 1.,
        })
        .unwrap();
        m.render(500, |_, _| {});
        assert_eq!(get(&c.crossfader), 0.);
        assert!(c.play[0].load(Relaxed));
        assert!(!c.play[1].load(Relaxed));
    }
}
