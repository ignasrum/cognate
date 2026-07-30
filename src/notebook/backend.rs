use std::sync::{OnceLock, RwLock};

use cognate_engine::storage::NoteMetadata;
use serde::{Deserialize, Serialize};

use crate::configuration::{Configuration, StorageBackend};
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

static SELECTED_BACKEND: OnceLock<RwLock<SelectedBackend>> = OnceLock::new();

fn backend_cell() -> &'static RwLock<SelectedBackend> {
    SELECTED_BACKEND.get_or_init(|| RwLock::new(SelectedBackend::Local))
}

pub(crate) fn is_api() -> bool {
    matches!(selected(), SelectedBackend::Api(_))
}

pub fn configure_backend(configuration: &Configuration) {
    let selected = match configuration.storage_backend {
        StorageBackend::Local => SelectedBackend::Local,
        StorageBackend::Api => SelectedBackend::Api(ApiClient {
            client: reqwest::Client::new(),
            base_url: configuration.api_url.trim_end_matches('/').to_string(),
            api_key: configuration.api_key.clone(),
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

pub async fn load_metadata(notebook_path: String) -> Result<MetadataLoadResult, NotebookError> {
    match selected() {
        SelectedBackend::Local => {
            crate::notebook::storage::load_notes_metadata_local(notebook_path).await
        }
        SelectedBackend::Api(client) => {
            let url = endpoint(&client, "/v1/notes")?;
            let notes = send(authorized(client.client.get(url), &client), "load metadata").await?;
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
            send_empty(
                authorized(client.client.put(url).json(notes), &client),
                "save metadata",
            )
            .await
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
            Ok(
                send::<ApiNotePayload>(authorized(client.client.get(url), &client), "load note")
                    .await?
                    .content,
            )
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
            let url = note_endpoint(&client, &rel_path)?;
            send_empty(
                authorized(client.client.put(url).body(content), &client),
                "save note",
            )
            .await
        }
    }
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
