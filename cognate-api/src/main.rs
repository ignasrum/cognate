use cognate_api::{config::Config, db, error::ApiError, server};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), ApiError> {
    let config = Config::from_env()?;
    let pool = db::connect(&config.database_path).await?;
    db::migrate(&pool).await?;
    let address: std::net::SocketAddr = format!("{}:{}", config.bind_address, config.port)
        .parse()
        .map_err(|error| ApiError::Config(format!("invalid bind address or port: {error}")))?;

    let listener = TcpListener::bind(address).await.map_err(ApiError::Io)?;
    let state = cognate_api::state::AppState::new(pool, config.notebook_path, config.admin_token);
    let router = cognate_api::api::router(state);
    eprintln!("cognate-api listening on http://{address}");
    server::serve_listener(listener, router, shutdown_signal()).await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {}
            Err(error) => {
                eprintln!("failed to install Ctrl-C handler: {error}");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                eprintln!("failed to install SIGTERM handler: {error}");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
