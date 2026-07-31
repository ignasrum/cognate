mod common;

use axum::body::Body;
use common::{TestApp, bearer_request};
use http_body_util::BodyExt;

#[tokio::test]
async fn admin_endpoints_require_the_admin_token() {
    let app = TestApp::new().await;
    let client = app.provision("admin-auth-tests").await;

    let create_without_admin = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/admin/clients")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"client_name":"unauthorized"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(create_without_admin.status(), 401);

    let list_with_client_key = app
        .request(
            axum::http::Request::builder()
                .method("GET")
                .uri("/v1/admin/clients")
                .header("authorization", format!("Bearer {}", client.secret))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(list_with_client_key.status(), 401);
}

#[tokio::test]
async fn revoking_an_unknown_client_returns_not_found() {
    let app = TestApp::new().await;
    let response = app
        .request(
            axum::http::Request::builder()
                .method("DELETE")
                .uri("/v1/admin/clients/does-not-exist")
                .header("x-admin-token", "admin-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn wildcard_metadata_update_is_rejected_after_note_initialization() {
    let app = TestApp::new().await;
    let client = app.provision("metadata-wildcard").await;
    let create = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rel_path":"wildcard-note"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(create.status(), 201);

    let update = app
        .request(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/v1/metadata")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("if-match", "*")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"[{"rel_path":"wildcard-note","labels":["initialized"],"last_updated":null}]"#,
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(update.status(), 409);

    let notes = app
        .request(bearer_request(
            "GET",
            "/v1/notes",
            &client.secret,
            Body::empty(),
        ))
        .await;
    let body = notes.into_body().collect().await.unwrap().to_bytes();
    let notes: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(notes[0]["labels"], serde_json::json!([]));
}

#[tokio::test]
async fn an_existing_note_can_have_an_empty_attachment_listing() {
    let app = TestApp::new().await;
    let client = app.provision("empty-attachments").await;
    let create = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rel_path":"no-images"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(create.status(), 201);

    let response = app
        .request(bearer_request(
            "GET",
            "/v1/attachments?note=no-images",
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        serde_json::json!([])
    );
}

#[tokio::test]
async fn empty_search_query_returns_a_valid_result() {
    let app = TestApp::new().await;
    let client = app.provision("empty-search").await;
    let response = app
        .request(bearer_request(
            "GET",
            "/v1/search?q=",
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload, serde_json::json!([]));
}

#[tokio::test]
async fn paginated_search_returns_cursor_contract() {
    let app = TestApp::new().await;
    let client = app.provision("search-page").await;
    let response = app
        .request(bearer_request(
            "GET",
            "/v1/search/page?q=",
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(payload["results"].is_array());
    assert_eq!(payload["total"], 0);
    assert!(payload.get("next_cursor").is_some());
}

#[tokio::test]
async fn paginated_search_rejects_a_malformed_cursor() {
    let app = TestApp::new().await;
    let client = app.provision("search-cursor").await;
    let response = app
        .request(bearer_request(
            "GET",
            "/v1/search/page?q=term&cursor=not-a-cursor",
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn paginated_search_returns_stable_validation_error_codes() {
    let app = TestApp::new().await;
    let client = app.provision("search-errors").await;
    let response = app
        .request(bearer_request(
            "GET",
            "/v1/search/page?q=%22unfinished",
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(response.status(), 400);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["error"], "invalid_query");
}

#[tokio::test]
async fn client_name_length_boundary_is_enforced() {
    let app = TestApp::new().await;
    let valid = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/admin/clients")
                .header("x-admin-token", "admin-secret")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"client_name": "v".repeat(128)}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(valid.status(), 201);

    let invalid = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/admin/clients")
                .header("x-admin-token", "admin-secret")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"client_name": "x".repeat(129)}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(invalid.status(), 400);
}

#[tokio::test]
async fn revocation_is_not_reported_as_success_twice() {
    let app = TestApp::new().await;
    let client = app.provision("revocation-idempotency").await;
    for (expected, method) in [(204, "first"), (404, "second")] {
        let response = app
            .request(
                axum::http::Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/admin/clients/{}", client.id))
                    .header("x-admin-token", "admin-secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), expected, "{method} revocation");
    }
}

#[tokio::test]
async fn unicode_note_content_round_trips_with_a_blake3_revision() {
    let app = TestApp::new().await;
    let client = app.provision("unicode-content").await;
    let create = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rel_path":"unicode-note"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(create.status(), 201);

    let content = "Hei, 世界 🌍 — café";
    let save = app
        .request(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/v1/notes/unicode-note")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("if-match", "*")
                .body(Body::from(content))
                .unwrap(),
        )
        .await;
    assert_eq!(save.status(), 204);

    let read = app
        .request(bearer_request(
            "GET",
            "/v1/notes/unicode-note",
            &client.secret,
            Body::empty(),
        ))
        .await;
    let body = read.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["content"], content);
    assert_eq!(payload["revision"].as_str().unwrap().len(), 64);
}

#[tokio::test]
async fn attachment_note_paths_are_validated_before_filesystem_access() {
    let app = TestApp::new().await;
    let client = app.provision("attachment-path-validation").await;
    let response = app
        .request(bearer_request(
            "GET",
            "/v1/attachments?note=../outside",
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn jpeg_attachment_downloads_with_jpeg_media_type() {
    let app = TestApp::new().await;
    let client = app.provision("jpeg-media-type").await;
    let create = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rel_path":"jpeg-note"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(create.status(), 201);

    let upload = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/attachments?note=jpeg-note")
                .header("authorization", format!("Bearer {}", client.secret))
                .body(Body::from(vec![0xff, 0xd8, 0xff, 0x00]))
                .unwrap(),
        )
        .await;
    assert_eq!(upload.status(), 201);
    let body = upload.into_body().collect().await.unwrap().to_bytes();
    let metadata: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let path = metadata["rel_path"].as_str().unwrap();
    let response = app
        .request(bearer_request(
            "GET",
            &format!("/v1/attachments/jpeg-note/{path}"),
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "image/jpeg"
    );
}
