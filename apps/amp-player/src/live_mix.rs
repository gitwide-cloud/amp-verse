//! Bounded master-output tap. No network or encoder work runs on the audio callback.
use std::{
    io::{Read, Write},
    process::{Child, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering::Relaxed},
    },
};
pub struct Broadcast {
    level: Arc<AtomicU32>,
    muted: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Child>>,
    pub status: Arc<Mutex<String>>,
}
impl Drop for Broadcast {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}
impl Broadcast {
    pub fn levels(&self) -> (f32, bool) {
        (
            f32::from_bits(self.level.load(Relaxed)),
            self.muted.load(Relaxed),
        )
    }
    pub fn start(
        audio: &crate::mixer::Engine,
        aux: &crate::mixer::Engine,
        address: String,
        password: String,
    ) -> Result<Self, String> {
        if audio.sample_rate != aux.sample_rate {
            return Err(
                "Sampler and DJ outputs need the same sample rate for live broadcast".into(),
            );
        }
        let mut main = audio.broadcast_tap()?;
        let mut samples = aux.broadcast_tap()?;
        let mut child = amp_core::command("ffmpeg")
            .args(["-nostdin", "-v", "error", "-f", "f32le", "-ar"])
            .arg(audio.sample_rate.to_string())
            .args([
                "-ac",
                "2",
                "-i",
                "pipe:0",
                "-c:a",
                "libmp3lame",
                "-b:a",
                "192k",
                "-reservoir",
                "0",
                "-f",
                "mp3",
                "-flush_packets",
                "1",
                "pipe:1",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut input = child.stdin.take().ok_or("Encoder input unavailable")?;
        let mut output = child.stdout.take().ok_or("Encoder output unavailable")?;
        let level = Arc::new(AtomicU32::new(1f32.to_bits()));
        let writer_level = level.clone();
        let muted = Arc::new(AtomicBool::new(false));
        let writer_muted = muted.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let child = Arc::new(Mutex::new(child));
        let process = child.clone();
        let status = Arc::new(Mutex::new("Connecting live DJ output…".into()));
        let message = status.clone();
        std::thread::spawn(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(5)))
                .proxy(None)
                .max_redirects(0)
                .build()
                .into();
            let result = (|| -> Result<(), String> {
                let session: serde_json::Value = agent
                    .post(format!("http://{address}/api/live/start"))
                    .header("x-amp-password", &password)
                    .send_empty()
                    .map_err(|e| e.to_string())?
                    .body_mut()
                    .read_json()
                    .map_err(|e| e.to_string())?;
                let token = session["session"]
                    .as_str()
                    .ok_or("Server does not support live DJ broadcast")?
                    .to_owned();
                let writer_stop = stopped.clone();
                let writer = std::thread::spawn(move || {
                    let mut bytes = Vec::with_capacity(8192);
                    while !writer_stop.load(Relaxed) {
                        bytes.clear();
                        for _ in 0..1024 {
                            let Ok(frame) = main.pop() else { break };
                            let aux = samples.pop().unwrap_or([0.; 2]);
                            for ch in 0..2 {
                                bytes.extend_from_slice(
                                    &((frame[ch] + aux[ch])
                                        * if writer_muted.load(Relaxed) {
                                            0.
                                        } else {
                                            f32::from_bits(writer_level.load(Relaxed))
                                        })
                                    .clamp(-0.98, 0.98)
                                    .to_le_bytes(),
                                );
                            }
                        }
                        if bytes.is_empty() {
                            std::thread::sleep(std::time::Duration::from_millis(2));
                        } else if input.write_all(&bytes).is_err() {
                            break;
                        }
                    }
                });
                *message.lock().unwrap() =
                    "Broadcasting live DJ mix · decks, mixer, stems and sampler".into();
                let outcome = (|| -> Result<(), String> {
                    let mut bytes = [0; 8192];
                    while !stopped.load(Relaxed) {
                        let n = output.read(&mut bytes).map_err(|e| e.to_string())?;
                        if n == 0 {
                            break;
                        }
                        agent
                            .post(format!("http://{address}/api/live/audio"))
                            .header("x-amp-password", &password)
                            .header("x-amp-live-session", &token)
                            .send(&bytes[..n])
                            .map_err(|e| e.to_string())?;
                    }
                    Ok(())
                })();
                stopped.store(true, Relaxed);
                let _ = process.lock().unwrap().kill();
                let _ = writer.join();
                let _ = agent
                    .post(format!("http://{address}/api/live/stop"))
                    .header("x-amp-password", &password)
                    .header("x-amp-live-session", &token)
                    .send_empty();
                outcome
            })();
            stopped.store(true, Relaxed);
            let mut child = process.lock().unwrap();
            let _ = child.kill();
            let _ = child.wait();
            if let Err(error) = result {
                *message.lock().unwrap() = format!("Live broadcast stopped: {error}");
            }
        });
        Ok(Self {
            level,
            muted,
            stop,
            child,
            status,
        })
    }
    pub fn set_level(&self, level: f32, muted: bool) -> Result<(), String> {
        if !level.is_finite() || !(0. ..=1.).contains(&level) {
            return Err("Broadcast level must be 0–1".into());
        }
        self.level.store(level.to_bits(), Relaxed);
        self.muted.store(muted, Relaxed);
        Ok(())
    }
    pub fn running(&self) -> bool {
        !self.stop.load(Relaxed)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires native audio and FFmpeg; output muted"]
    fn master_tap_encodes_and_streams_through_real_cast_server() {
        let root = tempfile::tempdir().unwrap();
        let library = amp_core::Library {
            tracks: vec![],
            tree: amp_core::Folder {
                name: String::new(),
                folders: vec![],
                tracks: vec![],
            },
        };
        let server = amp_cast::Cast::new(root.path().into(), library, None);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let app = amp_cast::router(server.clone());
        runtime.spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let main = crate::mixer::Engine::new().unwrap();
        main.set_master(0.);
        let aux = crate::mixer::Engine::new().unwrap();
        aux.set_master(0.);
        let broadcast = Broadcast::start(&main, &aux, address.clone(), String::new()).unwrap();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(5)))
            .proxy(None)
            .build()
            .into();
        let start = std::time::Instant::now();
        loop {
            let s: serde_json::Value = agent
                .get(format!("http://{address}/api/status"))
                .call()
                .unwrap()
                .body_mut()
                .read_json()
                .unwrap();
            if s["running"] == true {
                break;
            }
            assert!(
                start.elapsed().as_secs() < 6,
                "{}",
                broadcast.status.lock().unwrap()
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let mut response = agent
            .get(format!("http://{address}/listen.mp3"))
            .call()
            .unwrap();
        let mut encoded = [0; 4096];
        response
            .body_mut()
            .as_reader()
            .read_exact(&mut encoded)
            .unwrap();
        let file = root.path().join("live.mp3");
        std::fs::write(&file, encoded).unwrap();
        let result = amp_core::command("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(file)
            .args(["-t", "0.05", "-f", "f32le", "pipe:1"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!result.stdout.is_empty());
        drop(broadcast);
        main.stop_broadcast_tap();
        aux.stop_broadcast_tap();
        runtime.block_on(server.shutdown());
    }
}
