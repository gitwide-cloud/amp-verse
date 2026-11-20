//! One cancellable physical-output consumer; it never owns or restarts the station.
use super::*;
use std::{
    io::Write,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    thread::JoinHandle,
};
pub(super) struct Output {
    child: Child,
    cancel: Arc<AtomicBool>,
    pump: Option<JoinHandle<()>>,
    failure: Arc<std::sync::Mutex<Option<String>>>,
}
impl Output {
    pub(super) fn spawn(
        mut command: Command,
        mut receiver: broadcast::Receiver<Bytes>,
    ) -> std::io::Result<Self> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let Some(mut input) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::other("Output input unavailable"));
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let failure = Arc::new(std::sync::Mutex::new(None));
        let stopped = cancel.clone();
        let issue = failure.clone();
        let pump = std::thread::spawn(move || {
            while !stopped.load(Relaxed) {
                match receiver.try_recv() {
                    Ok(bytes) => {
                        if input.write_all(&bytes).is_err() {
                            if !stopped.load(Relaxed) {
                                *issue.lock().unwrap()=Some("Server speakers stopped accepting audio. Check the server audio device and restart its output.".into());
                            }
                            break;
                        }
                    }
                    Err(broadcast::error::TryRecvError::Empty) => {
                        std::thread::sleep(std::time::Duration::from_millis(10))
                    }
                    Err(broadcast::error::TryRecvError::Lagged(_)) => {
                        *issue.lock().unwrap() = Some(
                            "Server speakers fell behind the audio stream. Restart server output."
                                .into(),
                        );
                        break;
                    }
                    Err(broadcast::error::TryRecvError::Closed) => break,
                }
            }
        });
        Ok(Self {
            child,
            cancel,
            pump: Some(pump),
            failure,
        })
    }
    pub(super) fn running(&mut self) -> bool {
        let running = matches!(self.child.try_wait(), Ok(None))
            && self.pump.as_ref().is_some_and(|pump| !pump.is_finished());
        if !running && !self.cancel.load(Relaxed) && self.failure.lock().unwrap().is_none() {
            *self.failure.lock().unwrap() = Some(
                "Server speakers stopped. Check the server audio device, or choose This device."
                    .into(),
            );
        }
        running
    }
    pub(super) fn error(&self) -> Option<String> {
        self.failure.lock().unwrap().clone()
    }
    pub(super) fn stop(&mut self) {
        self.cancel.store(true, Relaxed);
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(pump) = self.pump.take() {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
            while !pump.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if pump.is_finished() {
                let _ = pump.join();
            }
        }
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        self.stop();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn idle_process() -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--ignored",
                "--exact",
                "server_output::tests::output_test_child",
                "--nocapture",
            ])
            .env("AMP_OUTPUT_TEST_CHILD", "1");
        command
    }
    #[test]
    #[ignore = "child fixture; invoked explicitly by bounded output lifecycle tests"]
    fn output_test_child() {
        if std::env::var("AMP_OUTPUT_TEST_CHILD").as_deref() == Ok("1") {
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
    }
    #[tokio::test]
    async fn failed_speaker_consumer_keeps_the_station_owner_running() {
        let root = tempfile::tempdir().unwrap();
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        {
            let mut station = cast.station.lock().await;
            station.running = true;
            station.generation = 42;
            station.source = "live_dj".into();
        }
        #[cfg(windows)]
        let mut command = Command::new("cmd");
        #[cfg(windows)]
        command.args(["/c", "exit", "1"]);
        #[cfg(not(windows))]
        let mut command = Command::new("sh");
        #[cfg(not(windows))]
        command.args(["-c", "exit 1"]);
        *cast.server_output.lock().unwrap() =
            Some(Output::spawn(command, cast.audio.subscribe()).unwrap());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while station_control::output_running(&cast) {
            assert!(std::time::Instant::now() < deadline);
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let station = cast.station.lock().await;
        assert!(station.running);
        assert_eq!(station.generation, 42);
        assert_eq!(station.source, "live_dj");
        assert!(station.error.is_none());
        assert!(cast.server_output_error.lock().unwrap().is_some());
    }
    #[test]
    fn idle_and_stalled_input_are_cancelled_and_children_reaped() {
        for stalled in [false, true] {
            let (tx, rx) = broadcast::channel(8);
            let mut output = Output::spawn(idle_process(), rx).unwrap();
            if stalled {
                tx.send(Bytes::from(vec![0; 2 * 1024 * 1024])).unwrap();
            }
            std::thread::sleep(std::time::Duration::from_millis(60));
            let start = std::time::Instant::now();
            output.stop();
            assert!(start.elapsed() < std::time::Duration::from_secs(2));
            assert!(output.child.try_wait().unwrap().is_some());
            assert!(output.pump.is_none());
        }
    }
}
