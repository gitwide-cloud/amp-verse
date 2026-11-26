//! Headless offline preparation, using the same engine as AMP Library.
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1).collect::<Vec<_>>().into_iter();
    let root = args
        .next()
        .map(std::path::PathBuf::from)
        .ok_or("Usage: amp-prepare MUSIC_FOLDER [--jobs N]")?;
    if args.clone().any(|arg| arg == "--organize") {
        let count = amp_core::storage::organize(&root)?;
        if root.join("ampdj/smart-playlists.json").is_file() {
            amp_core::storage::export_rules(&root)?;
        }
        println!("Organized {count} metadata files into ampdj; original music unchanged");
        return Ok(());
    }
    if args.clone().any(|arg| arg == "--compact") {
        println!(
            "Packed {} records into ampdj/catalog.ampdb; recovery metadata retained inside the database",
            amp_core::catalog_db::compact(&root)?
        );
        return Ok(());
    }
    if args.clone().any(|arg| arg == "--export-rules") {
        println!("{}", amp_core::storage::export_rules(&root)?.display());
        return Ok(());
    }
    if args.clone().any(|arg| arg == "--fence-catalog") {
        amp_core::catalog_db::fence_legacy_clients(&root)?;
        return Ok(());
    }
    let mut requested = 0;
    let mut force = false;
    let mut watch = None;
    while let Some(flag) = args.next() {
        if flag == "--force" {
            force = true;
        } else if flag == "--jobs" {
            requested = args
                .next()
                .and_then(|v| v.to_str().and_then(|s| s.parse::<usize>().ok()))
                .filter(|&n| n > 0)
                .ok_or("--jobs requires a positive worker count")?;
        } else if flag == "--watch" {
            watch = Some(
                args.next()
                    .and_then(|v| v.to_str().and_then(|s| s.parse::<u64>().ok()))
                    .filter(|&n| n >= 10)
                    .ok_or("--watch requires seconds >= 10")?,
            );
        } else {
            return Err(
                "Usage: amp-prepare MUSIC_FOLDER [--jobs N] [--force] [--watch SECONDS]".into(),
            );
        }
    }
    // Initialize identities once before concurrent workers race for their first job.
    amp_core::coordination::device_id()?;
    loop {
        let outcome = prepare(&root, requested, force);
        if let Some(seconds) = watch {
            if let Err(e) = outcome {
                eprintln!("Analysis paused: {e}");
            }
            std::thread::sleep(std::time::Duration::from_secs(seconds));
        } else {
            return outcome;
        }
    }
}
fn prepare(root: &std::path::Path, requested: usize, force: bool) -> Result<(), String> {
    amp_core::coordination::library_id(root)?;
    let library = amp_core::scan_job_inputs(root).map_err(|e| e.to_string())?;
    if !force {
        let queue = amp_core::analysis_queue::Queue::for_library(root, "cli")?;
        queue.enqueue_tracks(&library.tracks)?;
        let errors = std::sync::atomic::AtomicUsize::new(0);
        let count = amp_core::batch::workers(requested, queue.stats()?.pending);
        let slots = vec![(); count];
        amp_core::batch::for_each(&slots, count, |_| {
            loop {
                let job = match queue.next() {
                    Ok(Some(j)) => j,
                    Ok(None) => break,
                    Err(e) => {
                        eprintln!("{e}");
                        errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        break;
                    }
                };
                let _heartbeat = queue.keep_alive(&job);
                let outcome = amp_core::library_engine::analyze_job(root, &job.path, false);
                let result = match &outcome {
                    Ok(amp_core::coordination::AnalysisResult::Skipped(
                        amp_core::coordination::Skip::Busy,
                    )) => queue.defer(&job),
                    Ok(_) => queue.finish(&job, Ok(())),
                    Err(e) => queue.finish(&job, Err(e)),
                };
                if let Err(e) = outcome {
                    errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    eprintln!("{}: {e}", job.path.display());
                } else {
                    println!("{} / queue job processed", job.path.display());
                }
                if let Err(e) = result {
                    errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    eprintln!("{e}");
                }
            }
            true
        });
        let n = errors.load(std::sync::atomic::Ordering::Relaxed);
        return if n == 0 {
            Ok(())
        } else {
            Err(format!(
                "{n} queue jobs failed; completed work was retained"
            ))
        };
    }
    let errors = std::sync::atomic::AtomicUsize::new(0);
    let workers = amp_core::batch::workers(requested, library.tracks.len());
    eprintln!(
        "Analyzing {} tracks with {workers} parallel workers",
        library.tracks.len()
    );
    amp_core::batch::for_each(&library.tracks, workers, |track| {
        let result = amp_core::library_engine::analyze_job(root, &track.path, force);
        match result {
            Ok(amp_core::coordination::AnalysisResult::Skipped(reason)) => {
                println!("{} / skipped {reason:?}", track.path.display())
            }
            Ok(amp_core::coordination::AnalysisResult::Prepared(p)) => println!(
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
