use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::concurrency::ConcurrencyManager;
use super::fs_utils::{validate_relative_path, write_text_file_atomically};
use super::index_sync;
pub use super::metadata::{
    MetadataLoadResult, NoteMetadata, NotebookMetadata, current_timestamp_rfc3339,
};
use super::metadata::{reconcile_last_updated_timestamp, sanitize_loaded_notes};
use super::metadata_persistence;
use super::notebook_content;
use super::transactions::cleanup_stale_staged_delete_entries;
use crate::{EngineError, NotebookEngineState};

const METADATA_FILE_NAME: &str = "metadata.json";
const METADATA_BACKUP_FILE_NAME: &str = "metadata.json.bak";
#[allow(dead_code)]
pub(super) const FAIL_ATOMIC_RENAME_MARKER: &str = ".cognate_fail_atomic_rename";
#[allow(dead_code)]
pub(super) const FAIL_DELETE_ROLLBACK_MARKER: &str = ".cognate_fail_delete_rollback";
#[allow(dead_code)]
pub(super) const FAIL_MOVE_ROLLBACK_MARKER: &str = ".cognate_fail_move_rollback";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteContentSaveResult {
    pub note_revision: String,
    pub metadata_revision: String,
}

pub(super) async fn save_metadata(
    notebook_path: &Path,
    notes: &[NoteMetadata],
) -> Result<(), EngineError> {
    metadata_persistence::save(notebook_path, notes).await
}

pub struct NotebookManager {
    pub(super) notebook_path: PathBuf,
    pub(super) concurrency: ConcurrencyManager,
}

impl NotebookManager {
    pub fn new(path: &Path) -> Self {
        Self {
            notebook_path: path.to_path_buf(),
            concurrency: ConcurrencyManager::new(path),
        }
    }

    pub fn notebook_path(&self) -> &Path {
        &self.notebook_path
    }

    pub async fn load_engine_state(&self) -> Result<NotebookEngineState, EngineError> {
        let _lock = self.concurrency.acquire_notebook().await?;
        Ok(index_sync::load(&self.notebook_path).await)
    }

    pub async fn save_engine_state(&self, state: &NotebookEngineState) -> Result<(), EngineError> {
        let _lock = self.concurrency.acquire_notebook().await?;
        index_sync::save(&self.notebook_path, state).await
    }

    pub async fn get_note_modified_time(&self, rel_path: &str) -> Option<SystemTime> {
        notebook_content::modified_time(self, rel_path).await
    }

    pub async fn load_metadata(&self) -> Result<MetadataLoadResult, EngineError> {
        let _lock = self.concurrency.acquire_notebook().await?;
        self.load_metadata_unlocked().await
    }

    async fn load_metadata_unlocked(&self) -> Result<MetadataLoadResult, EngineError> {
        let file_path = self.notebook_path.join(METADATA_FILE_NAME);
        let backup_path = self.notebook_path.join(METADATA_BACKUP_FILE_NAME);
        cleanup_stale_staged_delete_entries(&self.notebook_path).await;

        let contents = match tokio::fs::read_to_string(&file_path).await {
            Ok(c) => c,
            Err(err) => {
                if err.kind() == ErrorKind::NotFound {
                    return Ok(MetadataLoadResult {
                        notes: Vec::new(),
                        warning: None,
                    });
                }
                return Err(EngineError::storage(
                    "load metadata",
                    format!(
                        "Failed to read metadata file '{}': {}",
                        file_path.display(),
                        err
                    ),
                ));
            }
        };

        let mut warning: Option<String> = None;
        let metadata: NotebookMetadata = match serde_json::from_str(&contents) {
            Ok(m) => m,
            Err(err) => {
                let backup_contents = tokio::fs::read_to_string(&backup_path).await.map_err(|backup_error| {
                    EngineError::recovery(
                        "metadata recovery",
                        format!(
                            "Failed to parse metadata at '{}': {}. Also failed to read backup '{}': {}",
                            file_path.display(),
                            err,
                            backup_path.display(),
                            backup_error
                        ),
                    )
                })?;

                let backup_metadata = serde_json::from_str::<NotebookMetadata>(&backup_contents)
                    .map_err(|backup_parse_error| {
                        EngineError::recovery(
                            "metadata recovery",
                            format!(
                                "Failed to parse metadata at '{}': {}. Backup '{}' is also invalid: {}",
                                file_path.display(),
                                err,
                                backup_path.display(),
                                backup_parse_error
                            ),
                        )
                    })?;

                write_text_file_atomically(&file_path, &backup_contents).await.map_err(|restore_error| {
                    EngineError::recovery(
                        "metadata recovery",
                        format!(
                            "Failed to parse metadata at '{}': {}. Backup '{}' was valid, but restore failed: {}",
                            file_path.display(),
                            err,
                            backup_path.display(),
                            restore_error
                        ),
                    )
                })?;

                warning = Some(format!(
                    "Recovered metadata from '{}' after parse failure in '{}'.",
                    backup_path.display(),
                    file_path.display()
                ));

                backup_metadata
            }
        };

        let (mut notes, path_warnings) = sanitize_loaded_notes(metadata.notes);
        let mut metadata_changed = false;
        if !path_warnings.is_empty() {
            metadata_changed = true;
            let path_warning_text = path_warnings.join("\n");
            if let Some(existing) = &mut warning {
                existing.push_str("\n\n");
                existing.push_str(&path_warning_text);
            } else {
                warning = Some(path_warning_text);
            }
        }

        for note in &mut notes {
            let Ok(rel_path) = validate_relative_path("metadata note path", &note.rel_path) else {
                metadata_changed = true;
                continue;
            };
            let note_file_path = self.notebook_path.join(rel_path).join("note.md");
            let note_file_modified_time = tokio::fs::metadata(note_file_path)
                .await
                .ok()
                .and_then(|file_metadata| file_metadata.modified().ok());
            let reconciled_last_updated = reconcile_last_updated_timestamp(
                note.last_updated.as_deref(),
                note_file_modified_time,
            );

            if note.last_updated != reconciled_last_updated {
                note.last_updated = reconciled_last_updated;
                metadata_changed = true;
            }
        }

        if metadata_changed {
            if let Err(error) = save_metadata(&self.notebook_path, &notes).await {
                if let Some(existing) = &mut warning {
                    existing.push_str("\n\n");
                    existing.push_str(&format!(
                        "Loaded metadata but failed to persist normalized timestamps: {}",
                        error
                    ));
                } else {
                    warning = Some(format!(
                        "Loaded metadata but failed to persist normalized timestamps: {}",
                        error
                    ));
                }
            }
        } else {
            if let Err(error) = index_sync::sync_metadata(&self.notebook_path, &notes).await {
                if let Some(existing) = &mut warning {
                    existing.push_str("\n\n");
                    existing.push_str(&format!(
                        "Loaded metadata but failed to synchronize engine index: {}",
                        error
                    ));
                } else {
                    warning = Some(format!(
                        "Loaded metadata but failed to synchronize engine index: {}",
                        error
                    ));
                }
            }
        }

        Ok(MetadataLoadResult { notes, warning })
    }

    pub async fn save_metadata(&self, notes: &[NoteMetadata]) -> Result<(), EngineError> {
        let _lock = self.concurrency.acquire_notebook().await?;
        save_metadata(&self.notebook_path, notes).await
    }

    pub async fn load_note_content(&self, rel_path: &str) -> Result<String, EngineError> {
        notebook_content::load(self, rel_path).await
    }

    pub async fn save_note_content(
        &self,
        rel_path: &str,
        content: &str,
    ) -> Result<(), EngineError> {
        self.save_note_content_internal(rel_path, content, None, false)
            .await
            .map(|_| ())
    }

    pub async fn save_note_content_if_match(
        &self,
        rel_path: &str,
        content: &str,
        expected_revision: Option<&str>,
    ) -> Result<NoteContentSaveResult, EngineError> {
        self.save_note_content_internal(rel_path, content, expected_revision, true)
            .await
    }

    async fn save_note_content_internal(
        &self,
        rel_path: &str,
        content: &str,
        expected_revision: Option<&str>,
        update_metadata: bool,
    ) -> Result<NoteContentSaveResult, EngineError> {
        let _notebook_lock = self.concurrency.acquire_notebook().await?;
        let rel_path_buf = validate_relative_path("note path", rel_path)?;
        let _note_lock = self.concurrency.acquire_note(rel_path).await?;
        let full_note_path = self.notebook_path.join(rel_path_buf).join("note.md");

        if let Some(parent) = full_note_path.parent()
            && let Err(error) = tokio::fs::create_dir_all(parent).await
        {
            return Err(EngineError::storage(
                "save note content",
                format!("Failed to create directory for note: {}", error),
            ));
        }

        let existing_content = match tokio::fs::read_to_string(&full_note_path).await {
            Ok(existing) => Some(existing),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => {
                return Err(EngineError::storage(
                    "save note content",
                    format!("Failed to read existing note before save: {}", error),
                ));
            }
        };

        let current_revision = existing_content
            .as_deref()
            .map(note_content_revision)
            .unwrap_or_else(|| note_content_revision(""));
        if let Some(expected_revision) = expected_revision
            && expected_revision != "*"
            && expected_revision != current_revision
        {
            return Err(EngineError::conflict(
                "save note content",
                format!(
                    "expected revision '{}' but found '{}'",
                    expected_revision, current_revision
                ),
            ));
        }

        if existing_content.as_deref() == Some(content) {
            let metadata_revision = if update_metadata {
                let metadata = self.load_metadata_unlocked().await?;
                let serialized = serde_json::to_string(&metadata.notes).unwrap_or_default();
                note_content_revision(&serialized)
            } else {
                String::new()
            };
            return Ok(NoteContentSaveResult {
                note_revision: current_revision,
                metadata_revision,
            });
        }

        write_text_file_atomically(&full_note_path, content).await?;

        // Sync engine state
        let last_updated = current_timestamp_rfc3339();
        let mut engine_state = index_sync::load(&self.notebook_path).await;
        let labels = engine_state
            .search_index
            .documents
            .get(rel_path)
            .map(|doc| doc.labels.clone())
            .unwrap_or_default();
        engine_state.process_document(rel_path, content, &labels, Some(last_updated));
        index_sync::save(&self.notebook_path, &engine_state).await?;

        if !update_metadata {
            return Ok(NoteContentSaveResult {
                note_revision: note_content_revision(content),
                metadata_revision: String::new(),
            });
        }

        // Keep the metadata timestamp in the same notebook lock transaction as the content
        // write. Otherwise a subsequent metadata save can observe a newer file mtime and
        // unexpectedly invalidate the UI's metadata ETag.
        let mut metadata = self.load_metadata_unlocked().await?.notes;
        if let Some(note) = metadata.iter_mut().find(|note| note.rel_path == rel_path) {
            note.last_updated = Some(current_timestamp_rfc3339());
        }
        save_metadata(&self.notebook_path, &metadata).await?;
        let serialized = serde_json::to_string(&metadata).unwrap_or_default();

        Ok(NoteContentSaveResult {
            note_revision: note_content_revision(content),
            metadata_revision: note_content_revision(&serialized),
        })
    }
}

pub fn note_content_revision(content: &str) -> String {
    blake3::hash(content.as_bytes()).to_hex().to_string()
}
