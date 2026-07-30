mod common;

use axum::body::Body;
use common::{TestApp, bearer_request};
use http_body_util::BodyExt;

#[tokio::test]
async fn note_lifecycle_and_move_routes_use_the_engine() {
    let app = TestApp::new().await;
    let client = app.provision("route-tests").await;

    let create = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rel_path":"draft/summary"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(create.status(), 201);

    let save = app
        .request(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/v1/notes/draft/summary")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("if-match", "*")
                .body(Body::from("AI summary"))
                .unwrap(),
        )
        .await;
    assert_eq!(save.status(), 204);

    let move_response = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes/move")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"from_rel_path":"draft/summary","to_rel_path":"final/summary"}"#,
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(move_response.status(), 200);

    let read = app
        .request(bearer_request(
            "GET",
            "/v1/notes/final/summary",
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(read.status(), 200);

    let delete = app
        .request(bearer_request(
            "DELETE",
            "/v1/notes/final/summary",
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(delete.status(), 204);
}

#[tokio::test]
async fn path_traversal_and_oversized_note_payloads_are_rejected() {
    let app = TestApp::new().await;
    let client = app.provision("validation-tests").await;

    let traversal = app
        .request(bearer_request(
            "GET",
            "/v1/notes/../outside",
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert!(matches!(traversal.status().as_u16(), 400 | 404));

    let oversized = app
        .request(bearer_request(
            "PUT",
            "/v1/notes/large",
            &client.secret,
            Body::from(vec![b'x'; 5 * 1024 * 1024]),
        ))
        .await;
    assert!(matches!(oversized.status().as_u16(), 400 | 413));
}

#[tokio::test]
async fn stale_note_revision_is_rejected_without_overwriting_server_content() {
    let app = TestApp::new().await;
    let client = app.provision("revision-tests").await;

    let create = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rel_path":"revision-note"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(create.status(), 201);

    let first = app
        .request(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/v1/notes/revision-note")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("if-match", "*")
                .body(Body::from("server version"))
                .unwrap(),
        )
        .await;
    assert_eq!(first.status(), 204);

    let read = app
        .request(bearer_request(
            "GET",
            "/v1/notes/revision-note",
            &client.secret,
            Body::empty(),
        ))
        .await;
    let etag = read.headers().get("etag").unwrap().clone();

    let second = app
        .request(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/v1/notes/revision-note")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("if-match", etag.clone())
                .body(Body::from("new server version"))
                .unwrap(),
        )
        .await;
    assert_eq!(second.status(), 204);

    let stale = app
        .request(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/v1/notes/revision-note")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("if-match", etag)
                .body(Body::from("stale client version"))
                .unwrap(),
        )
        .await;
    assert_eq!(stale.status(), 409);

    let read = app
        .request(bearer_request(
            "GET",
            "/v1/notes/revision-note",
            &client.secret,
            Body::empty(),
        ))
        .await;
    let body = read.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["content"], "new server version");
}

#[tokio::test]
async fn note_write_requires_a_revision_precondition() {
    let app = TestApp::new().await;
    let client = app.provision("precondition-tests").await;
    let response = app
        .request(bearer_request(
            "PUT",
            "/v1/notes/precondition-note",
            &client.secret,
            Body::from("content"),
        ))
        .await;
    assert_eq!(response.status(), 428);
}
