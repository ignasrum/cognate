use cognate_api::{api, auth, state::AppState};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

pub(crate) struct EmbeddedApiRuntime {
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl EmbeddedApiRuntime {
    pub(crate) fn start(notebook_path: PathBuf) -> Result<Self, String> {
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let thread = thread::Builder::new()
            .name("cognate-embedded-api".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_sender.send(Err(format!(
                            "failed to build embedded API runtime: {error}"
                        )));
                        return;
                    }
                };

                runtime.block_on(async move {
                    let state = AppState::new_in_memory(notebook_path);
                    let secret = match auth::generate_client_secret() {
                        Ok(secret) => secret,
                        Err(error) => {
                            let _ = ready_sender.send(Err(error.to_string()));
                            return;
                        }
                    };
                    let id = match auth::new_client_id() {
                        Ok(id) => id,
                        Err(error) => {
                            let _ = ready_sender.send(Err(error.to_string()));
                            return;
                        }
                    };
                    if state
                        .create_in_memory_client(id, "embedded-ui".to_string(), &secret)
                        .await
                        .is_none()
                    {
                        let _ = ready_sender.send(Err(
                            "embedded API did not initialize in-memory authentication".to_string(),
                        ));
                        return;
                    }

                    let listener = match TcpListener::bind(("127.0.0.1", 0)).await {
                        Ok(listener) => listener,
                        Err(error) => {
                            let _ = ready_sender
                                .send(Err(format!("failed to bind embedded API: {error}")));
                            return;
                        }
                    };
                    let address = match listener.local_addr() {
                        Ok(address) => address,
                        Err(error) => {
                            let _ = ready_sender.send(Err(format!(
                                "failed to determine embedded API address: {error}"
                            )));
                            return;
                        }
                    };
                    let router = api::router(state);
                    let _ = ready_sender.send(Ok((address, secret)));

                    let server = axum::serve(listener, router).with_graceful_shutdown(async {
                        let _ = shutdown_receiver.await;
                    });
                    if let Err(error) = server.await {
                        eprintln!("[cognate] embedded API stopped: {error}");
                    }
                });
            })
            .map_err(|error| format!("failed to start embedded API thread: {error}"))?;

        let (address, api_key) = ready_receiver
            .recv()
            .map_err(|_| "embedded API exited before becoming ready".to_string())??;
        Ok(Self {
            base_url: format!("http://{address}"),
            api_key,
            shutdown: Some(shutdown_sender),
            thread: Some(thread),
        })
    }
}

impl Drop for EmbeddedApiRuntime {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EmbeddedApiRuntime;
    use tempfile::TempDir;

    #[ignore = "requires permission to bind a loopback socket"]
    #[tokio::test]
    async fn embedded_api_uses_loopback_and_ephemeral_auth_without_sqlite() {
        let notebook = TempDir::new().unwrap();
        let runtime = EmbeddedApiRuntime::start(notebook.path().to_path_buf()).unwrap();
        assert!(runtime.base_url.starts_with("http://127.0.0.1:"));
        assert!(runtime.api_key.starts_with("cgnt_live_"));
        assert!(!notebook.path().join("cognate-api.sqlite").exists());

        let response = reqwest::Client::new()
            .get(format!("{}/v1/health", runtime.base_url))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }
}
