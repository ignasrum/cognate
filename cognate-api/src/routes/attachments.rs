use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::Response,
};
use cognate_engine::storage::{AttachmentManager, AttachmentMetadata, attachment_revision};

use crate::{
    api::{AttachmentQuery, MAX_PAYLOAD_BYTES},
    error::ApiError,
    state::AppState,
};

pub(crate) async fn list(
    State(state): State<AppState>,
    Query(query): Query<AttachmentQuery>,
) -> Result<Json<Vec<AttachmentMetadata>>, ApiError> {
    Ok(Json(
        AttachmentManager::list_attachments(&state.notebook_path, &query.note).await?,
    ))
}

fn split_path(path: &str) -> Result<(&str, String), ApiError> {
    path.split_once("/images/")
        .map(|(note, attachment)| (note, format!("images/{attachment}")))
        .ok_or_else(|| ApiError::BadRequest("attachment path must contain /images/".to_string()))
}

pub(crate) async fn upload(
    State(state): State<AppState>,
    Query(query): Query<AttachmentQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<AttachmentMetadata>), ApiError> {
    if body.is_empty() || body.len() > MAX_PAYLOAD_BYTES {
        return Err(ApiError::BadRequest("invalid attachment size".to_string()));
    }
    let rel_path =
        AttachmentManager::save_image_bytes(&state.notebook_path, &query.note, &body).await?;
    let bytes =
        AttachmentManager::read_attachment_bytes(&state.notebook_path, &query.note, &rel_path)
            .await?;
    let extension = rel_path.rsplit('.').next().unwrap_or_default().to_string();
    let metadata = AttachmentMetadata {
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
        revision: attachment_revision(&bytes),
    };
    Ok((StatusCode::CREATED, Json(metadata)))
}

pub(crate) async fn download(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Response, ApiError> {
    let (note, attachment) = split_path(&path)?;
    let bytes =
        AttachmentManager::read_attachment_bytes(&state.notebook_path, note, &attachment).await?;
    let revision = attachment_revision(&bytes);
    let media_type = media_type(&attachment);
    let content_length = bytes.len();
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        "etag",
        HeaderValue::from_str(&format!("\"{revision}\"")).expect("hash is header-safe"),
    );
    response
        .headers_mut()
        .insert("content-type", HeaderValue::from_static(media_type));
    response.headers_mut().insert(
        "content-length",
        HeaderValue::from_str(&content_length.to_string()).expect("body length is header-safe"),
    );
    Ok(response)
}

fn media_type(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

pub(crate) async fn replace(
    State(state): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    if body.is_empty() || body.len() > MAX_PAYLOAD_BYTES {
        return Err(ApiError::BadRequest("invalid attachment size".to_string()));
    }
    let (note, attachment) = split_path(&path)?;
    let current =
        AttachmentManager::read_attachment_bytes(&state.notebook_path, note, &attachment).await?;
    let expected = headers
        .get("if-match")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_matches('"'))
        .ok_or(ApiError::PreconditionRequired)?;
    if expected != attachment_revision(&current) {
        return Err(ApiError::Conflict {
            current_revision: attachment_revision(&current),
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
            current_revision: attachment_revision(&current),
            current_content: "attachment changed on server".to_string(),
        },
        other => other.into(),
    })?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn delete(
    State(state): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let (note, attachment) = split_path(&path)?;
    let expected = headers
        .get("if-match")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_matches('"'))
        .ok_or(ApiError::PreconditionRequired)?;
    let current =
        AttachmentManager::read_attachment_bytes(&state.notebook_path, note, &attachment).await?;
    let current_revision = attachment_revision(&current);
    if expected != "*" && expected != current_revision {
        return Err(ApiError::Conflict {
            current_revision,
            current_content: "attachment changed on server".to_string(),
        });
    }
    let full_path = std::path::Path::new(note).join(attachment);
    AttachmentManager::delete_attachment(
        &state.notebook_path,
        full_path.to_string_lossy().as_ref(),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
