use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use cognate_engine::storage::NoteMetadata;

use crate::configuration::{Configuration, StorageBackend};
use crate::notebook::embedded_api::EmbeddedApiRuntime;
use crate::notebook::offline_queue::{self, QueueStatus, QueuedNoteWrite};
use crate::notebook::write_coordinator::WriteCoordinator;
use crate::notebook::{MetadataLoadResult, NoteSearchPage, NoteSearchResult, NotebookError};

mod api_client;
#[path = "attachment_ops.rs"]
mod attachment_ops;
#[path = "offline_replay.rs"]
mod offline_replay;
use api_client::*;
pub(crate) use attachment_ops::{delete_attachment, download_attachment, upload_attachment};
pub(crate) use offline_replay::replay_offline_queue;
use offline_replay::{replay_queued_writes, revision_prefix};

#[derive(Clone)]
enum SelectedBackend {
    Unconfigured(Option<NotebookError>),
    Api(ApiClient),
}

static SELECTED_BACKEND: OnceLock<RwLock<SelectedBackend>> = OnceLock::new();

fn backend_cell() -> &'static RwLock<SelectedBackend> {
    SELECTED_BACKEND.get_or_init(|| RwLock::new(SelectedBackend::Unconfigured(None)))
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

pub fn configure_backend(configuration: &Configuration) -> Result<(), NotebookError> {
    let selected = match configuration.storage_backend {
        StorageBackend::Local => {
            let runtime = match EmbeddedApiRuntime::start(std::path::PathBuf::from(
                &configuration.notebook_path,
            )) {
                Ok(runtime) => runtime,
                Err(error) => {
                    let startup_error = NotebookError::initialization(
                        "embedded API",
                        format!("Could not start embedded API: {error}"),
                    );
                    *backend_cell()
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        SelectedBackend::Unconfigured(Some(startup_error.clone()));
                    return Err(startup_error);
                }
            };
            let runtime = Arc::new(runtime);
            SelectedBackend::Api(ApiClient {
                client: reqwest::Client::new(),
                base_url: runtime.base_url.clone(),
                api_key: runtime.api_key.clone(),
                revisions: Arc::new(Mutex::new(HashMap::new())),
                metadata_revision: Arc::new(Mutex::new(None)),
                queue_path: offline_queue::queue_path(&configuration.config_path),
                write_coordinator: WriteCoordinator::default(),
                _embedded_runtime: Some(runtime),
            })
        }
        StorageBackend::Api => SelectedBackend::Api(ApiClient {
            client: reqwest::Client::new(),
            base_url: configuration.api_url.trim_end_matches('/').to_string(),
            api_key: configuration.api_key.clone(),
            revisions: Arc::new(Mutex::new(HashMap::new())),
            metadata_revision: Arc::new(Mutex::new(None)),
            queue_path: offline_queue::queue_path(&configuration.config_path),
            write_coordinator: WriteCoordinator::default(),
            _embedded_runtime: None,
        }),
    };

    *backend_cell()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = selected;
    Ok(())
}

pub fn shutdown_backend() -> Result<(), String> {
    let previous = std::mem::replace(
        &mut *backend_cell()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        SelectedBackend::Unconfigured(None),
    );
    if let SelectedBackend::Api(client) = previous
        && let Some(runtime) = client._embedded_runtime
        && let Ok(mut runtime) = Arc::try_unwrap(runtime)
    {
        return runtime.shutdown();
    }
    Ok(())
}

fn selected() -> SelectedBackend {
    backend_cell()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

fn unconfigured_error(error: Option<NotebookError>) -> NotebookError {
    error.unwrap_or_else(|| {
        NotebookError::initialization("storage backend", "API backend is not configured")
    })
}

pub async fn load_metadata(_notebook_path: String) -> Result<MetadataLoadResult, NotebookError> {
    match selected() {
        SelectedBackend::Unconfigured(error) => Err(unconfigured_error(error)),
        SelectedBackend::Api(client) => {
            let url = endpoint(&client, "/v1/notes")?;
            let response = authorized(client.client.get(url), &client)
                .send()
                .await
                .map_err(|error| {
                    NotebookError::api("load metadata", None, None, error.to_string(), true)
                })?;
            if !response.status().is_success() {
                return Err(api_response_error(response, "load metadata").await);
            }
            let revision = required_etag(&response, "load metadata")?;
            let notes = response
                .json::<Vec<NoteMetadata>>()
                .await
                .map_err(|error| api_error("load metadata", error))?;
            *client.metadata_revision.lock().unwrap() = Some(revision);
            Ok(MetadataLoadResult {
                notes,
                warning: None,
            })
        }
    }
}

pub async fn check_connection() -> Result<(), NotebookError> {
    match selected() {
        SelectedBackend::Unconfigured(error) => Err(unconfigured_error(error)),
        SelectedBackend::Api(client) => {
            let url = endpoint(&client, "/v1/health")?;
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                authorized(client.client.get(url), &client).send(),
            )
            .await
            .map_err(|_| {
                NotebookError::api(
                    "connect to server",
                    None,
                    None,
                    "request timed out after 5 seconds",
                    true,
                )
            })?
            .map_err(|error| {
                NotebookError::api("connect to server", None, None, error.to_string(), true)
            })?;
            if !response.status().is_success() {
                return Err(api_response_error(response, "connect to server").await);
            }
            Ok(())
        }
    }
}

pub async fn save_metadata(
    _notebook_path: &str,
    notes: &[NoteMetadata],
) -> Result<(), NotebookError> {
    match selected() {
        SelectedBackend::Unconfigured(error) => Err(unconfigured_error(error)),
        SelectedBackend::Api(client) => {
            let url = endpoint(&client, "/v1/metadata")?;
            let revision = client
                .metadata_revision
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| {
                    api_error(
                        "save metadata",
                        "metadata ETag unavailable; reload required",
                    )
                })?;
            let request = authorized(client.client.put(url).json(notes), &client)
                .header("if-match", format!("\"{revision}\""));
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
    _notebook_path: String,
    rel_path: String,
) -> Result<String, NotebookError> {
    match selected() {
        SelectedBackend::Unconfigured(error) => Err(unconfigured_error(error)),
        SelectedBackend::Api(client) => {
            let url = note_endpoint(&client, &rel_path)?;
            let response = authorized(client.client.get(url), &client)
                .send()
                .await
                .map_err(|error| {
                    NotebookError::api("load note", None, None, error.to_string(), true)
                })?;
            if !response.status().is_success() {
                return Err(api_response_error(response, "load note").await);
            }
            let revision = required_etag(&response, "load note")?;
            let payload = response
                .json::<ApiNotePayload>()
                .await
                .map_err(|error| api_error("load note", error))?;
            client.revisions.lock().unwrap().insert(rel_path, revision);
            Ok(payload.content)
        }
    }
}

pub async fn save_note_content(
    _notebook_path: String,
    rel_path: String,
    content: String,
) -> Result<(), NotebookError> {
    match selected() {
        SelectedBackend::Unconfigured(error) => Err(unconfigured_error(error)),
        SelectedBackend::Api(client) => {
            let write_ticket = client.write_coordinator.begin(&rel_path);
            let _write_guard = write_ticket.acquire().await;
            // Give edits already queued by the UI event loop a chance to replace
            // this payload before reading the cached revision and sending it.
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            if write_ticket.was_superseded() {
                #[cfg(debug_assertions)]
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
                .any(|entry| {
                    entry.rel_path == rel_path && entry.status != QueueStatus::RemoteCommitConfirmed
                })
            {
                return Err(api_error(
                    "save note",
                    "note has an unresolved offline write or conflict",
                ));
            }
            let url = note_endpoint(&client, &rel_path)?;
            let revision = client
                .revisions
                .lock()
                .unwrap()
                .get(&rel_path)
                .cloned()
                .ok_or_else(|| api_error("save note", "note ETag unavailable; reload required"))?;
            let request = authorized(client.client.put(url).body(content.clone()), &client);
            let request = request.header("if-match", format!("\"{revision}\""));
            let response = match request.send().await {
                Ok(response) => response,
                Err(error) => {
                    eprintln!(
                        "[cognate] note_write_transport_error note={} expected_revision={}",
                        rel_path,
                        revision_prefix(Some(&revision))
                    );
                    offline_queue::enqueue(
                        &client.queue_path,
                        QueuedNoteWrite {
                            rel_path,
                            content,
                            expected_revision: Some(revision.clone()),
                            status: QueueStatus::Pending,
                            retry_count: 0,
                            next_retry_at: None,
                        },
                    )
                    .map_err(|queue_error| api_error("offline queue", queue_error))?;
                    return Err(NotebookError::api(
                        "save note",
                        None,
                        None,
                        error.to_string(),
                        true,
                    ));
                }
            };
            if !response.status().is_success() {
                if response.status() == reqwest::StatusCode::CONFLICT {
                    let status = response.status();
                    if let Ok(conflict) = response.json::<ApiConflict>().await {
                        eprintln!(
                            "[cognate] note_write_conflict note={} expected_revision={} server_revision={}",
                            rel_path,
                            revision_prefix(Some(&revision)),
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
            if response
                .headers()
                .get("x-metadata-repair-pending")
                .and_then(|value| value.to_str().ok())
                == Some("true")
            {
                *client.metadata_revision.lock().unwrap() = None;
                eprintln!(
                    "[cognate] note_write_metadata_repair_pending note={}",
                    rel_path
                );
            }
            #[cfg(debug_assertions)]
            eprintln!("[cognate] note_write_succeeded note={}", rel_path);
            Ok(())
        }
    }
}

pub async fn create_note(
    _notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<NoteMetadata, NotebookError> {
    match selected() {
        SelectedBackend::Unconfigured(error) => Err(unconfigured_error(error)),
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
    _notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<(), NotebookError> {
    match selected() {
        SelectedBackend::Unconfigured(error) => Err(unconfigured_error(error)),
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
    _notebook_path: &str,
    current_rel_path: &str,
    new_rel_path: &str,
    notes: &mut [NoteMetadata],
) -> Result<String, NotebookError> {
    match selected() {
        SelectedBackend::Unconfigured(error) => Err(unconfigured_error(error)),
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

pub async fn search_page(
    _notebook_path: String,
    query: String,
    limit: usize,
    cursor: Option<String>,
) -> Result<NoteSearchPage, NotebookError> {
    let client = match selected() {
        SelectedBackend::Api(client) => client,
        SelectedBackend::Unconfigured(error) => return Err(unconfigured_error(error)),
    };
    let Ok(url) = endpoint(&client, "/v1/search/page") else {
        return Err(api_error("search", "invalid API URL"));
    };
    let mut query_parameters = vec![("q", query), ("limit", limit.to_string())];
    if let Some(cursor) = cursor {
        query_parameters.push(("cursor", cursor));
    }
    let response = send_search(authorized(
        client.client.get(url).query(&query_parameters),
        &client,
    ))
    .await?;
    Ok(NoteSearchPage {
        results: response
            .results
            .into_iter()
            .map(|result| NoteSearchResult {
                rel_path: result.rel_path,
                snippet: result.snippet,
                match_type: result.match_type,
                highlights: result.highlights,
            })
            .collect(),
        next_cursor: response.next_cursor,
        total: response.total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_configuration_does_not_start_an_embedded_server() {
        let configuration = Configuration {
            theme: "Dark".to_string(),
            notebook_path: "/srv/cognate/notebook".to_string(),
            scale: 1.0,
            config_path: "config.json".to_string(),
            version: "test".to_string(),
            storage_backend: StorageBackend::Api,
            api_url: "http://127.0.0.1:1".to_string(),
            api_key: "cgnt_live_test".to_string(),
        };

        configure_backend(&configuration).expect("API configuration should not bind a port");
        assert!(is_api());
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime should build")
            .block_on(check_connection());
        assert!(result.is_err());
        shutdown_backend().expect("remote API selection has no embedded runtime to stop");
    }
}
