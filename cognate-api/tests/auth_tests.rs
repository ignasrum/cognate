mod common;

use axum::{body::Body, http::Request};
use cognate_api::auth::{CLIENT_KEY_PREFIX, digest_secret, encode_hex, generate_client_secret};

use common::TestApp;

#[test]
fn generated_client_secrets_have_expected_entropy_encoding() {
    let first = generate_client_secret().unwrap();
    let second = generate_client_secret().unwrap();
    assert!(first.starts_with(CLIENT_KEY_PREFIX));
    assert_eq!(first.len(), CLIENT_KEY_PREFIX.len() + 64);
    assert!(
        first[CLIENT_KEY_PREFIX.len()..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    );
    assert_ne!(first, second);
}

#[test]
fn blake3_digest_encoding_is_stable_and_fixed_length() {
    let digest = digest_secret("cgnt_live_test");
    assert_eq!(encode_hex(&digest).len(), 64);
    assert_eq!(digest, digest_secret("cgnt_live_test"));
    assert_ne!(digest, digest_secret("cgnt_live_other"));
}

#[tokio::test]
async fn malformed_and_duplicate_authorization_headers_are_rejected() {
    let app = TestApp::new().await;
    let client = app.provision("auth-tests").await;

    for value in ["Basic abc", "Bearer", "Bearer wrong key", ""] {
        let response = app
            .request(
                Request::builder()
                    .uri("/v1/notes")
                    .header("authorization", value)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), 401, "unexpected status for {value:?}");
    }

    let response = app
        .request(
            Request::builder()
                .uri("/v1/notes")
                .header("authorization", format!("Bearer {}", client.secret))
                .header("authorization", format!("Bearer {}", client.secret))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), 401);
}
