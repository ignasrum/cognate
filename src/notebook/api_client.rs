use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::notebook::NotebookError;
use crate::notebook::embedded_api::EmbeddedApiRuntime;
use crate::notebook::write_coordinator::WriteCoordinator;

#[derive(Clone)]
pub(super) struct ApiClient {
    pub(super) client: reqwest::Client,
    pub(super) base_url: String,
    pub(super) api_key: String,
    pub(super) revisions: Arc<Mutex<HashMap<String, String>>>,
    pub(super) metadata_revision: Arc<Mutex<Option<String>>>,
    pub(super) queue_path: std::path::PathBuf,
    pub(super) write_coordinator: WriteCoordinator,
    pub(super) _embedded_runtime: Option<Arc<EmbeddedApiRuntime>>,
}

#[derive(Debug, Serialize)]
pub(super) struct CreateNoteRequest<'a> {
    pub(super) rel_path: &'a str,
}

#[derive(Debug, Serialize)]
pub(super) struct MoveNoteRequest<'a> {
    pub(super) from_rel_path: &'a str,
    pub(super) to_rel_path: &'a str,
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiNotePayload {
    pub(super) content: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiSearchResult {
    pub(super) rel_path: String,
    pub(super) snippet: String,
    pub(super) match_type: cognate_engine::search::SearchMatchType,
    pub(super) highlights: Vec<cognate_engine::search::SearchHighlight>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiSearchResponse {
    pub(super) results: Vec<ApiSearchResult>,
    pub(super) next_cursor: Option<String>,
    pub(super) total: usize,
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiSearchError {
    pub(super) error: String,
    pub(super) detail: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiErrorBody {
    pub(super) error: Option<String>,
    pub(super) detail: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiConflict {
    pub(super) current_revision: String,
    pub(super) current_content: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiAttachmentPayload {
    pub(super) rel_path: String,
}

pub(super) fn api_error(context: &str, error: impl std::fmt::Display) -> NotebookError {
    NotebookError::storage("api", format!("{context}: {error}"))
}

pub(super) fn endpoint(client: &ApiClient, path: &str) -> Result<reqwest::Url, NotebookError> {
    reqwest::Url::parse(&format!("{}{}", client.base_url, path))
        .map_err(|error| api_error("invalid API URL", error))
}

pub(super) async fn send<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
    context: &'static str,
) -> Result<T, NotebookError> {
    let response = request
        .send()
        .await
        .map_err(|error| NotebookError::api(context, None, None, error.to_string(), true))?;
    if !response.status().is_success() {
        return Err(api_response_error(response, context).await);
    }
    response
        .json::<T>()
        .await
        .map_err(|error| api_error(context, error))
}

pub(super) async fn send_empty(
    request: reqwest::RequestBuilder,
    context: &'static str,
) -> Result<(), NotebookError> {
    let response = request
        .send()
        .await
        .map_err(|error| NotebookError::api(context, None, None, error.to_string(), true))?;
    if !response.status().is_success() {
        return Err(api_response_error(response, context).await);
    }
    Ok(())
}

pub(super) fn required_etag(
    response: &reqwest::Response,
    operation: &'static str,
) -> Result<String, NotebookError> {
    let value = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| api_error(operation, "successful response omitted ETag"))?;
    let revision = value.trim_matches('"').trim();
    if revision.is_empty() {
        return Err(api_error(
            operation,
            "successful response contained an empty ETag",
        ));
    }
    Ok(revision.to_string())
}

fn status_is_retryable(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

pub(super) async fn api_response_error(
    response: reqwest::Response,
    operation: &'static str,
) -> NotebookError {
    let status = response.status().as_u16();
    let parsed = response.json::<ApiErrorBody>().await.ok();
    let code = parsed.as_ref().and_then(|body| body.error.clone());
    let detail = parsed
        .as_ref()
        .and_then(|body| body.detail.clone())
        .unwrap_or_else(|| format!("server returned HTTP {status}"));
    NotebookError::api(
        operation,
        Some(status),
        code,
        detail,
        status_is_retryable(status),
    )
}

pub(super) async fn send_search(
    request: reqwest::RequestBuilder,
) -> Result<ApiSearchResponse, NotebookError> {
    let response = request
        .send()
        .await
        .map_err(|error| NotebookError::search("API search", error.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response
            .json::<ApiSearchError>()
            .await
            .map(|error| format!("{}: {}", error.error, error.detail))
            .unwrap_or_else(|_| format!("server returned HTTP {status}"));
        return Err(NotebookError::search("API search", detail));
    }
    response
        .json()
        .await
        .map_err(|error| NotebookError::search("API search", error.to_string()))
}

pub(super) fn authorized(
    request: reqwest::RequestBuilder,
    client: &ApiClient,
) -> reqwest::RequestBuilder {
    request.bearer_auth(&client.api_key)
}

pub(super) fn note_endpoint(
    client: &ApiClient,
    rel_path: &str,
) -> Result<reqwest::Url, NotebookError> {
    let mut url = endpoint(client, "/v1/notes")?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| api_error("build note URL", "URL cannot be a base"))?;
        for segment in rel_path.split('/') {
            segments.push(segment);
        }
    }
    Ok(url)
}

pub(super) fn attachment_endpoint(
    client: &ApiClient,
    rel_path: &str,
) -> Result<reqwest::Url, NotebookError> {
    let mut url = endpoint(client, "/v1/attachments")?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| api_error("build attachment URL", "URL cannot be a base"))?;
    for segment in rel_path.split('/') {
        segments.push(segment);
    }
    drop(segments);
    Ok(url)
}
