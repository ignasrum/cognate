mod common;

use axum::body::Body;
use common::{TestApp, bearer_request};
use http_body_util::BodyExt;

#[tokio::test]
async fn attachment_lifecycle_routes_use_authenticated_engine_storage() {
    let app = TestApp::new().await;
    let client = app.provision("attachment-tests").await;
    let png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];

    let create = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rel_path":"with-image"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(create.status(), 201);

    let upload = app
        .request(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/attachments?note=with-image")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("content-type", "image/png")
                .body(Body::from(png.clone()))
                .unwrap(),
        )
        .await;
    assert_eq!(upload.status(), 201);
    let upload_body = upload.into_body().collect().await.unwrap().to_bytes();
    let metadata: serde_json::Value = serde_json::from_slice(&upload_body).unwrap();
    let rel_path = metadata["rel_path"].as_str().unwrap().to_string();

    let list = app
        .request(bearer_request(
            "GET",
            "/v1/attachments?note=with-image",
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(list.status(), 200);
    let list_body = list.into_body().collect().await.unwrap().to_bytes();
    let listed: serde_json::Value = serde_json::from_slice(&list_body).unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 1);

    let download_uri = format!("/v1/attachments/with-image/{rel_path}");
    let download = app
        .request(bearer_request(
            "GET",
            &download_uri,
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(download.status(), 200);
    let stale_delete_etag = download.headers().get("etag").unwrap().clone();
    assert_eq!(download.headers().get("content-type").unwrap(), "image/png");
    assert_eq!(
        download
            .headers()
            .get("content-length")
            .unwrap()
            .to_str()
            .unwrap(),
        png.len().to_string()
    );
    assert_eq!(
        download.into_body().collect().await.unwrap().to_bytes(),
        png
    );

    let missing_delete_precondition = app
        .request(bearer_request(
            "DELETE",
            &download_uri,
            &client.secret,
            Body::empty(),
        ))
        .await;
    assert_eq!(missing_delete_precondition.status(), 428);

    let missing_replace_precondition = app
        .request(
            axum::http::Request::builder()
                .method("PUT")
                .uri(&download_uri)
                .header("authorization", format!("Bearer {}", client.secret))
                .body(Body::from(png.clone()))
                .unwrap(),
        )
        .await;
    assert_eq!(missing_replace_precondition.status(), 428);

    let replace = app
        .request(
            axum::http::Request::builder()
                .method("PUT")
                .uri(&download_uri)
                .header("authorization", format!("Bearer {}", client.secret))
                .header("if-match", stale_delete_etag.clone())
                .body(Body::from(vec![
                    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A,
                ]))
                .unwrap(),
        )
        .await;
    assert_eq!(replace.status(), 204);

    let stale_delete = app
        .request(
            axum::http::Request::builder()
                .method("DELETE")
                .uri(&download_uri)
                .header("authorization", format!("Bearer {}", client.secret))
                .header("if-match", stale_delete_etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(stale_delete.status(), 409);

    let current = app
        .request(bearer_request(
            "GET",
            &download_uri,
            &client.secret,
            Body::empty(),
        ))
        .await;
    let delete = app
        .request(
            axum::http::Request::builder()
                .method("DELETE")
                .uri(&download_uri)
                .header("authorization", format!("Bearer {}", client.secret))
                .header("if-match", current.headers().get("etag").unwrap())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(delete.status(), 204);
}
