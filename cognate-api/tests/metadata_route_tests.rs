mod common;

use axum::body::Body;
use common::{TestApp, bearer_request};
use http_body_util::BodyExt;

#[tokio::test]
async fn metadata_writes_use_etags_and_reject_stale_updates() {
    let app = TestApp::new().await;
    let client = app.provision("metadata-tests").await;

    let create = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rel_path":"metadata-note"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(create.status(), 201);

    let loaded = app
        .request(bearer_request(
            "GET",
            "/v1/notes",
            &client.secret,
            Body::empty(),
        ))
        .await;
    let etag = loaded.headers().get("etag").unwrap().clone();
    let notes = serde_json::json!([{
        "rel_path": "metadata-note",
        "labels": ["api", "updated"],
        "last_updated": null
    }]);

    let update = app
        .request(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/v1/metadata")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("if-match", etag.clone())
                .header("content-type", "application/json")
                .body(Body::from(notes.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(update.status(), 204);

    let stale = app
        .request(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/v1/metadata")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("if-match", etag)
                .header("content-type", "application/json")
                .body(Body::from(notes.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(stale.status(), 409);

    let refreshed = app
        .request(bearer_request(
            "GET",
            "/v1/notes",
            &client.secret,
            Body::empty(),
        ))
        .await;
    let body = refreshed.into_body().collect().await.unwrap().to_bytes();
    let metadata: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(metadata[0]["labels"], serde_json::json!(["api", "updated"]));
}

#[tokio::test]
async fn metadata_updates_cannot_add_or_remove_note_paths() {
    let app = TestApp::new().await;
    let client = app.provision("metadata-structure").await;
    let create = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rel_path":"tracked"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(create.status(), 201);

    let loaded = app
        .request(bearer_request(
            "GET",
            "/v1/notes",
            &client.secret,
            Body::empty(),
        ))
        .await;
    let etag = loaded.headers().get("etag").unwrap().clone();
    let invalid = serde_json::json!([{
        "rel_path": "not-on-disk",
        "labels": [],
        "last_updated": null
    }]);
    let response = app
        .request(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/v1/metadata")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("if-match", etag)
                .header("content-type", "application/json")
                .body(Body::from(invalid.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), 400);

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
    assert_eq!(notes[0]["rel_path"], "tracked");
}
