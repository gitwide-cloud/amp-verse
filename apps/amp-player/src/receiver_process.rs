//! Decoder ownership survives abrupt application exit, without a UI shutdown wait.
use std::{
    io,
    ops::{Deref, DerefMut},
    process::{Child, Command},
};

pub(crate) struct Decoder {
    child: Child,
    #[cfg(windows)]
    _job: windows_owner::Handle,
}
impl Deref for Decoder {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.child
    }
}
impl DerefMut for Decoder {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}
impl Drop for Decoder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(crate) fn spawn(command: &mut Command) -> io::Result<Decoder> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        let parent = std::process::id();
        // Only async-signal-safe Linux syscalls execute between fork and exec.
        // Checking getppid closes the race if the parent died before prctl.
        unsafe {
            command.pre_exec(move || {
                unsafe extern "C" {
                    fn prctl(option: std::ffi::c_int, ...) -> std::ffi::c_int;
                    fn getppid() -> std::ffi::c_int;
                }
                const PR_SET_PDEATHSIG: std::ffi::c_int = 1;
                const SIGKILL: std::ffi::c_ulong = 9;
                if prctl(
                    PR_SET_PDEATHSIG,
                    SIGKILL,
                    0 as std::ffi::c_ulong,
                    0 as std::ffi::c_ulong,
                    0 as std::ffi::c_ulong,
                ) != 0
                {
                    return Err(io::Error::last_os_error());
                }
                if getppid() as u32 != parent {
                    return Err(io::Error::from_raw_os_error(3));
                }
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::{io::AsRawHandle, process::CommandExt};
        use windows::Win32::{
            Foundation::HANDLE,
            System::{
                JobObjects::AssignProcessToJobObject,
                Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED},
            },
        };
        // Like process-wrap's std JobObject, assign while suspended, before the
        // decoder can execute. Unlike that wrapper (kill_on_drop=false), this
        // job explicitly kills on handle close, including process termination.
        let job = windows_owner::job()?;
        command.creation_flags(CREATE_NO_WINDOW.0 | CREATE_SUSPENDED.0);
        let decoder = Decoder {
            child: command.spawn()?,
            _job: job,
        };
        let handle = HANDLE(decoder.child.as_raw_handle());
        unsafe { AssignProcessToJobObject(decoder._job.0, handle) }.map_err(io::Error::other)?;
        windows_owner::resume(decoder.child.id())?;
        Ok(decoder)
    }
    #[cfg(not(windows))]
    {
        Ok(Decoder {
            child: command.spawn()?,
        })
    }
}

#[cfg(windows)]
mod windows_owner {
    use std::io;
    use windows::{
        Win32::{
            Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE},
            System::{
                Diagnostics::ToolHelp::{
                    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                    Thread32Next,
                },
                JobObjects::{
                    CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                    SetInformationJobObject,
                },
                Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
            },
        },
        core::HRESULT,
    };
    pub(super) struct Handle(pub HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
    pub(super) fn job() -> io::Result<Handle> {
        let handle = Handle(unsafe { CreateJobObjectW(None, None) }.map_err(io::Error::other)?);
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        unsafe {
            SetInformationJobObject(
                handle.0,
                JobObjectExtendedLimitInformation,
                &info as *const _ as _,
                std::mem::size_of_val(&info) as u32,
            )
        }
        .map_err(io::Error::other)?;
        Ok(handle)
    }
    pub(super) fn resume(pid: u32) -> io::Result<()> {
        let snapshot = Handle(
            unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }.map_err(io::Error::other)?,
        );
        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        unsafe { Thread32First(snapshot.0, &mut entry) }.map_err(io::Error::other)?;
        let mut resumed = false;
        loop {
            if entry.th32OwnerProcessID == pid {
                let thread = Handle(
                    unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID) }
                        .map_err(io::Error::other)?,
                );
                if unsafe { ResumeThread(thread.0) } == u32::MAX {
                    return Err(io::Error::last_os_error());
                }
                resumed = true;
            }
            match unsafe { Thread32Next(snapshot.0, &mut entry) } {
                Ok(()) => {}
                Err(error) if error.code() == HRESULT::from_win32(ERROR_NO_MORE_FILES.0) => break,
                Err(error) => return Err(io::Error::other(error)),
            }
        }
        if resumed {
            Ok(())
        } else {
            Err(io::Error::other("No decoder thread found to resume"))
        }
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    use super::*;
    use std::{
        thread,
        time::{Duration, Instant},
    };
    fn fixture_name(name: &str) -> String {
        format!("{}::{name}", module_path!().split_once("::").unwrap().1)
    }
    #[test]
    #[ignore = "Owned decoder subprocess fixture"]
    fn decoder_exit_fixture_child() {
        assert!(std::env::var_os("AMP_RECEIVER_EXIT_RECEIPT").is_some());
        thread::sleep(Duration::from_secs(30));
    }
    #[test]
    #[ignore = "Owned abrupt-exit fixture; requires its private PID receipt"]
    fn abrupt_parent_fixture() {
        let receipt =
            std::env::var_os("AMP_RECEIVER_EXIT_RECEIPT").expect("Owned fixture receipt required");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                &fixture_name("decoder_exit_fixture_child"),
                "--ignored",
                "--nocapture",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let child = spawn(&mut command).unwrap();
        std::fs::write(receipt, child.id().to_string()).unwrap();
        // Deliberately bypass all Rust destructors and async receiver cleanup.
        std::process::exit(0);
    }
    #[cfg(target_os = "linux")]
    fn alive(pid: u32) -> bool {
        std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
            stat.rsplit_once(") ")
                .is_some_and(|(_, tail)| !tail.starts_with('Z'))
        })
    }
    #[cfg(windows)]
    fn alive(pid: u32) -> bool {
        use windows::{
            Win32::{
                Foundation::{CloseHandle, ERROR_INVALID_PARAMETER},
                System::Threading::{
                    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
                },
            },
            core::HRESULT,
        };
        let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
            Ok(handle) => handle,
            Err(error) if error.code() == HRESULT::from_win32(ERROR_INVALID_PARAMETER.0) => {
                return false;
            }
            Err(error) => panic!("Cannot inspect owned decoder: {error}"),
        };
        let mut exit = 0;
        let result = unsafe { GetExitCodeProcess(handle, &mut exit) };
        let _ = unsafe { CloseHandle(handle) };
        result.unwrap();
        exit == 259
    }
    #[test]
    fn decoder_dies_when_parent_exits_without_running_rust_drops() {
        let temp = std::env::temp_dir().join(format!(
            "amp-receiver-parent-exit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&temp).unwrap();
        let receipt = temp.join("owned-decoder-pid");
        let mut parent = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &fixture_name("abrupt_parent_fixture"),
                "--ignored",
                "--nocapture",
            ])
            .env("AMP_RECEIVER_EXIT_RECEIPT", &receipt)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let start = Instant::now();
        loop {
            if let Some(exit) = parent.try_wait().unwrap() {
                assert!(exit.success());
                break;
            }
            if start.elapsed() > Duration::from_secs(4) {
                let _ = parent.kill();
                let _ = parent.wait();
                panic!("Owned parent fixture timed out");
            }
            thread::sleep(Duration::from_millis(10));
        }
        let pid = std::fs::read_to_string(receipt)
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let start = Instant::now();
        while alive(pid) {
            if start.elapsed() > Duration::from_secs(2) {
                // Never kill by a bare PID (it could have been reused). This
                // fixture alone exits after thirty seconds if cleanup failed.
                panic!("Decoder survived abrupt parent exit");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
