use crate::{error::ApiError, state::AppState};
use axum::{
    extract::{Request, State},
    http::{HeaderValue, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;

pub const CLIENT_KEY_PREFIX: &str = "cgnt_live_";

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct AuthenticatedClient {
    pub id: String,
    pub client_name: String,
}

#[derive(Clone, Debug)]
pub struct ClientRecord {
    pub id: String,
    pub client_name: String,
    pub key_hash: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
    pub last_used_at: Option<String>,
}

#[derive(Clone, Default)]
pub struct InMemoryClients {
    records: Arc<Mutex<HashMap<String, ClientRecord>>>,
}

impl InMemoryClients {
    pub async fn create(&self, id: String, client_name: String, secret: &str) -> ClientRecord {
        let record = ClientRecord {
            id: id.clone(),
            client_name,
            key_hash: encode_hex(&digest_secret(secret)),
            created_at: current_timestamp(),
            revoked_at: None,
            last_used_at: None,
        };
        self.records.lock().await.insert(id, record.clone());
        record
    }

    pub async fn find_active_by_hash(&self, key_hash: &str) -> Option<ClientRecord> {
        self.records
            .lock()
            .await
            .values()
            .find(|record| {
                record.revoked_at.is_none()
                    && record
                        .key_hash
                        .as_bytes()
                        .ct_eq(key_hash.as_bytes())
                        .unwrap_u8()
                        == 1
            })
            .cloned()
    }

    pub async fn mark_used(&self, id: &str) {
        if let Some(record) = self.records.lock().await.get_mut(id) {
            record.last_used_at = Some(current_timestamp());
        }
    }

    pub async fn list(&self) -> Vec<ClientRecord> {
        let mut records = self
            .records
            .lock()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        records
    }

    pub async fn revoke(&self, id: &str) -> bool {
        let mut records = self.records.lock().await;
        let Some(record) = records.get_mut(id) else {
            return false;
        };
        if record.revoked_at.is_some() {
            return false;
        }
        record.revoked_at = Some(current_timestamp());
        true
    }
}

fn current_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

pub fn digest_secret(secret: &str) -> [u8; 32] {
    *blake3::hash(secret.as_bytes()).as_bytes()
}

pub fn generate_client_secret() -> Result<String, ApiError> {
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random).map_err(|error| ApiError::Config(error.to_string()))?;
    Ok(format!("{CLIENT_KEY_PREFIX}{}", encode_hex(&random)))
}

pub async fn authenticate(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let values = request.headers().get_all(AUTHORIZATION);
    if values.iter().count() != 1 {
        return Err(ApiError::Unauthorized);
    }
    let value = values.iter().next().ok_or(ApiError::Unauthorized)?;
    let presented = bearer_value(value).ok_or(ApiError::Unauthorized)?;
    if presented.len() > 256 || !presented.starts_with(CLIENT_KEY_PREFIX) {
        return Err(ApiError::Unauthorized);
    }

    let digest = digest_secret(presented);
    let digest_text = encode_hex(&digest);
    let (id, client_name) = match &state.auth_store {
        crate::state::AuthStore::Sqlite => {
            let Some(db) = state.db.as_ref() else {
                return Err(ApiError::Config(
                    "SQLite auth store has no database".to_string(),
                ));
            };
            let record = sqlx::query_as::<_, (String, String, String)>(
                "SELECT id, client_name, key_hash FROM clients WHERE key_hash = ? AND revoked_at IS NULL",
            )
            .bind(&digest_text)
            .fetch_optional(db)
            .await
            .map_err(ApiError::Database)?;
            let Some((id, client_name, stored_digest)) = record else {
                return Err(ApiError::Unauthorized);
            };
            if stored_digest
                .as_bytes()
                .ct_eq(digest_text.as_bytes())
                .unwrap_u8()
                != 1
            {
                return Err(ApiError::Unauthorized);
            }
            let _ = sqlx::query("UPDATE clients SET last_used_at = datetime('now') WHERE id = ?")
                .bind(&id)
                .execute(db)
                .await;
            (id, client_name)
        }
        crate::state::AuthStore::InMemory(clients) => {
            let Some(record) = clients.find_active_by_hash(&digest_text).await else {
                return Err(ApiError::Unauthorized);
            };
            clients.mark_used(&record.id).await;
            (record.id, record.client_name)
        }
    };
    request
        .extensions_mut()
        .insert(AuthenticatedClient { id, client_name });
    Ok(next.run(request).await)
}

fn bearer_value(value: &HeaderValue) -> Option<&str> {
    let text = value.to_str().ok()?;
    let (scheme, key) = text.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("Bearer") && !key.is_empty() && !key.contains(' ') {
        Some(key)
    } else {
        None
    }
}

pub fn new_client_id() -> Result<String, ApiError> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|error| ApiError::Config(error.to_string()))?;
    Ok(encode_hex(&random))
}

pub fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
