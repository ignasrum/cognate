use crate::notebook::{NoteMetadata, NotebookError};

pub async fn create_new_note(
    notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<NoteMetadata, NotebookError> {
    crate::notebook::backend::create_note(notebook_path, rel_path, notes).await
}

pub async fn delete_note(
    notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<(), NotebookError> {
    crate::notebook::backend::delete_note(notebook_path, rel_path, notes).await
}

pub async fn move_note(
    notebook_path: &str,
    current_rel_path: &str,
    new_rel_path: &str,
    notes: &mut [NoteMetadata],
) -> Result<String, NotebookError> {
    crate::notebook::backend::move_note(notebook_path, current_rel_path, new_rel_path, notes).await
}
