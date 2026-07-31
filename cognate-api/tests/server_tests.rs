use cognate_api::server;
use tempfile::TempDir;
use tokio::sync::oneshot;

#[tokio::test]
async fn embedded_server_starts_without_sqlite_and_releases_its_port() {
    let notebook = TempDir::new().unwrap();
    let embedded = match server::bind_embedded(notebook.path().to_path_buf()).await {
        Ok(server) => server,
        Err(error)
            if (error.to_string().contains("Operation not permitted")
                || error.to_string().contains("Permission denied"))
                && std::env::var_os("COGNATE_REQUIRE_SOCKET_TESTS").is_none() =>
        {
            eprintln!("skipping socket test: {error}");
            return;
        }
        Err(error) => panic!("embedded server should bind: {error}"),
    };
    let address = embedded.address;
    let secret = embedded.client_secret.clone();
    assert!(address.ip().is_loopback());
    assert!(secret.starts_with("cgnt_live_"));
    assert!(!notebook.path().join("cognate-api.sqlite").exists());

    let (shutdown_sender, shutdown_receiver) = oneshot::channel();
    let task = tokio::spawn(server::serve(embedded, async {
        let _ = shutdown_receiver.await;
    }));

    let client = reqwest::Client::new();
    let health = client
        .get(format!("http://{address}/v1/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(health.status(), 200);
    let notes = client
        .get(format!("http://{address}/v1/notes"))
        .bearer_auth(&secret)
        .send()
        .await
        .unwrap();
    assert_eq!(notes.status(), 200);

    shutdown_sender.send(()).unwrap();
    task.await.unwrap().unwrap();

    let rebound = tokio::net::TcpListener::bind(address).await.unwrap();
    drop(rebound);
}
