//! Live pre-fader headphone bus. No independent decoder or transport.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;
type Frame = [f32; 2];
struct Resampler {
    input: rtrb::Consumer<Frame>,
    phase: f64,
    step: f64,
    current: Frame,
    next: Frame,
}
impl Resampler {
    fn frame(&mut self) -> Frame {
        while self.phase >= 1. {
            self.current = self.next;
            self.next = self.input.pop().unwrap_or([0.; 2]);
            self.phase -= 1.;
        }
        let out = std::array::from_fn(|ch| {
            self.current[ch] + (self.next[ch] - self.current[ch]) * self.phase as f32
        });
        self.phase += self.step;
        out
    }
}
pub fn open(
    name: &str,
    source_rate: u32,
    consumer: rtrb::Consumer<Frame>,
    controls: Arc<super::mixer::Controls>,
) -> Result<cpal::Stream, String> {
    let device = cpal::default_host()
        .output_devices()
        .map_err(|e| e.to_string())?
        .find(|d| d.description().is_ok_and(|d| d.name() == name))
        .ok_or("Headphone output is unavailable")?;
    let config = device.default_output_config().map_err(|e| e.to_string())?;
    let resampler = Resampler {
        input: consumer,
        phase: 1.,
        step: source_rate as f64 / config.sample_rate() as f64,
        current: [0.; 2],
        next: [0.; 2],
    };
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => stream::<f32>(&device, config.config(), resampler, controls),
        cpal::SampleFormat::I16 => stream::<i16>(&device, config.config(), resampler, controls),
        cpal::SampleFormat::U16 => stream::<u16>(&device, config.config(), resampler, controls),
        other => return Err(format!("Unsupported headphone format {other:?}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}
fn stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut resampler: Resampler,
    controls: Arc<super::mixer::Controls>,
) -> Result<cpal::Stream, String> {
    let channels = config.channels as usize;
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                // Separate devices have separate clocks. Bound monitor latency if they drift.
                if resampler.input.slots() > (config.sample_rate as usize / 10) {
                    while resampler.input.slots() > (config.sample_rate as usize / 50) {
                        let _ = resampler.input.pop();
                    }
                }
                for frame in data.chunks_mut(channels) {
                    let value = resampler.frame();
                    for (ch, sample) in frame.iter_mut().enumerate() {
                        *sample = T::from_sample(if channels == 1 {
                            (value[0] + value[1]) * 0.5
                        } else {
                            value.get(ch).copied().unwrap_or(0.)
                        });
                    }
                }
            },
            move |error| {
                if let Ok(mut message) = controls.monitor_error.lock() {
                    *message = Some(format!("Headphone output: {error}"));
                }
            },
            None,
        )
        .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resamples_monitor_rate_and_returns_silence_after_underrun() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(1000);
        for i in 0..480 {
            producer.push([i as f32 / 1000.; 2]).unwrap();
        }
        let mut r = Resampler {
            input: consumer,
            phase: 1.,
            step: 48000. / 44100.,
            current: [0.; 2],
            next: [0.; 2],
        };
        let output: Vec<_> = (0..600).map(|_| r.frame()).collect();
        assert!((output[220][0] - 0.238).abs() < 0.004);
        assert!(output[500..].iter().all(|f| *f == [0.; 2]));
    }
}
