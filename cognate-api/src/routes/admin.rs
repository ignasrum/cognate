use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use subtle::ConstantTimeEq;

use crate::{
    api::{ClientResponse, CreateClientRequest, CreateClientResponse},
    auth::{digest_secret, encode_hex, generate_client_secret, new_client_id},
    error::ApiError,
    state::{AppState, AuthStore},
};

pub(crate) async fn create_client(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateClientRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_admin(&state, &headers)?;
    let name = payload.client_name.trim();
    if name.is_empty() || name.len() > 128 {
        return Err(ApiError::BadRequest("invalid client_name".to_string()));
    }
    let secret = generate_client_secret()?;
    let id = new_client_id()?;
    let created_at = current_timestamp();
    match &state.auth_store {
        AuthStore::Sqlite => {
            let Some(db) = state.db.as_ref() else {
                return Err(ApiError::Config(
                    "SQLite auth store has no database".to_string(),
                ));
            };
            sqlx::query(
                "INSERT INTO clients (id, client_name, key_hash, created_at) VALUES (?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(name)
            .bind(encode_hex(&digest_secret(&secret)))
            .bind(&created_at)
            .execute(db)
            .await
            .map_err(ApiError::Database)?;
        }
        AuthStore::InMemory(clients) => {
            clients.create(id.clone(), name.to_string(), &secret).await;
        }
    }
    Ok((
        StatusCode::CREATED,
        Json(CreateClientResponse {
            id,
            client_name: name.to_string(),
            secret,
            created_at,
        }),
    ))
}

pub(crate) async fn list_clients(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ClientResponse>>, ApiError> {
    require_admin(&state, &headers)?;
    let clients = match &state.auth_store {
        AuthStore::Sqlite => {
            let Some(db) = state.db.as_ref() else {
                return Err(ApiError::Config(
                    "SQLite auth store has no database".to_string(),
                ));
            };
            sqlx::query_as::<_, (String, String, String, Option<String>)>(
                "SELECT id, client_name, created_at, revoked_at FROM clients ORDER BY created_at",
            )
            .fetch_all(db)
            .await
            .map_err(ApiError::Database)?
            .into_iter()
            .map(|(id, client_name, created_at, revoked_at)| ClientResponse {
                id,
                client_name,
                created_at,
                revoked_at,
            })
            .collect()
        }
        AuthStore::InMemory(clients) => clients
            .list()
            .await
            .into_iter()
            .map(|record| ClientResponse {
                id: record.id,
                client_name: record.client_name,
                created_at: record.created_at,
                revoked_at: record.revoked_at,
            })
            .collect(),
    };
    Ok(Json(clients))
}

pub(crate) async fn delete_client(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_admin(&state, &headers)?;
    let found = match &state.auth_store {
        AuthStore::Sqlite => {
            let Some(db) = state.db.as_ref() else {
                return Err(ApiError::Config(
                    "SQLite auth store has no database".to_string(),
                ));
            };
            sqlx::query(
                "UPDATE clients SET revoked_at = datetime('now') WHERE id = ? AND revoked_at IS NULL",
            )
            .bind(id.clone())
            .execute(db)
            .await
            .map_err(ApiError::Database)?
            .rows_affected()
                > 0
        }
        AuthStore::InMemory(clients) => clients.revoke(&id).await,
    };
    if !found {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(value) = headers.get("x-admin-token") else {
        return Err(ApiError::Unauthorized);
    };
    let Ok(token) = value.to_str() else {
        return Err(ApiError::Unauthorized);
    };
    if digest_secret(token)
        .as_slice()
        .ct_eq(&state.admin_digest)
        .unwrap_u8()
        == 1
    {
        Ok(())
    } else {
        Err(ApiError::Unauthorized)
    }
}

fn current_timestamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format!("{seconds}")
}
