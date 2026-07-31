use std::net::SocketAddr;

use cognate_api::{api, config::Config, db, error::ApiError, state::AppState};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), ApiError> {
    let config = Config::from_env()?;
    let pool = db::connect(&config.database_path).await?;
    db::migrate(&pool).await?;
    let state = AppState::new(pool, config.notebook_path, config.admin_token);
    let router = api::router(state);
    let address: SocketAddr = format!("{}:{}", config.bind_address, config.port)
        .parse()
        .map_err(|error| ApiError::Config(format!("invalid bind address or port: {error}")))?;

    let listener = TcpListener::bind(address).await.map_err(ApiError::Io)?;
    eprintln!("cognate-api listening on http://{address}");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(ApiError::Io)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
