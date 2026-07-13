use std::path::Path;

use crate::notebook::{EngineResultExt, NoteMetadata, NotebookError};
pub use cognate_engine::storage::current_timestamp_rfc3339;
pub use cognate_engine::storage::{MetadataLoadResult, NotebookManager};

pub async fn save_metadata(
    notebook_path: &str,
    notes: &[NoteMetadata],
) -> Result<(), NotebookError> {
    let manager = NotebookManager::new(Path::new(notebook_path));
    manager.save_metadata(notes).await.into_notebook_err()
}

pub async fn load_notes_metadata(
    notebook_path: String,
) -> Result<MetadataLoadResult, NotebookError> {
    let manager = NotebookManager::new(Path::new(&notebook_path));
    manager.load_metadata().await.into_notebook_err()
}

pub async fn save_note_content(
    notebook_path: String,
    rel_note_path: String,
    content: String,
) -> Result<(), NotebookError> {
    save_note_content_sync(&notebook_path, &rel_note_path, &content).await
}

pub async fn save_note_content_sync(
    notebook_path: &str,
    rel_note_path: &str,
    content: &str,
) -> Result<(), NotebookError> {
    let manager = NotebookManager::new(Path::new(notebook_path));
    manager
        .save_note_content(rel_note_path, content)
        .await
        .into_notebook_err()?;

    // Update the in-memory search index cache
    super::search::cache_upsert_search_index_note_content(
        notebook_path,
        rel_note_path,
        content,
        manager.get_note_modified_time(rel_note_path).await,
    )
    .await;

    Ok(())
}
