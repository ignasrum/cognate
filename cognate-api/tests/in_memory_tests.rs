use axum::{body::Body, http::Request};
use cognate_api::{api, auth, state::AppState};
use http_body_util::BodyExt;
use tempfile::TempDir;
use tower::ServiceExt;

#[tokio::test]
async fn embedded_auth_is_in_memory_and_does_not_need_sqlite() {
    let notebook = TempDir::new().unwrap();
    let state = AppState::new_in_memory(notebook.path().to_path_buf());
    assert!(state.db.is_none());

    let secret = auth::generate_client_secret().unwrap();
    state
        .create_in_memory_client(
            "embedded-client".to_string(),
            "embedded-ui".to_string(),
            &secret,
        )
        .await
        .expect("in-memory state should provision clients");

    let response = api::router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], br#"{"status":"ok"}"#);
}

#[tokio::test]
async fn in_memory_client_secret_is_not_reusable_after_store_drop() {
    let notebook = TempDir::new().unwrap();
    let state = AppState::new_in_memory(notebook.path().to_path_buf());
    let secret = auth::generate_client_secret().unwrap();
    state
        .create_in_memory_client("ephemeral".to_string(), "test".to_string(), &secret)
        .await
        .unwrap();
    drop(state);

    let next_state = AppState::new_in_memory(notebook.path().to_path_buf());
    assert!(next_state.db.is_none());
    let response = api::router(next_state)
        .oneshot(
            Request::builder()
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {secret}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}
