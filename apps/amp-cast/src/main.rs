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
    let cast = amp_cast::Cast::new(root, library, password);
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!(
        "AMP Cast prototype listening on http://{}",
        listener.local_addr()?
    );
    axum::serve(listener, amp_cast::router(cast.clone()))
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cast.shutdown().await;
        })
        .await?;
    Ok(())
}
