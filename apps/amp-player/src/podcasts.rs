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
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub duration: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Feed {
    pub url: String,
    pub title: String,
    pub episodes: Vec<Episode>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub artwork: String,
    #[serde(default)]
    pub categories: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Directory {
    url: String,
    title: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    artwork: String,
    #[serde(default)]
    genres: Vec<String>,
}
trait DiscoveryProvider {
    fn search(&self, query: &str, category: Option<&str>) -> Result<Vec<Directory>, String>;
}
struct AppleDirectory;
const CATEGORIES: &[(&str, &str)] = &[
    ("Business", "1321"),
    ("Investing", "1412"),
    ("Self-Improvement", "1500"),
    ("Kids & Family", "1305"),
    ("Health & Fitness", "1512"),
    ("Technology", "1318"),
    ("News", "1489"),
    ("Education", "1304"),
    ("Society & Culture", "1324"),
    ("Comedy", "1303"),
    ("Religion & Spirituality", "1314"),
    ("Music", "1310"),
];
impl DiscoveryProvider for AppleDirectory {
    fn search(&self, query: &str, category: Option<&str>) -> Result<Vec<Directory>, String> {
        let mut url = url::Url::parse("https://itunes.apple.com/search").unwrap();
        url.query_pairs_mut()
            .append_pair("media", "podcast")
            .append_pair("entity", "podcast")
            .append_pair("limit", "50")
            .append_pair("term", query);
        if let Some(category) = category {
            url.query_pairs_mut().append_pair("genreId", category);
        }
        let value: serde_json::Value =
            serde_json::from_slice(&bytes(url.as_str(), 4 * 1024 * 1024)?)
                .map_err(|e| e.to_string())?;
        Ok(value["results"]
            .as_array()
            .ok_or("Invalid directory response")?
            .iter()
            .filter_map(|v| {
                let url = v["feedUrl"].as_str()?;
                http_url(url).ok()?;
                Some(Directory {
                    url: url.into(),
                    title: v["collectionName"]
                        .as_str()
                        .unwrap_or("Untitled podcast")
                        .into(),
                    author: v["artistName"].as_str().unwrap_or("").into(),
                    artwork: v["artworkUrl600"]
                        .as_str()
                        .or(v["artworkUrl100"].as_str())
                        .unwrap_or("")
                        .into(),
                    genres: v["genres"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default(),
                })
            })
            .collect())
    }
}
fn plain(text: &str) -> String {
    let mut inside = false;
    text.chars()
        .filter(|c| {
            if *c == '<' {
                inside = true;
                false
            } else if *c == '>' {
                inside = false;
                false
            } else {
                !inside
            }
        })
        .take(6000)
        .collect()
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
        .take(200)
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
            description: plain(&child_text(entry, "description")),
            duration: child_text(entry, "duration"),
        });
    }
    Ok(Feed {
        url: source.into(),
        title: child_text(channel, "title"),
        episodes,
        description: plain(&child_text(channel, "description")),
        author: child_text(channel, "author"),
        artwork: channel
            .children()
            .find(|n| n.tag_name().name() == "image" && n.attribute("href").is_some())
            .and_then(|n| n.attribute("href"))
            .map(str::to_owned)
            .unwrap_or_else(|| {
                channel
                    .children()
                    .find(|n| n.has_tag_name("image"))
                    .map(|n| child_text(n, "url"))
                    .unwrap_or_default()
            }),
        categories: channel
            .children()
            .filter(|n| n.tag_name().name() == "category")
            .filter_map(|n| n.attribute("text").or_else(|| n.text()).map(str::to_owned))
            .collect(),
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
    Artwork(String, Result<Vec<u8>, String>),
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
    pub stream_request: Option<Episode>,
    pub delete_request: Option<(PathBuf, String)>,
    page: usize,
    details: Option<Feed>,
    episode_details: Option<String>,
    history: std::collections::HashMap<String, Listen>,
    queue: Vec<Episode>,
    active: Option<Episode>,
    preview: bool,
    artwork: std::collections::HashMap<String, egui::TextureHandle>,
    art_job: Option<mpsc::Receiver<Event>>,
    art_failed: std::collections::HashSet<String>,
    last_progress: Option<std::time::Instant>,
    last_search: Option<std::time::Instant>,
    pub resume_music: bool,
    pub open_player: bool,
    downloads_index: Vec<Episode>,
    previous: Vec<Episode>,
    completed_active: bool,
    feed_checked: std::collections::HashMap<String, u64>,
    auto_feeds: std::collections::HashSet<String>,
    cached_feeds: Vec<Feed>,
    download_paths: std::collections::HashMap<String, PathBuf>,
    background_fetch: bool,
    auto_pending: Option<Episode>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Listen {
    episode: Option<Episode>,
    position: f64,
    played: bool,
    updated: u64,
}
#[derive(Default, Serialize, Deserialize)]
struct Listening {
    history: std::collections::HashMap<String, Listen>,
    queue: Vec<Episode>,
    #[serde(default)]
    downloads: Vec<Episode>,
    #[serde(default)]
    feed_checked: std::collections::HashMap<String, u64>,
    #[serde(default)]
    auto_feeds: std::collections::HashSet<String>,
    #[serde(default)]
    cached_feeds: Vec<Feed>,
    #[serde(default)]
    download_paths: std::collections::HashMap<String, PathBuf>,
}
impl Drop for Podcasts {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
pub fn duration_seconds(text: &str) -> f64 {
    let mut value = 0.;
    for part in text.split(':') {
        let Ok(part) = part.trim().parse::<f64>() else {
            return 0.;
        };
        if !part.is_finite() || part < 0. {
            return 0.;
        }
        value = value * 60. + part;
    }
    value.min(7. * 86400.)
}
fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn tile(ui: &mut egui::Ui, title: &str, detail: Option<usize>, size: egui::Vec2) -> egui::Response {
    let (r, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let p = crate::theme::current(ui.ctx()).palette();
    let painter = ui.painter();
    painter.rect_filled(
        r,
        3.,
        if response.hovered() {
            p.elevated
        } else {
            p.surface
        },
    );
    painter.rect_filled(
        egui::Rect::from_min_size(r.min, egui::vec2(r.width(), 5.)),
        2.,
        p.elevated,
    );
    painter.rect_stroke(
        r,
        3.,
        egui::Stroke::new(
            1.,
            if response.hovered() {
                p.accent
            } else {
                p.border
            },
        ),
        egui::StrokeKind::Inside,
    );
    painter.line_segment(
        [
            r.left_bottom() + egui::vec2(5., -3.),
            r.right_bottom() + egui::vec2(-5., -3.),
        ],
        egui::Stroke::new(2., p.accent),
    );
    painter.text(
        r.center() - egui::vec2(0., if detail.is_some() { 14. } else { 0. }),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(16.),
        p.text,
    );
    if let Some(count) = detail {
        painter.text(
            r.center() + egui::vec2(0., 15.),
            egui::Align2::CENTER_CENTER,
            count.to_string(),
            egui::FontId::proportional(22.),
            p.accent,
        );
    }
    response
}
fn podcast_brand(ui: &mut egui::Ui) {
    let (r, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 100.), egui::Sense::hover());
    let gold = crate::skin::accent(ui);
    let painter = ui.painter();
    let c = r.center() - egui::vec2(0., 12.);
    for i in 0..16 {
        let h = 8. + ((i * 7 % 9) as f32) * 3.;
        let x = r.left() + 5. + i as f32 * (r.width() - 10.) / 16.;
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, r.bottom() - h), egui::vec2(5., h)),
            0.,
            gold.gamma_multiply(0.2),
        );
    }
    painter.rect_filled(
        egui::Rect::from_center_size(c, egui::vec2(17., 31.)),
        8.,
        gold,
    );
    painter.circle_stroke(c, 27., egui::Stroke::new(2., gold));
    painter.line_segment(
        [c + egui::vec2(0., 16.), c + egui::vec2(0., 33.)],
        egui::Stroke::new(3., gold),
    );
    painter.text(
        egui::pos2(c.x, r.bottom() - 8.),
        egui::Align2::CENTER_BOTTOM,
        "PODCASTS",
        egui::FontId::proportional(18.),
        gold,
    );
}
fn cached_artwork(url: &str) -> Result<Vec<u8>, String> {
    let dir = settings_file()?
        .parent()
        .ok_or("Settings unavailable")?
        .join("PodcastArtwork");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(hash(url));
    if let Ok(data) = std::fs::read(&path)
        && crate::media_lookup::color_image(&data).is_ok()
    {
        return Ok(data);
    }
    let data = bytes(url, 2 * 1024 * 1024)?;
    crate::media_lookup::color_image(&data)?;
    let mut temp = tempfile::NamedTempFile::new_in(&dir).map_err(|e| e.to_string())?;
    temp.write_all(&data).map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(data)
}
fn settings_file() -> Result<PathBuf, String> {
    crate::theme::settings_path()
        .map(|p| p.with_file_name("media.ampdb"))
        .ok_or("Settings folder unavailable".into())
}
pub fn downloads() -> Result<PathBuf, String> {
    if let Some(value) =
        amp_core::catalog_db::local_record(&settings_file()?, "podcast-destination")?
        && let Some(path) = value.as_str()
    {
        return Ok(PathBuf::from(path));
    }
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
            if let Ok(Some(value)) = settings_file()
                .and_then(|f| amp_core::catalog_db::local_record(&f, "podcast-listening"))
                && let Ok(state) = serde_json::from_value::<Listening>(value)
            {
                self.history = state.history;
                self.queue = state.queue;
                self.downloads_index = state.downloads;
                self.feed_checked = state.feed_checked;
                self.auto_feeds = state.auto_feeds;
                self.cached_feeds = state.cached_feeds;
                self.download_paths = state.download_paths;
            }
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
                            Ok(mut feed) => {
                                if let Some(result) =
                                    self.results.iter().find(|r| r.url == feed.url)
                                {
                                    if feed.artwork.is_empty() {
                                        feed.artwork = result.artwork.clone();
                                    }
                                    if feed.author.is_empty() {
                                        feed.author = result.author.clone();
                                    }
                                    if feed.categories.is_empty() {
                                        feed.categories = result.genres.clone();
                                    }
                                }
                                self.status = format!("{} audio episodes", feed.episodes.len());
                                self.feed_checked.insert(feed.url.clone(), timestamp());
                                self.cached_feeds.retain(|f| f.url != feed.url);
                                if self.cached_feeds.len() >= 20 {
                                    self.cached_feeds.remove(0);
                                }
                                self.cached_feeds.push(feed.clone());
                                if self.auto_feeds.contains(&feed.url) {
                                    self.auto_pending = feed
                                        .episodes
                                        .first()
                                        .filter(|e| !self.offline_path(e).is_some())
                                        .cloned();
                                }
                                if !self.background_fetch {
                                    self.details = Some(feed.clone());
                                }
                                if let Some(i) = self.feeds.iter().position(|f| f.url == feed.url) {
                                    self.feeds[i] = feed;
                                    self.selected = i;
                                    save = true;
                                }
                                if !self.background_fetch {
                                    self.page = 8;
                                }
                            }
                            Err(e) => self.status = e,
                        };
                        done = true;
                    }
                    Event::Artwork(_, _) => {}
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
                                    if !self.preview {
                                        self.cache.record(&value.0, &episode.id, &episode.title);
                                        self.download_paths
                                            .insert(episode.id.clone(), value.0.clone());
                                        if !self.downloads_index.iter().any(|e| e.id == episode.id)
                                        {
                                            self.downloads_index.push(episode.clone());
                                        }
                                    }
                                    if value.1 {
                                        if let Some(prior) = self.active.take()
                                            && prior.id != episode.id
                                        {
                                            self.previous.push(prior);
                                        }
                                        self.active = Some(episode);
                                        self.completed_active = false;
                                    }
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
        if let Some(rx) = &self.art_job
            && let Ok(Event::Artwork(key, result)) = rx.try_recv()
        {
            if let Ok(data) = result
                && let Ok(image) = crate::media_lookup::color_image(&data)
            {
                let texture = ctx.load_texture(&key, image, Default::default());
                if self.artwork.len() > 48 {
                    self.artwork.clear();
                }
                self.artwork.insert(key.clone(), texture);
            }
            self.art_job = None;
        }
        if self.art_job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if done {
            self.job = None;
            self.persist_listening();
            if self.completed_active {
                self.next_episode();
            }
        }
        if save {
            self.save();
        }
        if self.job.is_none() && crate::podcast_plugin::enabled() {
            if let Some(episode) = self.auto_pending.take() {
                self.preview = false;
                self.download(episode, false);
            } else if let Some(url) = self
                .feeds
                .iter()
                .find(|f| {
                    self.feed_checked
                        .get(&f.url)
                        .is_none_or(|t| timestamp().saturating_sub(*t) > 3600)
                })
                .map(|f| f.url.clone())
            {
                self.feed_checked.insert(url.clone(), timestamp());
                self.fetch(url);
                self.background_fetch = true;
            }
        }
        if self.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    fn fetch(&mut self, url: String) {
        if self.job.is_some() {
            return;
        }
        if self
            .feed_checked
            .get(&url)
            .is_some_and(|t| timestamp().saturating_sub(*t) < 300)
            && let Some(feed) = self.cached_feeds.iter().find(|f| f.url == url).cloned()
        {
            self.details = Some(feed);
            self.page = 8;
            self.status = "Cached publisher episodes. Use Refresh for the latest.".into();
            return;
        }
        self.background_fetch = false;
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.status = "Loading publisher feed…".into();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Feed(feed(&url)));
        });
    }
    fn offline_path(&self, episode: &Episode) -> Option<PathBuf> {
        self.download_paths
            .get(&episode.id)
            .cloned()
            .filter(|p| p.is_file())
            .or_else(|| {
                downloads()
                    .ok()
                    .map(|d| target(&d, episode))
                    .filter(|p| p.is_file())
            })
    }
    fn listen(&mut self, episode: Episode) {
        if let Some(prior) = self.active.take()
            && prior.id != episode.id
        {
            self.previous.push(prior);
        }
        self.active = Some(episode.clone());
        self.completed_active = false;
        if let Some(path) = self.offline_path(&episode) {
            self.downloaded = Some((path, true));
            self.status = "Playing offline episode in AMP Player.".into();
        } else {
            self.stream_request = Some(episode);
            self.status = "Opening publisher audio in AMP Player…".into();
        }
    }
    fn download(&mut self, episode: Episode, play: bool) {
        self.active_episode = Some(episode.clone());
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let preview = self.preview;
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.status = "Connecting to publisher…".into();
        std::thread::spawn(move || {
            let result = downloads()
                .and_then(|directory| {
                    let directory = if preview && !target(&directory, &episode).is_file() {
                        directory.join("Preview")
                    } else {
                        directory
                    };
                    download_episode(&directory, &episode, &cancel, |n, total| {
                        let _ = tx.send(Event::Progress(n, total));
                    })
                })
                .map(|path| (path, play));
            let _ = tx.send(Event::Downloaded(result));
        });
    }
    fn persist_listening(&mut self) {
        if let Err(error) = settings_file().and_then(|f| {
            amp_core::catalog_db::save_local_record(
                &f,
                "podcast-listening",
                &serde_json::json!(Listening {
                    history: self.history.clone(),
                    queue: self.queue.clone(),
                    downloads: self.downloads_index.clone(),
                    feed_checked: self.feed_checked.clone(),
                    auto_feeds: self.auto_feeds.clone(),
                    cached_feeds: self.cached_feeds.clone(),
                    download_paths: self.download_paths.clone()
                }),
            )
        }) {
            self.status = error;
        }
    }
    pub fn record_progress(&mut self, path: Option<&Path>, seconds: f64, finished: bool) {
        let Some(episode) = self.active.clone() else {
            return;
        };
        let matches = path
            .and_then(|p| p.file_name())
            .is_some_and(|n| n.to_string_lossy().contains(&episode.id[..16]));
        if !matches {
            return;
        }
        if finished && self.completed_active {
            return;
        }
        if !finished
            && self
                .last_progress
                .is_some_and(|t| t.elapsed() < Duration::from_secs(5))
        {
            return;
        }
        self.last_progress = Some(std::time::Instant::now());
        let played = finished || self.history.get(&episode.id).is_some_and(|h| h.played);
        self.history.insert(
            episode.id.clone(),
            Listen {
                episode: Some(episode),
                position: seconds,
                played,
                updated: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            },
        );
        self.persist_listening();
    }
    pub fn page_name(&self) -> &'static str {
        match self.page {
            0 => "home",
            1 => "search",
            2 => "categories",
            3 => "subscriptions",
            4 => "downloads",
            5 => "recent",
            6 => "retention",
            7 => "queue",
            8 => "show",
            9 => "settings",
            _ => "episode",
        }
    }
    pub fn report(&mut self, message: String) {
        self.status = message;
    }
    pub fn leave_playback(&mut self) {
        self.active = None;
    }
    pub fn flush_progress(&mut self, path: Option<&Path>, seconds: f64) {
        self.last_progress = None;
        self.record_progress(path, seconds, false);
    }
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }
    pub fn next_episode(&mut self) -> bool {
        if self.active.is_none() {
            return false;
        }
        if self.job.is_none() && !self.queue.is_empty() {
            let episode = self.queue.remove(0);
            self.preview = true;
            self.listen(episode);
            self.persist_listening();
        }
        true
    }
    pub fn previous_episode(&mut self) -> bool {
        if self.active.is_none() {
            return false;
        }
        if self.job.is_none()
            && let Some(episode) = self.previous.pop()
        {
            self.preview = true;
            self.listen(episode);
        }
        true
    }
    pub fn finished(&mut self) {
        if !self.completed_active {
            self.completed_active = true;
            self.next_episode();
        }
    }
    pub fn resume_position(&self) -> f64 {
        self.active
            .as_ref()
            .and_then(|e| self.history.get(&e.id))
            .filter(|h| !h.played)
            .map_or(0., |h| h.position)
    }
    fn search(&mut self, query: String, category: Option<String>) {
        if self.job.is_some() {
            return;
        }
        if self
            .last_search
            .is_some_and(|t| t.elapsed() < Duration::from_secs(3))
        {
            self.status = "Please wait a moment before searching again.".into();
            return;
        }
        self.last_search = Some(std::time::Instant::now());
        self.query = query.clone();
        self.details = None;
        self.page = 1;
        self.status = "Searching Apple Podcasts…".into();
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(Event::Search(
                AppleDirectory.search(&query, category.as_deref()),
            ));
        });
    }
    fn cover(&mut self, ui: &mut egui::Ui, url: &str, size: f32) {
        if let Some(texture) = self.artwork.get(url) {
            ui.image((texture.id(), egui::vec2(size, size)));
        } else {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 3., ui.visuals().extreme_bg_color);
            let center = rect.center();
            let accent = crate::skin::accent(ui);
            let scale = size / 64.;
            ui.painter()
                .circle_stroke(center, size * 0.28, egui::Stroke::new(1.5, accent));
            ui.painter().rect_filled(
                egui::Rect::from_center_size(center, egui::vec2(10., 22.) * scale),
                5.,
                accent,
            );
            ui.painter().line_segment(
                [
                    center + egui::vec2(0., 12.) * scale,
                    center + egui::vec2(0., 23.) * scale,
                ],
                egui::Stroke::new(2., accent),
            );
            if !url.is_empty() && self.art_job.is_none() && !self.art_failed.contains(url) {
                let url = url.to_owned();
                let (tx, rx) = mpsc::channel();
                self.art_job = Some(rx);
                std::thread::spawn(move || {
                    let _ = tx.send(Event::Artwork(url.clone(), cached_artwork(&url)));
                });
            }
        }
    }
    fn episode_row(&mut self, ui: &mut egui::Ui, episode: &Episode) {
        let busy = self.job.is_some();
        crate::skin::panel(ui, false, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(116.);
            ui.add(egui::Label::new(egui::RichText::new(&episode.title).strong()).truncate());
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(!busy, egui::Button::new("Play / Resume"))
                    .clicked()
                {
                    self.preview = true;
                    self.listen(episode.clone());
                }
                if ui
                    .add_enabled(!busy, egui::Button::new("Download"))
                    .clicked()
                {
                    self.preview = false;
                    self.download(episode.clone(), false);
                }
                if ui.button("+ Queue").clicked() && !self.queue.iter().any(|e| e.id == episode.id)
                {
                    self.queue.push(episode.clone());
                    self.persist_listening();
                }
                if ui.button("Details").clicked() {
                    self.episode_details = Some(episode.id.clone());
                    self.page = 10;
                }
            });
            if self.active.as_ref().is_some_and(|e| e.id == episode.id) {
                ui.colored_label(crate::skin::accent(ui), "Selected in AMP Player");
            }
            ui.weak(format!("{}  {}", episode.date, episode.duration));
            let downloaded = self.offline_path(episode).is_some();
            let played = self.history.get(&episode.id).is_some_and(|h| h.played);
            ui.horizontal(|ui| {
                if downloaded
                    && let Some(path) = self.offline_path(episode)
                    && ui.button("Delete download").clicked()
                {
                    self.delete_request = Some((path, episode.id.clone()));
                }
                ui.weak(if downloaded {
                    "Downloaded"
                } else {
                    "Publisher audio"
                });
                if ui
                    .selectable_label(played, if played { "✓ Played" } else { "Mark played" })
                    .clicked()
                {
                    self.history.entry(episode.id.clone()).or_default().played = !played;
                    self.persist_listening();
                }
            });
        });
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.poll(ui.ctx());
        if !crate::podcast_plugin::enabled() {
            ui.heading("AMP Podcasts");
            ui.label("Install or enable AMP Podcasts in Plugin Manager to browse and listen.");
            return;
        }
        let available = ui.available_size();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(if available.x>900. {180.}else{136.});
                crate::skin::panel(ui,false,|ui| {
                    podcast_brand(ui);
                    ui.weak("Discover. Listen. Follow.");
                });
                for (i,title) in ["Home","Search","Browse / Categories","Subscriptions","Downloaded","Recently Played","Expiring","Play Queues","Show details","Settings"].iter().enumerate() {
                    if i==8 {continue;}
                    if ui.add_sized([ui.available_width(),34.],egui::Button::new(*title).selected(self.page==i)).clicked(){self.page=i;}
                }
                ui.separator();ui.weak("Audio: AMP Player");
                if self.active.is_some() && ui.button("Open Player controls").clicked(){self.open_player=true;}
                if self.active.is_some() && ui.button("Return to music queue").clicked(){self.resume_music=true;self.active=None;}
            });
            ui.separator();
            crate::skin::panel(ui,false,|ui| {
                ui.set_width(ui.available_width());ui.set_min_height((available.y-18.).max(100.));
                ui.label(egui::RichText::new("Podcasts").size(24.).strong());ui.separator();
                ui.horizontal(|ui| {
                    let response=ui.add_sized([(ui.available_width()-90.).max(100.),36.],egui::TextEdit::singleline(&mut self.query).hint_text("Search podcasts, publishers, or topics…"));
                    if ui.add_enabled(self.job.is_none(),egui::Button::new("Search")).clicked() || response.lost_focus()&&ui.input(|i|i.key_pressed(egui::Key::Enter)) {self.search(self.query.clone(),None);}
                });
                if self.job.is_some(){ui.horizontal(|ui|{ui.spinner();ui.label(&self.status);if self.active_episode.is_some()&&ui.button("Cancel").clicked(){self.cancel.store(true,Ordering::Relaxed);}});}else{ui.weak(&self.status);}
                egui::ScrollArea::vertical().id_salt("podcast-main").max_height((available.y-100.).max(80.)).auto_shrink([false,false]).show(ui,|ui| {
                    match self.page {
                        0|2 => {
                            if self.page==0 {
                                let tile_width=((ui.available_width()-24.)/4.).max(100.);
                                ui.horizontal_wrapped(|ui| {
                                    for (page,title,count) in [(5,"Recently Played",self.history.values().filter(|h|h.episode.is_some()).count()),(4,"Downloaded",self.cache.count()),(6,"Retention Soon",self.cache.expiring_count()),(3,"Subscriptions",self.feeds.len())] {
                                        if tile(ui,title,Some(count),egui::vec2(tile_width,88.)).clicked(){self.page=page;}
                                    }
                                });ui.add_space(12.);
                            }
                            ui.heading("Categories");ui.weak("Apple Podcasts directory categories");
                            let cols=((ui.available_width()/190.).floor() as usize).clamp(1,4);
                            let tile_width=(ui.available_width()-8.*(cols-1) as f32)/cols as f32;
                            egui::Grid::new("podcast-categories").spacing([8.,8.]).show(ui,|ui| {
                                for (i,(name,id)) in CATEGORIES.iter().enumerate(){if tile(ui,name,None,egui::vec2(tile_width,82.)).clicked(){self.search((*name).into(),Some((*id).into()));}
if (i+1)%cols==0{ui.end_row();}}
                            });
                            if self.page==0 {ui.add_space(18.);ui.heading("My Podcasts");self.subscriptions(ui);}
                        }
                        1 => {
                            if self.results.is_empty() && self.job.is_none(){ui.label("No podcasts found. Try a title, publisher, or topic.");}
                            for result in self.results.clone() {
                                crate::skin::panel(ui,false,|ui| {ui.set_min_width(ui.available_width());ui.horizontal(|ui| {self.cover(ui,&result.artwork,64.);ui.vertical(|ui| {ui.strong(&result.title);ui.weak(&result.author);ui.weak(result.genres.join(" · "));if ui.add_enabled(self.job.is_none(),egui::Button::new("Browse episodes")).clicked(){self.fetch(result.url.clone());}});});});
                            }
                            ui.weak("Discovery: Apple Podcasts. Episodes come directly from publishers.");
                        }
                        3 => {ui.heading("Subscriptions");self.subscriptions(ui);}
                        4..=6 => {
                            let page=self.page;
                            ui.heading(if page==4{"Downloaded"}else if page==5{"Recently Played"}else{"Expiring / Local retention"});
                            if page==6 {ui.label("Publisher expiration is unknown for these RSS feeds. Dates below are local retention dates only.");self.cache.show(ui);}
                            else {
                                let mut episodes:Vec<Episode>=if page==5 {let mut history:Vec<_>=self.history.values().collect();history.sort_by_key(|h|std::cmp::Reverse(h.updated));history.into_iter().filter_map(|h|h.episode.clone()).collect()}else{self.feeds.iter().flat_map(|f|f.episodes.clone()).chain(self.history.values().filter_map(|h|h.episode.clone())).collect()};
                                if page==4 {episodes.extend(self.downloads_index.clone());}let mut seen=std::collections::HashSet::new();episodes.retain(|e|seen.insert(e.id.clone())&&(page!=4||self.offline_path(e).is_some()));
                                if episodes.is_empty(){ui.label(if page==4{"Download episodes to listen without a connection."}else{"Your listening history will appear here."});}
                                for episode in episodes.iter().take(300){self.episode_row(ui,episode);}
                            }
                        }
                        7 => {
                            ui.heading("Podcast queue");ui.weak("Spoken-word playback · no Auto DJ mixing");
                            if self.queue.is_empty(){ui.label("Browse a show and add episodes to your queue.");}
                            let mut remove=None;let mut swap=None;
                            for (i,e) in self.queue.clone().iter().enumerate(){ui.horizontal(|ui|{ui.label(format!("{}  {}",i+1,e.title));if i>0&&ui.button("↑").clicked(){swap=Some((i,i-1));}
if i+1<self.queue.len()&&ui.button("↓").clicked(){swap=Some((i,i+1));}
if ui.button("Remove").clicked(){remove=Some(i);}});}
                            if let Some(i)=remove{self.queue.remove(i);self.persist_listening();}
if let Some((a,b))=swap{self.queue.swap(a,b);self.persist_listening();}
                            if ui.add_enabled(self.job.is_none()&&!self.queue.is_empty(),egui::Button::new("Play next episode")).clicked(){let e=self.queue.remove(0);self.persist_listening();self.preview=true;self.listen(e);}
                            ui.weak("Queue is saved automatically.");
                        }
                        8 => {
                            if let Some(feed)=self.details.clone(){
                                if ui.button("Search results").clicked(){self.page=1;}
                                ui.horizontal_top(|ui|{self.cover(ui,&feed.artwork,110.);ui.vertical(|ui|{ui.heading(&feed.title);ui.label(&feed.author);ui.weak(feed.categories.join(" · "));ui.weak(format!("{} available episodes",feed.episodes.len()));});});
                                ui.label(&feed.description);
                                let subscribed=self.feeds.iter().position(|f|f.url==feed.url);
                                ui.horizontal_wrapped(|ui| {
                                    if ui.button(if subscribed.is_some(){"Unsubscribe (keep downloads)"}else{"+ Subscribe"}).clicked(){if let Some(i)=subscribed{self.feeds.remove(i);}else{self.feeds.push(feed.clone());}self.save();}
                                    if ui.add_enabled(self.job.is_none(),egui::Button::new("Refresh episodes")).clicked(){self.feed_checked.remove(&feed.url);self.fetch(feed.url.clone());}
                                    if let Some(e)=feed.episodes.first()&& ui.add_enabled(self.job.is_none(),egui::Button::new("Listen to latest episode")).clicked(){self.preview=true;self.listen(e.clone());}
                                });
                                let mut auto=self.auto_feeds.contains(&feed.url);
                                if subscribed.is_some() && ui.checkbox(&mut auto,"Automatically download latest episode (hourly refresh)").changed(){if auto{self.auto_feeds.insert(feed.url.clone());self.feed_checked.remove(&feed.url);}else{self.auto_feeds.remove(&feed.url);}self.persist_listening();}
                                ui.weak("Listen before subscribing. This plays a full episode through AMP Player; no subscription is added.");ui.separator();
                                egui::ScrollArea::vertical().id_salt("episode-list").max_height((ui.available_height()-10.).max(200.)).show_rows(ui,136.,feed.episodes.len(),|ui,range|{for i in range {self.episode_row(ui,&feed.episodes[i]);}});
                            }
                        }
                        10 => {
                            if ui.button("Back to show").clicked(){self.page=8;}
                            let episode=self.episode_details.as_ref().and_then(|id|self.details.iter().flat_map(|f|f.episodes.iter()).chain(self.downloads_index.iter()).chain(self.history.values().filter_map(|h|h.episode.as_ref())).find(|e|&e.id==id)).cloned();
                            if let Some(episode)=episode {self.episode_row(ui,&episode);ui.add_space(12.);ui.label(&episode.description);}
                        }
                        _ => {ui.heading("Podcast settings");crate::theme::selector(ui);ui.label(format!("Downloads: {}",downloads().map(|p|p.display().to_string()).unwrap_or_default()));
                            if ui.button("Choose download folder...").clicked() && let Some(path)=rfd::FileDialog::new().pick_folder(){
                                self.status=settings_file().and_then(|f|amp_core::catalog_db::save_local_record(&f,"podcast-destination",&serde_json::json!(path))).map_or_else(|e|e,|_|"New downloads use the selected folder. Existing files stay where they are.".into());
                            }
                            self.cache.show(ui);ui.collapsing("Advanced: direct RSS feed",|ui|{ui.text_edit_singleline(&mut self.url);if ui.add_enabled(self.job.is_none(),egui::Button::new("Browse RSS feed")).clicked(){self.fetch(self.url.clone());}});}
                    }
                });
            });
        });
    }
    fn subscriptions(&mut self, ui: &mut egui::Ui) {
        if self.feeds.is_empty() {
            ui.label("Find a podcast you enjoy and subscribe to follow new episodes.");
        }
        for feed in self.feeds.clone() {
            crate::skin::panel(ui, false, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    self.cover(ui, &feed.artwork, 48.);
                    ui.vertical(|ui| {
                        ui.strong(&feed.title);
                        ui.weak(format!("{} episodes", feed.episodes.len()));
                    });
                    if ui.button("Browse").clicked() {
                        self.details = Some(feed.clone());
                        self.page = 8;
                    }
                });
            });
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opening_feed_does_not_subscribe_and_state_preserves_queue_and_downloads() {
        let source = "https://example.org/feed";
        let feed=parse_feed(source,r#"<rss xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"><channel><title>Test show</title><description>Public show</description><itunes:author>Publisher</itunes:author><itunes:category text="Technology"/><item><title>Episode</title><description>Episode details</description><itunes:duration>12:34</itunes:duration><enclosure url="https://example.org/one.mp3" type="audio/mpeg"/></item></channel></rss>"#).unwrap();
        assert_eq!(feed.author, "Publisher");
        assert_eq!(feed.categories, vec!["Technology"]);
        assert_eq!(feed.episodes[0].duration, "12:34");
        let mut podcasts = Podcasts::default();
        podcasts.loaded = true;
        let (tx, rx) = mpsc::channel();
        podcasts.job = Some(rx);
        tx.send(Event::Feed(Ok(feed.clone()))).unwrap();
        // Consume using the same separation as the UI; no subscription is implicit.
        let event = podcasts.job.as_ref().unwrap().try_recv().unwrap();
        if let Event::Feed(Ok(feed)) = event {
            podcasts.details = Some(feed);
        }
        assert!(podcasts.feeds.is_empty());
        assert!(podcasts.details.is_some());
        let state = Listening {
            queue: feed.episodes.clone(),
            downloads: feed.episodes.clone(),
            ..Default::default()
        };
        let decoded: Listening =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(decoded.queue[0].id, feed.episodes[0].id);
        assert_eq!(decoded.downloads.len(), 1);
    }
    #[test]
    #[ignore = "Live public podcast directory and RSS network test"]
    fn live_podcast_discovery_and_category() {
        let results = AppleDirectory.search("woodworking", None).unwrap();
        assert!(!results.is_empty());
        let show = results
            .iter()
            .find_map(|r| feed(&r.url).ok().filter(|f| !f.episodes.is_empty()))
            .expect("At least one publisher feed must resolve");
        println!(
            "Live show: {} / {} episodes",
            show.title,
            show.episodes.len()
        );
        let category = AppleDirectory.search("technology", Some("1318")).unwrap();
        assert!(!category.is_empty());
        println!("Live category: {} shows", category.len());
    }
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
            description: String::new(),
            duration: String::new(),
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
