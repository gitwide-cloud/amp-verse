//! Headless offline preparation, using the same engine as AMP Library.
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let root = args
        .next()
        .map(std::path::PathBuf::from)
        .ok_or("Usage: amp-prepare MUSIC_FOLDER [--jobs N]")?;
    let mut requested = 0;
    if let Some(flag) = args.next() {
        if flag != "--jobs" {
            return Err("Usage: amp-prepare MUSIC_FOLDER [--jobs N]".into());
        }
        requested = args
            .next()
            .and_then(|v| v.to_str().and_then(|s| s.parse::<usize>().ok()))
            .filter(|&n| n > 0)
            .ok_or("--jobs requires a positive worker count")?;
        if args.next().is_some() {
            return Err("Unexpected extra arguments".into());
        }
    }
    let library = amp_core::scan(&root).map_err(|e| e.to_string())?;
    let errors = std::sync::atomic::AtomicUsize::new(0);
    let workers = amp_core::batch::workers(requested, library.tracks.len());
    eprintln!(
        "Analyzing {} tracks with {workers} parallel workers",
        library.tracks.len()
    );
    amp_core::batch::for_each(&library.tracks, workers, |track| {
        let result: Result<amp_core::Preparation, String> = (|| {
            let path = amp_core::resolve(&root, &track.path).map_err(|e| e.to_string())?;
            let (existing, before) = amp_core::preparation::load_with_revision(&path)?;
            let p = amp_core::analysis::analyze(&path, &existing)?;
            if before != amp_core::preparation::revision(&path) {
                return Err("Metadata was edited during analysis; newer metadata retained".into());
            }
            amp_core::preparation::save_if_unchanged(&root, &track.path, &p, before.as_deref())?;
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
                errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                eprintln!("{}: {e}", track.path.display());
            }
        }
        true
    });
    let errors = errors.load(std::sync::atomic::Ordering::Relaxed);
    if errors > 0 {
        return Err(format!("{errors} tracks could not be prepared"));
    }
    Ok(())
}
