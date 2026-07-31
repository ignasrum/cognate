mod common;

use axum::body::Body;
use common::{TestApp, bearer_request};

#[tokio::test]
async fn every_protected_route_requires_a_client_key() {
    let app = TestApp::new().await;
    let protected_requests = [
        ("GET", "/v1/notes"),
        ("GET", "/v1/notes/note"),
        ("GET", "/v1/attachments?note=note"),
        ("GET", "/v1/attachments/note/images/image.png"),
        ("GET", "/v1/search?q=test"),
    ];

    for (method, uri) in protected_requests {
        let response = app
            .request(
                axum::http::Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), 401, "{method} {uri} must require auth");
    }
}

#[tokio::test]
async fn revoking_a_client_immediately_blocks_existing_credentials() {
    let app = TestApp::new().await;
    let client = app.provision("revocation-security").await;

    let before = app
        .request(bearer_request(
            "GET",
            "/v1/notes",
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(before.status(), 200);

    let revoke = app
        .request(
            axum::http::Request::builder()
                .method("DELETE")
                .uri(format!("/v1/admin/clients/{}", client.id))
                .header("x-admin-token", "admin-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(revoke.status(), 204);

    let after = app
        .request(bearer_request(
            "GET",
            "/v1/notes",
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(after.status(), 401);
}

#[tokio::test]
async fn malformed_note_and_attachment_payloads_are_rejected() {
    let app = TestApp::new().await;
    let client = app.provision("payload-security").await;

    let invalid_utf8 = app
        .request(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/v1/notes/invalid-utf8")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("if-match", "*")
                .body(Body::from(vec![0xff, 0xfe]))
                .unwrap(),
        )
        .await;
    assert_eq!(invalid_utf8.status(), 400);

    let invalid_image = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/attachments?note=payload-security")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "image/png")
                .body(Body::from("not an image"))
                .unwrap(),
        )
        .await;
    assert_eq!(invalid_image.status(), 400);
}

#[tokio::test]
async fn metadata_write_requires_if_match_even_for_an_empty_notebook() {
    let app = TestApp::new().await;
    let client = app.provision("metadata-precondition-security").await;

    let response = app
        .request(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/v1/metadata")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "application/json")
                .body(Body::from("[]"))
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), 428);
}

#[tokio::test]
async fn admin_credentials_cannot_be_used_as_client_credentials() {
    let app = TestApp::new().await;
    let response = app
        .request(bearer_request(
            "GET",
            "/v1/notes",
            "admin-secret",
            Body::empty(),
        ))
        .await;
    assert_eq!(response.status(), 401);
}
