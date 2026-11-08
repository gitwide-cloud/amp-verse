//! Headless offline preparation, using the same engine as AMP Library.
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .ok_or("Usage: amp-prepare MUSIC_FOLDER")?;
    let library = amp_core::scan(&root).map_err(|e| e.to_string())?;
    let mut errors = 0;
    for track in &library.tracks {
        let result: Result<amp_core::Preparation, String> = (|| {
            let path = amp_core::resolve(&root, &track.path).map_err(|e| e.to_string())?;
            let before = std::fs::read(amp_core::sidecar(&path)).ok();
            let p = amp_core::analysis::analyze(&path, &track.preparation)?;
            if before != std::fs::read(amp_core::sidecar(&path)).ok() {
                return Err("Metadata was edited during analysis; newer metadata retained".into());
            }
            amp_core::preparation::save(&root, &track.path, &p)?;
            Ok(p)
        })();
        match result {
            Ok(p) => println!(
                "{} / BPM {} / Key {} / {} cues",
                track.path.display(),
                p.bpm
                    .map(|v| format!("{v:.1}"))
                    .unwrap_or_else(|| "unknown".into()),
                p.harmonic_key.unwrap_or_else(|| "unknown".into()),
                p.cues.len() + p.manual_cues.len()
            ),
            Err(e) => {
                errors += 1;
                eprintln!("{}: {e}", track.path.display());
            }
        }
    }
    if errors > 0 {
        return Err(format!("{errors} tracks could not be prepared"));
    }
    Ok(())
}
