mod common;

use axum::body::Body;
use common::{TestApp, bearer_request};
use http_body_util::BodyExt;

#[tokio::test]
async fn concurrent_note_creation_preserves_both_metadata_entries() {
    let app = TestApp::new().await;
    let client = app.provision("lifecycle-race").await;

    let first = app.request(
        axum::http::Request::builder()
            .method("POST")
            .uri("/v1/notes")
            .header("authorization", format!("Bearer {}", client.secret))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"rel_path":"race-a"}"#))
            .unwrap(),
    );
    let second = app.request(
        axum::http::Request::builder()
            .method("POST")
            .uri("/v1/notes")
            .header("authorization", format!("Bearer {}", client.secret))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"rel_path":"race-b"}"#))
            .unwrap(),
    );
    let (first, second) = tokio::join!(first, second);
    assert_eq!(first.status(), 201);
    assert_eq!(second.status(), 201);

    let metadata = app
        .request(bearer_request(
            "GET",
            "/v1/notes",
            &client.secret,
            Body::empty(),
        ))
        .await;
    let body = metadata.into_body().collect().await.unwrap().to_bytes();
    let notes: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    let paths: Vec<&str> = notes
        .iter()
        .filter_map(|note| note["rel_path"].as_str())
        .collect();
    assert!(paths.contains(&"race-a"));
    assert!(paths.contains(&"race-b"));
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
async fn two_clients_racing_with_the_same_revision_have_one_winner() {
    let app = TestApp::new().await;
    let first_client = app.provision("race-client-a").await;
    let second_client = app.provision("race-client-b").await;

    let create = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", first_client.secret))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rel_path":"race-note"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(create.status(), 201);

    let initial = app
        .request(bearer_request(
            "GET",
            "/v1/notes/race-note",
            &first_client.secret,
            Body::empty(),
        ))
        .await;
    let etag = initial.headers().get("etag").unwrap().clone();

    let first_request = app.request(
        axum::http::Request::builder()
            .method("PUT")
            .uri("/v1/notes/race-note")
            .header("authorization", format!("Bearer {}", first_client.secret))
            .header("if-match", etag.clone())
            .body(Body::from("client A"))
            .unwrap(),
    );
    let second_request = app.request(
        axum::http::Request::builder()
            .method("PUT")
            .uri("/v1/notes/race-note")
            .header("authorization", format!("Bearer {}", second_client.secret))
            .header("if-match", etag)
            .body(Body::from("client B"))
            .unwrap(),
    );
    let (first_response, second_response) = tokio::join!(first_request, second_request);
    let statuses = [
        first_response.status().as_u16(),
        second_response.status().as_u16(),
    ];
    assert_eq!(statuses.iter().filter(|status| **status == 204).count(), 1);
    assert_eq!(statuses.iter().filter(|status| **status == 409).count(), 1);
}

#[tokio::test]
async fn delayed_client_write_cannot_overwrite_newer_server_content() {
    let app = TestApp::new().await;
    let delayed_client = app.provision("delayed-client").await;
    let fast_client = app.provision("fast-client").await;

    let create = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", fast_client.secret))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rel_path":"delayed-note"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(create.status(), 201);

    let initial = app
        .request(bearer_request(
            "GET",
            "/v1/notes/delayed-note",
            &delayed_client.secret,
            Body::empty(),
        ))
        .await;
    let etag = initial.headers().get("etag").unwrap().clone();

    let delayed = async {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        app.request(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/v1/notes/delayed-note")
                .header("authorization", format!("Bearer {}", delayed_client.secret))
                .header("if-match", etag)
                .body(Body::from("delayed stale content"))
                .unwrap(),
        )
        .await
    };
    let fast = app.request(
        axum::http::Request::builder()
            .method("PUT")
            .uri("/v1/notes/delayed-note")
            .header("authorization", format!("Bearer {}", fast_client.secret))
            .header("if-match", initial.headers().get("etag").unwrap())
            .body(Body::from("fast newer content"))
            .unwrap(),
    );
    let (delayed_response, fast_response) = tokio::join!(delayed, fast);
    assert_eq!(fast_response.status(), 204);
    assert_eq!(delayed_response.status(), 409);

    let final_note = app
        .request(bearer_request(
            "GET",
            "/v1/notes/delayed-note",
            &fast_client.secret,
            Body::empty(),
        ))
        .await;
    let body = final_note.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["content"], "fast newer content");
}
