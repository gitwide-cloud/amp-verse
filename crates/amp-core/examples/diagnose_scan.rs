fn main() {
    let root = std::path::PathBuf::from(std::env::args_os().nth(1).unwrap());
    let start = std::time::Instant::now();
    match amp_core::scan_job_inputs(&root) {
        Ok(lib) => println!(
            "Scanned {} tracks, {} additional, in {:?}",
            lib.tracks.len(),
            lib.tracks
                .iter()
                .filter(|t| t.path.starts_with(amp_core::music_sources::PREFIX))
                .count(),
            start.elapsed()
        ),
        Err(e) => eprintln!("Scan failed after {:?}: {e}", start.elapsed()),
    }
}
