mod common;

use axum::body::Body;
use common::{TestApp, bearer_request};
use http_body_util::BodyExt;

#[tokio::test]
async fn health_is_public_but_does_not_authenticate_clients() {
    let app = TestApp::new().await;
    let response = app
        .request(
            axum::http::Request::builder()
                .method("GET")
                .uri("/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), br#"{"status":"ok"}"#);
}

#[tokio::test]
async fn search_rejects_queries_over_the_api_limit() {
    let app = TestApp::new().await;
    let client = app.provision("search-limits").await;
    let query = "x".repeat(257);
    let response = app
        .request(bearer_request(
            "GET",
            &format!("/v1/search?q={query}"),
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn note_creation_rejects_absolute_and_traversal_paths() {
    let app = TestApp::new().await;
    let client = app.provision("path-validation").await;

    for rel_path in ["../outside", "/absolute", "folder/../../outside"] {
        let response = app
            .request(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/v1/notes")
                    .header("authorization", format!("Bearer {}", client.secret))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"rel_path": rel_path}).to_string(),
                    ))
                    .unwrap(),
            )
            .await;
        assert_eq!(
            response.status(),
            400,
            "path should be rejected: {rel_path}"
        );
    }
}

#[tokio::test]
async fn note_move_and_delete_are_persisted_through_the_api() {
    let app = TestApp::new().await;
    let client = app.provision("lifecycle-persistence").await;

    let create = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rel_path":"before/move"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(create.status(), 201);

    let move_response = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes/move")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"from_rel_path":"before/move","to_rel_path":"after/move"}"#,
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(move_response.status(), 200);

    let after_move = app
        .request(bearer_request(
            "GET",
            "/v1/notes",
            &client.secret,
            Body::empty(),
        ))
        .await;
    let body = after_move.into_body().collect().await.unwrap().to_bytes();
    let notes: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let paths: Vec<&str> = notes
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|note| note["rel_path"].as_str())
        .collect();
    assert_eq!(paths, vec!["after/move"]);

    let delete_response = app
        .request(bearer_request(
            "DELETE",
            "/v1/notes/after/move",
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(delete_response.status(), 204);

    let after_delete = app
        .request(bearer_request(
            "GET",
            "/v1/notes",
            &client.secret,
            Body::empty(),
        ))
        .await;
    let body = after_delete.into_body().collect().await.unwrap().to_bytes();
    let notes: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(notes.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn wildcard_attachment_delete_is_an_explicit_unconditional_operation() {
    let app = TestApp::new().await;
    let client = app.provision("wildcard-delete").await;
    let create = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rel_path":"delete-image"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(create.status(), 201);

    let upload = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/attachments?note=delete-image")
                .header("authorization", format!("Bearer {}", client.secret))
                .body(Body::from(vec![
                    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A,
                ]))
                .unwrap(),
        )
        .await;
    assert_eq!(upload.status(), 201);
    let body = upload.into_body().collect().await.unwrap().to_bytes();
    let metadata: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let path = metadata["rel_path"].as_str().unwrap();
    let uri = format!("/v1/attachments/delete-image/{path}");

    let deleted = app
        .request(
            axum::http::Request::builder()
                .method("DELETE")
                .uri(&uri)
                .header("authorization", format!("Bearer {}", client.secret))
                .header("if-match", "*")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(deleted.status(), 204);
}
