//! Coordinates editor note loading and persistence operations.
//!
//! Keeping this logic in one place prevents `Editor` from becoming a mixed
//! UI + IO orchestrator.

use std::collections::HashMap;

use crate::notebook::{self, NoteMetadata, NotebookError};

#[derive(Debug, Clone)]
pub(crate) struct LoadedNotePayload {
    pub note_path: String,
    pub content: String,
    pub images: HashMap<String, String>,
}

pub async fn load_note_payload(
    notebook_path: String,
    selected_note_path: String,
) -> Result<LoadedNotePayload, NotebookError> {
    let loaded_content =
        notebook::load_note_content(notebook_path, selected_note_path.clone()).await?;

    Ok(LoadedNotePayload {
        note_path: selected_note_path,
        content: loaded_content,
        images: HashMap::new(),
    })
}

pub async fn save_metadata_snapshot(
    notebook_path: &str,
    notes: &[NoteMetadata],
) -> Result<(), NotebookError> {
    notebook::save_metadata(notebook_path, notes).await
}

pub async fn flush_for_shutdown(
    notebook_path: &str,
    content_note_path: Option<String>,
    markdown_text: &str,
    content_dirty: bool,
    notes: &[NoteMetadata],
    metadata_dirty: bool,
) -> Result<(), NotebookError> {
    if notebook_path.trim().is_empty() {
        return Ok(());
    }

    if content_dirty && let Some(note_path) = content_note_path {
        let result = notebook::save_note_content(
            notebook_path.to_string(),
            note_path,
            markdown_text.to_string(),
        )
        .await;
        if let Err(error) = result {
            if notebook::is_api_backend() && is_transient_api_error(&error) {
                eprintln!("[cognate] shutdown_saved_to_offline_queue: {error}");
                return Ok(());
            }
            return Err(error);
        }
    }

    if metadata_dirty {
        save_metadata_snapshot(notebook_path, notes).await
    } else {
        Ok(())
    }
}

fn is_transient_api_error(error: &NotebookError) -> bool {
    matches!(
        error,
        NotebookError::Api {
            retryable: true,
            ..
        }
    )
}
