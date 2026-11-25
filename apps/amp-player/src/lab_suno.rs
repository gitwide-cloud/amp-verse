//! Public Suno song imports. No cookies, guessed CDN paths, or restricted-media fallback.
use super::*;
const LIMIT: u64 = 256 * 1024 * 1024;
const UNAVAILABLE: &str = "Suno has not provided a downloadable audio file for this link. Download it through Suno's own export menu, then open that file in Lab.";

fn audio_host(host: &str) -> bool {
    host.strip_prefix("cdn")
        .and_then(|s| s.strip_suffix(".suno.ai"))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        || host == "d2lwuy8qc234o3.cloudfront.net"
}
pub(super) fn recognizes(input: &str) -> bool {
    url::Url::parse(input.trim()).ok().is_some_and(|u| {
        matches!(
            u.host_str(),
            Some("suno.com" | "www.suno.com" | "suno.ai" | "www.suno.ai")
        ) || audio_host(u.host_str().unwrap_or(""))
    })
}
fn audio_id(u: &url::Url) -> Option<&str> {
    let file = u.path_segments()?.next_back()?;
    [".mp3", ".m4a", ".wav"]
        .iter()
        .find_map(|ext| file.strip_suffix(ext).filter(|id| uuid(id)))
}
fn uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
pub(super) fn canonical_url(input: &str) -> Result<String, String> {
    let u = url::Url::parse(input.trim()).map_err(|_| "Paste a complete Suno song link.")?;
    if u.scheme() != "https"
        || !recognizes(input)
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
    {
        return Err("Use an ordinary https://suno.com/song/ link.".into());
    }
    if audio_host(u.host_str().unwrap_or("")) {
        return audio_url(u.as_str(), audio_id(&u).ok_or(UNAVAILABLE)?);
    }
    let path = u.path().trim_end_matches('/');
    let valid = path.strip_prefix("/song/").is_some_and(uuid)
        || path.strip_prefix("/s/").is_some_and(|s| {
            (4..=100).contains(&s.len())
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        });
    if !valid {
        return Err("Choose a single Suno song or share link, not a profile or playlist.".into());
    }
    Ok(if let Some(id) = path.strip_prefix("/song/") {
        format!("https://suno.com/song/{}", id.to_ascii_lowercase())
    } else {
        format!("https://suno.com{path}")
    })
}
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_per_read(Some(Duration::from_secs(5)))
        .timeout_recv_response(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(120)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into()
}
fn get(agent: &ureq::Agent, target: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    agent
        .get(target)
        .header("User-Agent", "AMPVerse/1.1")
        .call()
        .map_err(|e| format!("Could not read the Suno link: {e}"))
}
fn meta(html: &str, property: &str) -> Option<String> {
    for fragment in html.split('<').filter(|s| s.starts_with("meta ")) {
        let tag = fragment.split('>').next()?.trim_end_matches('/');
        let xml = format!("<{tag}/>");
        if let Ok(doc) = roxmltree::Document::parse(&xml) {
            let node = doc.root_element();
            if node.attribute("property") == Some(property) {
                return node.attribute("content").map(str::to_owned);
            }
        }
    }
    None
}
fn json_strings(text: &str, key: &str) -> Vec<String> {
    let needle = format!("\"{key}\":");
    text.split(&needle)
        .skip(1)
        .filter_map(|part| {
            serde_json::Deserializer::from_str(part.trim_start())
                .into_iter::<String>()
                .next()?
                .ok()
        })
        .collect()
}
fn audio_url(input: &str, id: &str) -> Result<String, String> {
    let u = url::Url::parse(input).map_err(|_| UNAVAILABLE)?;
    let host = u.host_str().unwrap_or("");
    let known = audio_host(host);
    let file = u
        .path_segments()
        .and_then(|mut p| p.next_back())
        .unwrap_or("");
    if u.scheme() != "https"
        || !known
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
        || ![
            format!("{id}.mp3"),
            format!("{id}.m4a"),
            format!("{id}.wav"),
        ]
        .contains(&file.to_owned())
    {
        return Err(UNAVAILABLE.into());
    }
    Ok(u.into())
}
fn resolve(html: &str, id: &str) -> Result<(String, String), String> {
    let mut chunks = vec![html.to_owned()];
    // Next.js exposes the public song record as JSON strings in flight script chunks.
    for fragment in html.split("self.__next_f.push(").skip(1) {
        if let Some(Ok(serde_json::Value::Array(values))) =
            serde_json::Deserializer::from_str(fragment)
                .into_iter()
                .next()
        {
            chunks.extend(
                values
                    .into_iter()
                    .filter_map(|v| v.as_str().map(str::to_owned)),
            );
        }
    }
    let mut urls: Vec<String> = chunks
        .iter()
        .flat_map(|text| json_strings(text, "audio_url"))
        .collect();
    if urls.is_empty()
        && let Some(og) = meta(html, "og:audio")
    {
        urls.push(og);
    }
    let audio = urls
        .iter()
        .find_map(|s| audio_url(s, id).ok())
        .ok_or(UNAVAILABLE)?;
    let title = meta(html, "og:title")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("Suno {id}"));
    Ok((audio, safe_name(&title)))
}
fn read_page(
    agent: &ureq::Agent,
    initial: &str,
    cancel: &AtomicBool,
) -> Result<(String, String), String> {
    let mut target = canonical_url(initial)?;
    for _ in 0..6 {
        cancelled(cancel)?;
        let mut response = get(agent, &target)?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or(UNAVAILABLE)?;
            let next = url::Url::parse(&target)
                .unwrap()
                .join(location)
                .map_err(|_| UNAVAILABLE)?;
            target = canonical_url(next.as_str())?;
            continue;
        }
        if !response.status().is_success() {
            return Err(UNAVAILABLE.into());
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("Suno returned an oversized page.".into());
        }
        let html =
            String::from_utf8(bytes).map_err(|_| "Suno returned invalid song information.")?;
        if url::Url::parse(&target).unwrap().path().starts_with("/s/") {
            let next = meta(&html, "og:url").ok_or(
                "This Suno share link did not identify a song. Copy its full song link instead.",
            )?;
            target = canonical_url(&next)?;
            continue;
        }
        return Ok((target, html));
    }
    Err("The Suno link redirected too many times. Copy the full song link instead.".into())
}
fn copy_audio(
    mut input: impl Read,
    output: &mut impl Write,
    cancel: &AtomicBool,
    tx: &mpsc::SyncSender<Event>,
    total: Option<u64>,
) -> Result<(), String> {
    if total.is_some_and(|n| n > LIMIT) {
        return Err("Suno audio exceeds the 256 MB import limit.".into());
    }
    let mut count = 0u64;
    let mut buffer = [0; 65536];
    loop {
        cancelled(cancel)?;
        let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > LIMIT {
            return Err("Suno audio exceeds the 256 MB import limit.".into());
        }
        output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        progress(
            tx,
            "Downloading Suno audio...",
            total
                .filter(|n| *n > 0)
                .map(|n| (count as f32 / n as f32).min(1.)),
        );
    }
    if count == 0 {
        return Err("Suno returned an empty audio file.".into());
    }
    Ok(())
}
fn audio_response(
    agent: &ureq::Agent,
    initial: &str,
    id: &str,
    cancel: &AtomicBool,
) -> Result<ureq::http::Response<ureq::Body>, String> {
    let mut target = audio_url(initial, id)?;
    for _ in 0..6 {
        cancelled(cancel)?;
        let response = get(agent, &target)?;
        if response.status().is_redirection() {
            let next = response
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or(UNAVAILABLE)?;
            let next = url::Url::parse(&target)
                .unwrap()
                .join(next)
                .map_err(|_| UNAVAILABLE)?;
            target = audio_url(next.as_str(), id)?;
        } else if response.status().is_success() {
            return Ok(response);
        } else {
            return Err(UNAVAILABLE.into());
        }
    }
    Err("Suno audio redirected too many times.".into())
}
pub(super) fn download(
    input: &str,
    dir: &Path,
    tools: &Tools,
    cancel: &AtomicBool,
    tx: &mpsc::SyncSender<Event>,
) -> Result<PathBuf, String> {
    let agent = agent();
    progress(tx, "Reading Suno song...", None);
    let initial = canonical_url(input)?;
    let initial_url = url::Url::parse(&initial).unwrap();
    let (canonical, audio, title, id) = if audio_host(initial_url.host_str().unwrap_or("")) {
        let id = audio_id(&initial_url).ok_or(UNAVAILABLE)?.to_owned();
        (
            format!("https://suno.com/song/{id}"),
            initial.clone(),
            format!("Suno {id}"),
            id,
        )
    } else {
        let (canonical, html) = read_page(&agent, &initial, cancel)?;
        let parsed = url::Url::parse(&canonical).unwrap();
        let id = parsed
            .path()
            .strip_prefix("/song/")
            .ok_or(UNAVAILABLE)?
            .to_owned();
        let (audio, title) = resolve(&html, &id)?;
        (canonical, audio, title, id)
    };
    let mut response = audio_response(&agent, &audio, &id, cancel)?;
    let content = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content.starts_with("audio/") && !content.starts_with("application/octet-stream") {
        return Err("Suno did not return a downloadable audio file.".into());
    }
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let work = tempfile::Builder::new()
        .prefix(".amp-lab-suno-")
        .tempdir_in(dir)
        .map_err(|e| e.to_string())?;
    let source = work.path().join("source.audio");
    copy_audio(
        response.body_mut().as_reader(),
        &mut fs::File::create(&source).map_err(|e| e.to_string())?,
        cancel,
        tx,
        total,
    )?;
    convert_publish(
        &source,
        dir,
        tools,
        cancel,
        tx,
        ImportLabel {
            name: &title,
            id: &id,
            suno_source: Some(&canonical),
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "ab39a04d-b2e6-463b-9b8e-ddea725422f5";
    #[test]
    #[ignore = "requires real FFmpeg/ffprobe; no Suno account or network"]
    fn local_codec_import_opens_in_lab_and_preserves_source() {
        let tools = Tools::audio_only().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source.wav");
        let (tx, _rx) = mpsc::sync_channel(64);
        let cancel = AtomicBool::new(false);
        let mut generate = Command::new(&tools.ffmpeg);
        generate
            .args([
                "-n",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.5",
            ])
            .arg(&input);
        run(generate, &cancel, &tx, Duration::from_secs(15)).unwrap();
        let original = fs::read(&input).unwrap();
        let destination = dir.path().join("downloads");
        fs::create_dir(&destination).unwrap();
        let saved = convert_publish(
            &input,
            &destination,
            &tools,
            &cancel,
            &tx,
            ImportLabel {
                name: "Suno codec fixture",
                id: ID,
                suno_source: Some("https://suno.com/song/ab39a04d-b2e6-463b-9b8e-ddea725422f5"),
            },
        )
        .unwrap();
        assert!(crate::lab_editor::duration(&saved).unwrap() > 0.);
        let mut probe = Command::new(&tools.probe);
        probe
            .args([
                "-v",
                "quiet",
                "-show_entries",
                "format_tags=title,comment",
                "-of",
                "json",
            ])
            .arg(&saved);
        let info: serde_json::Value =
            serde_json::from_str(&run(probe, &cancel, &tx, Duration::from_secs(10)).unwrap())
                .unwrap();
        assert_eq!(info["format"]["tags"]["title"], "Suno codec fixture");
        assert!(
            info["format"]["tags"]["comment"]
                .as_str()
                .unwrap()
                .contains("Suno source: https://suno.com/song/")
        );
        let mut editor = crate::lab_editor::Editor::default();
        assert!(editor.load_path(saved));
        assert_eq!(fs::read(&input).unwrap(), original);
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 1);
    }
    #[test]
    fn validates_song_share_and_untrusted_links() {
        assert_eq!(
            canonical_url(&format!("https://www.suno.com/song/{ID}?utm_source=x")).unwrap(),
            format!("https://suno.com/song/{ID}")
        );
        assert!(canonical_url("https://suno.com/s/abcDEF123").is_ok());
        assert!(canonical_url(&format!("https://cdn1.suno.ai/{ID}.mp3")).is_ok());
        assert!(canonical_url("https://cdn1.suno.ai/silence.mp3").is_err());
        for u in [
            "https://suno.com/playlist/abc",
            "https://suno.com.evil.test/song/x",
            "https://suno.com@127.0.0.1/song/x",
            "http://suno.com/song/x",
            "https://suno.com:9876/song/x",
        ] {
            assert!(canonical_url(u).is_err());
        }
    }
    #[test]
    fn selects_exact_song_not_silence_or_recommended_tracks() {
        let record = format!(
            r#"0:{{"audio_url":"https://cdn1.suno.ai/silence.mp3","other":{{"audio_url":"https://cdn1.suno.ai/{ID}.mp3"}}}}"#
        );
        let flight = serde_json::json!([1, record]);
        let html = format!(
            r#"<meta content="A &amp; B&#x27;s track" property="og:title"><script>self.__next_f.push({flight})</script>"#
        );
        let (audio, title) = resolve(&html, ID).unwrap();
        assert_eq!(audio, format!("https://cdn1.suno.ai/{ID}.mp3"));
        assert_eq!(title, "A & B's track");
        assert!(resolve(r#"{"audio_url":"https://studio-api.prod.suno.com/api/forbidden","media_urls":[{"url":"https://cdn1.suno.ai/ignored.mp3"}]}"#,ID).is_err());
        for u in [
            format!("http://cdn1.suno.ai/{ID}.mp3"),
            format!("https://127.0.0.1/{ID}.mp3"),
            format!("https://cdn1.suno.ai.evil.test/{ID}.mp3"),
        ] {
            assert!(audio_url(&u, ID).is_err());
        }
    }
    #[test]
    fn bounded_copy_cancels_without_publishing() {
        let (tx, _rx) = mpsc::sync_channel(64);
        let mut output = Vec::new();
        assert!(
            copy_audio(
                &b"audio"[..],
                &mut output,
                &AtomicBool::new(true),
                &tx,
                None
            )
            .is_err()
        );
        assert!(output.is_empty());
        assert!(
            copy_audio(
                &b"audio"[..],
                &mut output,
                &AtomicBool::new(false),
                &tx,
                Some(LIMIT + 1)
            )
            .is_err()
        );
        assert!(output.is_empty());
        assert!(copy_audio(&b""[..], &mut output, &AtomicBool::new(false), &tx, None).is_err());
        copy_audio(
            &b"audio"[..],
            &mut output,
            &AtomicBool::new(false),
            &tx,
            Some(5),
        )
        .unwrap();
        assert_eq!(output, b"audio");
    }
}
