//! MIT Signalsmith backend. Constructed and warmed on the load thread; fixed-size
//! processing buffers are reused by the audio callback. One processor follows
//! the summed stem signal, so stems cannot drift relative to one another.
use std::ffi::c_void;
#[cfg(not(target_os = "windows"))]
unsafe extern "C" {
    fn signalsmith_stretch_create_preset_cheaper(channels: i32, rate: f32) -> *mut c_void;
    fn signalsmith_stretch_destroy(handle: *mut c_void);
    fn signalsmith_stretch_set_transpose_factor(handle: *mut c_void, factor: f32, tonality: f32);
    fn signalsmith_stretch_process(
        handle: *mut c_void,
        input: *const f32,
        input_frames: usize,
        output: *mut f32,
        output_frames: usize,
    );
    fn signalsmith_stretch_input_latency(handle: *mut c_void) -> usize;
    fn signalsmith_stretch_output_latency(handle: *mut c_void) -> usize;
    fn signalsmith_stretch_reset(handle: *mut c_void);
}
#[cfg(target_os = "windows")]
mod native {
    use super::*;
    struct Backend {
        _library: libloading::Library,
        create: unsafe extern "C" fn(i32, f32) -> *mut c_void,
        destroy: unsafe extern "C" fn(*mut c_void),
        transpose: unsafe extern "C" fn(*mut c_void, f32, f32),
        process: unsafe extern "C" fn(*mut c_void, *const f32, usize, *mut f32, usize),
        input_latency: unsafe extern "C" fn(*mut c_void) -> usize,
        output_latency: unsafe extern "C" fn(*mut c_void) -> usize,
        reset: unsafe extern "C" fn(*mut c_void),
    }
    fn backend() -> Option<&'static Backend> {
        static BACKEND: std::sync::OnceLock<Option<Backend>> = std::sync::OnceLock::new();
        BACKEND
            .get_or_init(|| unsafe {
                let path = std::env::current_exe()
                    .ok()?
                    .parent()?
                    .join("amp-keylock.dll");
                let library = libloading::Library::new(path).ok()?;
                Some(Backend {
                    create: *library
                        .get(b"signalsmith_stretch_create_preset_cheaper\0")
                        .ok()?,
                    destroy: *library.get(b"signalsmith_stretch_destroy\0").ok()?,
                    transpose: *library
                        .get(b"signalsmith_stretch_set_transpose_factor\0")
                        .ok()?,
                    process: *library.get(b"signalsmith_stretch_process\0").ok()?,
                    input_latency: *library.get(b"signalsmith_stretch_input_latency\0").ok()?,
                    output_latency: *library.get(b"signalsmith_stretch_output_latency\0").ok()?,
                    reset: *library.get(b"signalsmith_stretch_reset\0").ok()?,
                    _library: library,
                })
            })
            .as_ref()
    }
    pub unsafe fn signalsmith_stretch_create_preset_cheaper(ch: i32, rate: f32) -> *mut c_void {
        backend().map_or(std::ptr::null_mut(), |b| unsafe { (b.create)(ch, rate) })
    }
    pub unsafe fn signalsmith_stretch_destroy(h: *mut c_void) {
        if let Some(b) = backend() {
            unsafe { (b.destroy)(h) }
        }
    }
    pub unsafe fn signalsmith_stretch_set_transpose_factor(h: *mut c_void, r: f32, t: f32) {
        if let Some(b) = backend() {
            unsafe { (b.transpose)(h, r, t) }
        }
    }
    pub unsafe fn signalsmith_stretch_process(
        h: *mut c_void,
        i: *const f32,
        n: usize,
        o: *mut f32,
        m: usize,
    ) {
        if let Some(b) = backend() {
            unsafe { (b.process)(h, i, n, o, m) }
        }
    }
    pub unsafe fn signalsmith_stretch_input_latency(h: *mut c_void) -> usize {
        backend().map_or(0, |b| unsafe { (b.input_latency)(h) })
    }
    pub unsafe fn signalsmith_stretch_output_latency(h: *mut c_void) -> usize {
        backend().map_or(0, |b| unsafe { (b.output_latency)(h) })
    }
    pub unsafe fn signalsmith_stretch_reset(h: *mut c_void) {
        if let Some(b) = backend() {
            unsafe { (b.reset)(h) }
        }
    }
}
#[cfg(target_os = "windows")]
use native::*;
const BLOCK: usize = 128;
pub struct Processor {
    handle: *mut c_void,
    input: [f32; BLOCK * 2],
    output: [f32; BLOCK * 2],
    at: usize,
    primed: usize,
    pub latency: usize,
    wet: f32,
}
// The opaque instance has exactly one owner and moves with its Source to the
// audio callback. No method provides shared mutation or calls it concurrently.
unsafe impl Send for Processor {}
impl Processor {
    pub fn new(rate: u32) -> Option<Self> {
        if std::env::var_os("AMP_DISABLE_KEYLOCK").is_some() {
            return None;
        }
        // SAFETY: two channels, valid sample rate; the returned C++ instance is
        // uniquely owned until Drop. The fixed buffer lengths match the ABI.
        let handle = unsafe { signalsmith_stretch_create_preset_cheaper(2, rate as f32) };
        if handle.is_null() {
            return None;
        }
        let latency = unsafe {
            signalsmith_stretch_input_latency(handle) + signalsmith_stretch_output_latency(handle)
        } + BLOCK;
        let zeros = [0f32; 4096];
        let mut output = [0f32; 4096];
        unsafe {
            signalsmith_stretch_process(handle, zeros.as_ptr(), 2048, output.as_mut_ptr(), 2048);
            signalsmith_stretch_reset(handle);
        }
        Some(Self {
            handle,
            input: [0.; BLOCK * 2],
            output: [0.; BLOCK * 2],
            at: 0,
            primed: 0,
            latency,
            wet: 0.,
        })
    }
    pub fn frame(
        &mut self,
        frame: [f32; 2],
        rate: f32,
        enabled: bool,
        scratching: bool,
    ) -> [f32; 2] {
        if !enabled || scratching || rate <= 0. {
            self.primed = 0;
            self.wet = 0.;
            return frame;
        }
        let offset = self.at * 2;
        self.input[offset..offset + 2].copy_from_slice(&frame);
        let processed = [self.output[offset], self.output[offset + 1]];
        self.at += 1;
        self.primed += 1;
        if self.at == BLOCK {
            // SAFETY: the unique handle is used only on its owning audio thread;
            // both fixed arrays hold BLOCK stereo frames and remain valid.
            unsafe {
                signalsmith_stretch_set_transpose_factor(self.handle, 1. / rate.clamp(0.5, 2.), 0.);
                signalsmith_stretch_process(
                    self.handle,
                    self.input.as_ptr(),
                    BLOCK,
                    self.output.as_mut_ptr(),
                    BLOCK,
                );
            }
            self.at = 0;
        }
        if self.primed > self.latency {
            self.wet = (self.wet + 0.001).min(1.);
        }
        std::array::from_fn(|ch| frame[ch] * (1. - self.wet) + processed[ch] * self.wet)
    }
}
impl Drop for Processor {
    fn drop(&mut self) {
        unsafe { signalsmith_stretch_destroy(self.handle) }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locked_tempo_preserves_a_measured_sine_pitch() {
        let rate = 48000;
        let mut processor = Processor::new(rate).unwrap();
        let mut crossings = 0;
        let mut last = 0.;
        let started = std::time::Instant::now();
        for i in 0..rate * 3 {
            let input =
                (i as f32 * std::f32::consts::TAU * (440. * 1.16) / rate as f32).sin() * 0.2;
            let value = processor.frame([input, input], 1.16, true, false)[0];
            if i >= rate * 2 && last <= 0. && value > 0. {
                crossings += 1;
            }
            last = value;
        }
        assert!(
            (crossings as f32 - 440.).abs() < 5.,
            "Measured {crossings} Hz"
        );
        eprintln!(
            "Key Lock 3 s stereo: {:?}, latency {} frames",
            started.elapsed(),
            processor.latency
        );
    }
}
