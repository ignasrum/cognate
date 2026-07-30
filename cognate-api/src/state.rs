use std::path::PathBuf;

use sqlx::SqlitePool;

use crate::auth::digest_secret;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub notebook_path: PathBuf,
    pub admin_digest: [u8; 32],
}

impl AppState {
    pub fn new(db: SqlitePool, notebook_path: PathBuf, admin_token: String) -> Self {
        Self {
            db,
            notebook_path,
            admin_digest: digest_secret(&admin_token),
        }
    }
}
