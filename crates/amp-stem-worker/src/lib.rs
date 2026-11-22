//! Bounded SCNet spectral adapter. No downloads, audio files or inference substitutes.
//! FFT layout/normalization matches the published SCNet spectral model contract.
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex, num_complex::Complex};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
    sync::Arc,
};
pub const MAX_FRAMES: usize = 485100;
pub const FFT: usize = 4096;
pub const HOP: usize = 1024;
pub const BINS: usize = 2049;
pub const TIMES: usize = 476;
pub const PLANES: usize = BINS * TIMES;
const PADDED: usize = 486400;
pub const MODEL_BYTES: u64 = 12900277;
pub const MODEL_SHA256: &str = "98228931494151762a1c4ab1ec7899a894b1f81fd4a509921a9d2f9bacc50845";
pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub version: u32,
    pub request_id: String,
    pub frames: usize,
    pub sample_rate: u32,
    pub channels: u32,
}
pub struct Request {
    pub header: Header,
    pub pcm: Vec<f32>,
}
pub fn read_request(input: &mut impl Read) -> Result<Option<Request>> {
    let mut line = Vec::with_capacity(1024);
    loop {
        if line.len() >= 1024 {
            return Err("Request header exceeds1024bytes".into());
        }
        let mut byte = [0];
        match input.read(&mut byte) {
            Ok(0) if line.is_empty() => return Ok(None),
            Ok(0) => return Err("Truncated request header".into()),
            Ok(_) => {
                line.push(byte[0]);
                if byte[0] == b'\n' {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("Request header: {e}")),
        }
    }
    let header: Header = serde_json::from_slice(&line).map_err(|_| "Invalid request JSON")?;
    if header.version != 1
        || header.sample_rate != 44100
        || header.channels != 2
        || !(1..=MAX_FRAMES).contains(&header.frames)
        || header.request_id.is_empty()
        || header.request_id.len() > 64
        || !header
            .request_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("Invalid request shape or identity".into());
    }
    let mut bytes = vec![0u8; header.frames * 8];
    input
        .read_exact(&mut bytes)
        .map_err(|_| "Truncated stereo PCM")?;
    let pcm: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| f32::from_le_bytes(*v))
        .collect();
    if pcm.iter().any(|v| !v.is_finite()) {
        return Err("Non-finite source PCM".into());
    }
    Ok(Some(Request { header, pcm }))
}
/// Read and verify the exact bytes later passed to ORT, avoiding hash/load path races.
pub fn load_model(path: &Path) -> Result<Vec<u8>> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "Pinned local SCNet model missing")?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != MODEL_BYTES {
        return Err("Pinned13MB SCNet model required".into());
    }
    let input = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::with_capacity(MODEL_BYTES as usize);
    input
        .take(MODEL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() != MODEL_BYTES as usize
        || format!("{:x}", Sha256::digest(&bytes)) != MODEL_SHA256
    {
        return Err("SCNet model SHA256 mismatch".into());
    }
    Ok(bytes)
}
pub fn verify_model(path: &Path) -> Result<()> {
    load_model(path).map(|_| ())
}
pub fn write_parts(
    output: &mut impl Write,
    header: &serde_json::Value,
    parts: &[Vec<f32>; 4],
) -> Result<()> {
    let frames = header["frames"].as_u64().ok_or("Response frames missing")? as usize;
    if !(1..=MAX_FRAMES).contains(&frames)
        || parts
            .iter()
            .any(|p| p.len() != frames * 2 || p.iter().any(|v| !v.is_finite()))
    {
        return Err("Invalid output PCM shape or samples".into());
    }
    let mut line = serde_json::to_vec(header).map_err(|e| e.to_string())?;
    line.push(b'\n');
    if line.len() > 1024 {
        return Err("Response header exceeds1024bytes".into());
    }
    output.write_all(&line).map_err(|e| e.to_string())?;
    let mut bytes = [0u8; 8192];
    for part in parts {
        for chunk in part.chunks(2048) {
            for (v, b) in chunk.iter().zip(bytes.as_chunks_mut::<4>().0.iter_mut()) {
                b.copy_from_slice(&v.to_le_bytes());
            }
            output
                .write_all(&bytes[..chunk.len() * 4])
                .map_err(|e| e.to_string())?;
        }
    }
    output.flush().map_err(|e| e.to_string())
}
fn reflect(index: i64, length: usize) -> usize {
    if length <= 1 {
        return 0;
    }
    let period = 2 * (length as i64 - 1);
    let wrapped = index.rem_euclid(period);
    if wrapped < length as i64 {
        wrapped as usize
    } else {
        (period - wrapped) as usize
    }
}
fn statistics(audio: &[f32]) -> Result<(f64, f64)> {
    if audio.is_empty()
        || !audio.len().is_multiple_of(2)
        || audio.len() > MAX_FRAMES * 2
        || audio.iter().any(|v| !v.is_finite())
    {
        return Err("Invalid bounded stereo source".into());
    }
    let frames = audio.len() / 2;
    let mono = |v: &[f32; 2]| (v[0] as f64 + v[1] as f64) * 0.5;
    let mean = audio.as_chunks::<2>().0.iter().map(mono).sum::<f64>() / frames as f64;
    let variance = if frames > 1 {
        audio
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| (mono(v) - mean).powi(2))
            .sum::<f64>()
            / (frames - 1) as f64
    } else {
        0.
    };
    Ok((mean, variance.sqrt() + 1e-8))
}
fn padded_sample(audio: &[f32], index: usize, channel: usize, mean: f64, scale: f64) -> f64 {
    let frames = audio.len() / 2;
    if index >= MAX_FRAMES {
        return 0.;
    }
    let source = if index < frames {
        index
    } else if frames > MAX_FRAMES / 2 {
        reflect(index as i64, frames)
    } else {
        return 0.;
    };
    (audio[source * 2 + channel] as f64 - mean) / scale
}
pub struct Spectral {
    forward: Arc<dyn RealToComplex<f64>>,
    inverse: Arc<dyn ComplexToReal<f64>>,
    real: Vec<f64>,
    complex: Vec<Complex<f64>>,
    forward_scratch: Vec<Complex<f64>>,
    inverse_scratch: Vec<Complex<f64>>,
    weight: Vec<f64>,
}
impl Default for Spectral {
    fn default() -> Self {
        Self::new()
    }
}
impl Spectral {
    pub fn new() -> Self {
        let mut planner = RealFftPlanner::<f64>::new();
        let forward = planner.plan_fft_forward(FFT);
        let inverse = planner.plan_fft_inverse(FFT);
        let real = forward.make_input_vec();
        let complex = forward.make_output_vec();
        let forward_scratch = forward.make_scratch_vec();
        let inverse_scratch = inverse.make_scratch_vec();
        let mut weight = vec![0.; (TIMES - 1) * HOP + FFT];
        for t in 0..TIMES {
            for v in &mut weight[t * HOP..t * HOP + FFT] {
                *v += 1.;
            }
        }
        Self {
            forward,
            inverse,
            real,
            complex,
            forward_scratch,
            inverse_scratch,
            weight,
        }
    }
    /// Returns [1,4,2049,476] f32 C-contiguous, sampleStd and mono mean.
    pub fn analyze(&mut self, audio: &[f32]) -> Result<(Vec<f32>, f64, f64)> {
        let (mean, scale) = statistics(audio)?;
        let mut spec = vec![0f32; 4 * PLANES];
        for channel in 0..2 {
            for t in 0..TIMES {
                for (i, v) in self.real.iter_mut().enumerate() {
                    let index = reflect((t * HOP + i) as i64 - (FFT / 2) as i64, PADDED);
                    *v = padded_sample(audio, index, channel, mean, scale);
                }
                self.forward
                    .process_with_scratch(
                        &mut self.real,
                        &mut self.complex,
                        &mut self.forward_scratch,
                    )
                    .map_err(|e| e.to_string())?;
                for (bin, z) in self.complex.iter().enumerate() {
                    spec[(channel * 2) * PLANES + bin * TIMES + t] = (z.re / 64.) as f32;
                    spec[(channel * 2 + 1) * PLANES + bin * TIMES + t] = (z.im / 64.) as f32;
                }
            }
        }
        if spec.iter().any(|v| !v.is_finite()) {
            return Err("Normalized spectrum overflow".into());
        }
        Ok((spec, scale, mean))
    }
    /// Reconstructs one four-plane complex stereo source, preserving NumPy irfft behavior.
    pub fn synthesize(&mut self, planes: &[f32], frames: usize, scale: f64) -> Result<Vec<f32>> {
        if planes.len() != 4 * PLANES
            || !(1..=MAX_FRAMES).contains(&frames)
            || !scale.is_finite()
            || scale <= 0.
            || planes.iter().any(|v| !v.is_finite())
        {
            return Err("Invalid spectral output".into());
        }
        let mut result = vec![0f32; frames * 2];
        let mut wave = vec![0f64; self.weight.len()];
        for channel in 0..2 {
            wave.fill(0.);
            for t in 0..TIMES {
                for (bin, z) in self.complex.iter_mut().enumerate() {
                    *z = Complex::new(
                        planes[channel * 2 * PLANES + bin * TIMES + t] as f64,
                        planes[(channel * 2 + 1) * PLANES + bin * TIMES + t] as f64,
                    );
                }
                // NumPy irfft discards imaginary DC/Nyquist; model output need not make these zero.
                self.complex[0].im = 0.;
                self.complex[BINS - 1].im = 0.;
                self.inverse
                    .process_with_scratch(
                        &mut self.complex,
                        &mut self.real,
                        &mut self.inverse_scratch,
                    )
                    .map_err(|e| e.to_string())?;
                for (i, v) in self.real.iter().enumerate() {
                    wave[t * HOP + i] += v / 64.;
                }
            }
            for frame in 0..frames {
                let index = frame + FFT / 2;
                result[frame * 2 + channel] = (wave[index] / self.weight[index] * scale) as f32;
            }
        }
        if result.iter().any(|v| !v.is_finite()) {
            return Err("Inverse PCM overflow".into());
        }
        Ok(result)
    }
    /// Graph source order drums,bass,other,vocals -> wire vocals,drums,bass,residual.
    pub fn separate_output(
        &mut self,
        spec: &[f32],
        original: &[f32],
        scale: f64,
    ) -> Result<[Vec<f32>; 4]> {
        if spec.len() != 16 * PLANES || spec.iter().any(|v| !v.is_finite()) {
            return Err("Unexpected SCNet spectral tensor".into());
        }
        statistics(original)?;
        let frames = original.len() / 2;
        let vocals = self.synthesize(&spec[12 * PLANES..16 * PLANES], frames, scale)?;
        let drums = self.synthesize(&spec[..4 * PLANES], frames, scale)?;
        let bass = self.synthesize(&spec[4 * PLANES..8 * PLANES], frames, scale)?;
        let other: Vec<f32> = original
            .iter()
            .zip(&vocals)
            .zip(&drums)
            .zip(&bass)
            .map(|(((a, v), d), b)| a - v - d - b)
            .collect();
        if other.iter().any(|v| !v.is_finite()) {
            return Err("Residual overflow".into());
        }
        Ok([vocals, drums, bass, other])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    fn request(frames: usize) -> Vec<u8> {
        let mut h=serde_json::to_vec(&serde_json::json!({"version":1,"frames":frames,"request_id":"fixture-1","sample_rate":44100,"channels":2})).unwrap();
        h.push(b'\n');
        h
    }
    #[test]
    fn protocol_eof_bounds_and_exact_finite_stereo() {
        assert!(read_request(&mut Cursor::new(vec![])).unwrap().is_none());
        assert!(read_request(&mut Cursor::new(vec![b'x'; 1025])).is_err());
        for frames in [0, MAX_FRAMES + 1] {
            assert!(read_request(&mut Cursor::new(request(frames))).is_err());
        }
        let mut h = request(1);
        h.extend([0; 7]);
        assert!(read_request(&mut Cursor::new(h)).is_err());
        let mut h = request(1);
        h.extend(f32::NAN.to_le_bytes());
        h.extend(0f32.to_le_bytes());
        assert!(read_request(&mut Cursor::new(h)).is_err());
        let mut h = request(1);
        h.extend(0.25f32.to_le_bytes());
        h.extend((-0.5f32).to_le_bytes());
        let r = read_request(&mut Cursor::new(h)).unwrap().unwrap();
        assert_eq!(r.pcm, vec![0.25, -0.5]);
    }
    #[test]
    fn reflection_and_padding_match_reference_rules() {
        assert_eq!(
            (0..8).map(|i| reflect(i, 4)).collect::<Vec<_>>(),
            [0, 1, 2, 3, 2, 1, 0, 1]
        );
        assert_eq!(reflect(-1, 4), 1);
        assert_eq!(reflect(-2048, PADDED), 2048);
        let short = vec![2f32; 8];
        assert_eq!(padded_sample(&short, 4, 0, 1., 2.), 0.);
        let long = vec![2f32; (MAX_FRAMES / 2 + 1) * 2];
        assert_eq!(padded_sample(&long, long.len() / 2 + 1, 0, 1., 2.), 0.5);
        assert_eq!(padded_sample(&long, MAX_FRAMES, 0, 1., 2.), 0.);
    }
    #[test]
    fn mono_mean_and_sample_std_use_both_channels() {
        let (mean, scale) = statistics(&[1., 3., 3., 5.]).unwrap();
        assert_eq!(mean, 3.);
        assert!((scale - (2f64.sqrt() + 1e-8)).abs() < 1e-12);
        let (_, scale) = statistics(&[2., 2.]).unwrap();
        assert_eq!(scale, 1e-8);
    }
    #[test]
    fn real_fft_ola_roundtrip_and_selected_bins_match_direct_dft() {
        let audio: Vec<f32> = (0..8192)
            .flat_map(|i| {
                [
                    ((i as f64 * 0.071).sin() * 0.4 + 0.1) as f32,
                    ((i as f64 * 0.037).cos() * 0.2 - 0.05) as f32,
                ]
            })
            .collect();
        let mut fft = Spectral::new();
        let (spec, scale, mean) = fft.analyze(&audio).unwrap();
        let restored = fft.synthesize(&spec, audio.len() / 2, scale).unwrap();
        let error = restored
            .iter()
            .zip(&audio)
            .map(|(a, b)| (*a as f64 + mean - *b as f64).abs())
            .fold(0f64, f64::max);
        assert!(error < 2e-7, "roundtrip error {error}");
        for bin in [0, 1, 17, 2048] {
            let mut re = 0.;
            let mut im = 0.;
            for i in 0..FFT {
                let index = reflect(i as i64 - 2048, PADDED);
                let sample = padded_sample(&audio, index, 0, mean, scale);
                let angle = -2. * std::f64::consts::PI * bin as f64 * i as f64 / FFT as f64;
                re += sample * angle.cos() / 64.;
                im += sample * angle.sin() / 64.;
            }
            assert!((spec[bin * TIMES] as f64 - re).abs() < 2e-5);
            assert!((spec[PLANES + bin * TIMES] as f64 - im).abs() < 2e-5);
        }
    }
    #[test]
    fn output_is_validated_before_any_publication() {
        let mut out = Vec::new();
        let h = serde_json::json!({"frames":1});
        assert!(
            write_parts(
                &mut out,
                &h,
                &[vec![0.; 2], vec![f32::NAN; 2], vec![0.; 2], vec![0.; 2]]
            )
            .is_err()
        );
        assert!(out.is_empty());
        let parts = std::array::from_fn(|i| vec![i as f32; 2]);
        write_parts(&mut out, &h, &parts).unwrap();
        let newline = out.iter().position(|v| *v == b'\n').unwrap();
        assert_eq!(out.len() - newline - 1, 32);
    }
    #[test]
    fn refuses_wrong_model_without_loading_runtime() {
        assert!(verify_model(Path::new("not-an-installed-model.onnx")).is_err());
    }
    #[test]
    fn matching_size_untrusted_model_is_hash_rejected() {
        // Sparse, all-zero test model only; never an owner path or audio file.
        let name = format!(
            "amp-scnet-invalid-model-{}-{}.onnx",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(name);
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.set_len(MODEL_BYTES).unwrap();
        drop(file);
        assert!(load_model(&path).unwrap_err().contains("SHA256"));
    }
}
