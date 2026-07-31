use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware,
    routing::{delete, get, post, put},
};
use cognate_engine::storage::{NoteMetadata, NotebookManager};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{authenticate, require_read_write},
    error::ApiError,
    state::AppState,
};

#[path = "routes.rs"]
mod routes;

pub const MAX_PAYLOAD_BYTES: usize = 48 * 1024 * 1024;

pub(crate) fn response_header(value: impl Into<String>) -> Result<HeaderValue, ApiError> {
    HeaderValue::from_str(&value.into())
        .map_err(|error| ApiError::Config(format!("invalid response header value: {error}")))
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct CreateClientRequest {
    pub client_name: String,
    #[serde(default)]
    pub access_mode: Option<crate::auth::AccessMode>,
}

#[derive(Debug, Serialize)]
pub struct CreateClientResponse {
    pub id: String,
    pub client_name: String,
    pub secret: String,
    pub created_at: String,
    pub access_mode: crate::auth::AccessMode,
}

#[derive(Debug, Serialize)]
pub struct ClientResponse {
    pub id: String,
    pub client_name: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
    pub access_mode: crate::auth::AccessMode,
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
    pub limit: Option<usize>,
    pub cursor: Option<String>,
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
            get(routes::attachments::list).post(routes::attachments::upload),
        )
        .route(
            "/v1/attachments/{*path}",
            get(routes::attachments::download)
                .put(routes::attachments::replace)
                .delete(routes::attachments::delete),
        )
        .route("/v1/search", get(routes::search::search))
        .route("/v1/search/page", get(routes::search::search_page))
        .layer(middleware::from_fn(require_read_write))
        .layer(middleware::from_fn_with_state(state.clone(), authenticate));

    let mut public = Router::new().route("/v1/health", get(health));
    if state.has_persistent_auth() {
        public = public
            .route(
                "/v1/admin/clients",
                post(routes::admin::create_client).get(routes::admin::list_clients),
            )
            .route(
                "/v1/admin/clients/{id}",
                delete(routes::admin::delete_client),
            );
    }
    public
        .merge(protected)
        .layer(DefaultBodyLimit::max(MAX_PAYLOAD_BYTES))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn list_notes(
    State(state): State<AppState>,
) -> Result<(HeaderMap, Json<Vec<NoteMetadata>>), ApiError> {
    let result = NotebookManager::new(&state.notebook_path)
        .load_metadata()
        .await?;
    let revision = metadata_revision(&result.notes);
    let mut headers = HeaderMap::new();
    headers.insert("etag", response_header(format!("\"{revision}\""))?);
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
    if let Err(error) = manager
        .save_metadata_if_match(&notes, expected_revision)
        .await
    {
        if matches!(error, cognate_engine::EngineError::Conflict { .. }) {
            let current = manager.load_metadata().await?.notes;
            return Err(ApiError::Conflict {
                current_revision: metadata_revision(&current),
                current_content: serde_json::to_string(&current).unwrap_or_default(),
            });
        }
        return Err(error.into());
    }
    update_search_metadata(&state, &notes).await;
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
    if body.len() > MAX_PAYLOAD_BYTES {
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
    let save_result = match manager
        .save_note_content_if_match(&rel_path, &content, Some(expected_revision))
        .await
    {
        Ok(result) => result,
        Err(cognate_engine::EngineError::Conflict { .. }) => {
            let current_content = manager.load_note_content(&rel_path).await?;
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
        response_header(format!("\"{}\"", save_result.note_revision))?,
    );
    if !save_result.metadata_revision.is_empty() {
        response_headers.insert(
            "x-metadata-etag",
            response_header(format!("\"{}\"", save_result.metadata_revision))?,
        );
    }
    response_headers.insert("x-canonical-committed", HeaderValue::from_static("true"));
    if save_result.metadata_repair_pending {
        response_headers.insert(
            "x-metadata-repair-pending",
            HeaderValue::from_static("true"),
        );
    }
    if save_result.index_repair_pending {
        response_headers.insert("x-index-repair-pending", HeaderValue::from_static("true"));
    }
    update_search_note(&state, &rel_path, &content).await;
    Ok((StatusCode::NO_CONTENT, response_headers))
}

async fn create_note(
    State(state): State<AppState>,
    Json(payload): Json<CreateNoteRequest>,
) -> Result<(StatusCode, Json<NoteMetadata>), ApiError> {
    let manager = NotebookManager::new(&state.notebook_path);
    let note = manager.create_note_atomic(&payload.rel_path).await?;
    update_search_note(&state, &payload.rel_path, "").await;
    Ok((StatusCode::CREATED, Json(note)))
}

async fn delete_note(
    State(state): State<AppState>,
    Path(rel_path): Path<String>,
) -> Result<StatusCode, ApiError> {
    let manager = NotebookManager::new(&state.notebook_path);
    manager.delete_note_atomic(&rel_path).await?;
    remove_search_note(&state, &rel_path).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn move_note(
    State(state): State<AppState>,
    Json(payload): Json<MoveNoteRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let manager = NotebookManager::new(&state.notebook_path);
    let new_path = manager
        .move_note_atomic(&payload.from_rel_path, &payload.to_rel_path)
        .await?;
    rename_search_note(&state, &payload.from_rel_path, &payload.to_rel_path).await;
    Ok(Json(serde_json::json!({ "rel_path": new_path })))
}

async fn update_search_note(state: &AppState, rel_path: &str, content: &str) {
    let loaded = match NotebookManager::new(&state.notebook_path)
        .load_metadata()
        .await
    {
        Ok(loaded) => loaded.notes,
        Err(error) => {
            eprintln!("[cognate] search_metadata_refresh_failed: {error}");
            return;
        }
    };
    let Some(note) = loaded.iter().find(|note| note.rel_path == rel_path) else {
        return;
    };
    let manager = state.search_manager().await;
    if let Err(error) = manager
        .lock()
        .await
        .upsert_note(rel_path, content, &note.labels, note.last_updated.clone())
        .await
    {
        eprintln!("[cognate] search_incremental_update_failed: {error}");
        manager.lock().await.clear_cache();
    }
}

async fn update_search_metadata(state: &AppState, notes: &[NoteMetadata]) {
    let manager = state.search_manager().await;
    let mut search = manager.lock().await;
    for note in notes {
        if let Err(error) = search
            .update_note_metadata(&note.rel_path, &note.labels, note.last_updated.clone())
            .await
        {
            eprintln!("[cognate] search_incremental_metadata_failed: {error}");
            search.clear_cache();
            break;
        }
    }
}

async fn remove_search_note(state: &AppState, rel_path: &str) {
    let manager = state.search_manager().await;
    if let Err(error) = manager.lock().await.remove_note(rel_path).await {
        eprintln!("[cognate] search_incremental_remove_failed: {error}");
        manager.lock().await.clear_cache();
    }
}

async fn rename_search_note(state: &AppState, from_rel_path: &str, to_rel_path: &str) {
    let manager = state.search_manager().await;
    if let Err(error) = manager
        .lock()
        .await
        .rename_note(from_rel_path, to_rel_path)
        .await
    {
        eprintln!("[cognate] search_incremental_rename_failed: {error}");
        manager.lock().await.clear_cache();
    }
}
