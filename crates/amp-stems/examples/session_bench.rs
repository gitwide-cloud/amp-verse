//! Measures session control/validation only. No model or audio-quality claim.
use amp_stems::{Block, Session, Window};
fn main() {
    let start = std::time::Instant::now();
    let mut session = Session::default();
    let mut peak = 0;
    for i in 0..10000 {
        let window = Window {
            start_frame: i * 480,
            frames: 480,
            sample_rate: 48000,
        };
        let ticket = session.begin(0, window).unwrap();
        peak = peak.max(session.accounted_bytes());
        let block = Block {
            parts: std::array::from_fn(|_| vec![0.; 960]),
        };
        session.finish(ticket, Ok(block)).unwrap();
        peak = peak.max(session.accounted_bytes());
        session.unload(0).unwrap();
    }
    println!(
        "control_plane_only=true windows=10000 peak_accounted_pcm_bytes={peak} final_accounted_pcm_bytes={} elapsed_ms={:.3} model_inference=not_run",
        session.accounted_bytes(),
        start.elapsed().as_secs_f64() * 1000.
    );
}
