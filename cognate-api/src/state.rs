use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cognate_engine::search::SearchIndexManager;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::auth::{ClientRecord, InMemoryClients, digest_secret};

const MAX_SEARCH_MANAGERS: usize = 24;
const SEARCH_MANAGER_IDLE_SECS: u64 = 15 * 60;

struct SearchManagerEntry {
    manager: Arc<Mutex<SearchIndexManager>>,
    last_accessed: Instant,
}

#[derive(Clone)]
pub struct AppState {
    pub db: Option<SqlitePool>,
    pub notebook_path: PathBuf,
    pub admin_digest: [u8; 32],
    pub auth_store: AuthStore,
    search_managers: Arc<Mutex<HashMap<PathBuf, SearchManagerEntry>>>,
}

#[derive(Clone)]
pub enum AuthStore {
    Sqlite,
    InMemory(InMemoryClients),
}

impl AppState {
    pub fn has_persistent_auth(&self) -> bool {
        matches!(&self.auth_store, AuthStore::Sqlite)
    }

    pub fn new(db: SqlitePool, notebook_path: PathBuf, admin_token: String) -> Self {
        Self::with_store(Some(db), notebook_path, admin_token, AuthStore::Sqlite)
    }

    pub fn new_in_memory(notebook_path: PathBuf) -> Self {
        let mut admin_digest = [0_u8; 32];
        let _ = getrandom::fill(&mut admin_digest);
        Self {
            db: None,
            notebook_path,
            admin_digest,
            auth_store: AuthStore::InMemory(InMemoryClients::default()),
            search_managers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn with_store(
        db: Option<SqlitePool>,
        notebook_path: PathBuf,
        admin_token: String,
        auth_store: AuthStore,
    ) -> Self {
        Self {
            db,
            notebook_path,
            admin_digest: digest_secret(&admin_token),
            auth_store,
            search_managers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn create_in_memory_client(
        &self,
        id: String,
        client_name: String,
        secret: &str,
    ) -> Option<ClientRecord> {
        match &self.auth_store {
            AuthStore::InMemory(clients) => Some(clients.create(id, client_name, secret).await),
            AuthStore::Sqlite => None,
        }
    }

    pub async fn search_manager(&self) -> Arc<Mutex<SearchIndexManager>> {
        let mut managers = self.search_managers.lock().await;
        let now = Instant::now();
        managers.retain(|_, entry| {
            now.duration_since(entry.last_accessed) < Duration::from_secs(SEARCH_MANAGER_IDLE_SECS)
        });
        if !managers.contains_key(&self.notebook_path)
            && managers.len() >= MAX_SEARCH_MANAGERS
            && let Some(oldest_path) = managers
                .iter()
                .min_by_key(|(_, entry)| entry.last_accessed)
                .map(|(path, _)| path.clone())
        {
            managers.remove(&oldest_path);
        }
        let entry = managers
            .entry(self.notebook_path.clone())
            .or_insert_with(|| SearchManagerEntry {
                manager: Arc::new(Mutex::new(SearchIndexManager::new(&self.notebook_path))),
                last_accessed: now,
            });
        entry.last_accessed = now;
        entry.manager.clone()
    }
}
