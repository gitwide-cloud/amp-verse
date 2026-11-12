#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#[allow(dead_code)] // The GUI uses the manifest/download half of this shared module.
mod update_package;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Serialize, Deserialize)]
struct Job {
    parent: u32,
    destination: PathBuf,
    stage: PathBuf,
    files: Vec<PathBuf>,
    result: PathBuf,
}
fn wait(pid: u32) -> Result<(), String> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Threading::{
            OpenProcess, PROCESS_ACCESS_RIGHTS, WaitForSingleObject,
        };
        match OpenProcess(PROCESS_ACCESS_RIGHTS(0x00100000), false, pid) {
            Ok(handle) => {
                let result = WaitForSingleObject(handle, 120000).0;
                let _ = windows::Win32::Foundation::CloseHandle(handle);
                if result != 0 {
                    return Err("Player did not close; no files were changed".into());
                }
            }
            Err(error) if error.code().0 as u32 == 0x80070057 => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    #[cfg(not(windows))]
    {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while std::path::Path::new(&format!("/proc/{pid}")).exists() {
            if std::time::Instant::now() > end {
                return Err("Player did not close; no files were changed".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
    Ok(())
}
fn run() -> Result<(), String> {
    let path = std::env::args_os().nth(1).ok_or("Missing update job")?;
    let job: Job = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    wait(job.parent)?;
    let outcome = update_package::install(&job.stage, &job.destination, &job.files);
    let message = match &outcome {
        Ok(()) => "Update installed".to_string(),
        Err(e) => format!("Update failed: {e}"),
    };
    let _ = std::fs::write(&job.result, message);
    let exe = job.destination.join(if cfg!(windows) {
        "amp-player.exe"
    } else {
        "amp-player"
    });
    std::process::Command::new(exe)
        .current_dir(&job.destination)
        .spawn()
        .map_err(|e| e.to_string())?;
    outcome
}
fn main() {
    if let Err(error) = run() {
        let fallback = std::env::temp_dir().join("amp-update-error.txt");
        let _ = std::fs::write(fallback, error);
        std::process::exit(1);
    }
}
