//! Direct RSS/Atom subscriptions and publisher downloads; no user-hosted service.
use crate::media_lookup::{USER_AGENT, bytes, http_url};
use eframe::egui;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub title: String,
    pub url: String,
    pub date: String,
    pub mime: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Feed {
    pub url: String,
    pub title: String,
    pub episodes: Vec<Episode>,
}
#[derive(Clone, Deserialize)]
struct Directory {
    url: String,
    title: String,
    #[serde(default)]
    author: String,
}
fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn child_text(node: roxmltree::Node<'_, '_>, name: &str) -> String {
    node.children()
        .find(|n| n.has_tag_name(name))
        .and_then(|n| n.text())
        .unwrap_or("")
        .trim()
        .to_owned()
}
pub fn parse_feed(source: &str, xml: &str) -> Result<Feed, String> {
    let base = http_url(source)?;
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("Invalid RSS/Atom feed: {e}"))?;
    let channel = doc
        .descendants()
        .find(|n| n.has_tag_name("channel") || n.has_tag_name("feed"))
        .ok_or("This page is not a podcast RSS/Atom feed. Paste the show's RSS link.")?;
    let mut episodes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entry in channel
        .children()
        .filter(|n| n.has_tag_name("item") || n.has_tag_name("entry"))
        .take(5000)
    {
        let Some(link) = entry.children().find(|n| {
            n.has_tag_name("enclosure")
                || (n.has_tag_name("link") && n.attribute("rel") == Some("enclosure"))
        }) else {
            continue;
        };
        let mime = link.attribute("type").unwrap_or("").to_lowercase();
        let Some(raw) = link.attribute("url").or_else(|| link.attribute("href")) else {
            continue;
        };
        let Ok(url) = base.join(raw) else {
            continue;
        };
        if http_url(url.as_str()).is_err() {
            continue;
        }
        let supported = amp_core::supported(Path::new(url.path()));
        if mime.starts_with("video/") || (!mime.starts_with("audio/") && !supported) {
            continue;
        }
        let guid = [child_text(entry, "guid"), child_text(entry, "id")]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or_else(|| url.to_string());
        let id = hash(&format!("{source}:{guid}"));
        if !seen.insert(id.clone()) {
            continue;
        }
        let date = [
            child_text(entry, "pubDate"),
            child_text(entry, "published"),
            child_text(entry, "updated"),
        ]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or_default();
        episodes.push(Episode {
            id,
            title: child_text(entry, "title"),
            url: url.to_string(),
            date,
            mime,
        });
    }
    Ok(Feed {
        url: source.into(),
        title: child_text(channel, "title"),
        episodes,
    })
}
fn feed(url: &str) -> Result<Feed, String> {
    let data = bytes(url, 8 * 1024 * 1024)?;
    let xml = std::str::from_utf8(&data).map_err(|_| "Feed is not UTF-8 XML")?;
    parse_feed(url, xml)
}
fn extension(e: &Episode) -> &'static str {
    let path = http_url(&e.url)
        .ok()
        .map(|u| u.path().to_ascii_lowercase())
        .unwrap_or_default();
    if path.ends_with(".wav") || e.mime.contains("wav") {
        "wav"
    } else if path.ends_with(".aac") || e.mime.contains("aac") {
        "aac"
    } else if path.ends_with(".m4a") || path.ends_with(".m4b") || e.mime.contains("mp4") {
        "m4a"
    } else if path.ends_with(".ogg") || e.mime.contains("ogg") {
        "ogg"
    } else if path.ends_with(".opus") || e.mime.contains("opus") {
        "opus"
    } else if path.ends_with(".flac") || e.mime.contains("flac") {
        "flac"
    } else {
        "mp3"
    }
}
fn safe_title(s: &str) -> String {
    let value: String = s
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(90)
        .collect();
    let value = value.trim().trim_matches('.');
    if value.is_empty() {
        "Episode".into()
    } else {
        value.into()
    }
}
fn target(dir: &Path, e: &Episode) -> PathBuf {
    dir.join(format!(
        "{} - {}.{}",
        safe_title(&e.title),
        &e.id[..16],
        extension(e)
    ))
}
fn prepare_episode(directory: &Path, dest: &Path, episode: &Episode) -> Result<(), String> {
    let (mut preparation, revision) = amp_core::preparation::load_with_revision(dest)?;
    preparation
        .extensions
        .entry("title".into())
        .or_insert_with(|| serde_json::json!(episode.title));
    preparation
        .extensions
        .insert("media_kind".into(), serde_json::json!("podcast"));
    amp_core::preparation::save_if_unchanged(
        directory,
        Path::new(dest.file_name().ok_or("Missing episode filename")?),
        &preparation,
        revision.as_deref(),
    )?;
    if !amp_core::catalog_db::path(directory).exists() {
        amp_core::catalog_db::compact(directory)?;
    }
    Ok(())
}
fn download_episode(
    directory: &Path,
    episode: &Episode,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<PathBuf, String> {
    if episode.id.len() != 64 || !episode.id.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid cached episode. Refresh its publisher feed and retry.".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Download cancelled".into());
    }
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let dest = target(directory, episode);
    if dest.is_file() {
        prepare_episode(directory, &dest, episode)?;
        return Ok(dest);
    }
    http_url(&episode.url)?;
    let download_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_per_read(Some(Duration::from_secs(5)))
        .timeout_recv_response(Some(Duration::from_secs(20)))
        .max_redirects(8)
        .build()
        .into();
    let mut response = download_agent
        .get(&episode.url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| e.to_string())?;
    let content = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if content.starts_with("text/") || content.starts_with("video/") {
        return Err("Publisher returned a web page or video instead of audio".into());
    }
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    if total.is_some_and(|n| n > 2 * 1024 * 1024 * 1024) {
        return Err("Episode is larger than the 2 GB download limit".into());
    }
    let mut temp = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    let mut count = 0;
    let mut buf = [0u8; 65536];
    let mut last = std::time::Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Download cancelled; incomplete file removed".into());
        }
        let n = response
            .body_mut()
            .as_reader()
            .read(&mut buf)
            .map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > 2 * 1024 * 1024 * 1024 {
            return Err("Episode exceeds the 2 GB download limit".into());
        }
        temp.write_all(&buf[..n])
            .map_err(|e| format!("Cannot save episode: {e}"))?;
        if count == n as u64 || last.elapsed() > Duration::from_millis(200) {
            progress(count, total);
            last = std::time::Instant::now();
        }
    }
    if count == 0 || total.is_some_and(|n| n != count) {
        return Err("Episode download was empty or incomplete; retry".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Download cancelled".into());
    }
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist_noclobber(&dest).map_err(|e| e.to_string())?;
    prepare_episode(directory, &dest, episode)?;
    Ok(dest)
}

enum Event {
    Search(Result<Vec<Directory>, String>),
    Feed(Result<Feed, String>),
    Progress(u64, Option<u64>),
    Downloaded(Result<(PathBuf, bool), String>),
}
#[derive(Default)]
pub struct Podcasts {
    pub cache: crate::podcast_cache::Cache,
    active_episode: Option<Episode>,
    query: String,
    url: String,
    results: Vec<Directory>,
    feeds: Vec<Feed>,
    selected: usize,
    loaded: bool,
    job: Option<mpsc::Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    status: String,
    pub downloaded: Option<(PathBuf, bool)>,
}
impl Drop for Podcasts {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn settings_file() -> Result<PathBuf, String> {
    crate::theme::settings_path()
        .map(|p| p.with_file_name("media.ampdb"))
        .ok_or("Settings folder unavailable".into())
}
pub fn downloads() -> Result<PathBuf, String> {
    Ok(settings_file()?
        .parent()
        .ok_or("Settings folder unavailable")?
        .join("Podcasts"))
}
impl Podcasts {
    fn save(&mut self) {
        let result = settings_file().and_then(|f| {
            amp_core::catalog_db::save_local_record(&f, "podcasts", &serde_json::json!(self.feeds))
        });
        if let Err(e) = result {
            self.status = format!("Could not save subscriptions: {e}");
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if !self.loaded {
            self.loaded = true;
            match settings_file().and_then(|f| amp_core::catalog_db::local_record(&f, "podcasts")) {
                Ok(Some(v)) => match serde_json::from_value(v) {
                    Ok(f) => self.feeds = f,
                    Err(e) => self.status = format!("Subscriptions could not be read: {e}"),
                },
                Err(e) => self.status = e,
                _ => {}
            }
        }
        let mut done = false;
        let mut save = false;
        if let Some(rx) = &self.job {
            while let Ok(event) = rx.try_recv() {
                match event {
                    Event::Search(result) => {
                        match result {
                            Ok(results) => {
                                self.status = format!("{} shows found", results.len());
                                self.results = results;
                            }
                            Err(e) => {
                                self.status = format!(
                                    "Directory search failed: {e}. You can still paste a publisher RSS link."
                                )
                            }
                        };
                        done = true;
                    }
                    Event::Feed(result) => {
                        match result {
                            Ok(feed) => {
                                self.status = format!("{} audio episodes", feed.episodes.len());
                                if let Some(i) = self.feeds.iter().position(|f| f.url == feed.url) {
                                    self.feeds[i] = feed;
                                    self.selected = i;
                                } else {
                                    self.feeds.push(feed);
                                    self.selected = self.feeds.len() - 1;
                                }
                                save = true;
                            }
                            Err(e) => self.status = e,
                        };
                        done = true;
                    }
                    Event::Progress(n, total) => {
                        self.status = total.map_or_else(
                            || format!("Downloading: {:.1} MB", n as f64 / 1048576.),
                            |t| {
                                format!(
                                    "Downloading: {:.1} / {:.1} MB",
                                    n as f64 / 1048576.,
                                    t as f64 / 1048576.
                                )
                            },
                        )
                    }
                    Event::Downloaded(result) => {
                        match result {
                            Ok(value) => {
                                if let Some(episode) = self.active_episode.take() {
                                    self.cache.record(&value.0, &episode.id, &episode.title);
                                }
                                self.status = "Episode saved for offline listening.".into();
                                self.downloaded = Some(value);
                            }
                            Err(e) => self.status = e,
                        };
                        done = true;
                    }
                }
            }
        }
        if done {
            self.job = None;
        }
        if save {
            self.save();
        }
        if self.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    fn fetch(&mut self, url: String) {
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.status = "Loading publisher feed…".into();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Feed(feed(&url)));
        });
    }
    fn download(&mut self, episode: Episode, play: bool) {
        self.active_episode = Some(episode.clone());
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.status = "Connecting to publisher…".into();
        std::thread::spawn(move || {
            let result = downloads()
                .and_then(|directory| {
                    download_episode(&directory, &episode, &cancel, |n, total| {
                        let _ = tx.send(Event::Progress(n, total));
                    })
                })
                .map(|path| (path, play));
            let _ = tx.send(Event::Downloaded(result));
        });
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.poll(ui.ctx());
        ui.heading("Podcasts");
        ui.weak("Subscribe directly to public RSS feeds. No account or server needed. Audio only.");
        self.cache.show(ui);
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("Search the gPodder podcast directory"),
            );
            if ui
                .add_enabled(self.job.is_none(), egui::Button::new("Find podcasts"))
                .clicked()
            {
                let query = self.query.clone();
                let (tx, rx) = mpsc::channel();
                self.job = Some(rx);
                self.status = "Searching gPodder…".into();
                std::thread::spawn(move || {
                    let result = (|| {
                        let mut u = url::Url::parse("https://gpodder.net/search.json").unwrap();
                        u.query_pairs_mut().append_pair("q", &query);
                        serde_json::from_slice(&bytes(u.as_str(), 4 * 1024 * 1024)?)
                            .map_err(|e| e.to_string())
                    })();
                    let _ = tx.send(Event::Search(result));
                });
            }
        });
        let mut subscribe = None;
        egui::ScrollArea::vertical()
            .id_salt("podcast-results")
            .max_height(130.)
            .show(ui, |ui| {
                for result in &self.results {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("{} — {}", result.title, result.author));
                        if ui
                            .add_enabled(self.job.is_none(), egui::Button::new("Subscribe"))
                            .clicked()
                        {
                            subscribe = Some(result.url.clone());
                        }
                    });
                }
            });
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.url).hint_text("Or paste a podcast RSS URL"),
            );
            if ui
                .add_enabled(self.job.is_none(), egui::Button::new("Add RSS feed"))
                .clicked()
            {
                subscribe = Some(self.url.clone());
            }
        });
        if let Some(url) = subscribe {
            self.fetch(url);
        }
        ui.separator();
        ui.strong("Your subscriptions");
        let mut remove = None;
        let mut refresh = None;
        ui.horizontal_wrapped(|ui| {
            for (i, feed) in self.feeds.iter().enumerate() {
                ui.selectable_value(&mut self.selected, i, &feed.title);
            }
        });
        if let Some(feed) = self.feeds.get(self.selected) {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(self.job.is_none(), egui::Button::new("Refresh episodes"))
                    .clicked()
                {
                    refresh = Some(feed.url.clone());
                }
                if ui.button("Unsubscribe (keep downloads)").clicked() {
                    remove = Some(self.selected);
                }
            });
        }
        if let Some(i) = remove {
            self.feeds.remove(i);
            self.selected = 0;
            self.save();
        }
        if let Some(url) = refresh {
            self.fetch(url);
        }
        let mut download = None;
        if let Some(feed) = self.feeds.get(self.selected) {
            egui::ScrollArea::vertical()
                .id_salt("podcast-episodes")
                .max_height((ui.available_height() - 90.).max(100.))
                .show_rows(ui, 54., feed.episodes.len(), |ui, range| {
                    for i in range {
                        let e = &feed.episodes[i];
                        ui.horizontal_wrapped(|ui| {
                            ui.add_sized(
                                [ui.available_width().max(250.) - 235., 24.],
                                egui::Label::new(&e.title).truncate(),
                            );
                            if ui
                                .add_enabled(self.job.is_none(), egui::Button::new("Download"))
                                .clicked()
                            {
                                download = Some((e.clone(), false));
                            }
                            if ui
                                .add_enabled(
                                    self.job.is_none(),
                                    egui::Button::new("Download & play"),
                                )
                                .clicked()
                            {
                                download = Some((e.clone(), true));
                            }
                        });
                        ui.weak(&e.date);
                    }
                });
        }
        if let Some((e, play)) = download {
            self.download(e, play);
        }
        if self.job.is_some() && self.active_episode.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                if ui.button("Cancel download").clicked() {
                    self.cancel.store(true, Ordering::Relaxed);
                }
            });
        }
        ui.label(&self.status);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn serve(body: Vec<u8>, declared: usize) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/episode.mp3", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0u8; 8192];
            let _ = stream.read(&mut request);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: {declared}\r\nConnection: close\r\n\r\n"
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
        });
        (url, worker)
    }
    #[test]
    fn local_download_cancellation_collision_and_incomplete_cleanup() {
        let root = tempfile::tempdir().unwrap();
        let body = b"local audio fixture".to_vec();
        let (url, worker) = serve(body.clone(), body.len());
        let episode = Episode {
            id: hash("first"),
            title: "Episode".into(),
            url,
            date: String::new(),
            mime: "audio/mpeg".into(),
        };
        let cancel = AtomicBool::new(false);
        let path = download_episode(root.path(), &episode, &cancel, |_, _| {}).unwrap();
        worker.join().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert!(amp_core::catalog_db::path(root.path()).is_file());
        assert_eq!(
            amp_core::preparation::load(&path).unwrap().extensions["media_kind"],
            "podcast"
        );
        assert_eq!(
            download_episode(root.path(), &episode, &cancel, |_, _| {}).unwrap(),
            path
        );
        assert_eq!(std::fs::read(&path).unwrap(), body);
        let (url, worker) = serve(vec![1u8; 1024 * 1024], 1024 * 1024);
        let cancelled = Episode {
            id: hash("cancelled"),
            url,
            ..episode.clone()
        };
        assert!(
            download_episode(root.path(), &cancelled, &cancel, |_, _| cancel
                .store(true, Ordering::Relaxed))
            .unwrap_err()
            .contains("cancelled")
        );
        worker.join().unwrap();
        assert!(!target(root.path(), &cancelled).exists());
        cancel.store(false, Ordering::Relaxed);
        let (url, worker) = serve(b"short".to_vec(), 1000);
        let broken = Episode {
            id: hash("broken"),
            url,
            ..episode
        };
        assert!(download_episode(root.path(), &broken, &cancel, |_, _| {}).is_err());
        worker.join().unwrap();
        assert!(!target(root.path(), &broken).exists());
        assert_eq!(
            std::fs::read_dir(root.path())
                .unwrap()
                .filter(|e| e.as_ref().unwrap().file_type().unwrap().is_file())
                .count(),
            1
        );
    }
    #[test]
    #[ignore = "public gPodder directory and NPR RSS publisher feed"]
    fn live_directory_and_publisher_feed() {
        let shows: Vec<Directory> = serde_json::from_slice(
            &bytes("https://gpodder.net/search.json?q=science", 4 * 1024 * 1024).unwrap(),
        )
        .unwrap();
        assert!(!shows.is_empty());
        let show = feed("https://feeds.npr.org/510298/podcast.xml").unwrap();
        assert!(!show.title.is_empty());
        assert!(!show.episodes.is_empty());
        assert!(show.episodes.iter().all(|e| !e.mime.starts_with("video/")));
    }
    #[test]
    fn rss_filters_video_deduplicates_and_resolves_audio_urls() {
        let xml = r#"<rss><channel><title>Show</title><item><title>A</title><guid>1</guid><enclosure url="one.mp3" type="audio/mpeg"/></item><item><guid>1</guid><enclosure url="two.mp3" type="audio/mpeg"/></item><item><enclosure url="movie.mp4" type="video/mp4"/></item><item><enclosure url="file:///tmp/audio.mp3" type="audio/mpeg"/></item></channel></rss>"#;
        let f = parse_feed("https://example.com/feed.xml", xml).unwrap();
        assert_eq!(f.episodes.len(), 1);
        assert_eq!(f.episodes[0].url, "https://example.com/one.mp3");
        assert!(!safe_title("../bad: /title").contains('/'));
        assert!(parse_feed("https://example.com", "<html/>").is_err());
    }
    #[test]
    fn atom_and_stable_offline_filename() {
        let f=parse_feed("https://example.com/rss",r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Show</title><entry><id>x</id><title>Episode</title><link rel="enclosure" href="https://example.com/audio" type="audio/mp4"/></entry></feed>"#).unwrap();
        assert_eq!(f.episodes.len(), 1);
        assert_eq!(extension(&f.episodes[0]), "m4a");
        assert_eq!(
            target(Path::new("downloads"), &f.episodes[0]),
            target(Path::new("downloads"), &f.episodes[0])
        );
    }
}
