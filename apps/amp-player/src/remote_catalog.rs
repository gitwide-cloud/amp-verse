//! Catalog membership and JSON are not animation work. Rebuild only after data
//! changes, keep one builder in flight, and publish from that background worker.
use amp_core::{Library, Playlist, show};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::mpsc};

pub struct Input {
    pub library: Option<Library>,
    pub settings: show::Settings,
    pub playlists: Vec<(PathBuf, Playlist)>,
}
pub struct Startup {
    pub key: Vec<u8>,
    pub value: Value,
    pub address: String,
    pub password: Option<String>,
}
pub fn build(
    library: Option<&Library>,
    settings: &show::Settings,
    playlists: &[(PathBuf, Playlist)],
) -> Value {
    let tracks = library.map_or(&[][..], |l| l.tracks.as_slice());
    json!({"version":1,"scenes":crate::music_scenes::resources()["scenes"],
        "songs":tracks.iter().map(|t|json!({"id":show::song_id(t),"title":t.title,"path":t.path,"resource_id":crate::resources::stable_song_id(t),"available":true,"analyzed":t.preparation.analysis.is_some(),"artist":t.preparation.extensions.get("artist"),"cues":t.preparation.manual_cues.iter().chain(&t.preparation.cues).map(|c|json!({"number":c.number,"id":c.id,"name":c.label,"seconds":c.seconds})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "playlists":settings.playlists,
        "regular_playlists":playlists.iter().map(|(path,p)|json!({"id":crate::performance::regular_playlist_id(path),"name":p.name})).collect::<Vec<_>>(),
        "resource_playlists":crate::resources::playlist_resources(tracks, settings, playlists)})
}
#[derive(Default)]
pub struct Cache {
    key: Option<Vec<u8>>,
    pending: Option<mpsc::Receiver<()>>,
}
impl Cache {
    pub fn ready(&mut self, key: Vec<u8>) {
        self.key = Some(key);
    }
    pub fn poll(&mut self) {
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(()) => self.pending = None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.key = None;
                }
                Err(mpsc::TryRecvError::Empty) => (),
            }
        }
    }
    pub fn needs_update(&self, key: &[u8]) -> bool {
        self.pending.is_none() && self.key.as_deref() != Some(key)
    }
    pub fn start(
        &mut self,
        key: Vec<u8>,
        input: Input,
        publish: impl FnOnce(Value) + Send + 'static,
        ctx: eframe::egui::Context,
    ) {
        self.start_job(
            key,
            move || {
                publish(build(
                    input.library.as_ref(),
                    &input.settings,
                    &input.playlists,
                ))
            },
            move || ctx.request_repaint(),
        );
    }
    fn start_job(
        &mut self,
        key: Vec<u8>,
        work: impl FnOnce() + Send + 'static,
        wake: impl FnOnce() + Send + 'static,
    ) {
        if !self.needs_update(&key) {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.key = Some(key);
        self.pending = Some(rx);
        std::thread::spawn(move || {
            work();
            let _ = tx.send(());
            wake();
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_data_does_not_rebuild_and_changed_data_waits_for_inflight_work() {
        let mut cache = Cache::default();
        cache.ready(vec![1]);
        assert!(!cache.needs_update(&[1]));
        let (started, active) = mpsc::channel();
        let (finish, gate) = mpsc::channel();
        let (done, completed) = mpsc::channel();
        cache.start_job(
            vec![2],
            move || {
                started.send(()).unwrap();
                gate.recv().unwrap();
            },
            move || {
                done.send(()).unwrap();
            },
        );
        active
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert!(!cache.needs_update(&[2]));
        assert!(
            !cache.needs_update(&[3]),
            "Never overlap full-library builders"
        );
        finish.send(()).unwrap();
        completed
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        cache.poll();
        assert!(!cache.needs_update(&[2]));
        assert!(
            cache.needs_update(&[3]),
            "A newer revision must still be published"
        );
    }
}
