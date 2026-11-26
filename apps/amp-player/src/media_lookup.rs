//! Public metadata suggestions. Nothing replaces existing tags or protected DJ preparation.
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, mpsc},
    time::{Duration, Instant},
};
pub const USER_AGENT: &str = "AMPVerse/0.1 (https://github.com/gitwide-cloud/amp-verse)";
pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(25)))
        .max_redirects(8)
        .build()
        .into()
}
pub fn http_url(value: &str) -> Result<url::Url, String> {
    let u =
        url::Url::parse(value.trim()).map_err(|_| "Enter a complete http:// or https:// URL")?;
    if !matches!(u.scheme(), "http" | "https")
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
    {
        return Err("Only HTTP/HTTPS links without embedded passwords are supported".into());
    }
    Ok(u)
}
pub fn bytes(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    http_url(url)?;
    let mut response = agent()
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("Could not fetch online data: {e}"))?;
    let mut data = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(limit + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() as u64 > limit {
        return Err("Online response is too large".into());
    }
    Ok(data)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Match {
    pub recording: String,
    pub release: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub date: String,
    pub score: u64,
    pub length: Option<f64>,
}
fn text(v: &serde_json::Value, k: &str) -> String {
    v[k].as_str().unwrap_or("").into()
}
pub fn parse_matches(v: serde_json::Value) -> Vec<Match> {
    let mut results = Vec::new();
    for r in v["recordings"].as_array().into_iter().flatten().take(20) {
        let artist = r["artist-credit"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|a| {
                        format!(
                            "{}{}",
                            a["name"]
                                .as_str()
                                .or(a["artist"]["name"].as_str())
                                .unwrap_or(""),
                            a["joinphrase"].as_str().unwrap_or("")
                        )
                    })
                    .collect::<String>()
            })
            .unwrap_or_default();
        for release in r["releases"].as_array().into_iter().flatten().take(3) {
            let id = text(release, "id");
            if id.len() != 36 || !id.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-') {
                continue;
            }
            results.push(Match {
                recording: text(r, "id"),
                release: id,
                title: text(r, "title"),
                artist: artist.clone(),
                album: text(release, "title"),
                date: text(release, "date"),
                score: r["score"].as_u64().unwrap_or(0),
                length: r["length"].as_f64().map(|n| n / 1000.),
            });
        }
    }
    results
}
fn musicbrainz(url: &str) -> Result<Vec<u8>, String> {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let mut last = LAST
        .lock()
        .map_err(|_| "Metadata search lock unavailable")?;
    if let Some(previous) = *last {
        std::thread::sleep(Duration::from_millis(1100).saturating_sub(previous.elapsed()));
    }
    *last = Some(Instant::now());
    bytes(url, 2 * 1024 * 1024)
}
fn recording_genres(id: &str) -> Result<Vec<String>, String> {
    if id.len() != 36 || !id.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-') {
        return Err("Invalid MusicBrainz recording ID".into());
    }
    let data = musicbrainz(&format!(
        "https://musicbrainz.org/ws/2/recording/{id}?inc=genres&fmt=json"
    ))?;
    let value: serde_json::Value = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    let mut genres: Vec<_> = value["genres"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| {
            Some((
                v["count"].as_u64().unwrap_or(0),
                v["name"].as_str()?.trim().to_owned(),
            ))
        })
        .filter(|(_, s)| !s.is_empty())
        .collect();
    genres.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    Ok(genres.into_iter().take(5).map(|(_, s)| s).collect())
}
pub fn search(query: &str) -> Result<Vec<Match>, String> {
    let mut url = url::Url::parse("https://musicbrainz.org/ws/2/recording/").unwrap();
    url.query_pairs_mut()
        .append_pair("query", query)
        .append_pair("fmt", "json")
        .append_pair("limit", "10");
    let result = musicbrainz(url.as_str())?;
    let value = serde_json::from_slice(&result)
        .map_err(|e| format!("Invalid MusicBrainz response: {e}"))?;
    Ok(parse_matches(value))
}
pub fn color_image(data: &[u8]) -> Result<egui::ColorImage, String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let rgba = reader
        .decode()
        .map_err(|e| format!("Unsupported artwork: {e}"))?
        .thumbnail(500, 500)
        .to_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}
fn missing(p: &amp_core::Preparation, key: &str, embedded: &str) -> bool {
    fn empty(v: Option<&serde_json::Value>) -> bool {
        match v {
            None | Some(serde_json::Value::Null) => true,
            Some(serde_json::Value::String(s)) => s.trim().is_empty(),
            Some(serde_json::Value::Array(a)) => a.iter().all(|v| empty(Some(v))),
            _ => false,
        }
    }
    empty(p.extensions.get(key))
        && empty(
            p.extensions
                .get("embedded_tags")
                .and_then(|v| v.get(embedded)),
        )
}
pub fn fill_missing(p: &mut amp_core::Preparation, m: &Match) {
    for (key, embedded, value) in [
        ("title", "TrackTitle", &m.title),
        ("artist", "TrackArtist", &m.artist),
        ("album", "AlbumTitle", &m.album),
        ("year", "RecordingDate", &m.date),
    ] {
        if !value.is_empty() && missing(p, key, embedded) {
            if key == "year" {
                if let Some(year) = value.get(..4).and_then(|s| s.parse::<u32>().ok()) {
                    p.extensions.insert(key.into(), serde_json::json!(year));
                }
            } else {
                p.extensions.insert(key.into(), serde_json::json!(value));
            }
        }
    }
    p.extensions.insert("musicbrainz_match".into(),serde_json::json!({"recording":m.recording,"release":m.release,"source":"MusicBrainz","score":m.score}));
}
type SearchResult = (PathBuf, Option<Vec<u8>>, Vec<Match>);
type BatchUpdate = (usize, Option<SearchResult>, Option<String>);
enum Update {
    Found(Result<SearchResult, String>),
    Saved(Result<(PathBuf, Box<amp_core::Preparation>), String>),
    Cover(Result<Option<Vec<u8>>, String>),
}
#[derive(Default)]
pub struct Lookup {
    batch_job: Option<mpsc::Receiver<BatchUpdate>>,
    batch_error: Option<String>,
    batch_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    suggestions: Vec<SearchResult>,
    batch_root: PathBuf,
    pub review_song: Option<PathBuf>,
    context_root: PathBuf,
    pub query: String,
    results: Vec<Match>,
    selected: usize,
    job: Option<mpsc::Receiver<Update>>,
    snapshot: Option<(PathBuf, Option<Vec<u8>>)>,
    pub status: String,
    cover_key: String,
    cover: Option<egui::TextureHandle>,
    cover_job: Option<mpsc::Receiver<Update>>,
}
impl Lookup {
    pub fn poll(
        &mut self,
        ctx: &egui::Context,
        root: &Path,
        library: Option<&mut amp_core::Library>,
    ) -> bool {
        let mut changed = false;
        if let Some(rx) = &self.job
            && let Ok(update) = rx.try_recv()
        {
            match update {
                Update::Found(Ok((path, revision, results))) => {
                    self.status = format!(
                        "{} release matches. Select the correct edition before saving.",
                        results.len()
                    );
                    self.results = results;
                    self.selected = 0;
                    self.snapshot = Some((path, revision));
                }
                Update::Found(Err(e)) | Update::Saved(Err(e)) => self.status = e,
                Update::Saved(Ok((path, p))) => {
                    if self.context_root == root
                        && let Some(l) = library
                        && let Some(t) = l.tracks.iter_mut().find(|t| t.path == path)
                    {
                        if let Some(title) = p.extensions.get("title").and_then(|v| v.as_str()) {
                            t.title = title.into();
                        }
                        t.preparation = *p;
                    }
                    self.status="Missing metadata saved to ampdj; existing tags, cues and original audio preserved.".into();
                    self.snapshot = None;
                    self.cover_key.clear();
                    changed = true;
                }
                _ => {}
            }
            self.job = None;
        }
        if let Some(rx) = &self.batch_job {
            let mut done = false;
            while let Ok((count, result, error)) = rx.try_recv() {
                if let Some(error) = error {
                    self.batch_error = Some(error);
                }
                if count == usize::MAX {
                    done = true;
                } else {
                    self.status = format!(
                        "Looked up {count} songs. {} matches ready to review.",
                        self.suggestions.len()
                    );
                }
                if let Some(suggestion) = result {
                    self.suggestions.push(suggestion);
                }
            }
            if done {
                self.batch_job = None;
                self.status = format!(
                    "Lookup finished. {} songs have suggestions to review. {}",
                    self.suggestions.len(),
                    self.batch_error.as_ref().map_or(String::new(), |e| format!(
                        "Some requests failed: {e}. Retry Find for a song."
                    ))
                );
            }
        }
        if self.job.is_some() || self.cover_job.is_some() || self.batch_job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        changed
    }
    pub fn batch_controls(
        &mut self,
        ui: &mut egui::Ui,
        root: &Path,
        library: Option<&amp_core::Library>,
    ) {
        if ui
            .add_enabled(
                self.batch_job.is_none() && self.job.is_none() && library.is_some(),
                egui::Button::new("Look up missing metadata across Library"),
            )
            .clicked()
        {
            self.suggestions.clear();
            self.batch_error = None;
            self.batch_root = root.to_owned();
            self.batch_cancel = Default::default();
            let cancel = self.batch_cancel.clone();
            let root = root.to_owned();
            let tracks = library.unwrap().tracks.clone();
            let (tx, rx) = mpsc::channel();
            self.batch_job = Some(rx);
            std::thread::spawn(move || {
                let mut failures = 0;
                for (index, t) in tracks.into_iter().enumerate() {
                    if t.preparation
                        .extensions
                        .get("media_kind")
                        .and_then(|v| v.as_str())
                        == Some("podcast")
                    {
                        continue;
                    }
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    if !missing(&t.preparation, "artist", "TrackArtist")
                        && !missing(&t.preparation, "album", "AlbumTitle")
                        && t.preparation.extensions.contains_key("artwork_id")
                    {
                        continue;
                    }
                    let result = (|| {
                        let audio = amp_core::resolve(&root, &t.path).map_err(|e| e.to_string())?;
                        let (_, revision) = amp_core::preparation::load_with_revision(&audio)?;
                        let query =
                            format!("{} {}", crate::track_columns::cell(&t, "artist"), t.title);
                        let results = search(&query)?;
                        Ok::<_, String>((t.path, revision, results))
                    })();
                    match result {
                        Ok((path, revision, matches)) => {
                            failures = 0;
                            let suggestion =
                                (!matches.is_empty()).then_some((path, revision, matches));
                            if tx.send((index + 1, suggestion, None)).is_err() {
                                return;
                            }
                        }
                        Err(error) => {
                            failures += 1;
                            let throttled = error.contains("429") || error.contains("503");
                            let _ = tx.send((index + 1, None, Some(error)));
                            if throttled || failures >= 3 {
                                break;
                            }
                            std::thread::sleep(Duration::from_secs(3));
                        }
                    }
                }
                let _ = tx.send((usize::MAX, None, None));
            });
        }
        if self.batch_job.is_some() && ui.button("Stop lookup (keep suggestions)").clicked() {
            self.batch_cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let mut selected = None;
        if self.batch_root == root {
            egui::ScrollArea::vertical()
                .id_salt("metadata-review-queue")
                .max_height(100.)
                .show_rows(ui, 24., self.suggestions.len(), |ui, range| {
                    for i in range {
                        if ui
                            .button(format!("Review: {}", self.suggestions[i].0.display()))
                            .clicked()
                        {
                            selected = Some(i);
                        }
                    }
                });
        }
        if let Some(i) = selected {
            let (path, revision, matches) = self.suggestions[i].clone();
            self.context_root = root.to_owned();
            self.snapshot = Some((path.clone(), revision));
            self.results = matches;
            self.selected = 0;
            self.review_song = Some(path);
        }
    }
    pub fn cover(&mut self, ui: &mut egui::Ui, root: &Path, track: &amp_core::Track) {
        let id = track
            .preparation
            .extensions
            .get("artwork_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let key = format!("{}:{}:{id}", root.display(), track.path.display());
        if key != self.cover_key {
            self.cover_key = key;
            self.cover = None;
            let root = root.to_owned();
            let path = track.path.clone();
            let id = id.to_owned();
            let (tx, rx) = mpsc::channel();
            self.cover_job = Some(rx);
            std::thread::spawn(move || {
                let result = (|| {
                    let (owner, relative) =
                        amp_core::music_sources::owner(&root, &path).map_err(|e| e.to_string())?;
                    if !id.is_empty() {
                        amp_core::artwork::read(&owner, &id)
                    } else {
                        amp_core::genres::artwork(&owner.join(relative))
                    }
                })();
                let _ = tx.send(Update::Cover(result));
            });
        }
        if let Some(rx) = &self.cover_job
            && let Ok(Update::Cover(result)) = rx.try_recv()
        {
            if let Ok(Some(bytes)) = result
                && let Ok(image) = color_image(&bytes)
            {
                self.cover = Some(ui.ctx().load_texture(
                    &self.cover_key,
                    image,
                    Default::default(),
                ));
            }
            self.cover_job = None;
        }
        if let Some(texture) = &self.cover {
            ui.image((texture.id(), egui::vec2(100., 100.)));
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui, root: &Path, track: Option<&amp_core::Track>) {
        ui.heading("Tags & artwork");
        ui.weak("MusicBrainz metadata · Cover Art Archive album covers. Search text is sent only when you click Find.");
        let Some(track) = track else {
            ui.label("Select a song in Library first.");
            return;
        };
        ui.strong(&track.title);
        self.cover(ui, root, track);
        if self.query.is_empty() {
            self.query = format!(
                "{} {}",
                crate::track_columns::cell(track, "artist"),
                track.title
            );
        }
        ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("Artist and song title"));
        if ui
            .add_enabled(
                self.job.is_none(),
                egui::Button::new("Find missing tags / artwork"),
            )
            .clicked()
        {
            self.context_root = root.to_owned();
            let root = root.to_owned();
            let path = track.path.clone();
            let query = self.query.clone();
            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            self.results.clear();
            self.snapshot = None;
            self.status = "Searching MusicBrainz…".into();
            std::thread::spawn(move || {
                let result = (|| {
                    let audio = amp_core::resolve(&root, &path).map_err(|e| e.to_string())?;
                    let (_, revision) = amp_core::preparation::load_with_revision(&audio)?;
                    Ok((path, revision, search(&query)?))
                })();
                let _ = tx.send(Update::Found(result));
            });
        }
        egui::ScrollArea::vertical()
            .id_salt("metadata-matches")
            .max_height(220.)
            .show(ui, |ui| {
                for (i, m) in self.results.iter().enumerate() {
                    ui.selectable_value(
                        &mut self.selected,
                        i,
                        format!(
                            "{} — {} | {} ({}) · score {}{}",
                            m.artist,
                            m.title,
                            m.album,
                            m.date,
                            m.score,
                            m.length.map(|s| format!(" · {s:.0}s")).unwrap_or_default()
                        ),
                    );
                }
            });
        if let Some(m) = self.results.get(self.selected) {
            ui.hyperlink_to(
                "Check release / artwork source",
                format!("https://musicbrainz.org/release/{}", m.release),
            );
            ui.weak("Save fills empty fields only. Album editions and remixes can differ; search score is not identification certainty.");
            if ui
                .add_enabled(
                    self.job.is_none()
                        && self.context_root == root
                        && self
                            .snapshot
                            .as_ref()
                            .is_some_and(|(p, _)| *p == track.path),
                    egui::Button::new("Save missing tags and album cover"),
                )
                .clicked()
            {
                let (root, m) = (root.to_owned(), m.clone());
                let (path, revision) = self.snapshot.clone().unwrap();
                let (tx, rx) = mpsc::channel();
                self.job = Some(rx);
                self.status = "Saving metadata / fetching cover…".into();
                std::thread::spawn(move || {
                    let result = (|| {
                        let audio = amp_core::resolve(&root, &path).map_err(|e| e.to_string())?;
                        let mut p = amp_core::preparation::load(&audio)?;
                        fill_missing(&mut p, &m);
                        if amp_core::genres::values(&p).is_empty() {
                            match recording_genres(&m.recording) {
                                Ok(genres) if !genres.is_empty() => {
                                    p.extensions
                                        .insert("genre".into(), serde_json::json!(genres));
                                }
                                Err(e) => {
                                    p.extensions
                                        .insert("genre_lookup_status".into(), serde_json::json!(e));
                                }
                                _ => {}
                            }
                        }
                        let mut artwork_note = None;
                        if !p.extensions.contains_key("artwork_id")
                            && amp_core::genres::artwork(&audio)?.is_none()
                        {
                            match bytes(
                                &format!(
                                    "https://coverartarchive.org/release/{}/front-500",
                                    m.release
                                ),
                                8 * 1024 * 1024,
                            )
                            .and_then(|data| {
                                color_image(&data)?;
                                let (owner, _) = amp_core::music_sources::owner(&root, &path)
                                    .map_err(|e| e.to_string())?;
                                amp_core::artwork::save(&owner, &data)
                            }) {
                                Ok(id) => {
                                    p.extensions
                                        .insert("artwork_id".into(), serde_json::json!(id));
                                    p.extensions.insert(
                                        "artwork_source".into(),
                                        serde_json::json!(format!(
                                            "https://coverartarchive.org/release/{}",
                                            m.release
                                        )),
                                    );
                                }
                                Err(e) => artwork_note = Some(e),
                            }
                        }
                        if let Some(note) = artwork_note {
                            p.extensions
                                .insert("artwork_lookup_status".into(), serde_json::json!(note));
                        }
                        amp_core::preparation::save_if_unchanged(
                            &root,
                            &path,
                            &p,
                            revision.as_deref(),
                        )?;
                        Ok((path, Box::new(p)))
                    })();
                    let _ = tx.send(Update::Saved(result));
                });
            }
        }
        if self.job.is_some() {
            ui.spinner();
        }
        ui.label(&self.status);
        if let Some(message) = track
            .preparation
            .extensions
            .get("artwork_lookup_status")
            .and_then(|v| v.as_str())
        {
            ui.weak(format!(
                "Cover lookup: {message}. Metadata can still be saved; try another release."
            ));
        }
    }
}
impl Drop for Lookup {
    fn drop(&mut self) {
        self.batch_cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "public MusicBrainz and Cover Art Archive services"]
    fn live_metadata_and_cover_sources() {
        let matches = search("recording:\"Come Together\" AND artist:\"The Beatles\"").unwrap();
        assert!(!matches.is_empty());
        recording_genres(&matches[0].recording).unwrap();
        assert!(matches.iter().any(|m| !m.artist.is_empty()));
        let mut found = false;
        for m in matches.iter().take(5) {
            if let Ok(data) = bytes(
                &format!(
                    "https://coverartarchive.org/release/{}/front-500",
                    m.release
                ),
                8 * 1024 * 1024,
            ) {
                color_image(&data).unwrap();
                found = true;
                break;
            }
        }
        assert!(
            found,
            "No cover available for the first five public release results"
        );
    }
    #[test]
    fn fills_only_empty_fields_and_preserves_dj_edits() {
        let m = Match {
            recording: "r".into(),
            release: "a".into(),
            title: "Online".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            date: "2020".into(),
            score: 100,
            length: None,
        };
        let mut p = amp_core::Preparation {
            cue_in: 12.,
            bpm: Some(123.),
            manual_override: true,
            ..Default::default()
        };
        p.extensions
            .insert("title".into(), serde_json::json!("My edit"));
        p.extensions.insert(
            "embedded_tags".into(),
            serde_json::json!({"TrackArtist":["Existing artist"]}),
        );
        let before = p.clone();
        fill_missing(&mut p, &m);
        assert_eq!(p.extensions["title"], "My edit");
        assert!(!p.extensions.contains_key("artist"));
        assert_eq!(p.extensions["album"], "Album");
        assert_eq!(p.cue_in, before.cue_in);
        assert_eq!(p.bpm, before.bpm);
        assert!(p.manual_override);
        assert!(http_url("file:///tmp/song").is_err());
        assert!(http_url("https://user:password@example.com").is_err());
        assert!(color_image(b"not an image").is_err());
    }
}
