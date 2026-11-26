//! Shared RSS/Atom parsing for desktop and browser-server Podcasts.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
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
pub fn http_url(value: &str) -> Result<url::Url, String> {
    let u = url::Url::parse(value.trim()).map_err(|_| "Enter a complete HTTP/HTTPS URL")?;
    if !matches!(u.scheme(), "http" | "https")
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
    {
        return Err("Use HTTP/HTTPS without embedded passwords".into());
    }
    Ok(u)
}
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
        let supported = crate::supported(Path::new(url.path()));
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rss_audio_only_and_relative_enclosures() {
        let f=parse_feed("https://example.org/show/feed.xml",r#"<rss><channel><title>Show</title><item><title>One</title><guid>one</guid><enclosure url="../one.mp3" type="audio/mpeg"/></item><item><title>Video</title><enclosure url="film.mp4" type="video/mp4"/></item></channel></rss>"#).unwrap();
        assert_eq!(f.episodes.len(), 1);
        assert_eq!(f.episodes[0].url, "https://example.org/one.mp3");
    }
    #[test]
    fn rejects_embedded_credentials_and_non_feeds() {
        assert!(http_url("https://user:secret@example.org/feed").is_err());
        assert!(http_url("file:///tmp/feed").is_err());
        assert!(parse_feed("https://example.org/feed", "<html/>").is_err());
    }
}
