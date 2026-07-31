use std::net::SocketAddr;
use std::path::PathBuf;

use axum::Router;
use tokio::net::TcpListener;

use crate::{api, auth, error::ApiError, state::AppState};

pub struct EmbeddedServer {
    pub address: SocketAddr,
    pub client_secret: String,
    listener: TcpListener,
    router: Router,
}

pub async fn bind_embedded(notebook_path: PathBuf) -> Result<EmbeddedServer, ApiError> {
    let state = AppState::new_in_memory(notebook_path);
    let client_secret = auth::generate_client_secret()?;
    let client_id = auth::new_client_id()?;
    state
        .create_in_memory_client(client_id, "embedded-ui".to_string(), &client_secret)
        .await
        .ok_or_else(|| {
            ApiError::Config("embedded API did not initialize in-memory authentication".to_string())
        })?;

    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    Ok(EmbeddedServer {
        address,
        client_secret,
        listener,
        router: api::router(state),
    })
}

pub async fn serve(
    server: EmbeddedServer,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), ApiError> {
    serve_listener(server.listener, server.router, shutdown).await
}

pub async fn serve_listener(
    listener: TcpListener,
    router: Router,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), ApiError> {
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(ApiError::Io)
}
