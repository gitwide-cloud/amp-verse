//! Transactional offline playlist copies; never sends edits back to the station.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
pub fn save(
    base: &str,
    password: &str,
    manifest: &Value,
    playlist: &Value,
    destination: &Path,
) -> Result<PathBuf, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .max_redirects(0)
        .proxy(None)
        .build()
        .into();
    let stage = tempfile::Builder::new()
        .prefix(".amp-download-")
        .tempdir_in(destination)
        .map_err(|e| e.to_string())?;
    let mut paths = Vec::new();
    let mut copied = std::collections::HashSet::new();
    let ids = playlist["tracks"]
        .as_array()
        .ok_or("Playlist unavailable")?;
    for id in ids {
        let id = id.as_u64().ok_or("Invalid track ID")?;
        let entry = manifest["tracks"]
            .as_array()
            .and_then(|a| a.iter().find(|t| t["id"].as_u64() == Some(id)))
            .ok_or("Track missing from station")?;
        let extension = entry["extension"]
            .as_str()
            .filter(|s| {
                !s.is_empty() && s.len() <= 8 && s.chars().all(|c| c.is_ascii_alphanumeric())
            })
            .unwrap_or("audio");
        let path = PathBuf::from(format!("track-{id}.{extension}"));
        paths.push(path.clone());
        if !copied.insert(id) {
            continue;
        }
        let metadata: Value = agent
            .get(format!("{base}/api/smart/tracks/{id}/metadata"))
            .header("x-amp-password", password)
            .call()
            .map_err(|e| e.to_string())?
            .body_mut()
            .read_json()
            .map_err(|e| e.to_string())?;
        if metadata["station_id"] != manifest["station_id"]
            || metadata["library_id"] != manifest["library_id"]
            || metadata["server_id"] != manifest["server_id"]
        {
            return Err("Station identity changed during download".into());
        }
        let mut preparation: amp_core::Preparation =
            serde_json::from_value(metadata["preparation"].clone()).map_err(|e| e.to_string())?;
        preparation.validate()?;
        let expected = metadata["bytes"].as_u64().ok_or("Missing audio length")?;
        if expected > 2 * 1024 * 1024 * 1024 {
            return Err("Track exceeds 2 GiB download limit".into());
        }
        let mut response = agent
            .get(format!("{base}/api/smart/tracks/{id}/audio"))
            .header("x-amp-password", password)
            .call()
            .map_err(|e| e.to_string())?;
        let mut file =
            std::fs::File::create(stage.path().join(&path)).map_err(|e| e.to_string())?;
        let mut reader = response.body_mut().as_reader();
        let mut hash = Sha256::new();
        let mut buffer = [0; 65536];
        let mut count = 0u64;
        loop {
            let n = std::io::Read::read(&mut reader, &mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > expected {
                return Err("Audio changed during download".into());
            }
            hash.update(&buffer[..n]);
            std::io::Write::write_all(&mut file, &buffer[..n]).map_err(|e| e.to_string())?;
        }
        let checksum = format!("{:x}", hash.finalize());
        if count != expected || metadata["sha256"].as_str() != Some(checksum.as_str()) {
            return Err("Audio integrity check failed; existing downloads were preserved".into());
        }
        file.sync_all().map_err(|e| e.to_string())?;
        preparation.source_sha256 = Some(checksum);
        let sidecar = amp_core::preparation::portable(&stage.path().join(&path));
        std::fs::create_dir_all(sidecar.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(
            sidecar,
            serde_json::to_vec_pretty(&preparation).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    let playlist = serde_json::json!({"version":1,"name":playlist["name"],"tracks":paths,"station_source":manifest,"station_playlist":playlist,"offline_changes":"local_only"});
    std::fs::write(
        stage.path().join("playlist.json"),
        serde_json::to_vec_pretty(&playlist).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let target = destination.join(format!("AMP-Station-{}", amp_core::coordination::token()));
    std::fs::rename(stage.path(), &target).map_err(|e| e.to_string())?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_station_download_keeps_audio_cues_and_existing_copies() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("song.wav"), b"unchanged audio").unwrap();
        let mut prep = amp_core::Preparation {
            manual_override: true,
            bpm: Some(128.),
            ..Default::default()
        };
        prep.manual_cues.push(amp_core::preparation::Cue {
            id: "drop".into(),
            label: "My drop".into(),
            number: Some(2),
            seconds: 1.25,
            ..Default::default()
        });
        amp_core::preparation::save(source.path(), Path::new("song.wav"), &prep).unwrap();
        amp_core::show::Settings::default()
            .save(source.path())
            .unwrap();
        let station = amp_cast::Cast::new(
            source.path().into(),
            amp_core::scan(source.path()).unwrap(),
            Some("test".into()),
        );
        station
            .sharing
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = std::sync::mpsc::channel();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let server = std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(listener.local_addr().unwrap()).unwrap();
                axum::serve(listener, amp_cast::router(station))
                    .with_graceful_shutdown(async {
                        let _ = stopped.await;
                    })
                    .await
                    .unwrap();
            })
        });
        let base = format!("http://{}", rx.recv().unwrap());
        let manifest: Value = ureq::get(format!("{base}/api/smart/manifest"))
            .header("x-amp-password", "test")
            .call()
            .unwrap()
            .body_mut()
            .read_json()
            .unwrap();
        let playlist = manifest["playlists"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| !p["tracks"].as_array().unwrap().is_empty())
            .unwrap();
        let saved = save(&base, "test", &manifest, playlist, target.path()).unwrap();
        let offline = amp_core::Playlist::load(&saved.join("playlist.json"), &saved).unwrap();
        assert_eq!(offline.tracks.len(), 1);
        assert_eq!(
            std::fs::read(saved.join(&offline.tracks[0])).unwrap(),
            b"unchanged audio"
        );
        assert_eq!(
            amp_core::preparation::load(&saved.join(&offline.tracks[0]))
                .unwrap()
                .manual_cues[0]
                .seconds,
            1.25
        );
        let mut changed = manifest.clone();
        changed["station_id"] = serde_json::json!("other");
        assert!(save(&base, "test", &changed, playlist, target.path()).is_err());
        assert_eq!(std::fs::read_dir(target.path()).unwrap().count(), 1);
        assert_eq!(
            amp_core::preparation::load(&source.path().join("song.wav"))
                .unwrap()
                .manual_cues[0]
                .id,
            "drop"
        );
        stop.send(()).unwrap();
        server.join().unwrap();
    }
}
