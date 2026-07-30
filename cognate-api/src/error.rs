use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use cognate_engine::EngineError;
use serde::Serialize;
use sqlx::Error as SqlxError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("database error: {0}")]
    Database(#[from] SqlxError),
    #[error("database migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("engine error: {0}")]
    Engine(#[from] EngineError),
    #[error("authentication failed")]
    Unauthorized,
    #[allow(dead_code)]
    #[error("forbidden")]
    Forbidden,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[allow(dead_code)]
    #[error("not found")]
    NotFound,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: &'static str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Engine(EngineError::Validation { .. }) => {
                (StatusCode::BAD_REQUEST, "bad_request")
            }
            Self::Engine(EngineError::LockUnavailable { .. }) => {
                (StatusCode::CONFLICT, "lock_conflict")
            }
            Self::Config(_)
            | Self::Database(_)
            | Self::Migration(_)
            | Self::Io(_)
            | Self::Engine(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        (status, Json(ErrorBody { error: message })).into_response()
    }
}
