#![allow(dead_code)]

use axum::{Router, body::Body, http::Request};
use cognate_api::{api, db, state::AppState};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use tower::util::ServiceExt;

pub struct TestApp {
    pub state: AppState,
    _temp: TempDir,
}

impl TestApp {
    pub async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db::migrate(&pool).await.unwrap();
        let state = AppState::new(pool, temp.path().to_path_buf(), "admin-secret".to_string());
        Self { state, _temp: temp }
    }

    pub fn router(&self) -> Router {
        api::router(self.state.clone())
    }

    pub async fn request(&self, request: Request<Body>) -> axum::response::Response {
        self.router().oneshot(request).await.unwrap()
    }

    pub async fn json(&self, request: Request<Body>) -> Value {
        let response = self.request(request).await;
        let body = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&body).unwrap()
    }

    pub async fn provision(&self, name: &str) -> ClientCredentials {
        self.provision_with_mode(name, None).await
    }

    pub async fn provision_read_only(&self, name: &str) -> ClientCredentials {
        self.provision_with_mode(name, Some("read_only")).await
    }

    async fn provision_with_mode(
        &self,
        name: &str,
        access_mode: Option<&str>,
    ) -> ClientCredentials {
        let mut payload = serde_json::json!({"client_name": name});
        if let Some(access_mode) = access_mode {
            payload["access_mode"] = serde_json::json!(access_mode);
        }
        let response = self
            .request(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/clients")
                    .header("x-admin-token", "admin-secret")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), 201);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        ClientCredentials {
            id: json["id"].as_str().unwrap().to_string(),
            secret: json["secret"].as_str().unwrap().to_string(),
        }
    }
}

pub struct ClientCredentials {
    pub id: String,
    pub secret: String,
}

pub fn bearer_request(method: &str, uri: &str, secret: &str, body: Body) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {secret}"))
        .body(body)
        .unwrap()
}
