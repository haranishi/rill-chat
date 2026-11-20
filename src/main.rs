use axum::serve::ListenerExt;
use std::{env, path::PathBuf};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind = env::var("RILL_BIND").unwrap_or_else(|_| "127.0.0.1:3847".into());
    let db = env::var_os("RILL_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|| "data/rill.sqlite3".into());
    let secure = match env::var("RILL_ENV").as_deref() {
        Ok("production") => true,
        Ok("development") | Err(env::VarError::NotPresent) => false,
        _ => return Err("RILL_ENV must be development or production".into()),
    };
    let service = rill_chat::service(&db, secure)?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    println!("Rill listening on http://{}", listener.local_addr()?);
    let listener = listener.tap_io(|socket| {
        if let Err(err) = socket.set_nodelay(true) {
            eprintln!("TCP_NODELAY unavailable ({:?})", err.kind());
        }
    });
    axum::serve(listener, service.router)
        .with_graceful_shutdown(async move {
            stop_signal().await;
            service.shutdown.send_replace(true);
        })
        .await?;
    Ok(())
}

async fn stop_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("register termination handler");
        tokio::select! { _=tokio::signal::ctrl_c()=>{}, _=term.recv()=>{} }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
