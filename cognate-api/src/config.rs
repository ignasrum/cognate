use std::path::PathBuf;

use crate::error::ApiError;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_address: String,
    pub port: u16,
    pub database_path: PathBuf,
    pub notebook_path: PathBuf,
    pub admin_token: String,
}

impl Config {
    pub fn from_env() -> Result<Self, ApiError> {
        let bind_address =
            std::env::var("COGNATE_API_BIND_ADDRESS").unwrap_or_else(|_| "127.0.0.1".to_string());

        let port = std::env::var("COGNATE_API_BIND_PORT")
            .unwrap_or_else(|_| "8787".to_string())
            .parse::<u16>()
            .map_err(|error| ApiError::Config(format!("invalid COGNATE_API_BIND_PORT: {error}")))?;
        if port == 0 {
            return Err(ApiError::Config(
                "COGNATE_API_BIND_PORT must be non-zero".to_string(),
            ));
        }

        let notebook_path = required_path("COGNATE_NOTEBOOK_PATH")?;
        let database_path = std::env::var("COGNATE_API_DATABASE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("cognate-api.sqlite"));
        let admin_token = required_secret("COGNATE_API_ADMIN_TOKEN")?;

        Ok(Self {
            bind_address,
            port,
            database_path,
            notebook_path,
            admin_token,
        })
    }
}

fn required_path(name: &str) -> Result<PathBuf, ApiError> {
    std::env::var(name)
        .map(PathBuf::from)
        .map_err(|_| ApiError::Config(format!("{name} is required")))
}

fn required_secret(name: &str) -> Result<String, ApiError> {
    let value = std::env::var(name).map_err(|_| ApiError::Config(format!("{name} is required")))?;
    if value.trim().is_empty() {
        return Err(ApiError::Config(format!("{name} must not be empty")));
    }
    Ok(value)
}
