//! Coordinates editor note loading and persistence operations.
//!
//! Keeping this logic in one place prevents `Editor` from becoming a mixed
//! UI + IO orchestrator.

use std::collections::HashMap;
use std::path::Path;

use crate::notebook::{self, NoteMetadata, NotebookError};
use cognate_engine::storage::NotebookManager;

#[derive(Debug, Clone)]
pub struct LoadedNotePayload {
    pub note_path: String,
    pub content: String,
    pub images: HashMap<String, String>,
}

pub async fn load_note_payload(
    notebook_path: String,
    selected_note_path: String,
) -> LoadedNotePayload {
    let manager = NotebookManager::new(Path::new(&notebook_path));
    let loaded_content = manager
        .load_note_content(&selected_note_path)
        .await
        .unwrap_or_else(|_err| {
            #[cfg(debug_assertions)]
            eprintln!("Failed to read note file for editor: {}", _err);
            String::new()
        });

    // Legacy cleanup: embedded image state is now inferred from markdown.
    if let Ok(rel_path) = cognate_engine::storage::fs_utils::validate_relative_path(
        "note path",
        &selected_note_path,
    ) {
        let note_dir_path = Path::new(&notebook_path).join(rel_path);
        let _ = tokio::fs::remove_file(note_dir_path.join("embedded_images.json")).await;
    }

    LoadedNotePayload {
        note_path: selected_note_path,
        content: loaded_content,
        images: HashMap::new(),
    }
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
    notes: &[NoteMetadata],
) -> Result<(), NotebookError> {
    if notebook_path.trim().is_empty() {
        return Ok(());
    }

    if let Some(note_path) = content_note_path {
        notebook::save_note_content_sync(notebook_path, &note_path, markdown_text).await?;
    }

    save_metadata_snapshot(notebook_path, notes).await
}
