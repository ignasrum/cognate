mod common;

use axum::{body::Body, http::Request};
use common::TestApp;

#[tokio::test]
async fn wrong_admin_token_cannot_provision_or_list_clients() {
    let app = TestApp::new().await;
    for method in ["POST", "GET"] {
        let response = app
            .request(
                Request::builder()
                    .method(method)
                    .uri("/v1/admin/clients")
                    .header("x-admin-token", "wrong")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"client_name":"blocked"}"#))
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), 401);
    }
}

#[tokio::test]
async fn client_listing_excludes_secrets_and_revocation_is_idempotently_visible() {
    let app = TestApp::new().await;
    let client = app.provision("list-and-revoke").await;

    let listed = app
        .json(
            Request::builder()
                .uri("/v1/admin/clients")
                .header("x-admin-token", "admin-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(listed[0].get("secret").is_none());
    assert!(listed[0].get("key_hash").is_none());

    let revoke = app
        .request(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/admin/clients/{}", client.id))
                .header("x-admin-token", "admin-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(revoke.status(), 204);

    let second_revoke = app
        .request(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/admin/clients/{}", client.id))
                .header("x-admin-token", "admin-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(second_revoke.status(), 404);
}

#[tokio::test]
async fn invalid_client_names_are_rejected() {
    let app = TestApp::new().await;
    for name in ["", "   ", &"x".repeat(129)] {
        let response = app
            .request(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/clients")
                    .header("x-admin-token", "admin-secret")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"client_name": name}).to_string(),
                    ))
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), 400);
    }
}
