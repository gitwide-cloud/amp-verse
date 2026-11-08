use std::{net::SocketAddr, path::PathBuf};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("Usage: amp-cast <music-folder>; AMP_CAST_BIND=0.0.0.0:8787 enables LAN")?,
    )
    .canonicalize()?;
    let library = amp_core::scan(&root)?;
    let address: SocketAddr = std::env::var("AMP_CAST_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8787".into())
        .parse()?;
    let password = std::env::var("AMP_CAST_PASSWORD")
        .ok()
        .filter(|s| !s.is_empty());
    let config_dir = std::env::var_os("AMP_CAST_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or(amp_core::coordination::device_config_dir()?.join("cast"));
    let first_start = !config_dir.join("server.json").exists();
    let config = amp_cast::config::Config::load(&config_dir)?;
    amp_core::coordination::device_id()?;
    amp_core::coordination::library_id(&root)?;
    let cast = amp_cast::Cast::with_identity(
        root,
        library,
        password,
        config.server_id,
        config.station_id,
        config.station_name,
    );
    *cast.config_dir.lock().unwrap() = Some(config_dir);
    let enabled = if first_start {
        std::env::var("AMP_ANALYSIS_ENABLED")
            .map_or(config.analysis_enabled, |v| v == "true" || v == "1")
    } else {
        config.analysis_enabled
    };
    if first_start {
        amp_cast::config::Config::set_analysis(
            cast.config_dir.lock().unwrap().as_ref().unwrap(),
            enabled,
        )?;
    }
    let workers = std::env::var("AMP_ANALYSIS_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(config.analysis_workers);
    cast.analysis
        .enabled
        .store(enabled, std::sync::atomic::Ordering::Relaxed);
    let listener = tokio::net::TcpListener::bind(address).await?;
    cast.analysis.start(
        &cast,
        workers,
        config.scan_interval_seconds,
        config.pause_analysis_during_radio,
    );
    println!(
        "AMP Cast prototype listening on http://{}",
        listener.local_addr()?
    );
    axum::serve(listener, amp_cast::router(cast.clone()))
        .with_graceful_shutdown(async move {
            #[cfg(unix)]
            {
                let mut terminate =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("SIGTERM handler");
                tokio::select! { _=tokio::signal::ctrl_c()=>{}, _=terminate.recv()=>{} }
            }
            #[cfg(not(unix))]
            let _ = tokio::signal::ctrl_c().await;
            cast.analysis.stop();
            cast.shutdown().await;
        })
        .await?;
    Ok(())
}
