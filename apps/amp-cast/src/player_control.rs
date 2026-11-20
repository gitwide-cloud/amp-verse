//! Authenticated server-owned Player uses the station's single audio timeline.
use super::*;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Play {
    library_id: String,
    #[serde(default)]
    tracks: Vec<String>,
    playlist_id: Option<String>,
    #[serde(default)]
    shuffle: bool,
    #[serde(default)]
    server_speakers: bool,
}
pub(super) fn routes() -> Router<Arc<Cast>> {
    Router::new()
        .route("/api/player/audio", get(audio))
        .route("/api/player/playlist", get(playlist))
        .route("/api/player/play", post(play))
        .route("/api/player/queue", post(queue))
}
#[derive(Deserialize)]
struct AudioRef {
    library_id: String,
    path: String,
    #[serde(default)]
    download: bool,
}
async fn audio(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    axum::extract::Query(reference): axum::extract::Query<AudioRef>,
) -> Response {
    if !allowed(&cast, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if cast.library_id.as_deref() != Some(reference.library_id.as_str()) {
        return error(
            StatusCode::CONFLICT,
            "Library changed. Refresh before playing or downloading.",
        );
    }
    if reference.path.len() > 4096 {
        return error(StatusCode::BAD_REQUEST, "Song path is too long.");
    }
    let Some(track) = track_index(&cast, &reference.path).and_then(|index| cast.tracks.get(index))
    else {
        return error(
            StatusCode::NOT_FOUND,
            "Song is no longer in this library. Refresh Library.",
        );
    };
    let path = track.path.clone();
    let source = cast.clone();
    let file =
        match tokio::task::spawn_blocking(move || amp_core::resolve(&source.root, &path)).await {
            Ok(Ok(file)) => file,
            _ => {
                return error(
                    StatusCode::NOT_FOUND,
                    "Song source is unavailable. Reconnect its music folder.",
                );
            }
        };
    studio::file_response(
        file,
        headers,
        if reference.download {
            studio::Attachment::Original
        } else {
            studio::Attachment::Inline
        },
    )
    .await
}
#[derive(Deserialize)]
struct PlaylistRef {
    library_id: String,
    playlist_id: String,
}
async fn playlist(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    axum::extract::Query(reference): axum::extract::Query<PlaylistRef>,
) -> Response {
    if !allowed(&cast, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let request = Play {
        library_id: reference.library_id,
        playlist_id: Some(reference.playlist_id.clone()),
        tracks: vec![],
        shuffle: false,
        server_speakers: false,
    };
    let selected = match selection(&cast, &request).await {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let tracks: Vec<_> = selected["tracks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| cast.tracks.get(v.as_u64()? as usize))
        .map(|t| json!({"path":portable_path(&t.path),"title":t.title}))
        .collect();
    Json(json!({"library_id":cast.library_id,"playlist_id":reference.playlist_id,"kind":selected["kind"],"selection_mode":if selected["kind"]=="smart" {"eligible_snapshot"} else {"ordered"},"unavailable":selected["unavailable"].as_u64().unwrap_or(0),"tracks":tracks})).into_response()
}
pub(super) fn portable_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
pub(super) fn track_index(cast: &Cast, path: &str) -> Option<usize> {
    let path = path.replace('\\', "/");
    cast.tracks
        .iter()
        .position(|track| portable_path(&track.path) == path)
}
fn allowed(cast: &Cast, headers: &HeaderMap) -> bool {
    cast.authorized(headers)
        && headers
            .get("x-amp-library")
            .is_none_or(|value| value.to_str().ok() == cast.library_id.as_deref())
}
fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"error":message}))).into_response()
}
fn problem(status: StatusCode, message: &'static str) -> (StatusCode, &'static str) {
    (status, message)
}
fn indices_sync(cast: &Cast, request: &Play) -> Result<Vec<usize>, (StatusCode, &'static str)> {
    if cast.library_id.as_deref() != Some(request.library_id.as_str()) {
        return Err(problem(
            StatusCode::CONFLICT,
            "This server library changed. Refresh Library before playing.",
        ));
    }
    if request.tracks.is_empty() || request.tracks.len() > 1000 {
        return Err(problem(
            StatusCode::BAD_REQUEST,
            "Choose between 1 and 1000 songs.",
        ));
    }
    request.tracks.iter().map(|path| {
        if path.len()>4096 {return Err(problem(StatusCode::BAD_REQUEST,"A selected song path is too long."));}
        let index=track_index(cast,path).ok_or_else(||problem(StatusCode::CONFLICT,"A selected song is no longer in this library. Refresh Library."))?;
        let file=amp_core::resolve(&cast.root,&cast.tracks[index].path).map_err(|_|problem(StatusCode::UNPROCESSABLE_ENTITY,"A selected song is missing or unavailable. Reconnect its music source before playing."))?;
        if !file.metadata().is_ok_and(|m|m.is_file()&&m.len()>0) {return Err(problem(StatusCode::UNPROCESSABLE_ENTITY,"A selected song is empty or unavailable. Replace it before playing."));}
        Ok(index)
    }).collect()
}
async fn indices(cast: Arc<Cast>, request: Play) -> Result<Vec<usize>, (StatusCode, &'static str)> {
    tokio::task::spawn_blocking(move || indices_sync(&cast, &request))
        .await
        .map_err(|_| {
            problem(
                StatusCode::SERVICE_UNAVAILABLE,
                "Song availability checking stopped. Check the music source and retry.",
            )
        })?
}
#[derive(Clone, Copy)]
struct Intent {
    generation: u64,
    number: u64,
    deadline: tokio::time::Instant,
}
async fn begin_play(cast: &Cast, deadline: tokio::time::Instant) -> Intent {
    let _operation = cast.operation.lock().await;
    Intent {
        deadline,
        generation: cast.station.lock().await.generation,
        number: cast
            .player_intent
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1,
    }
}
fn deadline_error() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "Playback checking timed out. Current playback was kept; check the music source and retry explicitly.",
    )
}
async fn before<T>(
    deadline: tokio::time::Instant,
    future: impl std::future::Future<Output = T>,
) -> Result<T, ()> {
    if tokio::time::Instant::now() >= deadline {
        return Err(());
    }
    tokio::time::timeout_at(deadline, future)
        .await
        .map_err(|_| ())
}
async fn current_intent(cast: &Cast, intent: Intent) -> bool {
    cast.player_intent
        .load(std::sync::atomic::Ordering::Relaxed)
        == intent.number
        && cast.station.lock().await.generation == intent.generation
}
fn shuffle(indices: &mut [usize]) {
    // Explicit shuffle only; each occurrence remains present exactly once.
    for i in (1..indices.len()).rev() {
        let token = amp_core::coordination::token();
        let n = token
            .bytes()
            .fold(0usize, |n, b| n.wrapping_mul(31).wrapping_add(b as usize));
        indices.swap(i, n % (i + 1));
    }
}
async fn selection(cast: &Arc<Cast>, request: &Play) -> Result<Value, (StatusCode, &'static str)> {
    if cast.library_id.as_deref() != Some(request.library_id.as_str()) {
        return Err(problem(
            StatusCode::CONFLICT,
            "This server library changed. Refresh Library.",
        ));
    }
    let mut selected = if let Some(id) = &request.playlist_id {
        if !request.tracks.is_empty() {
            return Err(problem(
                StatusCode::BAD_REQUEST,
                "Choose a playlist or songs, not both.",
            ));
        }
        let source = cast.clone();
        let id = id.clone();
        match tokio::task::spawn_blocking(move || {
            sharing::lists(&source).map(|list| list.into_iter().find(|p| p["id"] == id))
        })
        .await
        {
            Ok(Ok(Some(value))) => value,
            Ok(Ok(None)) => {
                return Err(problem(
                    StatusCode::NOT_FOUND,
                    "That saved playlist no longer exists. Refresh playlists.",
                ));
            }
            _ => {
                return Err(problem(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Saved playlists cannot be read. Check the server's music source and library metadata.",
                ));
            }
        }
    } else {
        json!({"kind":"queue","name":"Player queue","tracks":indices(cast.clone(),request.clone()).await?})
    };
    let smart = selected["kind"] == "smart";
    let mut tracks: Vec<usize> = selected["tracks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_u64().map(|i| i as usize))
        .collect();
    if tracks.is_empty() {
        return Err(problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            "This playlist has no available songs. Check its sources and rules.",
        ));
    }
    if tracks.iter().any(|index| cast.tracks.get(*index).is_none()) {
        return Err(problem(
            StatusCode::CONFLICT,
            "Playlist songs changed. Refresh Library.",
        ));
    }
    if request.shuffle && !smart {
        shuffle(&mut tracks);
    }
    selected["tracks"] = json!(tracks);
    selected["finite"] = json!(!smart);
    Ok(selected)
}
async fn play(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Json(request): Json<Play>,
) -> Response {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    if !allowed(&cast, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let intent = match before(deadline, begin_play(&cast, deadline)).await {
        Ok(intent) => intent,
        Err(()) => return deadline_error(),
    };
    play_selected(
        cast.clone(),
        headers,
        request.server_speakers,
        intent,
        selection(&cast, &request),
    )
    .await
}
async fn play_selected(
    cast: Arc<Cast>,
    headers: HeaderMap,
    server_speakers: bool,
    intent: Intent,
    selected: impl std::future::Future<Output = Result<Value, (StatusCode, &'static str)>>,
) -> Response {
    let mut selected = match before(intent.deadline, selected).await {
        Ok(Ok(selected)) => selected,
        Ok(Err(problem)) => return problem.into_response(),
        Err(()) => return deadline_error(),
    };
    // Probe before the operation lock and before stopping a healthy owner. The
    // decoder only reads generated/selected audio; it never opens a speaker.
    let preflight = tokio::time::timeout_at(
        intent
            .deadline
            .min(tokio::time::Instant::now() + std::time::Duration::from_secs(20)),
        radio::validate_selection(cast.clone(), &mut selected),
    )
    .await;
    if preflight.is_err() {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Audio checking took too long. Current playback was kept; check the server music source and retry.",
        );
    }
    if matches!(&preflight,Ok(Err(error)) if error=="Audio decoder unavailable") {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "The server audio decoder is unavailable. Install FFmpeg on the server; current playback was kept.",
        );
    }
    if preflight.is_ok_and(|result| result.is_err()) {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "No playable songs were found in the first 32 selected occurrences. Repair their audio or choose another playlist; current playback was kept.",
        );
    }
    finish_play(cast, headers, selected, server_speakers, intent).await
}
async fn finish_play(
    cast: Arc<Cast>,
    headers: HeaderMap,
    selected: Value,
    server_speakers: bool,
    intent: Intent,
) -> Response {
    let _operation = match before(intent.deadline, cast.operation.lock()).await {
        Ok(guard) => guard,
        Err(()) => return deadline_error(),
    };
    let current = match before(intent.deadline, current_intent(&cast, intent)).await {
        Ok(current) => current,
        Err(()) => return deadline_error(),
    };
    if !current {
        return error(
            StatusCode::CONFLICT,
            "A newer playback or Stop request took priority. Current playback was kept.",
        );
    }
    if server_speakers {
        match before(intent.deadline, station_control::probe_output()).await {
            Ok(Ok(())) => {}
            Ok(Err(message)) => {
                *cast.server_output_error.lock().unwrap() = Some(message.clone());
                return error(StatusCode::SERVICE_UNAVAILABLE, &message);
            }
            Err(()) => return deadline_error(),
        }
    }
    let current = match before(intent.deadline, current_intent(&cast, intent)).await {
        Ok(current) => current,
        Err(()) => return deadline_error(),
    };
    if !current {
        return error(
            StatusCode::CONFLICT,
            "A newer playback request took priority. Current playback was kept.",
        );
    }
    if tokio::time::Instant::now() >= intent.deadline {
        return deadline_error();
    }
    // Commit inline only after all checking stages fit the common budget. No
    // detached checking task can perform this handoff after timeout/cancellation.
    cast.shutdown().await;
    *cast.selection.lock().unwrap() = Some(selected);
    let response = start_unlocked(State(cast.clone()), headers).await;
    if !response.status().is_success() {
        return response;
    }
    if server_speakers {
        return station_control::output_start_unlocked(cast.clone()).await;
    }
    StatusCode::NO_CONTENT.into_response()
}
async fn queue(
    State(cast): State<Arc<Cast>>,
    headers: HeaderMap,
    Json(request): Json<Play>,
) -> Response {
    if !allowed(&cast, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if request.playlist_id.is_some() || request.shuffle || request.server_speakers {
        return error(
            StatusCode::BAD_REQUEST,
            "Queue accepts ordered songs only. Choose output in Player.",
        );
    }
    let expected_generation = cast.station.lock().await.generation;
    let tracks = match indices(cast.clone(), request.clone()).await {
        Ok(indices) => indices,
        Err(response) => return response.into_response(),
    };
    let _operation = cast.operation.lock().await;
    {
        let station = cast.station.lock().await;
        if station.generation != expected_generation {
            return error(
                StatusCode::CONFLICT,
                "The server Player changed while songs were being checked. Current playback was kept; add them to the current queue explicitly.",
            );
        }
        if !station.running || station.source != "player_queue" {
            return error(
                StatusCode::CONFLICT,
                "Start a server Player queue before adding songs. Live and Smart programs keep their own selection.",
            );
        }
    }
    let mut selection = cast.selection.lock().unwrap();
    let Some(selected) = selection.as_mut().filter(|v| v["finite"] == true) else {
        return error(
            StatusCode::CONFLICT,
            "The server is playing a live or Smart program. Start an ordered Player queue first.",
        );
    };
    if selected["queue_closed"] == true {
        return error(
            StatusCode::CONFLICT,
            "The final server song has finished. Start a new Player queue.",
        );
    }
    let Some(queue) = selected["tracks"].as_array_mut() else {
        return error(
            StatusCode::CONFLICT,
            "The server queue is unavailable. Start it again.",
        );
    };
    if queue.len() + tracks.len() > 1000 {
        return error(
            StatusCode::BAD_REQUEST,
            "The server queue is limited to 1000 song occurrences.",
        );
    }
    queue.extend(tracks.into_iter().map(|i| json!(i)));
    StatusCode::NO_CONTENT.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;
    fn wave(path: &std::path::Path, hz: f32) {
        let frames = 44100u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + frames * 2).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&44100u32.to_le_bytes());
        bytes.extend_from_slice(&88200u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(frames * 2).to_le_bytes());
        for i in 0..frames {
            bytes.extend_from_slice(
                &((i as f32 * hz * std::f32::consts::TAU / 44100.)
                    .sin()
                    .mul_add(3000., 0.) as i16)
                    .to_le_bytes(),
            );
        }
        std::fs::write(path, bytes).unwrap();
    }
    async fn request(app: &Router, uri: &str, body: Value) -> Response {
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("x-amp-password", "secret")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }
    #[tokio::test]
    async fn shared_deadline_cancels_blocked_selection_and_lock_wait_without_handoff() {
        for stage in ["selection", "lock"] {
            let root = tempfile::tempdir().unwrap();
            let cast = Cast::new(
                root.path().into(),
                amp_core::scan(root.path()).unwrap(),
                None,
            );
            {
                let mut station = cast.station.lock().await;
                station.running = true;
                station.source = "live_dj".into();
                station.generation = 77;
            }
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(40);
            let intent = begin_play(&cast, deadline).await;
            if stage == "selection" {
                let (release, blocked) = tokio::sync::oneshot::channel::<()>();
                let response =
                    play_selected(cast.clone(), HeaderMap::new(), false, intent, async move {
                        blocked.await.unwrap();
                        Ok(json!({"finite":true,"tracks":[0]}))
                    })
                    .await;
                assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
                assert!(
                    release.send(()).is_err(),
                    "timed-out selection must be dropped, not a detached future that can handoff later"
                );
            } else {
                let guard = cast.operation.lock().await;
                let response = finish_play(
                    cast.clone(),
                    HeaderMap::new(),
                    json!({"finite":true,"tracks":[0]}),
                    false,
                    intent,
                )
                .await;
                assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
                drop(guard);
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            {
                let station = cast.station.lock().await;
                assert!(station.running);
                assert_eq!(station.generation, 77);
                assert_eq!(station.source, "live_dj");
                assert!(cast.selection.lock().unwrap().is_none());
            }
            cast.shutdown().await;
        }
    }
    #[tokio::test]
    async fn paused_preflight_cannot_resurrect_after_stop_or_replace_a_new_owner_or_intent() {
        for replacement in ["stop", "live", "intent"] {
            let root = tempfile::tempdir().unwrap();
            wave(&root.path().join("a.wav"), 440.);
            let cast = Cast::new(
                root.path().into(),
                amp_core::scan(root.path()).unwrap(),
                None,
            );
            let intent = begin_play(
                &cast,
                tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            )
            .await;
            let generation = intent.generation;
            let (entered, waiting) = tokio::sync::oneshot::channel();
            let (release, resume) = tokio::sync::oneshot::channel();
            let pending = cast.clone();
            let task = tokio::spawn(async move {
                let _ = entered.send(());
                resume.await.unwrap();
                finish_play(
                    pending,
                    HeaderMap::new(),
                    json!({"finite":true,"name":"Old request","tracks":[0]}),
                    false,
                    intent,
                )
                .await
            });
            waiting.await.unwrap();
            if replacement == "intent" {
                let _ = begin_play(
                    &cast,
                    tokio::time::Instant::now() + std::time::Duration::from_secs(30),
                )
                .await;
            } else {
                let uri = if replacement == "stop" {
                    "/api/stop"
                } else {
                    "/api/live/start"
                };
                let response = crate::router(cast.clone())
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri(uri)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert!(response.status().is_success());
            }
            release.send(()).unwrap();
            assert_eq!(task.await.unwrap().status(), StatusCode::CONFLICT);
            {
                let station = cast.station.lock().await;
                if replacement == "live" {
                    assert!(station.running);
                    assert_eq!(station.source, "live_dj");
                } else {
                    assert!(!station.running);
                }
                if replacement != "intent" {
                    assert!(station.generation > generation);
                }
                assert!(cast.selection.lock().unwrap().is_none());
            }
            cast.shutdown().await;
        }
    }
    #[tokio::test]
    async fn stale_refs_and_invalid_sources_never_replace_existing_owner() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("empty.wav"), b"").unwrap();
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            Some("secret".into()),
        );
        {
            let mut state = cast.station.lock().await;
            state.running = true;
            state.generation = 41;
            state.source = "live_dj".into();
        }
        let app = super::routes().with_state(cast.clone());
        assert_eq!(
            request(
                &app,
                "/api/player/play",
                json!({"library_id":"old","tracks":["empty.wav"]})
            )
            .await
            .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            request(
                &app,
                "/api/player/play",
                json!({"library_id":cast.library_id,"tracks":["missing.wav"]})
            )
            .await
            .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            request(
                &app,
                "/api/player/play",
                json!({"library_id":cast.library_id,"tracks":["empty.wav"]})
            )
            .await
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let state = cast.station.lock().await;
        assert!(state.running);
        assert_eq!(state.generation, 41);
        assert_eq!(state.source, "live_dj");
        assert!(cast.selection.lock().unwrap().is_none());
    }
    #[tokio::test]
    async fn private_audio_resolves_portable_refs_across_path_separators() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("artist")).unwrap();
        wave(&root.path().join("artist/song.wav"), 440.);
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        let url = format!(
            "/api/player/audio?library_id={}&path=artist%5Csong.wav",
            cast.library_id.as_ref().unwrap()
        );
        assert!(track_index(&cast, "artist/song.wav").is_some());
        let app = super::routes().with_state(cast);
        let response = app
            .oneshot(Request::builder().uri(url).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    #[tokio::test]
    async fn trusted_lan_private_audio_needs_no_code_but_still_rejects_cross_site() {
        let root = tempfile::tempdir().unwrap();
        wave(&root.path().join("a.wav"), 440.);
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        let url = format!(
            "/api/player/audio?library_id={}&path=a.wav",
            cast.library_id.as_ref().unwrap()
        );
        let app = super::routes().with_state(cast);
        let allowed = app
            .clone()
            .oneshot(Request::builder().uri(&url).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);
        let denied = app
            .oneshot(
                Request::builder()
                    .uri(&url)
                    .header("sec-fetch-site", "cross-site")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    }
    #[tokio::test]
    async fn private_stable_audio_and_download_do_not_take_over_station_or_require_sharing() {
        let root = tempfile::tempdir().unwrap();
        wave(&root.path().join("a.wav"), 440.);
        wave(&root.path().join("b.wav"), 660.);
        let mut library = amp_core::scan(root.path()).unwrap();
        library.tracks.reverse();
        let cast = Cast::new(root.path().into(), library, Some("secret".into()));
        {
            let mut station = cast.station.lock().await;
            station.running = true;
            station.generation = 77;
            station.source = "live_dj".into();
        }
        let app = super::routes().with_state(cast.clone());
        let url = format!(
            "/api/player/audio?library_id={}&path=a.wav&download=true",
            cast.library_id.as_ref().unwrap()
        );
        let wrong = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&url)
                    .header("x-amp-password", "secret")
                    .header("x-amp-library", "other-library")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
        let denied = app
            .clone()
            .oneshot(Request::builder().uri(&url).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&url)
                    .header("x-amp-password", "secret")
                    .header("range", "bytes=0-43")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()["content-type"], "audio/wav");
        assert_eq!(
            response.headers()["content-disposition"],
            "attachment; filename=amp-verse-original.wav"
        );
        let bytes = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        assert_eq!(
            &bytes[..],
            &std::fs::read(root.path().join("a.wav")).unwrap()[..44]
        );
        *cast.selection.lock().unwrap() = Some(json!({"finite":true,"tracks":[]}));
        assert_eq!(
            request(
                &app,
                "/api/player/queue",
                json!({"library_id":cast.library_id,"tracks":["a.wav"]})
            )
            .await
            .status(),
            StatusCode::CONFLICT,
            "stale finite selection cannot append into a newer live owner"
        );
        let head = app
            .oneshot(
                Request::builder()
                    .method("HEAD")
                    .uri(url)
                    .header("x-amp-password", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(head.status(), StatusCode::OK);
        assert!(
            axum::body::to_bytes(head.into_body(), 1024)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(!cast.sharing.load(std::sync::atomic::Ordering::Relaxed));
        let station = cast.station.lock().await;
        assert!(station.running);
        assert_eq!(station.generation, 77);
        assert_eq!(station.source, "live_dj");
    }
    #[tokio::test]
    #[ignore = "requires FFmpeg; generated saved playlists, no physical audio device"]
    async fn generated_saved_normal_skips_bad_occurrences_but_all_bad_keeps_current_program() {
        let root = tempfile::tempdir().unwrap();
        wave(&root.path().join("a.wav"), 440.);
        std::fs::write(root.path().join("broken.wav"), b"not audio").unwrap();
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            Some("secret".into()),
        );
        let library = cast.library_id.clone().unwrap();
        let folder = root.path().join("ampdj/playlists");
        std::fs::create_dir_all(&folder).unwrap();
        for (id, tracks) in [
            (
                "good",
                vec![
                    PathBuf::from("broken.wav"),
                    PathBuf::from("a.wav"),
                    PathBuf::from("a.wav"),
                ],
            ),
            (
                "bad",
                vec![PathBuf::from("broken.wav"), PathBuf::from("broken.wav")],
            ),
        ] {
            amp_core::Playlist {
                version: 1,
                companion_available: false,
                sync: None,
                name: id.into(),
                tracks,
            }
            .save_in_library(&folder.join(format!("{id}.json")), root.path())
            .unwrap();
        }
        let good_path = folder.join("good.json");
        let raw = amp_core::catalog_db::read(&good_path)
            .unwrap()
            .unwrap_or_else(|| std::fs::read(&good_path).unwrap());
        let mut document: Value = serde_json::from_slice(&raw).unwrap();
        document["tracks"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!("missing.wav"));
        amp_core::coordination::atomic_write(&good_path, &document).unwrap();
        let original = amp_core::catalog_db::read(&good_path)
            .unwrap()
            .unwrap_or_else(|| std::fs::read(&good_path).unwrap());
        let app = super::routes().with_state(cast.clone());
        let snapshot = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/player/playlist?library_id={library}&playlist_id=normal_good"
                    ))
                    .header("x-amp-password", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(snapshot.status(), StatusCode::OK);
        let value: Value = serde_json::from_slice(
            &axum::body::to_bytes(snapshot.into_body(), 100000)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value["selection_mode"], "ordered");
        assert_eq!(value["unavailable"], 1);
        assert_eq!(
            value["tracks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["path"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["broken.wav", "a.wav", "a.wav"]
        );
        assert!(!cast.station.lock().await.running);

        assert_eq!(
            request(
                &app,
                "/api/player/play",
                json!({"library_id":library,"tracks":["a.wav"]})
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
        let owner = cast.station.lock().await.generation;
        assert_eq!(
            request(
                &app,
                "/api/player/play",
                json!({"library_id":library,"playlist_id":"normal_bad"})
            )
            .await
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(cast.station.lock().await.generation, owner);
        assert!(cast.station.lock().await.running);
        assert_eq!(
            request(
                &app,
                "/api/player/play",
                json!({"library_id":library,"playlist_id":"normal_good"})
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(8);
        loop {
            let station = cast.station.lock().await;
            if !station.running {
                assert!(station.error.is_none(), "{:?}", station.error);
                assert_eq!(station.player_failures, 2);
                break;
            }
            drop(station);
            assert!(tokio::time::Instant::now() < deadline);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let after = amp_core::catalog_db::read(&good_path)
            .unwrap()
            .unwrap_or_else(|| std::fs::read(&good_path).unwrap());
        assert_eq!(
            after, original,
            "read/play must preserve every savedreference byte"
        );
        let saved: Value = serde_json::from_slice(&after).unwrap();
        assert_eq!(
            saved["tracks"],
            json!(["missing.wav", "broken.wav", "a.wav", "a.wav"])
        );
        assert_eq!(
            amp_core::Playlist::load(&folder.join("bad.json"), root.path())
                .unwrap()
                .tracks
                .len(),
            2
        );
        cast.shutdown().await;
    }
    #[tokio::test]
    #[ignore = "requires FFmpeg; generated finite server queue, no physical audio device"]
    async fn generated_server_queue_keeps_order_duplicates_appends_and_survives_browser_close() {
        let root = tempfile::tempdir().unwrap();
        wave(&root.path().join("a.wav"), 440.);
        wave(&root.path().join("b.wav"), 660.);
        std::fs::write(root.path().join("broken.wav"), b"not audio").unwrap();
        assert!(
            amp_core::command("ffmpeg")
                .args(["-v", "fatal", "-i"])
                .arg(root.path().join("a.wav"))
                .arg(root.path().join("original.flac"))
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        );
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            Some("secret".into()),
        );
        let app = super::routes().with_state(cast.clone());
        let library = cast.library_id.clone().unwrap();
        let mut audio = cast.audio.subscribe();
        let download = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/player/audio?library_id={library}&path=original.flac&download=true"
                    ))
                    .header("x-amp-password", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(download.status(), StatusCode::OK);
        assert_eq!(download.headers()["content-type"], "audio/flac");
        assert_eq!(
            download.headers()["content-disposition"],
            "attachment; filename=amp-verse-original.flac"
        );
        let original = axum::body::to_bytes(download.into_body(), 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(&original[..4], b"fLaC");
        assert_eq!(
            &original[..],
            std::fs::read(root.path().join("original.flac")).unwrap()
        );
        assert!(!cast.station.lock().await.running);

        let response = request(
            &app,
            "/api/player/play",
            json!({"library_id":library,"tracks":["b.wav"]}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let generation = cast.station.lock().await.generation;
        assert_eq!(
            request(
                &app,
                "/api/player/queue",
                json!({"library_id":library,"tracks":["b.wav","broken.wav","a.wav"]})
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            request(
                &app,
                "/api/player/play",
                json!({"library_id":library,"tracks":["broken.wav"]})
            )
            .await
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            cast.station.lock().await.generation,
            generation,
            "bad request must not replace healthy audio owner"
        );
        drop(app); // No browser/client lifetime owns the station worker.
        let mut bytes = Vec::new();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(12);
        loop {
            if let Ok(Ok(chunk)) =
                tokio::time::timeout(std::time::Duration::from_millis(100), audio.recv()).await
            {
                bytes.extend_from_slice(&chunk);
            }
            if !cast.station.lock().await.running {
                while let Ok(chunk) = audio.try_recv() {
                    bytes.extend_from_slice(&chunk);
                }
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "finite queue must finish without looping"
            );
        }
        {
            let state = cast.station.lock().await;
            assert_eq!(state.generation, generation);
            assert!(state.error.is_none(), "{:?}", state.error);
            assert_eq!(state.source, "player_queue");
            assert_eq!(state.player_failures, 1);
            assert!(state.player_last_issue.is_some());
        }
        assert!(
            !station_control::output_running(&cast),
            "stream-only test must never open speakers"
        );
        let encoded = root.path().join("test-output.mp3");
        std::fs::write(&encoded, &bytes).unwrap();
        let output = amp_core::command("ffmpeg")
            .args(["-v", "fatal", "-i"])
            .arg(encoded)
            .args(["-ac", "1", "-ar", "44100", "-f", "f32le", "pipe:1"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let pcm: Vec<f32> = output
            .stdout
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        assert!(
            pcm.len() > 44100 * 29 / 10 && pcm.len() < 44100 * 33 / 10,
            "exactly three occurrences, not a repeating single song"
        );
        for (second, expected, other) in [(0, 660., 440.), (1, 660., 440.), (2, 440., 660.)] {
            let slice = &pcm[second * 44100 + 12000..second * 44100 + 24000];
            let power = |hz: f32| {
                let (a, b) = slice.iter().enumerate().fold((0., 0.), |(a, b), (i, s)| {
                    let phase = i as f32 * hz * std::f32::consts::TAU / 44100.;
                    (a + s * phase.cos(), b + s * phase.sin())
                });
                a * a + b * b
            };
            assert!(
                power(expected) > power(other) * 10.,
                "wrong actual PCM for occurrence {second}"
            );
        }
        cast.shutdown().await;
    }
}
