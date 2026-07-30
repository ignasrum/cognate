use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware,
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use cognate_engine::storage::{AttachmentManager, NoteMetadata, NotebookManager};
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

#[derive(Debug, Deserialize)]
pub struct AttachmentQuery {
    pub note: String,
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
        .route(
            "/v1/attachments",
            get(list_attachments).post(upload_attachment),
        )
        .route(
            "/v1/attachments/{*path}",
            get(download_attachment)
                .put(replace_attachment)
                .delete(delete_attachment),
        )
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

async fn list_attachments(
    State(state): State<AppState>,
    Query(query): Query<AttachmentQuery>,
) -> Result<Json<Vec<cognate_engine::storage::AttachmentMetadata>>, ApiError> {
    Ok(Json(
        AttachmentManager::list_attachments(&state.notebook_path, &query.note).await?,
    ))
}

fn split_attachment_path(path: &str) -> Result<(&str, String), ApiError> {
    path.split_once("/images/")
        .map(|(note, attachment)| (note, format!("images/{attachment}")))
        .ok_or_else(|| ApiError::BadRequest("attachment path must contain /images/".to_string()))
}

async fn upload_attachment(
    State(state): State<AppState>,
    Query(query): Query<AttachmentQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<
    (
        StatusCode,
        Json<cognate_engine::storage::AttachmentMetadata>,
    ),
    ApiError,
> {
    const MAX_ATTACHMENT_BYTES: usize = 16 * 1024 * 1024;
    if body.is_empty() || body.len() > MAX_ATTACHMENT_BYTES {
        return Err(ApiError::BadRequest("invalid attachment size".to_string()));
    }
    let rel_path =
        AttachmentManager::save_image_bytes(&state.notebook_path, &query.note, &body).await?;
    let bytes =
        AttachmentManager::read_attachment_bytes(&state.notebook_path, &query.note, &rel_path)
            .await?;
    let extension = rel_path.rsplit('.').next().unwrap_or_default().to_string();
    let metadata = cognate_engine::storage::AttachmentMetadata {
        rel_path,
        media_type: headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or(match extension.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                _ => "application/octet-stream",
            })
            .to_string(),
        size: bytes.len() as u64,
        revision: cognate_engine::storage::attachment_revision(&bytes),
    };
    Ok((StatusCode::CREATED, Json(metadata)))
}

async fn download_attachment(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let (note, attachment) = split_attachment_path(&path)?;
    let bytes =
        AttachmentManager::read_attachment_bytes(&state.notebook_path, note, &attachment).await?;
    let revision = cognate_engine::storage::attachment_revision(&bytes);
    let mut response = axum::response::Response::new(axum::body::Body::from(bytes));
    response.headers_mut().insert(
        "etag",
        HeaderValue::from_str(&format!("\"{revision}\"")).expect("hash is header-safe"),
    );
    Ok(response)
}

async fn replace_attachment(
    State(state): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    const MAX_ATTACHMENT_BYTES: usize = 16 * 1024 * 1024;
    if body.is_empty() || body.len() > MAX_ATTACHMENT_BYTES {
        return Err(ApiError::BadRequest("invalid attachment size".to_string()));
    }
    let (note, attachment) = split_attachment_path(&path)?;
    let current =
        AttachmentManager::read_attachment_bytes(&state.notebook_path, note, &attachment).await?;
    let expected = headers
        .get("if-match")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_matches('"'))
        .ok_or(ApiError::PreconditionRequired)?;
    if expected != cognate_engine::storage::attachment_revision(&current) {
        return Err(ApiError::Conflict {
            current_revision: cognate_engine::storage::attachment_revision(&current),
            current_content: "attachment changed on server".to_string(),
        });
    }
    AttachmentManager::replace_attachment_bytes(
        &state.notebook_path,
        note,
        &attachment,
        expected,
        &body,
    )
    .await
    .map_err(|error| match error {
        cognate_engine::EngineError::Conflict { .. } => ApiError::Conflict {
            current_revision: cognate_engine::storage::attachment_revision(&current),
            current_content: "attachment changed on server".to_string(),
        },
        other => other.into(),
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_attachment(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<StatusCode, ApiError> {
    let (note, attachment) = split_attachment_path(&path)?;
    let full_path = std::path::Path::new(note).join(attachment);
    let _ = AttachmentManager::delete_attachment(
        &state.notebook_path,
        full_path.to_string_lossy().as_ref(),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
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

async fn list_notes(
    State(state): State<AppState>,
) -> Result<(HeaderMap, Json<Vec<NoteMetadata>>), ApiError> {
    let result = NotebookManager::new(&state.notebook_path)
        .load_metadata()
        .await?;
    let revision = metadata_revision(&result.notes);
    let mut headers = HeaderMap::new();
    headers.insert(
        "etag",
        HeaderValue::from_str(&format!("\"{revision}\"")).expect("hash is header-safe"),
    );
    Ok((headers, Json(result.notes)))
}

async fn save_metadata(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(notes): Json<Vec<NoteMetadata>>,
) -> Result<StatusCode, ApiError> {
    let expected_revision = headers
        .get("if-match")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_matches('"'))
        .ok_or(ApiError::PreconditionRequired)?;
    let manager = NotebookManager::new(&state.notebook_path);
    let current = manager.load_metadata().await?.notes;
    if expected_revision != "*" && expected_revision != metadata_revision(&current) {
        return Err(ApiError::Conflict {
            current_revision: metadata_revision(&current),
            current_content: serde_json::to_string(&current).unwrap_or_default(),
        });
    }
    manager.save_metadata(&notes).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn metadata_revision(notes: &[NoteMetadata]) -> String {
    let serialized = serde_json::to_string(notes).unwrap_or_default();
    cognate_engine::storage::note_content_revision(&serialized)
}

async fn get_note(
    State(state): State<AppState>,
    Path(rel_path): Path<String>,
) -> Result<(HeaderMap, Json<NotePayload>), ApiError> {
    let manager = NotebookManager::new(&state.notebook_path);
    let content = manager.load_note_content(&rel_path).await?;
    let revision = cognate_engine::storage::note_content_revision(&content);
    let mut headers = HeaderMap::new();
    headers.insert(
        "etag",
        HeaderValue::from_str(&format!("\"{revision}\"")).expect("hash is header-safe"),
    );
    Ok((
        headers,
        Json(NotePayload {
            rel_path,
            content,
            revision,
        }),
    ))
}

#[derive(Debug, Serialize)]
pub struct NotePayload {
    pub rel_path: String,
    pub content: String,
    pub revision: String,
}

async fn save_note(
    State(state): State<AppState>,
    Path(rel_path): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, HeaderMap), ApiError> {
    const MAX_NOTE_BYTES: usize = 4 * 1024 * 1024;
    if body.len() > MAX_NOTE_BYTES {
        return Err(ApiError::BadRequest(
            "note content is too large".to_string(),
        ));
    }
    let content = String::from_utf8(body.to_vec())
        .map_err(|_| ApiError::BadRequest("note content must be UTF-8".to_string()))?;
    let expected_revision = headers
        .get("if-match")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_matches('"'));
    let Some(expected_revision) = expected_revision else {
        return Err(ApiError::PreconditionRequired);
    };
    let manager = NotebookManager::new(&state.notebook_path);
    let revision = match manager
        .save_note_content_if_match(&rel_path, &content, Some(expected_revision))
        .await
    {
        Ok(revision) => revision,
        Err(cognate_engine::EngineError::Conflict { .. }) => {
            let current_content = manager
                .load_note_content(&rel_path)
                .await
                .unwrap_or_default();
            let current_revision = cognate_engine::storage::note_content_revision(&current_content);
            return Err(ApiError::Conflict {
                current_revision,
                current_content,
            });
        }
        Err(error) => return Err(error.into()),
    };
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        "etag",
        HeaderValue::from_str(&format!("\"{revision}\"")).expect("hash is header-safe"),
    );
    Ok((StatusCode::NO_CONTENT, response_headers))
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
