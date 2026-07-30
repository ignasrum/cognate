mod common;

use axum::body::Body;
use common::{TestApp, bearer_request};

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
        .request(bearer_request(
            "PUT",
            "/v1/notes/draft/summary",
            &client.secret,
            Body::from("AI summary"),
        ))
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
