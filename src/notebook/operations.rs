use std::path::Path;
use cognate_engine::storage::NotebookManager;
use crate::notebook::{NoteMetadata, NotebookError, EngineResultExt};

pub async fn create_new_note(
    notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<NoteMetadata, NotebookError> {
    let manager = NotebookManager::new(Path::new(notebook_path));
    let metadata = manager.create_note(rel_path, notes).await.into_notebook_err()?;

    // Update search index cache
    super::search::cache_upsert_search_index_note_content(
        notebook_path,
        rel_path,
        "",
        manager.get_note_modified_time(rel_path).await,
    ).await;

    Ok(metadata)
}

pub async fn delete_note(
    notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<(), NotebookError> {
    let manager = NotebookManager::new(Path::new(notebook_path));
    manager.delete_note(rel_path, notes).await.into_notebook_err()?;

    // Update search index cache
    super::search::cache_remove_search_index_entries(notebook_path, rel_path).await;

    Ok(())
}

pub async fn move_note(
    notebook_path: &str,
    current_rel_path: &str,
    new_rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<String, NotebookError> {
    let manager = NotebookManager::new(Path::new(notebook_path));
    let path = manager.move_note(current_rel_path, new_rel_path, notes).await.into_notebook_err()?;

    // Update search index cache
    super::search::cache_rename_search_index_entries(
        notebook_path,
        current_rel_path,
        new_rel_path,
    ).await;

    Ok(path)
}
