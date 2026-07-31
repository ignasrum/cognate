use cognate_api::server;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tokio::sync::oneshot;

#[derive(Debug, thiserror::Error)]
pub(crate) enum EmbeddedApiError {
    #[error("failed to initialize embedded API runtime: {0}")]
    RuntimeInit(String),
    #[error("failed to bind embedded API: {0}")]
    Bind(String),
    #[error("embedded API startup channel closed")]
    StartupChannelClosed,
    #[error("embedded API thread could not be started: {0}")]
    ThreadSpawn(String),
    #[error("embedded API thread panicked during shutdown")]
    ThreadPanicked,
    #[error("embedded API shutdown timed out")]
    ShutdownTimeout,
    #[error("embedded API stopped with an error: {0}")]
    Server(String),
}

pub(crate) struct EmbeddedApiRuntime {
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    shutdown: Option<oneshot::Sender<()>>,
    completion: Option<Mutex<Receiver<Result<(), EmbeddedApiError>>>>,
    thread: Option<JoinHandle<()>>,
}

impl EmbeddedApiRuntime {
    pub(crate) fn start(notebook_path: PathBuf) -> Result<Self, EmbeddedApiError> {
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let (completion_sender, completion_receiver) = mpsc::sync_channel(1);
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
                        let failure = EmbeddedApiError::RuntimeInit(error.to_string());
                        let _ = ready_sender.send(Err(failure.to_string()));
                        let _ = completion_sender.send(Err(failure));
                        return;
                    }
                };

                runtime.block_on(async move {
                    let embedded = match server::bind_embedded(notebook_path).await {
                        Ok(server) => server,
                        Err(error) => {
                            let failure = EmbeddedApiError::Bind(error.to_string());
                            let _ = ready_sender.send(Err(failure.to_string()));
                            let _ = completion_sender.send(Err(failure));
                            return;
                        }
                    };
                    let _ =
                        ready_sender.send(Ok((embedded.address, embedded.client_secret.clone())));
                    let result = server::serve(embedded, async {
                        let _ = shutdown_receiver.await;
                    })
                    .await
                    .map_err(|error| EmbeddedApiError::Server(error.to_string()));
                    let _ = completion_sender.send(result);
                });
            })
            .map_err(|error| EmbeddedApiError::ThreadSpawn(error.to_string()))?;

        let (address, api_key) = ready_receiver
            .recv()
            .map_err(|_| EmbeddedApiError::StartupChannelClosed)?
            .map_err(EmbeddedApiError::Server)?;
        Ok(Self {
            base_url: format!("http://{address}"),
            api_key,
            shutdown: Some(shutdown_sender),
            completion: Some(Mutex::new(completion_receiver)),
            thread: Some(thread),
        })
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), String> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(completion) = self.completion.as_ref() {
            let outcome = {
                let completion = completion
                    .lock()
                    .map_err(|_| "embedded API completion channel was poisoned".to_string())?;
                completion.recv_timeout(Duration::from_secs(5))
            };
            match outcome {
                Ok(result) => {
                    self.completion.take();
                    if let Some(thread) = self.thread.take() {
                        thread
                            .join()
                            .map_err(|_| EmbeddedApiError::ThreadPanicked.to_string())?;
                    }
                    result.map_err(|error| error.to_string())?;
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(EmbeddedApiError::ShutdownTimeout.to_string());
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.completion.take();
                    if let Some(thread) = self.thread.take() {
                        thread
                            .join()
                            .map_err(|_| EmbeddedApiError::ThreadPanicked.to_string())?;
                    }
                }
            }
        } else if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| EmbeddedApiError::ThreadPanicked.to_string())?;
        }
        Ok(())
    }
}

impl Drop for EmbeddedApiRuntime {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::EmbeddedApiRuntime;
    use tempfile::TempDir;

    #[tokio::test]
    async fn embedded_api_uses_loopback_and_ephemeral_auth_without_sqlite() {
        let notebook = TempDir::new().unwrap();
        let mut runtime = match EmbeddedApiRuntime::start(notebook.path().to_path_buf()) {
            Ok(runtime) => runtime,
            Err(error)
                if (error.to_string().contains("Operation not permitted")
                    || error.to_string().contains("Permission denied"))
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
        runtime.shutdown().unwrap();
        runtime.shutdown().unwrap();
    }
}
