use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use cognate_engine::storage::NoteMetadata;
use serde::{Deserialize, Serialize};

use crate::configuration::{Configuration, StorageBackend};
use crate::notebook::offline_queue::{self, QueueStatus, QueuedNoteWrite};
use crate::notebook::write_coordinator::WriteCoordinator;
use crate::notebook::{MetadataLoadResult, NoteSearchResult, NotebookError};

#[derive(Clone)]
enum SelectedBackend {
    Local,
    Api(ApiClient),
}

#[derive(Clone)]
struct ApiClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    revisions: Arc<Mutex<HashMap<String, String>>>,
    metadata_revision: Arc<Mutex<Option<String>>>,
    queue_path: std::path::PathBuf,
    write_coordinator: WriteCoordinator,
}

#[derive(Debug, Serialize)]
struct CreateNoteRequest<'a> {
    rel_path: &'a str,
}

#[derive(Debug, Serialize)]
struct MoveNoteRequest<'a> {
    from_rel_path: &'a str,
    to_rel_path: &'a str,
}

#[derive(Debug, Deserialize)]
struct ApiNotePayload {
    content: String,
}

#[derive(Debug, Deserialize)]
struct ApiSearchResult {
    rel_path: String,
    snippet: String,
}

#[derive(Debug, Deserialize)]
struct ApiConflict {
    current_revision: String,
    current_content: String,
}

#[derive(Debug, Deserialize)]
struct ApiAttachmentPayload {
    rel_path: String,
}

static SELECTED_BACKEND: OnceLock<RwLock<SelectedBackend>> = OnceLock::new();

fn backend_cell() -> &'static RwLock<SelectedBackend> {
    SELECTED_BACKEND.get_or_init(|| RwLock::new(SelectedBackend::Local))
}

pub(crate) fn is_api() -> bool {
    matches!(selected(), SelectedBackend::Api(_))
}

pub(crate) fn set_note_revision(rel_path: &str, revision: &str) {
    if let SelectedBackend::Api(client) = selected() {
        client
            .revisions
            .lock()
            .unwrap()
            .insert(rel_path.to_string(), revision.to_string());
    }
}

pub fn configure_backend(configuration: &Configuration) {
    let selected = match configuration.storage_backend {
        StorageBackend::Local => SelectedBackend::Local,
        StorageBackend::Api => SelectedBackend::Api(ApiClient {
            client: reqwest::Client::new(),
            base_url: configuration.api_url.trim_end_matches('/').to_string(),
            api_key: configuration.api_key.clone(),
            revisions: Arc::new(Mutex::new(HashMap::new())),
            metadata_revision: Arc::new(Mutex::new(None)),
            queue_path: offline_queue::queue_path(&configuration.config_path),
            write_coordinator: WriteCoordinator::default(),
        }),
    };

    *backend_cell()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = selected;
}

fn selected() -> SelectedBackend {
    backend_cell()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

fn api_error(context: &str, error: impl std::fmt::Display) -> NotebookError {
    NotebookError::storage("api", format!("{context}: {error}"))
}

fn endpoint(client: &ApiClient, path: &str) -> Result<reqwest::Url, NotebookError> {
    reqwest::Url::parse(&format!("{}{}", client.base_url, path))
        .map_err(|error| api_error("invalid API URL", error))
}

async fn send<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
    context: &str,
) -> Result<T, NotebookError> {
    let response = request
        .send()
        .await
        .map_err(|error| api_error(context, error))?;
    if !response.status().is_success() {
        return Err(api_error(
            context,
            format!("server returned HTTP {}", response.status()),
        ));
    }
    response
        .json::<T>()
        .await
        .map_err(|error| api_error(context, error))
}

async fn send_empty(request: reqwest::RequestBuilder, context: &str) -> Result<(), NotebookError> {
    let response = request
        .send()
        .await
        .map_err(|error| api_error(context, error))?;
    if !response.status().is_success() {
        return Err(api_error(
            context,
            format!("server returned HTTP {}", response.status()),
        ));
    }
    Ok(())
}

fn authorized(request: reqwest::RequestBuilder, client: &ApiClient) -> reqwest::RequestBuilder {
    request.bearer_auth(&client.api_key)
}

fn note_endpoint(client: &ApiClient, rel_path: &str) -> Result<reqwest::Url, NotebookError> {
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

fn attachment_endpoint(client: &ApiClient, rel_path: &str) -> Result<reqwest::Url, NotebookError> {
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

pub(crate) async fn upload_attachment(
    _notebook_path: String,
    note_path: String,
    bytes: Vec<u8>,
) -> Result<String, NotebookError> {
    let SelectedBackend::Api(client) = selected() else {
        return Err(api_error(
            "upload attachment",
            "API backend is not selected",
        ));
    };
    let mut url = endpoint(&client, "/v1/attachments")?;
    url.query_pairs_mut().append_pair("note", &note_path);
    let response = authorized(client.client.post(url).body(bytes), &client)
        .header("content-type", "application/octet-stream")
        .send()
        .await
        .map_err(|error| api_error("upload attachment", error))?;
    if !response.status().is_success() {
        return Err(api_error(
            "upload attachment",
            format!("server returned HTTP {}", response.status()),
        ));
    }
    Ok(response
        .json::<ApiAttachmentPayload>()
        .await
        .map_err(|error| api_error("upload attachment", error))?
        .rel_path)
}

pub(crate) async fn download_attachment(
    _notebook_path: String,
    rel_path: String,
) -> Result<Vec<u8>, NotebookError> {
    let SelectedBackend::Api(client) = selected() else {
        return Err(api_error(
            "download attachment",
            "API backend is not selected",
        ));
    };
    let response = authorized(
        client.client.get(attachment_endpoint(&client, &rel_path)?),
        &client,
    )
    .send()
    .await
    .map_err(|error| api_error("download attachment", error))?;
    if !response.status().is_success() {
        return Err(api_error(
            "download attachment",
            format!("server returned HTTP {}", response.status()),
        ));
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| api_error("download attachment", error))
}

pub(crate) async fn delete_attachment(
    _notebook_path: String,
    rel_path: String,
) -> Result<(), NotebookError> {
    let SelectedBackend::Api(client) = selected() else {
        return Err(api_error(
            "delete attachment",
            "API backend is not selected",
        ));
    };
    let get_response = authorized(
        client.client.get(attachment_endpoint(&client, &rel_path)?),
        &client,
    )
    .send()
    .await
    .map_err(|error| api_error("delete attachment", error))?;
    if !get_response.status().is_success() {
        return Err(api_error(
            "delete attachment",
            format!("server returned HTTP {}", get_response.status()),
        ));
    }
    let revision = get_response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| api_error("delete attachment", "server response omitted ETag"))?;
    send_empty(
        authorized(
            client
                .client
                .delete(attachment_endpoint(&client, &rel_path)?)
                .header("if-match", revision),
            &client,
        ),
        "delete attachment",
    )
    .await
}

pub async fn load_metadata(notebook_path: String) -> Result<MetadataLoadResult, NotebookError> {
    match selected() {
        SelectedBackend::Local => {
            crate::notebook::storage::load_notes_metadata_local(notebook_path).await
        }
        SelectedBackend::Api(client) => {
            let url = endpoint(&client, "/v1/notes")?;
            let response = authorized(client.client.get(url), &client)
                .send()
                .await
                .map_err(|error| api_error("load metadata", error))?;
            if !response.status().is_success() {
                return Err(api_error(
                    "load metadata",
                    format!("server returned HTTP {}", response.status()),
                ));
            }
            let revision = response
                .headers()
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.trim_matches('"').to_string());
            let notes = response
                .json::<Vec<NoteMetadata>>()
                .await
                .map_err(|error| api_error("load metadata", error))?;
            *client.metadata_revision.lock().unwrap() = revision;
            Ok(MetadataLoadResult {
                notes,
                warning: None,
            })
        }
    }
}

pub async fn save_metadata(
    notebook_path: &str,
    notes: &[NoteMetadata],
) -> Result<(), NotebookError> {
    match selected() {
        SelectedBackend::Local => {
            crate::notebook::storage::save_metadata_local(notebook_path, notes).await
        }
        SelectedBackend::Api(client) => {
            let url = endpoint(&client, "/v1/metadata")?;
            let revision = client.metadata_revision.lock().unwrap().clone();
            let request = authorized(client.client.put(url).json(notes), &client).header(
                "if-match",
                revision
                    .as_deref()
                    .map(|revision| format!("\"{revision}\""))
                    .unwrap_or_else(|| "*".to_string()),
            );
            let result = send_empty(request, "save metadata").await;
            if result.is_ok() {
                let serialized = serde_json::to_string(notes).unwrap_or_default();
                *client.metadata_revision.lock().unwrap() =
                    Some(cognate_engine::storage::note_content_revision(&serialized));
            }
            result
        }
    }
}

pub async fn load_note_content(
    notebook_path: String,
    rel_path: String,
) -> Result<String, NotebookError> {
    match selected() {
        SelectedBackend::Local => {
            crate::notebook::storage::load_note_content_local(&notebook_path, &rel_path).await
        }
        SelectedBackend::Api(client) => {
            let url = note_endpoint(&client, &rel_path)?;
            let response = authorized(client.client.get(url), &client)
                .send()
                .await
                .map_err(|error| api_error("load note", error))?;
            if !response.status().is_success() {
                return Err(api_error(
                    "load note",
                    format!("server returned HTTP {}", response.status()),
                ));
            }
            let revision = response
                .headers()
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();
            let payload = response
                .json::<ApiNotePayload>()
                .await
                .map_err(|error| api_error("load note", error))?;
            if !revision.is_empty() {
                client.revisions.lock().unwrap().insert(rel_path, revision);
            }
            Ok(payload.content)
        }
    }
}

pub async fn save_note_content(
    notebook_path: String,
    rel_path: String,
    content: String,
) -> Result<(), NotebookError> {
    match selected() {
        SelectedBackend::Local => {
            crate::notebook::storage::save_note_content_local(notebook_path, rel_path, content)
                .await
        }
        SelectedBackend::Api(client) => {
            let write_ticket = client.write_coordinator.begin(&rel_path);
            let _write_guard = write_ticket.acquire().await;
            // Give edits already queued by the UI event loop a chance to replace
            // this payload before reading the cached revision and sending it.
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            if write_ticket.was_superseded() {
                eprintln!(
                    "[cognate] note_write_coalesced note={} reason=newer_local_generation",
                    rel_path
                );
                return Ok(());
            }
            replay_queued_writes(&client).await?;
            if offline_queue::read(&client.queue_path)
                .map_err(|error| api_error("offline queue", error))?
                .iter()
                .any(|entry| entry.rel_path == rel_path)
            {
                return Err(api_error(
                    "save note",
                    "note has an unresolved offline write or conflict",
                ));
            }
            let url = note_endpoint(&client, &rel_path)?;
            let revision = client.revisions.lock().unwrap().get(&rel_path).cloned();
            let request = authorized(client.client.put(url).body(content.clone()), &client);
            let request = request.header(
                "if-match",
                revision
                    .as_deref()
                    .map(|revision| format!("\"{revision}\""))
                    .unwrap_or_else(|| "*".to_string()),
            );
            let response = match request.send().await {
                Ok(response) => response,
                Err(error) => {
                    eprintln!(
                        "[cognate] note_write_transport_error note={} expected_revision={}",
                        rel_path,
                        revision_prefix(revision.as_deref())
                    );
                    offline_queue::enqueue(
                        &client.queue_path,
                        QueuedNoteWrite {
                            rel_path,
                            content,
                            expected_revision: revision,
                            status: QueueStatus::Pending,
                            retry_count: 0,
                            next_retry_at: None,
                        },
                    )
                    .map_err(|queue_error| api_error("offline queue", queue_error))?;
                    return Err(api_error("save note", error));
                }
            };
            if !response.status().is_success() {
                if response.status() == reqwest::StatusCode::CONFLICT {
                    let status = response.status();
                    if let Ok(conflict) = response.json::<ApiConflict>().await {
                        eprintln!(
                            "[cognate] note_write_conflict note={} expected_revision={} server_revision={}",
                            rel_path,
                            revision_prefix(revision.as_deref()),
                            revision_prefix(Some(&conflict.current_revision))
                        );
                        return Err(NotebookError::conflict(
                            "save note",
                            content,
                            conflict.current_content,
                            conflict.current_revision,
                        ));
                    }
                    return Err(api_error(
                        "save note",
                        format!("server returned HTTP {status}"),
                    ));
                }
                eprintln!(
                    "[cognate] note_write_failed note={} status={}",
                    rel_path,
                    response.status()
                );
                return Err(api_error(
                    "save note",
                    format!("server returned HTTP {}", response.status()),
                ));
            }
            if let Some(revision) = response
                .headers()
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.trim_matches('"').to_string())
            {
                client
                    .revisions
                    .lock()
                    .unwrap()
                    .insert(rel_path.clone(), revision);
            }
            if let Some(metadata_revision) = response
                .headers()
                .get("x-metadata-etag")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.trim_matches('"').to_string())
            {
                *client.metadata_revision.lock().unwrap() = Some(metadata_revision);
            }
            eprintln!("[cognate] note_write_succeeded note={}", rel_path);
            Ok(())
        }
    }
}

fn revision_prefix(revision: Option<&str>) -> String {
    revision.unwrap_or("none").chars().take(12).collect()
}

pub(crate) async fn replay_offline_queue() -> Result<(), NotebookError> {
    let SelectedBackend::Api(client) = selected() else {
        return Ok(());
    };
    let _write_guard = client
        .write_coordinator
        .begin("__offline_queue__")
        .acquire()
        .await;
    replay_queued_writes(&client).await
}

async fn replay_queued_writes(client: &ApiClient) -> Result<(), NotebookError> {
    let entries = offline_queue::read(&client.queue_path)
        .map_err(|error| api_error("offline queue", error))?;
    if entries.is_empty() {
        return Ok(());
    }

    let mut remaining = Vec::new();
    let now = offline_queue::now_seconds();
    for mut entry in entries {
        if entry.status == QueueStatus::Conflict {
            eprintln!(
                "[cognate] offline_write_conflict_paused note={}",
                entry.rel_path
            );
            remaining.push(entry);
            continue;
        }
        if entry.next_retry_at.is_some_and(|retry_at| retry_at > now) {
            remaining.push(entry);
            continue;
        }
        let url = note_endpoint(client, &entry.rel_path)?;
        let mut request = authorized(client.client.put(url).body(entry.content.clone()), client);
        if let Some(revision) = entry.expected_revision.as_deref() {
            request = request.header("if-match", format!("\"{revision}\""));
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                eprintln!(
                    "[cognate] offline_write_transport_error note={}",
                    entry.rel_path
                );
                entry.retry_count = entry.retry_count.saturating_add(1);
                entry.next_retry_at =
                    Some(now.saturating_add(offline_queue::retry_delay_seconds(entry.retry_count)));
                remaining.push(entry);
                offline_queue::write(&client.queue_path, &remaining)
                    .map_err(|queue_error| api_error("offline queue", queue_error))?;
                return Err(api_error("offline queue retry", error));
            }
        };
        if response.status().is_success() {
            if let Some(revision) = response
                .headers()
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.trim_matches('"').to_string())
            {
                client
                    .revisions
                    .lock()
                    .unwrap()
                    .insert(entry.rel_path.clone(), revision);
            }
            if let Some(metadata_revision) = response
                .headers()
                .get("x-metadata-etag")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.trim_matches('"').to_string())
            {
                *client.metadata_revision.lock().unwrap() = Some(metadata_revision);
            }
            eprintln!("[cognate] offline_write_replayed note={}", entry.rel_path);
        } else {
            if response.status() == reqwest::StatusCode::CONFLICT {
                let status = response.status();
                if let Ok(conflict) = response.json::<ApiConflict>().await {
                    eprintln!(
                        "[cognate] offline_write_conflict note={} server_revision={}",
                        entry.rel_path,
                        revision_prefix(Some(&conflict.current_revision))
                    );
                    entry.status = QueueStatus::Conflict;
                    entry.next_retry_at = None;
                    remaining.push(entry.clone());
                    offline_queue::write(&client.queue_path, &remaining)
                        .map_err(|error| api_error("offline queue", error))?;
                    return Err(NotebookError::conflict_for_note(
                        "offline write",
                        entry.rel_path,
                        entry.content,
                        conflict.current_content,
                        conflict.current_revision,
                    ));
                }
                remaining.push(entry);
                offline_queue::write(&client.queue_path, &remaining)
                    .map_err(|error| api_error("offline queue", error))?;
                return Err(api_error(
                    "offline queue retry",
                    format!("server returned HTTP {status}"),
                ));
            }
            eprintln!(
                "[cognate] offline_write_failed note={} status={}",
                entry.rel_path,
                response.status()
            );
            entry.retry_count = entry.retry_count.saturating_add(1);
            entry.next_retry_at =
                Some(now.saturating_add(offline_queue::retry_delay_seconds(entry.retry_count)));
            remaining.push(entry);
        }
    }
    offline_queue::write(&client.queue_path, &remaining)
        .map_err(|error| api_error("offline queue", error))
}

pub async fn create_note(
    notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<NoteMetadata, NotebookError> {
    match selected() {
        SelectedBackend::Local => {
            crate::notebook::operations::create_new_note_local(notebook_path, rel_path, notes).await
        }
        SelectedBackend::Api(client) => {
            let url = endpoint(&client, "/v1/notes")?;
            let note: NoteMetadata = send(
                authorized(
                    client
                        .client
                        .post(url)
                        .json(&CreateNoteRequest { rel_path }),
                    &client,
                ),
                "create note",
            )
            .await?;
            notes.push(note.clone());
            Ok(note)
        }
    }
}

pub async fn delete_note(
    notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<(), NotebookError> {
    match selected() {
        SelectedBackend::Local => {
            crate::notebook::operations::delete_note_local(notebook_path, rel_path, notes).await
        }
        SelectedBackend::Api(client) => {
            let url = note_endpoint(&client, rel_path)?;
            send_empty(
                authorized(client.client.delete(url), &client),
                "delete note",
            )
            .await?;
            notes.retain(|note| note.rel_path != rel_path);
            Ok(())
        }
    }
}

pub async fn move_note(
    notebook_path: &str,
    current_rel_path: &str,
    new_rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<String, NotebookError> {
    match selected() {
        SelectedBackend::Local => {
            crate::notebook::operations::move_note_local(
                notebook_path,
                current_rel_path,
                new_rel_path,
                notes,
            )
            .await
        }
        SelectedBackend::Api(client) => {
            let url = endpoint(&client, "/v1/notes/move")?;
            send_empty(
                authorized(
                    client.client.post(url).json(&MoveNoteRequest {
                        from_rel_path: current_rel_path,
                        to_rel_path: new_rel_path,
                    }),
                    &client,
                ),
                "move note",
            )
            .await?;
            for note in notes.iter_mut() {
                if note.rel_path == current_rel_path {
                    note.rel_path = new_rel_path.to_string();
                }
            }
            Ok(new_rel_path.to_string())
        }
    }
}

pub async fn search(
    notebook_path: String,
    notes: Vec<crate::notebook::SearchNote>,
    query: String,
) -> Vec<NoteSearchResult> {
    let SelectedBackend::Api(client) = selected() else {
        return crate::notebook::search::search_notes_with_snapshot_local(
            notebook_path,
            notes,
            query,
        )
        .await;
    };
    let Ok(url) = endpoint(&client, "/v1/search") else {
        return Vec::new();
    };
    let Ok(results) = send::<Vec<ApiSearchResult>>(
        authorized(client.client.get(url).query(&[("q", query)]), &client),
        "search",
    )
    .await
    else {
        return Vec::new();
    };
    results
        .into_iter()
        .map(|result| NoteSearchResult {
            rel_path: result.rel_path,
            snippet: result.snippet,
        })
        .collect()
}
