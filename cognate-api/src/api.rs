use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use cognate_engine::storage::{NoteMetadata, NotebookManager};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use crate::{
    auth::{authenticate, digest_secret, encode_hex, generate_client_secret, new_client_id},
    error::ApiError,
    state::AppState,
};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct CreateClientRequest {
    pub client_name: String,
}

#[derive(Debug, Serialize)]
pub struct CreateClientResponse {
    pub id: String,
    pub client_name: String,
    pub secret: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ClientResponse {
    pub id: String,
    pub client_name: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateNoteRequest {
    pub rel_path: String,
}

#[derive(Debug, Deserialize)]
pub struct MoveNoteRequest {
    pub from_rel_path: String,
    pub to_rel_path: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/v1/notes", get(list_notes).post(create_note))
        .route(
            "/v1/notes/{*rel_path}",
            get(get_note).put(save_note).delete(delete_note),
        )
        .route("/v1/notes/move", post(move_note))
        .route("/v1/metadata", put(save_metadata))
        .route("/v1/search", get(search))
        .layer(middleware::from_fn_with_state(state.clone(), authenticate));

    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/admin/clients", post(create_client).get(list_clients))
        .route("/v1/admin/clients/{id}", delete(delete_client_route))
        .merge(protected)
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn create_client(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
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
    sqlx::query("INSERT INTO clients (id, client_name, key_hash, created_at) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(name)
        .bind(encode_hex(&digest_secret(&secret)))
        .bind(&created_at)
        .execute(&state.db)
        .await
        .map_err(ApiError::Database)?;
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

async fn list_clients(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<ClientResponse>>, ApiError> {
    require_admin(&state, &headers)?;
    let rows = sqlx::query_as::<_, (String, String, String, Option<String>)>(
        "SELECT id, client_name, created_at, revoked_at FROM clients ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, client_name, created_at, revoked_at)| ClientResponse {
                id,
                client_name,
                created_at,
                revoked_at,
            })
            .collect(),
    ))
}

async fn delete_client_route(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_admin(&state, &headers)?;
    let result = sqlx::query(
        "UPDATE clients SET revoked_at = datetime('now') WHERE id = ? AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(ApiError::Database)?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

fn require_admin(state: &AppState, headers: &axum::http::HeaderMap) -> Result<(), ApiError> {
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

async fn list_notes(State(state): State<AppState>) -> Result<Json<Vec<NoteMetadata>>, ApiError> {
    let result = NotebookManager::new(&state.notebook_path)
        .load_metadata()
        .await?;
    Ok(Json(result.notes))
}

async fn save_metadata(
    State(state): State<AppState>,
    Json(notes): Json<Vec<NoteMetadata>>,
) -> Result<StatusCode, ApiError> {
    NotebookManager::new(&state.notebook_path)
        .save_metadata(&notes)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_note(
    State(state): State<AppState>,
    Path(rel_path): Path<String>,
) -> Result<Json<NotePayload>, ApiError> {
    let manager = NotebookManager::new(&state.notebook_path);
    let content = manager.load_note_content(&rel_path).await?;
    Ok(Json(NotePayload { rel_path, content }))
}

#[derive(Debug, Serialize)]
pub struct NotePayload {
    pub rel_path: String,
    pub content: String,
}

async fn save_note(
    State(state): State<AppState>,
    Path(rel_path): Path<String>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    const MAX_NOTE_BYTES: usize = 4 * 1024 * 1024;
    if body.len() > MAX_NOTE_BYTES {
        return Err(ApiError::BadRequest(
            "note content is too large".to_string(),
        ));
    }
    let content = String::from_utf8(body.to_vec())
        .map_err(|_| ApiError::BadRequest("note content must be UTF-8".to_string()))?;
    NotebookManager::new(&state.notebook_path)
        .save_note_content(&rel_path, &content)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_note(
    State(state): State<AppState>,
    Json(payload): Json<CreateNoteRequest>,
) -> Result<(StatusCode, Json<NoteMetadata>), ApiError> {
    let manager = NotebookManager::new(&state.notebook_path);
    let loaded = manager.load_metadata().await?;
    let mut notes = loaded.notes;
    let note = manager.create_note(&payload.rel_path, &mut notes).await?;
    Ok((StatusCode::CREATED, Json(note)))
}

async fn delete_note(
    State(state): State<AppState>,
    Path(rel_path): Path<String>,
) -> Result<StatusCode, ApiError> {
    let manager = NotebookManager::new(&state.notebook_path);
    let loaded = manager.load_metadata().await?;
    let mut notes = loaded.notes;
    manager.delete_note(&rel_path, &mut notes).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn move_note(
    State(state): State<AppState>,
    Json(payload): Json<MoveNoteRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let manager = NotebookManager::new(&state.notebook_path);
    let loaded = manager.load_metadata().await?;
    let mut notes = loaded.notes;
    let new_path = manager
        .move_note(&payload.from_rel_path, &payload.to_rel_path, &mut notes)
        .await?;
    Ok(Json(serde_json::json!({ "rel_path": new_path })))
}

fn current_timestamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format!("{seconds}")
}

async fn search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<cognate_engine::search::SearchResultEntry>>, ApiError> {
    if query.q.len() > 256 {
        return Err(ApiError::BadRequest("query is too long".to_string()));
    }
    let manager = NotebookManager::new(&state.notebook_path);
    let loaded = manager.load_metadata().await?;
    let mut search = cognate_engine::search::SearchIndexManager::new(&state.notebook_path);
    Ok(Json(
        search
            .search(&query.q, &loaded.notes, std::time::Duration::from_secs(5))
            .await?,
    ))
}
