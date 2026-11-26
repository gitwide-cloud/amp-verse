//! Synthetic-only baseline. Run with TMPDIR and XDG_DATA_HOME in isolated Sager scratch.
//! No library path argument is accepted; all catalog writes use core publication APIs.
use amp_core::{Preparation, Track, analysis_queue::Queue, catalog_db};
use std::{fs, hint::black_box, path::PathBuf, time::Instant};

fn sample<T>(count: usize, mut f: impl FnMut() -> T) -> serde_json::Value {
    let mut us = Vec::with_capacity(count);
    for _ in 0..count {
        let start = Instant::now();
        black_box(f());
        us.push(start.elapsed().as_secs_f64() * 1e6);
    }
    us.sort_by(f64::total_cmp);
    serde_json::json!({"samples": count, "p50_us": us[(count-1)/2],
        "p95_us": us[((count*95).div_ceil(100)-1).min(count-1)]})
}
fn write_bytes() -> Option<u64> {
    fs::read_to_string("/proc/self/io")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("write_bytes:")?.trim().parse().ok())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count: usize = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "1000".into())
        .parse()?;
    if !(16..=5000).contains(&count) {
        return Err("fixture count must be 16..=5000".into());
    }
    // Queue::for_library chooses its work file independently of the fixture root.
    // Require explicit scratch placement rather than falling back to real app data.
    let scratch = std::env::var_os("TMPDIR").ok_or("Set TMPDIR to isolated scratch storage")?;
    let scratch = fs::canonicalize(scratch)?;
    let data = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .ok_or("Set XDG_DATA_HOME (or LOCALAPPDATA) inside TMPDIR")?;
    let data = fs::canonicalize(data)?;
    if data == scratch || !data.starts_with(&scratch) {
        return Err("Local queue data must be in a subdirectory of TMPDIR".into());
    }
    // Never open a supplied or discovered production root.
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("synthetic-library");
    fs::create_dir(&root)?;
    let tracks: Vec<_> = (0..count)
        .map(|i| Track {
            path: PathBuf::from(format!("fixture-{i:05}.wav")),
            title: format!("Synthetic {i}"),
            preparation: Preparation::default(),
        })
        .collect();
    for track in &tracks {
        fs::write(root.join(&track.path), b"synthetic; never decoded")?;
    }
    // Register only the temporary root, and create the catalog with the real core API.
    amp_core::scan_job_inputs(&root)?;
    catalog_db::compact(&root)?;
    for track in tracks.iter().take(16) {
        amp_core::preparation::save(&root, &track.path, &track.preparation)?;
    }
    let start = Instant::now();
    let q = Queue::for_library(&root, "sag66-synthetic")?;
    let first_queue_open_us = start.elapsed().as_secs_f64() * 1e6;
    q.seed_known(&tracks)?;
    let start = Instant::now();
    black_box(catalog_db::preparations(&root)?);
    let first_preparations_read_us = start.elapsed().as_secs_f64() * 1e6;
    let warm_preparations = sample(20, || catalog_db::preparations(&root).unwrap());
    let lookup = root.join("ampdj/tracks/fixture-00000.wav.json");
    assert!(catalog_db::read(&lookup)?.is_some());
    let lookup_timings = sample(100, || catalog_db::read(&lookup).unwrap());
    let queue_stats = sample(20, || q.stats().unwrap());
    let job = q.next()?.ok_or("fixture queue unexpectedly empty")?;
    let catalog_bytes = fs::metadata(catalog_db::path(&root))?.len();
    let before_io = write_bytes();
    let heartbeats = sample(10, || q.heartbeat(&job).unwrap());
    let physical_write_bytes = before_io
        .zip(write_bytes())
        .map(|(a, b)| b.saturating_sub(a));
    q.defer(&job)?;
    let accepted_after_defer = q.heartbeat(&job).is_ok();
    q.finish(&job, Ok(()))?;
    let stats = q.stats()?;
    assert_eq!(stats.total, count);
    assert_eq!(stats.done, 1);
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "fixture_tracks": count, "fixture_preparations": 16,
            "first_queue_open_us": first_queue_open_us,
            "first_preparations_read_us": first_preparations_read_us,
            "warm_preparations": warm_preparations, "lookup": lookup_timings,
            "queue_stats": queue_stats, "heartbeat": heartbeats,
            "catalog_snapshot_bytes": catalog_bytes,
            "heartbeat_process_write_bytes": physical_write_bytes,
            "heartbeat_accepts_deferred_claim": accepted_after_defer,
            "limitations": "Local synthetic fixture, first reads are not OS-cold. No NAS timing, DSP retries, playback underruns or real-library opening measured. /proc write_bytes covers process block writes, not network traffic."
        }))?
    );
    Ok(())
}
