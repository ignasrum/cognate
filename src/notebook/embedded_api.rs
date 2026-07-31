use cognate_api::server;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
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
                    let embedded = match server::bind_embedded(notebook_path).await {
                        Ok(server) => server,
                        Err(error) => {
                            let _ = ready_sender.send(Err(error.to_string()));
                            return;
                        }
                    };
                    let _ =
                        ready_sender.send(Ok((embedded.address, embedded.client_secret.clone())));
                    if let Err(error) = server::serve(embedded, async {
                        let _ = shutdown_receiver.await;
                    })
                    .await
                    {
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

    #[tokio::test]
    async fn embedded_api_uses_loopback_and_ephemeral_auth_without_sqlite() {
        let notebook = TempDir::new().unwrap();
        let runtime = match EmbeddedApiRuntime::start(notebook.path().to_path_buf()) {
            Ok(runtime) => runtime,
            Err(error)
                if (error.contains("Operation not permitted")
                    || error.contains("Permission denied"))
                    && std::env::var_os("COGNATE_REQUIRE_SOCKET_TESTS").is_none() =>
            {
                eprintln!("skipping socket test: {error}");
                return;
            }
            Err(error) => panic!("embedded API should start: {error}"),
        };
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
