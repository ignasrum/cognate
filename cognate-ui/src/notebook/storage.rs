use crate::notebook::{NoteMetadata, NotebookError};
pub use cognate_engine::storage::MetadataLoadResult;
pub use cognate_engine::storage::current_timestamp_rfc3339;

pub async fn save_metadata(
    notebook_path: &str,
    notes: &[NoteMetadata],
) -> Result<(), NotebookError> {
    crate::notebook::backend::save_metadata(notebook_path, notes).await
}

pub async fn load_notes_metadata(
    notebook_path: String,
) -> Result<MetadataLoadResult, NotebookError> {
    crate::notebook::backend::load_metadata(notebook_path).await
}

pub async fn save_note_content(
    notebook_path: String,
    rel_note_path: String,
    content: String,
) -> Result<(), NotebookError> {
    crate::notebook::backend::save_note_content(notebook_path, rel_note_path, content).await
}
