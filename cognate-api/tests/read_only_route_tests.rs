mod common;

use axum::body::Body;
use common::{TestApp, bearer_request};
use http_body_util::BodyExt;
use serde_json::Value;

#[tokio::test]
async fn read_only_client_can_read_metadata_notes_and_search() {
    let app = TestApp::new().await;
    let writer = app.provision("desktop-writer").await;
    let reader = app.provision_read_only("search-integration").await;

    let created = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", writer.secret))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rel_path":"reflect/note"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(created.status(), 201);

    let metadata = app
        .request(bearer_request(
            "GET",
            "/v1/notes",
            &reader.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(metadata.status(), 200);
    assert!(metadata.headers().get("etag").is_some());

    let note = app
        .request(bearer_request(
            "GET",
            "/v1/notes/reflect/note",
            &reader.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(note.status(), 200);
    assert!(note.headers().get("etag").is_some());

    let search = app
        .request(bearer_request(
            "GET",
            "/v1/search?q=reflect",
            &reader.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(search.status(), 200);
}

#[tokio::test]
async fn read_only_client_is_denied_every_mutation_route() {
    let app = TestApp::new().await;
    let reader = app.provision_read_only("search-integration").await;

    let requests = [
        axum::http::Request::builder()
            .method("POST")
            .uri("/v1/notes")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"rel_path":"blocked"}"#))
            .unwrap(),
        axum::http::Request::builder()
            .method("PUT")
            .uri("/v1/notes/blocked")
            .body(Body::from("blocked"))
            .unwrap(),
        axum::http::Request::builder()
            .method("DELETE")
            .uri("/v1/notes/blocked")
            .body(Body::empty())
            .unwrap(),
        axum::http::Request::builder()
            .method("PUT")
            .uri("/v1/metadata")
            .header("content-type", "application/json")
            .body(Body::from("[]"))
            .unwrap(),
        axum::http::Request::builder()
            .method("POST")
            .uri("/v1/attachments?note=blocked")
            .body(Body::from(vec![
                0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A,
            ]))
            .unwrap(),
        axum::http::Request::builder()
            .method("PUT")
            .uri("/v1/attachments/blocked/images/image.png")
            .body(Body::from("blocked"))
            .unwrap(),
        axum::http::Request::builder()
            .method("DELETE")
            .uri("/v1/attachments/blocked/images/image.png")
            .body(Body::empty())
            .unwrap(),
    ];

    for mut request in requests {
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", reader.secret).parse().unwrap(),
        );
        let response = app.request(request).await;
        assert_eq!(response.status(), 403);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "read_only_client");
    }

    let notes = app
        .request(bearer_request(
            "GET",
            "/v1/notes",
            &reader.secret,
            Body::empty(),
        ))
        .await;
    let body = notes.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        serde_json::from_slice::<Vec<Value>>(&body).unwrap().len(),
        0
    );
}

#[tokio::test]
async fn client_listing_reports_access_mode_without_exposing_secrets() {
    let app = TestApp::new().await;
    let writer = app.provision("writer").await;
    let reader = app.provision_read_only("reader").await;

    let response = app
        .request(
            axum::http::Request::builder()
                .method("GET")
                .uri("/v1/admin/clients")
                .header("x-admin-token", "admin-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let clients: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(clients.len(), 2);
    assert!(clients.iter().all(|client| client.get("secret").is_none()));
    assert!(
        clients
            .iter()
            .any(|client| { client["id"] == writer.id && client["access_mode"] == "read_write" })
    );
    assert!(
        clients
            .iter()
            .any(|client| { client["id"] == reader.id && client["access_mode"] == "read_only" })
    );
}
