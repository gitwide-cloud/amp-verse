//! Opt-in playlist sharing. Audio is original; preparation remains portable and read-only.
use super::*;
use axum::extract::Path;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::atomic::Ordering::Relaxed;

pub fn routes() -> Router<Arc<Cast>> {
    Router::new()
        .route("/api/smart/enable", post(enable))
        .route("/api/smart/manifest", get(manifest))
        .route("/api/smart/tracks/{id}/metadata", get(metadata))
        .route("/api/smart/tracks/{id}/audio", get(audio))
        .route("/api/smart/playlists/{id}/play", post(play))
}
async fn enable(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if let Some(dir) = cast.config_dir.lock().unwrap().as_ref()
        && let Err(error) = config::Config::set_sharing(dir, true)
    {
        return (StatusCode::SERVICE_UNAVAILABLE, error).into_response();
    }
    cast.sharing.store(true, Relaxed);
    StatusCode::NO_CONTENT.into_response()
}
pub(crate) fn lists(cast: &Cast) -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    let settings = if cast.root.join("ampdj/smart-playlists.json").is_file() {
        amp_core::show::Settings::load(&cast.root)?
    } else {
        amp_core::show::Settings {
            version: 1,
            sync: None,
            playlists: Vec::new(),
        }
    };
    for p in settings.playlists {
        let ordered: Vec<_> = cast
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| amp_core::show::eligible(&p, t))
            .map(|(i, _)| i)
            .collect();
        result.push(json!({"id":format!("smart_{}",p.id),"name":p.name,"kind":"smart","rules":p,"tracks":ordered}));
    }
    let directory = cast.root.join("ampdj/playlists");
    if directory.exists() {
        let directory = directory.canonicalize().map_err(|e| e.to_string())?;
        if !directory.starts_with(&cast.root) {
            return Err("Playlist folder leaves the library".into());
        }
        let mut files = amp_core::catalog_db::playlist_paths(&cast.root)?;
        files.sort();
        for path in files.into_iter().take(1000) {
            let path = path.canonicalize().map_err(|e| e.to_string())?;
            if !path.starts_with(&cast.root)
                || std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 4 * 1024 * 1024
            {
                continue;
            }
            if let Ok(p) = amp_core::Playlist::load(&path, &cast.root) {
                let tracks: Vec<_> = p
                    .tracks
                    .iter()
                    .filter_map(|path| cast.tracks.iter().position(|t| &t.path == path))
                    .collect();
                result.push(json!({"id":format!("normal_{}",path.file_stem().unwrap_or_default().to_string_lossy()),"name":p.name,"kind":"playlist","tracks":tracks}));
            }
        }
    }
    Ok(result)
}
fn allowed(cast: &Cast, headers: &HeaderMap) -> bool {
    cast.sharing.load(Relaxed) && cast.authorized(headers)
}
#[cfg(test)]
mod playback_tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;
    #[tokio::test]
    #[ignore = "requires FFmpeg; no hardware playback"]
    async fn remote_smart_playlist_streams_only_its_selected_folder() {
        let root = tempfile::tempdir().unwrap();
        for folder in ["allowed", "excluded"] {
            std::fs::create_dir(root.path().join(folder)).unwrap();
            let result = amp_core::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=1",
                ])
                .arg(root.path().join(folder).join("song.wav"))
                .output()
                .unwrap();
            assert!(result.status.success());
        }
        let mut rules = amp_core::show::Settings::default();
        let mut profile = rules.playlists[0].clone();
        profile.id = "remote".into();
        profile.folders = vec!["allowed".into()];
        profile.restrict_folders = true;
        profile.restrict_allowed = false;
        profile.allowed.clear();
        rules.playlists = vec![profile];
        rules.save(root.path()).unwrap();
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        cast.sharing.store(true, Relaxed);
        let mut audio = cast.audio.subscribe();
        let result = router(cast.clone())
            .oneshot(
                Request::post("/api/smart/playlists/smart_remote/play")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(result.status(), 204);
        let bytes = tokio::time::timeout(std::time::Duration::from_secs(5), audio.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(!bytes.is_empty());
        let index = cast.station.lock().await.current_index.unwrap();
        assert!(cast.tracks[index].path.starts_with("allowed"));
        cast.shutdown().await;
    }
}
async fn play(
    State(cast): State<Arc<Cast>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !allowed(&cast, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let source = cast.clone();
    let selection = tokio::task::spawn_blocking(move || {
        lists(&source)?
            .into_iter()
            .find(|p| p["id"].as_str() == Some(&id))
            .ok_or_else(|| "Playlist unavailable".to_string())
    })
    .await;
    let value = match selection {
        Ok(Ok(value)) => value,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    if value["tracks"]
        .as_array()
        .is_none_or(|tracks| tracks.is_empty())
    {
        return (StatusCode::CONFLICT, "Playlist has no songs").into_response();
    }
    let _operation = cast.operation.lock().await;
    cast.shutdown().await;
    *cast.selection.lock().unwrap() = Some(value);
    super::start_unlocked(State(cast.clone()), headers).await
}
fn shared_track(cast: &Cast, id: usize) -> Result<&Track, StatusCode> {
    let lists = lists(cast).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !lists.iter().any(|p| {
        p["tracks"]
            .as_array()
            .is_some_and(|a| a.iter().any(|i| i.as_u64() == Some(id as u64)))
    }) {
        return Err(StatusCode::NOT_FOUND);
    }
    cast.tracks.get(id).ok_or(StatusCode::NOT_FOUND)
}
async fn manifest(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !allowed(&cast, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let work=tokio::task::spawn_blocking(move||lists(&cast).map(|playlists|{
        let shared:std::collections::HashSet<u64>=playlists.iter().filter_map(|p|p["tracks"].as_array()).flatten().filter_map(Value::as_u64).collect();
        json!({"version":1,"server_id":cast.server_id,"station_id":cast.station_id,"library_id":cast.library_id,"name":cast.station_name,"playlists":playlists,"tracks":cast.tracks.iter().enumerate().filter(|(id,_)|shared.contains(&(*id as u64))).map(|(id,t)|json!({"id":id,"title":t.title,"extension":t.path.extension().unwrap_or_default().to_string_lossy()})).collect::<Vec<_>>()})
    })).await;
    match work {
        Ok(Ok(value)) => Json(value).into_response(),
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
async fn metadata(
    State(cast): State<Arc<Cast>>,
    Path(id): Path<usize>,
    headers: HeaderMap,
) -> Response {
    if !allowed(&cast, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let work=tokio::task::spawn_blocking(move||->Result<Value,StatusCode>{
        let track=shared_track(&cast,id)?; let path=amp_core::resolve(&cast.root,&track.path).map_err(|_|StatusCode::NOT_FOUND)?;
        let (preparation,revision)=amp_core::preparation::load_with_revision(&path).map_err(|_|StatusCode::UNPROCESSABLE_ENTITY)?;
        let mut file=std::fs::File::open(&path).map_err(|_|StatusCode::NOT_FOUND)?;
        let mut hasher=Sha256::new(); let mut buffer=[0;65536]; let mut size=0u64;
        loop {let n=std::io::Read::read(&mut file,&mut buffer).map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;if n==0 {break;} size+=n as u64;hasher.update(&buffer[..n]);}
        if amp_core::preparation::revision(&path)!=revision {return Err(StatusCode::CONFLICT);}
        let checksum=format!("{:x}",hasher.finalize());
        if preparation.source_sha256.as_deref().is_some_and(|saved|saved!=checksum) {return Err(StatusCode::CONFLICT);}
        Ok(json!({"version":1,"id":id,"title":track.title,"server_id":cast.server_id,"station_id":cast.station_id,"library_id":cast.library_id,"bytes":size,"sha256":checksum,"preparation":preparation}))
    }).await;
    match work {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(code)) => code.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
async fn audio(
    State(cast): State<Arc<Cast>>,
    Path(id): Path<usize>,
    headers: HeaderMap,
) -> Response {
    if !allowed(&cast, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let location = tokio::task::spawn_blocking(move || {
        let t = shared_track(&cast, id)?;
        amp_core::resolve(&cast.root, &t.path).map_err(|_| StatusCode::NOT_FOUND)
    })
    .await;
    let path = match location {
        Ok(Ok(p)) => p,
        Ok(Err(code)) => return code.into_response(),
        _ => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let mut file = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let size = match file.metadata().await {
        Ok(m) => m.len(),
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let stream = async_stream::stream! {let mut buffer=vec![0;65536];loop {match file.read(&mut buffer).await { Ok(0)=>break, Ok(n)=>yield Ok::<Bytes,std::io::Error>(Bytes::copy_from_slice(&buffer[..n])), Err(e)=>{yield Err(e);break;} }}};
    Response::builder()
        .header("content-type", "application/octet-stream")
        .header("content-length", size)
        .header("cache-control", "no-store")
        .body(Body::from_stream(stream))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;
    #[tokio::test]
    async fn sharing_is_opt_in_authenticated_and_copies_originals_with_cues() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("song.mp3"), b"original audio bytes").unwrap();
        let mut prep = amp_core::Preparation {
            bpm: Some(128.),
            manual_override: true,
            ..Default::default()
        };
        prep.manual_cues.push(amp_core::preparation::Cue {
            id: "my-drop".into(),
            label: "My drop".into(),
            seconds: 12.,
            number: Some(3),
            ..Default::default()
        });
        amp_core::preparation::save(dir.path(), std::path::Path::new("song.mp3"), &prep).unwrap();
        amp_core::show::Settings::default()
            .save(dir.path())
            .unwrap();
        let library = amp_core::scan(dir.path()).unwrap();
        std::fs::create_dir_all(dir.path().join("ampdj/playlists")).unwrap();
        amp_core::Playlist {
            sync: None,
            version: 1,
            name: "My saved playlist".into(),
            tracks: vec!["song.mp3".into(), "song.mp3".into()],
        }
        .save_in_library(&dir.path().join("ampdj/playlists/test.json"), dir.path())
        .unwrap();
        let cast = Cast::new(dir.path().into(), library, Some("access".into()));
        let app = router(cast.clone());
        let request = |path: &str, password: &str| {
            Request::get(path)
                .header("x-amp-password", password)
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(
            app.clone()
                .oneshot(request("/api/smart/manifest", "access"))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        cast.sharing.store(true, Relaxed);
        assert_eq!(
            app.clone()
                .oneshot(request("/api/smart/manifest", "wrong"))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let response = app
            .clone()
            .oneshot(request("/api/smart/manifest", "access"))
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(
            value["playlists"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["kind"] == "smart")
        );
        let regular = value["playlists"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["kind"] == "playlist")
            .unwrap();
        assert_eq!(regular["tracks"].as_array().unwrap().len(), 2);
        let response = app
            .clone()
            .oneshot(request("/api/smart/tracks/0/metadata", "access"))
            .await
            .unwrap();
        let metadata: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(metadata["preparation"]["manual_cues"][0]["id"], "my-drop");
        let response = app
            .clone()
            .oneshot(request("/api/smart/tracks/0/audio", "access"))
            .await
            .unwrap();
        assert_eq!(
            axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap()
                .as_ref(),
            b"original audio bytes"
        );
        assert_eq!(
            app.oneshot(request("/api/smart/tracks/999/audio", "access"))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            amp_core::preparation::load(&dir.path().join("song.mp3"))
                .unwrap()
                .manual_cues[0]
                .seconds,
            12.
        );
    }
}
