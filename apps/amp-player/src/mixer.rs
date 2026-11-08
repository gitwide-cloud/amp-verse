//! Shared two-deck device stream. FFmpeg decoding stays off the audio callback.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, PushError, RingBuffer};
use std::{
    io::Read,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed},
        mpsc,
    },
    time::Duration,
};
type Frame = [f32; 2];
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
    pub play: [AtomicBool; 2],
    pub loaded: [AtomicBool; 2],
    pub ready: [AtomicBool; 2],
    pub finished: [AtomicBool; 2],
    pub gain: [AtomicU32; 2],
    pub rate: [AtomicU32; 2],
    pub eq: [[AtomicU32; 3]; 2],
    pub cutoff: [AtomicU32; 2],
    pub position: [AtomicU64; 2],
    pub duration_frames: [AtomicU64; 2],
    pub peak: [AtomicU32; 2],
    pub scope: [[AtomicU32; 96]; 2],
    pub looping: [AtomicBool; 2],
    pub crossfader: AtomicU32,
    pub master: AtomicU32,
    pub fading: AtomicBool,
    pub underruns: [AtomicU64; 2],
    pub rendered: AtomicU64,
    pub error: Mutex<Option<String>>,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            play: std::array::from_fn(|_| AtomicBool::new(false)),
            loaded: std::array::from_fn(|_| AtomicBool::new(false)),
            ready: std::array::from_fn(|_| AtomicBool::new(false)),
            finished: std::array::from_fn(|_| AtomicBool::new(false)),
            gain: std::array::from_fn(|_| atom(1.)),
            rate: std::array::from_fn(|_| atom(1.)),
            eq: std::array::from_fn(|_| std::array::from_fn(|_| atom(0.))),
            cutoff: std::array::from_fn(|_| atom(22000.)),
            position: std::array::from_fn(|_| AtomicU64::new(0)),
            duration_frames: std::array::from_fn(|_| AtomicU64::new(0)),
            peak: std::array::from_fn(|_| atom(0.)),
            scope: std::array::from_fn(|_| std::array::from_fn(|_| atom(0.))),
            looping: std::array::from_fn(|_| AtomicBool::new(false)),
            crossfader: atom(0.),
            master: atom(0.75),
            fading: AtomicBool::new(false),
            underruns: std::array::from_fn(|_| AtomicU64::new(0)),
            rendered: AtomicU64::new(0),
            error: Mutex::new(None),
        }
    }
}
struct Source {
    ring: Consumer<Frame>,
    duration: Arc<AtomicU64>,
    done: Arc<AtomicBool>,
    a: Option<Frame>,
    b: Option<Frame>,
    phase: f64,
    position: f64,
    primed: bool,
}
impl Source {
    fn buffered(&self, rate: u32) -> bool {
        self.ring.slots() >= rate as usize / 4
            || (self.done.load(std::sync::atomic::Ordering::Acquire) && self.ring.slots() > 1)
    }
    fn next(&mut self, rate: f32) -> Option<Frame> {
        if self.a.is_none() {
            self.a = self.ring.pop().ok();
        }
        if self.b.is_none() {
            self.b = self.ring.pop().ok();
        }
        let (a, b) = (self.a?, self.b?);
        let f = self.phase as f32;
        let frame = [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f];
        self.phase += rate as f64;
        self.position += rate as f64;
        while self.phase >= 1. {
            self.phase -= 1.;
            self.a = self.b.take();
            self.b = self.ring.pop().ok();
            if self.b.is_none() {
                break;
            }
        }
        Some(frame)
    }
}
struct Loop {
    frames: Vec<Frame>,
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
    gain: f32,
    looped: Option<Loop>,
}
struct Fade {
    from: f32,
    to: f32,
    total: u64,
    elapsed: u64,
    incoming: usize,
    started: bool,
}
enum Command {
    Load(usize, Source, bool),
    Stop(usize),
    Fader(f32),
    Fade { to: f32, seconds: f32 },
    Cancel,
    Loop(usize, Option<Vec<Frame>>),
}
struct Mixer {
    voices: [Voice; 2],
    commands: mpsc::Receiver<Command>,
    pub controls: Arc<Controls>,
    rate: u32,
    fade: Option<Fade>,
}
impl Mixer {
    fn new(rate: u32, commands: mpsc::Receiver<Command>, controls: Arc<Controls>) -> Self {
        Self {
            voices: Default::default(),
            commands,
            controls,
            rate,
            fade: None,
        }
    }
    fn process_commands(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                Command::Load(d, s, play) => {
                    self.controls.play[d].store(play, Relaxed);
                    self.voices[d] = Voice {
                        source: Some(s),
                        ..Default::default()
                    };
                    self.controls.loaded[d].store(true, Relaxed);
                    self.controls.finished[d].store(false, Relaxed);
                    self.controls.ready[d].store(false, Relaxed);
                    self.controls.looping[d].store(false, Relaxed);
                    self.controls.position[d].store(0, Relaxed);
                    self.fade = None;
                    self.controls.fading.store(false, Relaxed);
                }
                Command::Stop(d) => {
                    self.voices[d] = Voice::default();
                    self.controls.play[d].store(false, Relaxed);
                    self.controls.loaded[d].store(false, Relaxed);
                    self.controls.ready[d].store(false, Relaxed);
                    self.controls.looping[d].store(false, Relaxed);
                    self.fade = None;
                    self.controls.fading.store(false, Relaxed);
                }
                Command::Fader(x) => {
                    set(&self.controls.crossfader, x.clamp(0., 1.));
                    self.fade = None;
                    self.controls.fading.store(false, Relaxed);
                }
                Command::Fade { to, seconds } => {
                    self.fade = Some(Fade {
                        from: get(&self.controls.crossfader),
                        to,
                        total: (seconds * self.rate as f32).max(1.) as u64,
                        elapsed: 0,
                        incoming: usize::from(to > 0.5),
                        started: false,
                    });
                    self.controls.fading.store(true, Relaxed);
                }
                Command::Cancel => {
                    self.fade = None;
                    self.controls.fading.store(false, Relaxed);
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
        let rates = std::array::from_fn::<_, 2, _>(|d| get(&c.rate[d]).clamp(0.5, 2.));
        let gains = std::array::from_fn::<_, 2, _>(|d| get(&c.gain[d]).clamp(0., 1.));
        let eq = std::array::from_fn::<_, 2, _>(|d| {
            std::array::from_fn::<_, 3, _>(|b| 10f32.powf(get(&c.eq[d][b]) / 20.))
        });
        let alpha_low = 1. - (-std::f32::consts::TAU * 200. / self.rate as f32).exp();
        let alpha_high = 1. - (-std::f32::consts::TAU * 2000. / self.rate as f32).exp();
        let filter = std::array::from_fn::<_, 2, _>(|d| {
            1. - (-std::f32::consts::TAU * get(&c.cutoff[d]).min(self.rate as f32 * 0.45)
                / self.rate as f32)
                .exp()
        });
        let master = get(&c.master);
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
            if self
                .fade
                .as_ref()
                .is_some_and(|fade| c.finished[fade.incoming].load(Relaxed))
            {
                self.fade = None;
                c.fading.store(false, Relaxed);
            }
            if let Some(fade) = &mut self.fade {
                if !fade.started && c.ready[fade.incoming].load(Relaxed) {
                    c.play[fade.incoming].store(true, Relaxed);
                    fade.started = true;
                }
                if fade.started {
                    let t = (fade.elapsed as f32 / fade.total as f32).clamp(0., 1.);
                    set(&c.crossfader, fade.from + (fade.to - fade.from) * t);
                    fade.elapsed += 1;
                    if fade.elapsed > fade.total {
                        set(&c.crossfader, fade.to);
                        c.play[1 - fade.incoming].store(false, Relaxed);
                        c.fading.store(false, Relaxed);
                        self.fade = None;
                    }
                }
            }
            let x = get(&c.crossfader);
            let weights = [
                (x * std::f32::consts::FRAC_PI_2).cos(),
                (x * std::f32::consts::FRAC_PI_2).sin(),
            ];
            let mut mix = [0.; 2];
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
                let frame = if repeating {
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
                    source.next(rates[d])
                };
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
                if let Some(l) = &mut v.looped
                    && l.capturing
                {
                    l.frames.push(frame);
                    if l.frames.len() >= l.length {
                        l.capturing = false;
                    }
                }
                v.gain += (gains[d] - v.gain) * 0.004;
                for ch in 0..2 {
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
                    frame[ch] *= v.gain;
                    peaks[d] = peaks[d].max(frame[ch].abs());
                    mix[ch] += frame[ch] * weights[d] * std::f32::consts::FRAC_1_SQRT_2;
                }
                let bin = i * 96 / frames.max(1);
                waves[d][bin] = waves[d][bin].max((frame[0] + frame[1]).abs() * 0.5);
            }
            output(
                i,
                [
                    (mix[0] * master).clamp(-0.98, 0.98),
                    (mix[1] * master).clamp(-0.98, 0.98),
                ],
            );
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
    commands: mpsc::SyncSender<Command>,
    pub controls: Arc<Controls>,
    pub sample_rate: u32,
    pub device: String,
    cancel: [Option<Arc<AtomicBool>>; 2],
}
impl Engine {
    pub fn new() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No audio output device is available")?;
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
            commands: tx,
            controls,
            sample_rate: rate,
            device: name,
            cancel: Default::default(),
        })
    }
    pub fn load(
        &mut self,
        deck: usize,
        root: &Path,
        track: &amp_core::Track,
        play: bool,
    ) -> Result<(), String> {
        track.preparation.validate()?;
        let path = amp_core::resolve(root, &track.path).map_err(|e| e.to_string())?;
        if let Some(cancel) = self.cancel[deck].take() {
            cancel.store(true, Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let (mut producer, consumer) = RingBuffer::<Frame>::new(self.sample_rate as usize * 4);
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
                let mut child = amp_core::command("ffmpeg")
                    .args(["-v", "error"])
                    .args(amp_core::cue_args(&prep)?)
                    .arg("-i")
                    .arg(path)
                    .args([
                        "-vn",
                        "-ac",
                        "2",
                        "-ar",
                        &rate.to_string(),
                        "-f",
                        "f32le",
                        "pipe:1",
                    ])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .map_err(|e| e.to_string())?;
                let decoded: Result<(), String> = (|| {
                    let mut input = std::io::BufReader::new(
                        child.stdout.take().ok_or("Decoder output missing")?,
                    );
                    let mut bytes = [0; 8192];
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
                        if n % 8 != 0 {
                            return Err("Incomplete PCM frame".into());
                        }
                        for b in bytes[..n].as_chunks::<8>().0 {
                            let mut frame = [
                                f32::from_le_bytes(b[..4].try_into().unwrap()),
                                f32::from_le_bytes(b[4..].try_into().unwrap()),
                            ];
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
            if let Err(error) = result
                && !stopped.load(Relaxed)
                && let Ok(mut message) = errors.error.lock()
            {
                *message = Some(error);
            }
            eof.store(true, std::sync::atomic::Ordering::Release);
        });
        self.cancel[deck] = Some(cancel);
        self.send(Command::Load(
            deck,
            Source {
                ring: consumer,
                duration,
                done,
                a: None,
                b: None,
                phase: 0.,
                position: 0.,
                primed: false,
            },
            play,
        ))?;
        self.controls.play[deck].store(play, Relaxed);
        Ok(())
    }
    fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .try_send(command)
            .map_err(|_| "Audio command queue is busy".into())
    }
    pub fn stop(&mut self, deck: usize) {
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
    pub fn set_gain(&self, deck: usize, value: f32) {
        set(&self.controls.gain[deck], value);
    }
    pub fn set_rate(&self, deck: usize, value: f32) {
        set(&self.controls.rate[deck], value);
    }
    pub fn set_master(&self, value: f32) {
        set(&self.controls.master, value);
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
    pub fn wave(&self, deck: usize) -> [f32; 96] {
        std::array::from_fn(|i| get(&self.controls.scope[deck][i]))
    }
    pub fn take_error(&self) -> Option<String> {
        self.controls.error.lock().ok()?.take()
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        for c in self.cancel.iter().flatten() {
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
            p.push([value, value]).unwrap();
        }
        Source {
            ring: c,
            duration: Arc::new(AtomicU64::new(frames as u64)),
            done: Arc::new(AtomicBool::new(true)),
            a: None,
            b: None,
            phase: 0.,
            position: 0.,
            primed: false,
        }
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
                producer.push([value, value]).unwrap();
            }
            let mut voice = source(0., 2);
            voice.ring = consumer;
            tx.send(Command::Load(0, voice, true)).unwrap();
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
        tx.send(Command::Load(0, source(0.2, 4000), true)).unwrap();
        tx.send(Command::Load(1, source(0.3, 4000), false)).unwrap();
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
        tx.send(Command::Load(0, source(0.2, 5000), true)).unwrap();
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
    fn fade_waits_for_incoming_pcm() {
        let c = Arc::new(Controls::default());
        let (tx, rx) = mpsc::sync_channel(16);
        let mut m = Mixer::new(1000, rx, c.clone());
        tx.send(Command::Load(0, source(0.2, 5000), true)).unwrap();
        let (_p, r) = RingBuffer::new(1000);
        tx.send(Command::Load(
            1,
            Source {
                ring: r,
                duration: Arc::new(AtomicU64::new(0)),
                done: Arc::new(AtomicBool::new(false)),
                a: None,
                b: None,
                phase: 0.,
                position: 0.,
                primed: false,
            },
            false,
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
