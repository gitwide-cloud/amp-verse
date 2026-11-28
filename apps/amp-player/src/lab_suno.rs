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
pub(super) fn song_page(input: &str) -> Option<String> {
    let canonical = canonical_url(input).ok()?;
    let parsed = url::Url::parse(&canonical).ok()?;
    Some(if let Some(id) = audio_id(&parsed) {
        format!("https://suno.com/song/{id}")
    } else {
        canonical
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
fn song_record<'a>(value: &'a serde_json::Value, id: &str) -> Option<&'a serde_json::Value> {
    match value {
        serde_json::Value::Object(fields) => {
            if fields.get("id").and_then(|v| v.as_str()) == Some(id)
                && (fields.contains_key("audio_url") || fields.contains_key("media_urls"))
            {
                return Some(value);
            }
            fields.values().find_map(|v| song_record(v, id))
        }
        serde_json::Value::Array(values) => values.iter().find_map(|v| song_record(v, id)),
        _ => None,
    }
}
fn record_audio(record: &serde_json::Value, id: &str) -> Result<String, String> {
    if record["drm"] == true || record["is_encrypted"] == true {
        return Err(UNAVAILABLE.into());
    }
    if record["status"].as_str().is_some_and(|s| s != "complete") {
        return Err("This Suno song is not ready to download yet.".into());
    }
    if record["action_config"]["actions"]
        .as_array()
        .is_some_and(|actions| {
            actions.iter().any(|action| {
                matches!(
                    action["action_type"].as_str(),
                    Some("download" | "download_audio" | "download_song")
                ) && (action["disabled"] == true || action["visible"] == false)
            })
        })
    {
        return Err(UNAVAILABLE.into());
    }
    if let Some(url) = record["audio_url"]
        .as_str()
        .and_then(|s| audio_url(s, id).ok())
    {
        return Ok(url);
    }
    // Modern public pages publish progressive audio here; never guess a URL from the song ID.
    record["media_urls"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|media| {
            media["delivery"] == "progressive"
                && media["encoding"]
                    .as_str()
                    .is_none_or(|encoding| matches!(encoding, "identity" | "none" | "plain"))
                && media["drm"] != true
                && media["is_encrypted"] != true
        })
        .find_map(|media| media["url"].as_str().and_then(|s| audio_url(s, id).ok()))
        .ok_or_else(|| UNAVAILABLE.into())
}
fn resolve(html: &str, id: &str) -> Result<(String, String), String> {
    let mut chunks = vec![html.to_owned()];
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
    for text in &chunks {
        for line in text.lines() {
            let payload = line.split_once(':').map_or(line, |(_, body)| body);
            for candidate in [line, payload] {
                if let Ok(value) = serde_json::from_str(candidate)
                    && let Some(record) = song_record(&value, id)
                {
                    let audio = record_audio(record, id)?;
                    let title = record["title"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| meta(html, "og:title"))
                        .unwrap_or_else(|| format!("Suno {id}"));
                    return Ok((audio, safe_name(&title)));
                }
            }
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
    let title = meta(html, "og:title").unwrap_or_else(|| format!("Suno {id}"));
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
fn plain_audio_header(bytes: &[u8]) -> bool {
    bytes.starts_with(b"ID3")
        || bytes.starts_with(b"OggS")
        || bytes.starts_with(b"fLaC")
        || (bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0)
        || (bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WAVE")
        || (bytes.len() >= 8 && &bytes[4..8] == b"ftyp")
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
    let mut header = [0u8; 12];
    let read = fs::File::open(&source)
        .map_err(|e| e.to_string())?
        .read(&mut header)
        .map_err(|e| e.to_string())?;
    if !plain_audio_header(&header[..read]) {
        return Err("Suno returned encoded playback data rather than a standard audio file. Use Suno's export menu and open the exported audio in Lab.".into());
    }
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
    #[ignore = "live Suno availability; requires AMP_SUNO_TEST_URL and real FFmpeg"]
    fn live_public_suno_import() {
        let input = std::env::var("AMP_SUNO_TEST_URL").expect("Set an accessible Suno song URL");
        let dir = tempfile::tempdir().unwrap();
        let tools = Tools::audio_only().unwrap();
        let (tx, _rx) = mpsc::sync_channel(64);
        let result = download(&input, dir.path(), &tools, &AtomicBool::new(false), &tx).unwrap();
        let duration = crate::lab_editor::duration(&result).unwrap();
        assert!(duration > 30., "Downloaded only a placeholder: {duration}");
        let mut editor = crate::lab_editor::Editor::default();
        assert!(editor.load_path(result));
        println!("Suno public audio downloaded, converted and decoded: {duration:.1} seconds");
    }
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
        assert!(editor.load_path(saved.clone()));
        let prep = amp_core::preparation::load(&saved).unwrap();
        assert!(amp_core::music_annotations::marked(&prep, amp_core::music_annotations::Category::AiGenerated));
        assert_eq!(prep.extensions["source_service"], "suno");
        assert_eq!(prep.extensions["source_url"], "https://suno.com/song/ab39a04d-b2e6-463b-9b8e-ddea725422f5");
        assert_eq!(fs::read(&input).unwrap(), original);
        assert_eq!(fs::read_dir(&destination).unwrap().filter_map(Result::ok).filter(|e| e.path().extension().is_some_and(|ext| ext == "mp3")).count(), 1);
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
    fn current_public_progressive_media_is_selected_with_matching_identity() {
        let record = serde_json::json!({"id":ID,"status":"complete","title":"Current public song","audio_url":"https://studio-api.prod.suno.com/api/forbidden","media_urls":[{"url":format!("https://d2lwuy8qc234o3.cloudfront.net/1/clip/{ID}.m4a"),"delivery":"progressive","content_type":"m4a-opus"}]});
        let flight =
            serde_json::json!([1, format!("7:{}", serde_json::json!(["$",{"clip":record}]))]);
        let html = format!("<script>self.__next_f.push({flight})</script>");
        let (url, title) = resolve(&html, ID).unwrap();
        assert!(url.ends_with(&format!("{ID}.m4a")));
        assert_eq!(title, "Current public song");
        let mut denied = record.clone();
        denied["action_config"] =
            serde_json::json!({"actions":[{"action_type":"download","disabled":true}]});
        assert!(record_audio(&denied, ID).is_err());
        let mut encoded = record.clone();
        encoded["media_urls"][0]["encoding"] = serde_json::json!("1.0.0");
        assert!(record_audio(&encoded, ID).is_err());
        assert!(!plain_audio_header(&[
            0xaa, 0xd7, 0xc8, 0xa6, 0x97, 0xc9, 0xff, 0xe8, 0x57, 0xbb, 0xab, 0x5e
        ]));
        assert!(plain_audio_header(b"RIFF0000WAVE"));
        let mut encrypted = record.clone();
        encrypted["media_urls"][0]["drm"] = serde_json::json!(true);
        assert!(record_audio(&encrypted, ID).is_err());
        let mut wrong = record;
        wrong["media_urls"][0]["url"] =
            serde_json::json!("https://cdn1.suno.ai/00000000-0000-0000-0000-000000000000.mp3");
        assert!(record_audio(&wrong, ID).is_err());
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
