use super::*;
impl Player {
    pub(super) fn playlist_processing_state(&self) -> (bool, bool) {
        self.audio.as_ref().map_or((false, false), |audio| {
            let cached = audio
                .controls
                .prepared_stems
                .iter()
                .enumerate()
                .any(|(d, v)| audio.loaded(d) && v.load(std::sync::atomic::Ordering::Relaxed));
            let live = audio
                .controls
                .on_demand_stems
                .iter()
                .enumerate()
                .any(|(d, v)| {
                    audio.loaded(d)
                        && v.load(std::sync::atomic::Ordering::Relaxed)
                        && amp_core::stem_mix::changed(std::array::from_fn(|i| {
                            f32::from_bits(
                                audio.controls.stem_knobs[d][i]
                                    .load(std::sync::atomic::Ordering::Relaxed),
                            )
                        }))
                        && !audio.controls.prepared_stems[d]
                            .load(std::sync::atomic::Ordering::Relaxed)
                });
            (live, cached)
        })
    }
    pub(super) fn offer_playlist_stems(&mut self, name: String, paths: Vec<PathBuf>) {
        self.stem_panel.clear_playlist_offer();
        let mut unique = std::collections::HashSet::new();
        let paths: Vec<_> = paths
            .into_iter()
            .filter(|p| unique.insert(p.clone()))
            .collect();
        if paths.len() > 1000 {
            self.status="This playlist exceeds the 1,000-song preparation limit. Choose a smaller playlist; no preparation has started.".into();
            return;
        }

        let mut sources = Vec::new();
        let mut seconds = 0f64;
        let mut unknown = 0;
        let mut seen = std::collections::HashSet::new();
        for path in paths {
            if !seen.insert(path.clone()) {
                continue;
            }
            let source = match amp_core::resolve(&self.root, &path) {
                Ok(path) => path,
                Err(_) => {
                    self.status="This playlist contains unavailable songs. Reconnect its music source before preparing stems.".into();
                    return;
                }
            };
            let duration = self
                .library
                .as_ref()
                .and_then(|lib| lib.tracks.iter().find(|t| t.path == path))
                .and_then(|t| {
                    t.preparation
                        .analysis
                        .as_ref()
                        .map(|a| a.duration)
                        .or_else(|| {
                            t.preparation
                                .extensions
                                .get("duration_seconds")
                                .and_then(|v| v.as_f64())
                        })
                })
                .filter(|s| s.is_finite() && *s > 0.);
            seconds += duration.unwrap_or_else(|| {
                unknown += 1;
                240.
            });
            sources.push(source);
        }
        self.stem_panel
            .offer_playlist(name, sources, (seconds * 96000.).ceil() as u64, unknown);
    }
    pub(super) fn offer_smart_playlist_stems(&mut self, id: &str) {
        self.stem_panel.clear_playlist_offer();
        if self.show_editor.playlist_unsaved(id) {
            self.stem_panel.clear_playlist_offer();
            self.status =
                "Save or reload this Smart Playlist's changes before preparing its selected songs."
                    .into();
            return;
        }
        let mut settings = self.run.settings.clone();
        settings.resolve_sources(&self.root);
        let Some(playlist) = settings.playlists.iter().find(|p| p.id == id) else {
            return;
        };
        let Some(library) = &self.library else {
            self.status = "Open this playlist's music library first.".into();
            return;
        };
        let paths = library
            .tracks
            .iter()
            .filter(|t| amp_core::show::eligible(playlist, t))
            .map(|t| t.path.clone())
            .collect();
        self.offer_playlist_stems(playlist.name.clone(), paths);
    }
}
