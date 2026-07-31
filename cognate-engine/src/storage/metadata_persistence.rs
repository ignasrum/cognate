use std::path::Path;

use super::fs_utils::write_text_file_atomically;
use super::index_sync;
use super::metadata::{NoteMetadata, NotebookMetadata};
use crate::EngineError;

const METADATA_FILE_NAME: &str = "metadata.json";
const METADATA_BACKUP_FILE_NAME: &str = "metadata.json.bak";

async fn snapshot_known_good(metadata_path: &Path, backup_path: &Path) -> Result<(), EngineError> {
    if !tokio::fs::try_exists(metadata_path).await.unwrap_or(false) {
        return Ok(());
    }
    let existing_metadata = tokio::fs::read_to_string(metadata_path)
        .await
        .map_err(|error| {
            EngineError::recovery(
                "metadata snapshot",
                format!(
                    "Failed to read existing metadata at '{}' before backup: {}",
                    metadata_path.display(),
                    error
                ),
            )
        })?;
    serde_json::from_str::<NotebookMetadata>(&existing_metadata).map_err(|error| {
        EngineError::recovery(
            "metadata snapshot",
            format!(
                "Refusing to overwrite invalid metadata at '{}': {}",
                metadata_path.display(),
                error
            ),
        )
    })?;
    write_text_file_atomically(backup_path, &existing_metadata)
        .await
        .map_err(|error| {
            EngineError::recovery(
                "metadata snapshot",
                format!(
                    "Failed to update metadata recovery copy at '{}': {}",
                    backup_path.display(),
                    error
                ),
            )
        })
}

pub(super) async fn save(notebook_path: &Path, notes: &[NoteMetadata]) -> Result<(), EngineError> {
    let metadata_path = notebook_path.join(METADATA_FILE_NAME);
    let backup_path = notebook_path.join(METADATA_BACKUP_FILE_NAME);
    if let Some(parent) = metadata_path.parent()
        && let Err(error) = tokio::fs::create_dir_all(parent).await
    {
        return Err(EngineError::storage(
            "save metadata",
            format!(
                "Failed to create metadata parent directory '{}': {}",
                parent.display(),
                error
            ),
        ));
    }
    let notebook_metadata = NotebookMetadata {
        notes: notes.to_vec(),
    };
    snapshot_known_good(&metadata_path, &backup_path).await?;
    let json_string = serde_json::to_string_pretty(&notebook_metadata).map_err(|error| {
        EngineError::storage(
            "save metadata",
            format!("Failed to serialize metadata.json: {}", error),
        )
    })?;
    write_text_file_atomically(&metadata_path, &json_string).await?;
    index_sync::sync_metadata(notebook_path, notes).await
}
