//! Own the lifetime of blocking Cast V2 I/O in a child process. A stuck TLS read,
//! connect or stop_app cannot retain an in-process thread or block reconnection.
use super::{Device, Session, cast_in_process};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
        mpsc,
    },
    time::{Duration, Instant},
};
const STOP_GRACE: Duration = Duration::from_secs(2);
const CONNECT_LIMIT: Duration = Duration::from_secs(15);
const IDLE_LIMIT: Duration = Duration::from_secs(30);
#[derive(serde::Serialize, serde::Deserialize)]
struct Report {
    message: String,
    done: bool,
    accepted: bool,
    stop_confirmed: bool,
    activity: u64,
}

pub(super) fn start(device: Device, url: String) -> Session {
    let stop = Arc::new(AtomicBool::new(false));
    let status = Arc::new(Mutex::new("Connecting receiver…".into()));
    let done = Arc::new(AtomicBool::new(false));
    let accepted = Arc::new(AtomicBool::new(false));
    let stop_confirmed = Arc::new(AtomicBool::new(false));
    let activity = Arc::new(AtomicU64::new(0));
    let (cancel, message, ended) = (stop.clone(), status.clone(), done.clone());
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            #[cfg(not(test))]
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            #[cfg(test)]
            let exe = std::env::var_os("AMP_PLAYER_EXE")
                .map(std::path::PathBuf::from)
                .ok_or("Set AMP_PLAYER_EXE to the built player for an authorized receiver test")?;
            let payload = serde_json::to_string(&(device, url)).map_err(|e| e.to_string())?;
            let child = amp_core::command(
                exe.to_str()
                    .ok_or("Player executable path is not valid Unicode")?,
            )
            .arg("--amp-cast-worker")
            .arg(payload)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
            supervise(child, &cancel, &message, STOP_GRACE)
        })();
        if let Err(error) = result {
            *message.lock().unwrap() = format!("{error}. Local playback continues.");
        }
        // Only release receiver selection after the child has been reaped.
        ended.store(true, Relaxed);
    });
    Session {
        stop,
        status,
        done,
        accepted,
        stop_confirmed,
        activity,
    }
}

fn supervise(
    mut child: Child,
    cancel: &AtomicBool,
    message: &Mutex<String>,
    grace: Duration,
) -> Result<(), String> {
    let mut input = child.stdin.take();
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Receiver worker output missing".into());
    };
    let (tx, rx) = mpsc::sync_channel(8);
    let reader = std::thread::spawn(move || {
        let mut stream = BufReader::new(stdout);
        loop {
            let mut line = Vec::new();
            // Bound even malformed worker output; normal reports are < 4 KiB.
            let result = (&mut stream).take(4096).read_until(b'\n', &mut line);
            if !matches!(result, Ok(n) if n > 0) {
                break;
            }
            if let Ok(report) = serde_json::from_slice::<Report>(&line) {
                let _ = tx.try_send(report);
            }
        }
    });
    let started = Instant::now();
    let mut last_activity = started;
    let mut packets = 0;
    let mut accepted = false;
    let mut stop_at = None;
    let mut forced = false;
    let mut finished_report = false;
    let mut stop_confirmed = false;
    let result = loop {
        while let Ok(report) = rx.try_recv() {
            accepted |= report.accepted;
            if report.activity != packets {
                packets = report.activity;
                last_activity = Instant::now();
            }
            finished_report |= report.done;
            stop_confirmed |= report.stop_confirmed;
            if stop_at.is_none() {
                *message.lock().unwrap() = report.message;
            }
        }
        if cancel.load(Relaxed) && stop_at.is_none() {
            stop_at = Some(Instant::now());
            *message.lock().unwrap() = "Stopping cast… Local playback continues.".into();
            if let Some(mut pipe) = input.take() {
                let _ = pipe.write_all(b"s");
            }
        }
        if stop_at.is_some_and(|at: Instant| at.elapsed() >= grace) {
            forced = true;
            let _ = child.kill();
        }
        if stop_at.is_none()
            && ((!accepted && started.elapsed() > CONNECT_LIMIT)
                || (accepted && last_activity.elapsed() > IDLE_LIMIT))
        {
            let _ = child.kill();
            break Err("Receiver stopped responding; Cast connection closed".into());
        }
        match child.try_wait() {
            Ok(Some(code)) => {
                if stop_at.is_some() || code.success() {
                    break Ok(());
                }
                break Err("Receiver connection ended unexpectedly".into());
            }
            Err(e) => break Err(e.to_string()),
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();
    // Drain the final report after joining: process exit can race the pipe reader.
    while let Ok(report) = rx.try_recv() {
        finished_report |= report.done;
        stop_confirmed |= report.stop_confirmed;
        if stop_at.is_none() {
            *message.lock().unwrap() = report.message;
        }
    }
    if stop_at.is_some() {
        *message.lock().unwrap() = if forced || !stop_confirmed {
            "Cast connection closed. Receiver did not confirm stop and may finish buffered audio. Local playback continues.".into()
        } else {
            "Casting stopped. Local playback continues.".into()
        };
    } else if result.is_ok() && !finished_report {
        return Err("Receiver worker exited without reporting its result".into());
    }
    result
}

pub(super) fn worker_main() -> bool {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--amp-cast-worker") {
        return false;
    }
    let Some(payload) = args.next() else {
        return true;
    };
    let Ok((device, url)) = serde_json::from_str::<(Device, String)>(&payload) else {
        return true;
    };
    let session = cast_in_process(device, url);
    let cancel = session.stop.clone();
    std::thread::spawn(move || {
        let mut byte = [0];
        let _ = std::io::stdin().read(&mut byte);
        // EOF also shuts down the worker if the parent exits unexpectedly.
        cancel.store(true, Relaxed);
    });
    let mut stop_at = None;
    loop {
        let report = Report {
            message: session.status.lock().unwrap().chars().take(512).collect(),
            done: session.done.load(Relaxed),
            accepted: session.accepted.load(Relaxed),
            stop_confirmed: session.stop_confirmed.load(Relaxed),
            activity: session.activity.load(Relaxed),
        };
        let mut out = std::io::stdout().lock();
        if serde_json::to_writer(&mut out, &report).is_err()
            || writeln!(out).is_err()
            || out.flush().is_err()
        {
            break;
        }
        if report.done {
            break;
        }
        if session.stop.load(Relaxed) {
            let at = stop_at.get_or_insert_with(Instant::now);
            // Parent supervises this too. Exit does not wait for detached I/O threads.
            if at.elapsed() > STOP_GRACE {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "subprocess fixture; invoked only by bounded cancellation test"]
    fn blocked_receiver_worker_fixture() {
        if std::env::var_os("AMP_CAST_BLOCKED_FIXTURE").is_some() {
            std::thread::sleep(Duration::from_secs(60));
        }
    }
    #[test]
    fn cancellation_reaps_unresponsive_worker_before_reconnect() {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "google_cast::worker::tests::blocked_receiver_worker_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("AMP_CAST_BLOCKED_FIXTURE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stop = AtomicBool::new(true);
        let message = Mutex::new(String::new());
        let start = Instant::now();
        supervise(child, &stop, &message, Duration::from_millis(100)).unwrap();
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(
            message
                .lock()
                .unwrap()
                .contains("Receiver did not confirm stop")
        );
    }
}
