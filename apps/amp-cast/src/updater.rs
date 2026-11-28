//! Server-owned update: official GitHub feed, verified core package, graceful restart.
use super::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Default)]
pub struct Updater {
    busy: AtomicBool,
    cancel: AtomicBool,
    state: std::sync::Mutex<Value>,
    committed: AtomicBool,
    pub exit: tokio::sync::Notify,
}
fn available() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.file_name().map(|n| {
                n == if cfg!(windows) {
                    "amp-cast.exe"
                } else {
                    "amp-cast"
                }
            })
        })
        .unwrap_or(false)
}
pub fn routes() -> Router<Arc<Cast>> {
    Router::new()
        .route("/api/update", get(status))
        .route("/api/update/install", post(install))
        .route("/api/update/cancel", post(cancel))
}
async fn status(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut state = cast.updater.state.lock().unwrap().clone();
    if state.is_null()
        && let Some(dir) = cast.config_dir.lock().unwrap().as_ref()
        && let Ok(message) = std::fs::read_to_string(dir.join("updates/last-result.txt"))
    {
        state = json!({"status":message});
    }
    Json(json!({"version":env!("CARGO_PKG_VERSION"),"available":available(),"busy":cast.updater.busy.load(Ordering::Relaxed),"update":state})).into_response()
}
async fn cancel(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if cast.updater.committed.load(Ordering::SeqCst) {
        return (
            StatusCode::CONFLICT,
            "Installation has started; server is restarting",
        )
            .into_response();
    }
    cast.updater.cancel.store(true, Ordering::Relaxed);
    Json(json!({"status":"Cancelling update; installed files are retained"})).into_response()
}
async fn install(State(cast): State<Arc<Cast>>, headers: HeaderMap) -> Response {
    if !cast.authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !available() {
        return (
            StatusCode::CONFLICT,
            "Update the desktop host from its Add-ons tab",
        )
            .into_response();
    }
    if cast.updater.busy.swap(true, Ordering::SeqCst) {
        return (StatusCode::CONFLICT, "An update is already in progress").into_response();
    }
    cast.updater.cancel.store(false, Ordering::Relaxed);
    *cast.updater.state.lock().unwrap() = json!({"status":"Checking for updates…"});
    tokio::task::spawn_blocking(move || {
        let result = prepare(&cast);
        if let Err(error) = result {
            *cast.updater.state.lock().unwrap() = json!({"status":error,"failed":true});
        }
        if !cast.updater.committed.load(Ordering::SeqCst) {
            cast.updater.busy.store(false, Ordering::SeqCst);
        }
    });
    (
        StatusCode::ACCEPTED,
        Json(json!({"status":"Checking for updates…"})),
    )
        .into_response()
}
fn prepare(cast: &Cast) -> Result<(), String> {
    let key = format!("server-{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let Some(update) =
        amp_updater::delivery::resolve(env!("CARGO_PKG_VERSION"), &key, &cast.updater.cancel)?
    else {
        *cast.updater.state.lock().unwrap() = json!({"status":"Server is up to date"});
        return Ok(());
    };
    *cast.updater.state.lock().unwrap() =
        json!({"status":format!("Downloading {}…",update.version)});
    let root = cast
        .config_dir
        .lock()
        .unwrap()
        .clone()
        .ok_or("Server settings folder unavailable")?
        .join("updates");
    let (temp, files) = amp_updater::delivery::extract(&update, &root, &cast.updater.cancel)?;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let destination = executable
        .parent()
        .ok_or("Server application folder unavailable")?
        .to_path_buf();
    let helper = if cfg!(windows) {
        "amp-server-update.exe"
    } else {
        "amp-server-update"
    };
    let server = if cfg!(windows) {
        "amp-cast.exe"
    } else {
        "amp-cast"
    };
    if !files.contains(&PathBuf::from(helper)) || !files.contains(&PathBuf::from(server)) {
        return Err("Server update package is incomplete; installed files retained".into());
    }
    // The manager remains open. Only server core files belong to this transaction.
    if files.iter().any(|p| {
        p.components().count() != 1
            || !matches!(
                p.to_str(),
                Some(
                    "amp-cast"
                        | "amp-cast.exe"
                        | "amp-server-update"
                        | "amp-server-update.exe"
                        | "amp-engine"
                        | "amp-engine.exe"
                        | "amp-prepare"
                        | "amp-prepare.exe"
                        | "build.json"
                        | "LICENSE"
                        | "THIRD_PARTY_NOTICES.md"
                )
            )
    }) {
        return Err("Server core update contains unexpected nested files".into());
    }
    let probe = tempfile::NamedTempFile::new_in(&destination)
        .map_err(|e| format!("Server folder is not writable: {e}"))?;
    drop(probe);
    let stage = temp.path().join("files");
    let runner = temp.path().join(helper);
    std::fs::copy(stage.join(helper), &runner).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    let job = temp.path().join("job.json");
    let plan = amp_updater::helper::Job {
        parent: std::process::id(),
        destination,
        stage,
        files,
        result: root.join("last-result.txt"),
        server: true,
        arguments: vec![cast.root.to_string_lossy().into_owned()],
    };
    std::fs::write(&job, serde_json::to_vec(&plan).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if cast.updater.cancel.load(Ordering::Relaxed) {
        return Err("Update cancelled; installed files retained".into());
    }
    let mut command = std::process::Command::new(runner);
    command.arg(&job);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .spawn()
        .map_err(|e| format!("Could not start updater: {e}"))?;
    let _ = temp.keep();
    cast.updater.committed.store(true, Ordering::SeqCst);
    *cast.updater.state.lock().unwrap() =
        json!({"status":"Installing update and restarting the server…"});
    cast.updater.exit.notify_one();
    Ok(())
}
// Bound HTTP draining: a radio listener can otherwise keep its stream open forever
// and prevent the verified helper from replacing the exited server process.
async fn drain(
    listener: tokio::net::TcpListener,
    app: Router,
    signal: impl std::future::Future<Output = ()> + Send + 'static,
    grace: std::time::Duration,
) -> std::io::Result<()> {
    let started = Arc::new(tokio::sync::Notify::new());
    let notify = started.clone();
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        signal.await;
        notify.notify_one();
    });
    let serving = std::future::IntoFuture::into_future(server);
    tokio::pin!(serving);
    tokio::select! {
        result=&mut serving=>result,
        _=started.notified()=>match tokio::time::timeout(grace,&mut serving).await {Ok(result)=>result,Err(_)=>Ok(())}
    }
}
// A Windows server launched without a console has no Ctrl+C source. Treat an
// unavailable optional signal as disabled, never as a request to stop serving.
async fn optional_signal<E>(signal: impl std::future::Future<Output = Result<(), E>>) {
    if signal.await.is_err() {
        std::future::pending::<()>().await;
    }
}
pub async fn serve(listener: tokio::net::TcpListener, cast: Arc<Cast>) -> std::io::Result<()> {
    let app = super::router(cast.clone());
    drain(listener,app,async move {
        #[cfg(unix)] {
            let mut terminate=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM handler");
            tokio::select!{_=optional_signal(tokio::signal::ctrl_c())=>{},_=terminate.recv()=>{},_=cast.updater.exit.notified()=>{}}
        }
        #[cfg(not(unix))]
        tokio::select!{_=optional_signal(tokio::signal::ctrl_c())=>{},_=cast.updater.exit.notified()=>{}};
        cast.analysis.stop();cast.shutdown().await;
    },std::time::Duration::from_secs(3)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;
    #[tokio::test]
    async fn cross_site_and_wrong_password_cannot_start_an_update() {
        for (name, value) in [
            ("sec-fetch-site", "cross-site"),
            ("x-amp-password", "wrong"),
        ] {
            let root = tempfile::tempdir().unwrap();
            let cast = Cast::new(
                root.path().into(),
                amp_core::scan(root.path()).unwrap(),
                Some("private".into()),
            );
            let app = super::super::router(cast.clone());
            let request = axum::http::Request::builder()
                .method("POST")
                .uri("/api/update/install")
                .header(name, value)
                .body(Body::empty())
                .unwrap();
            assert_eq!(
                app.oneshot(request).await.unwrap().status(),
                StatusCode::UNAUTHORIZED
            );
            assert!(!cast.updater.busy.load(Ordering::Relaxed));
        }
    }
    #[tokio::test]
    async fn held_stream_cannot_block_a_server_update_forever() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let app = Router::new().route(
            "/held",
            get(|| async {
                Body::from_stream(async_stream::stream! {
                    yield Ok::<Bytes,std::convert::Infallible>(Bytes::from_static(b"ready"));
                    std::future::pending::<()>().await;
                })
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(drain(
            listener,
            app,
            async move {
                let _ = rx.await;
            },
            std::time::Duration::from_millis(50),
        ));
        let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
        client
            .write_all(b"GET /held HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let mut bytes = [0; 1024];
        assert!(client.read(&mut bytes).await.unwrap() > 0);
        tx.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    #[tokio::test]
    async fn missing_console_cannot_trigger_server_shutdown() {
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(5),
                optional_signal(std::future::ready(Err::<(), ()>(())))
            )
            .await
            .is_err()
        );
    }
}
