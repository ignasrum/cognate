use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use cognate_engine::search::SearchIndexManager;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::auth::digest_secret;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub notebook_path: PathBuf,
    pub admin_digest: [u8; 32],
    pub search_managers: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<SearchIndexManager>>>>>,
}

impl AppState {
    pub fn new(db: SqlitePool, notebook_path: PathBuf, admin_token: String) -> Self {
        Self {
            db,
            notebook_path,
            admin_digest: digest_secret(&admin_token),
            search_managers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn search_manager(&self) -> Arc<Mutex<SearchIndexManager>> {
        let mut managers = self.search_managers.lock().await;
        managers
            .entry(self.notebook_path.clone())
            .or_insert_with(|| Arc::new(Mutex::new(SearchIndexManager::new(&self.notebook_path))))
            .clone()
    }
}
