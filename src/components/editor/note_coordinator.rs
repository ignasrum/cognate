//! Coordinates editor note loading and persistence operations.
//!
//! Keeping this logic in one place prevents `Editor` from becoming a mixed
//! UI + IO orchestrator.

use std::collections::HashMap;

use crate::notebook::{self, NoteMetadata, NotebookError};

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
    let full_note_path = format!("{}/{}/note.md", notebook_path, selected_note_path);
    let loaded_content = match std::fs::read_to_string(full_note_path) {
        Ok(content) => content,
        Err(_err) => {
            #[cfg(debug_assertions)]
            eprintln!("Failed to read note file for editor: {}", _err);
            String::new()
        }
    };

    // Legacy cleanup: embedded image state is now inferred from markdown.
    let legacy_images_path = format!(
        "{}/{}/embedded_images.json",
        notebook_path, selected_note_path
    );
    let _ = std::fs::remove_file(legacy_images_path);

    LoadedNotePayload {
        note_path: selected_note_path,
        content: loaded_content,
        images: HashMap::new(),
    }
}

pub fn save_metadata_snapshot(
    notebook_path: &str,
    notes: &[NoteMetadata],
) -> Result<(), NotebookError> {
    notebook::save_metadata(notebook_path, notes)
}

pub fn flush_for_shutdown(
    notebook_path: &str,
    content_note_path: Option<String>,
    markdown_text: &str,
    notes: &[NoteMetadata],
) -> Result<(), NotebookError> {
    if notebook_path.trim().is_empty() {
        return Ok(());
    }

    if let Some(note_path) = content_note_path {
        notebook::save_note_content_sync(notebook_path, &note_path, markdown_text)?;
    }

    save_metadata_snapshot(notebook_path, notes)
}
