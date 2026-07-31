use std::path::PathBuf;
use std::time::SystemTime;

use super::fs_utils::validate_relative_path;
use super::notebook::NotebookManager;
use crate::EngineError;

pub(super) async fn modified_time(manager: &NotebookManager, rel_path: &str) -> Option<SystemTime> {
    let note_file_path = manager.notebook_path().join(rel_path).join("note.md");
    tokio::fs::metadata(note_file_path)
        .await
        .ok()
        .and_then(|metadata| metadata.modified().ok())
}

pub(super) async fn load(manager: &NotebookManager, rel_path: &str) -> Result<String, EngineError> {
    let rel_path_buf = validate_relative_path("note path", rel_path)?;
    let note_file_path: PathBuf = manager.notebook_path().join(rel_path_buf).join("note.md");
    tokio::fs::read_to_string(&note_file_path)
        .await
        .map_err(|error| {
            EngineError::storage(
                "load note content",
                format!(
                    "Failed to read note file '{}': {}",
                    note_file_path.display(),
                    error
                ),
            )
        })
}
