use std::path::PathBuf;

use super::fs_utils::{
    ensure_path_within_notebook_if_canonicalizable, remove_empty_parent_directories,
    rollback_rename, validate_relative_path, write_text_file_atomically,
};
use super::metadata::NoteMetadata;
use super::notebook::{
    FAIL_DELETE_ROLLBACK_MARKER, FAIL_MOVE_ROLLBACK_MARKER, NotebookManager,
    current_timestamp_rfc3339, save_metadata,
};
use super::transactions::build_staging_path;
use crate::EngineError;

impl NotebookManager {
    pub async fn create_note(
        &self,
        rel_path: &str,
        metadata: &mut Vec<NoteMetadata>,
    ) -> Result<NoteMetadata, EngineError> {
        let _lock = self.concurrency.acquire_notebook().await?;
        self.create_note_unlocked(rel_path, metadata).await
    }

    pub async fn create_note_atomic(&self, rel_path: &str) -> Result<NoteMetadata, EngineError> {
        let _lock = self.concurrency.acquire_notebook().await?;
        let mut metadata = self.load_metadata_unlocked().await?.notes;
        self.create_note_unlocked(rel_path, &mut metadata).await
    }

    async fn create_note_unlocked(
        &self,
        rel_path: &str,
        metadata: &mut Vec<NoteMetadata>,
    ) -> Result<NoteMetadata, EngineError> {
        let rel_path_buf = validate_relative_path("relative path", rel_path)?;
        let note_dir_path = self.notebook_path.join(&rel_path_buf);
        let note_file_path = note_dir_path.join("note.md");

        ensure_path_within_notebook_if_canonicalizable(
            &self.notebook_path,
            &note_dir_path,
            rel_path,
            "Cannot create note outside the notebook directory:",
        )
        .await?;

        if metadata.iter().any(|note| note.rel_path == rel_path) {
            return Err(EngineError::validation(
                "create note",
                format!(
                    "A note with the path '{}' already exists in metadata.",
                    rel_path
                ),
            ));
        }

        if tokio::fs::try_exists(&note_dir_path).await.unwrap_or(false)
            || tokio::fs::try_exists(&note_file_path)
                .await
                .unwrap_or(false)
        {
            return Err(EngineError::validation(
                "create note",
                format!("A directory or file already exists at '{}'.", rel_path),
            ));
        }

        if let Err(error) = tokio::fs::create_dir_all(&note_dir_path).await {
            return Err(EngineError::storage(
                "create note",
                format!("Failed to create directory for new note: {}", error),
            ));
        }

        if let Err(error) = write_text_file_atomically(&note_file_path, "").await {
            let _ = tokio::fs::remove_dir_all(&note_dir_path).await;
            return Err(EngineError::storage(
                "create note",
                format!("Failed to create note file: {}", error),
            ));
        }

        let new_note_metadata = NoteMetadata {
            rel_path: rel_path.to_string(),
            labels: Vec::new(),
            last_updated: Some(current_timestamp_rfc3339()),
        };

        let previous_notes = metadata.clone();
        metadata.push(new_note_metadata.clone());

        if let Err(error) = save_metadata(&self.notebook_path, metadata).await {
            *metadata = previous_notes;
            let cleanup_result = tokio::fs::remove_dir_all(&note_dir_path).await;
            if let Err(cleanup_error) = cleanup_result {
                return Err(EngineError::recovery(
                    "create note rollback",
                    format!(
                        "Failed to save metadata after creating note: {}. Rollback cleanup failed: {}",
                        error, cleanup_error
                    ),
                ));
            }
            return Err(EngineError::storage(
                "create note",
                format!("Failed to save metadata after creating note: {}", error),
            ));
        }

        Ok(new_note_metadata)
    }

    pub async fn delete_note(
        &self,
        rel_path: &str,
        metadata: &mut Vec<NoteMetadata>,
    ) -> Result<(), EngineError> {
        let _lock = self.concurrency.acquire_notebook().await?;
        self.delete_note_unlocked(rel_path, metadata).await
    }

    pub async fn delete_note_atomic(&self, rel_path: &str) -> Result<(), EngineError> {
        let _lock = self.concurrency.acquire_notebook().await?;
        let mut metadata = self.load_metadata_unlocked().await?.notes;
        self.delete_note_unlocked(rel_path, &mut metadata).await
    }

    async fn delete_note_unlocked(
        &self,
        rel_path: &str,
        metadata: &mut Vec<NoteMetadata>,
    ) -> Result<(), EngineError> {
        let rel_path_buf = validate_relative_path("relative path", rel_path)?;
        let _note_lock = self.concurrency.acquire_note(rel_path).await?;
        let note_dir_path = self.notebook_path.join(&rel_path_buf);

        if let Ok(canonical_notebook_path) = tokio::fs::canonicalize(&self.notebook_path).await {
            if let Ok(canonical_note_dir_path) = tokio::fs::canonicalize(&note_dir_path).await {
                if !canonical_note_dir_path.starts_with(&canonical_notebook_path) {
                    return Err(EngineError::validation(
                        "delete note",
                        format!(
                            "Cannot delete path outside the notebook directory: '{}'",
                            rel_path
                        ),
                    ));
                }
            } else {
                if !tokio::fs::try_exists(&note_dir_path).await.unwrap_or(false) {
                    return Err(EngineError::validation(
                        "delete note",
                        format!("Path '{}' does not exist within the notebook.", rel_path),
                    ));
                }
            }
        } else {
            if !tokio::fs::try_exists(&note_dir_path).await.unwrap_or(false) {
                return Err(EngineError::validation(
                    "delete note",
                    format!("Path '{}' does not exist within the notebook.", rel_path),
                ));
            }
        }

        let previous_notes = metadata.clone();
        let mut metadata_changed = false;
        if let Some(index) = metadata.iter().position(|note| note.rel_path == rel_path) {
            metadata.remove(index);
            metadata_changed = true;
        }

        let mut staged_delete_path: Option<PathBuf> = None;
        let mut staged_note_contents = false;

        if tokio::fs::try_exists(&note_dir_path).await.unwrap_or(false) {
            let transaction_path = build_staging_path(&self.notebook_path, rel_path, "delete");

            let note_file_path = note_dir_path.join("note.md");
            if tokio::fs::try_exists(&note_file_path)
                .await
                .unwrap_or(false)
            {
                staged_note_contents = true;
                if let Err(error) = tokio::fs::create_dir(&transaction_path).await {
                    return Err(EngineError::storage(
                        "delete note",
                        format!("Failed to stage note contents for deletion: {}", error),
                    ));
                }

                if let Err(error) =
                    tokio::fs::rename(&note_file_path, transaction_path.join("note.md")).await
                {
                    let _ = tokio::fs::remove_dir(&transaction_path).await;
                    return Err(EngineError::storage(
                        "delete note",
                        format!("Failed to stage note content for deletion: {}", error),
                    ));
                }

                let images_path = note_dir_path.join("images");
                if tokio::fs::try_exists(&images_path).await.unwrap_or(false)
                    && let Err(error) =
                        tokio::fs::rename(&images_path, transaction_path.join("images")).await
                {
                    let _ =
                        tokio::fs::rename(transaction_path.join("note.md"), &note_file_path).await;
                    let _ = tokio::fs::remove_dir(&transaction_path).await;
                    return Err(EngineError::storage(
                        "delete note",
                        format!("Failed to stage note attachments for deletion: {}", error),
                    ));
                }
            } else if let Err(error) = tokio::fs::rename(&note_dir_path, &transaction_path).await {
                return Err(EngineError::storage(
                    "delete note",
                    format!("Failed to stage item for deletion on filesystem: {}", error),
                ));
            }

            staged_delete_path = Some(transaction_path);
        }

        if metadata_changed
            && let Err(metadata_error) = save_metadata(&self.notebook_path, metadata).await
        {
            *metadata = previous_notes;

            if let Some(staged_path) = staged_delete_path {
                let rollback_result = if staged_note_contents {
                    Self::restore_staged_note_contents(
                        &staged_path,
                        &note_dir_path,
                        &self.notebook_path,
                    )
                    .await
                } else {
                    rollback_rename(
                        &staged_path,
                        &note_dir_path,
                        &self.notebook_path,
                        FAIL_DELETE_ROLLBACK_MARKER,
                    )
                    .await
                };
                if let Err(rollback_error) = rollback_result {
                    return Err(EngineError::recovery(
                        "delete note rollback",
                        format!(
                            "{} Rollback failed while restoring filesystem state: {}",
                            metadata_error, rollback_error
                        ),
                    ));
                }
            }

            return Err(metadata_error);
        }

        if let Some(staged_path) = staged_delete_path
            && let Err(_) = tokio::fs::remove_dir_all(&staged_path).await
        {
            // Just warn or ignore since staged cleanup will get it next time
        }

        remove_empty_parent_directories(&self.notebook_path, &note_dir_path).await;

        Ok(())
    }

    async fn restore_staged_note_contents(
        staged_path: &std::path::Path,
        note_dir_path: &std::path::Path,
        notebook_path: &std::path::Path,
    ) -> Result<(), EngineError> {
        if tokio::fs::try_exists(notebook_path.join(FAIL_DELETE_ROLLBACK_MARKER))
            .await
            .unwrap_or(false)
        {
            return Err(EngineError::storage(
                "delete note rollback",
                "simulated delete rollback failure",
            ));
        }
        tokio::fs::rename(staged_path.join("note.md"), note_dir_path.join("note.md"))
            .await
            .map_err(|error| EngineError::storage("delete note rollback", error.to_string()))?;
        let staged_images = staged_path.join("images");
        if tokio::fs::try_exists(&staged_images).await.unwrap_or(false) {
            tokio::fs::rename(staged_images, note_dir_path.join("images"))
                .await
                .map_err(|error| EngineError::storage("delete note rollback", error.to_string()))?;
        }
        tokio::fs::remove_dir(staged_path)
            .await
            .map_err(|error| EngineError::storage("delete note rollback", error.to_string()))
    }

    pub async fn move_note(
        &self,
        from_rel: &str,
        to_rel: &str,
        metadata: &mut Vec<NoteMetadata>,
    ) -> Result<String, EngineError> {
        let _lock = self.concurrency.acquire_notebook().await?;
        self.move_note_unlocked(from_rel, to_rel, metadata).await
    }

    pub async fn move_note_atomic(
        &self,
        from_rel: &str,
        to_rel: &str,
    ) -> Result<String, EngineError> {
        let _lock = self.concurrency.acquire_notebook().await?;
        let mut metadata = self.load_metadata_unlocked().await?.notes;
        self.move_note_unlocked(from_rel, to_rel, &mut metadata)
            .await
    }

    async fn move_note_unlocked(
        &self,
        from_rel: &str,
        to_rel: &str,
        metadata: &mut Vec<NoteMetadata>,
    ) -> Result<String, EngineError> {
        let from_rel_buf = validate_relative_path("current relative path", from_rel)?;
        let to_rel_buf = validate_relative_path("new relative path", to_rel)?;

        // All note-directory mutations use notebook -> note lock ordering. The
        // notebook lock is already held by the public wrapper; lock both names
        // in deterministic order so concurrent moves cannot deadlock.
        let (first_rel, second_rel) = if from_rel <= to_rel {
            (from_rel, to_rel)
        } else {
            (to_rel, from_rel)
        };
        let _first_note_lock = self.concurrency.acquire_note(first_rel).await?;
        let _second_note_lock = if first_rel == second_rel {
            None
        } else {
            Some(self.concurrency.acquire_note(second_rel).await?)
        };

        let current_fs_path = self.notebook_path.join(&from_rel_buf);
        let new_fs_path = self.notebook_path.join(&to_rel_buf);

        let is_descendant_move =
            new_fs_path != current_fs_path && new_fs_path.starts_with(&current_fs_path);

        if !tokio::fs::try_exists(&current_fs_path)
            .await
            .unwrap_or(false)
        {
            return Err(EngineError::validation(
                "move note",
                format!("Item at path '{}' not found on the filesystem.", from_rel),
            ));
        }

        if let Ok(canonical_notebook_path) = tokio::fs::canonicalize(&self.notebook_path).await {
            if let Ok(canonical_current_path) = tokio::fs::canonicalize(&current_fs_path).await {
                if !canonical_current_path.starts_with(&canonical_notebook_path) {
                    return Err(EngineError::validation(
                        "move note",
                        format!(
                            "Cannot move/rename item from path outside the notebook directory: '{}'",
                            from_rel
                        ),
                    ));
                }
            } else {
                return Err(EngineError::storage(
                    "move note",
                    format!("Failed to canonicalize current item path: '{}'", from_rel),
                ));
            }

            ensure_path_within_notebook_if_canonicalizable(
                &self.notebook_path,
                &new_fs_path,
                to_rel,
                "Cannot move/rename item to path outside the notebook directory:",
            )
            .await?;
        }

        if tokio::fs::try_exists(&new_fs_path).await.unwrap_or(false) {
            if let Ok(canonical_notebook_path) = tokio::fs::canonicalize(&self.notebook_path).await
            {
                if let Ok(canonical_new_fs_path) = tokio::fs::canonicalize(&new_fs_path).await
                    && canonical_new_fs_path.starts_with(&canonical_notebook_path)
                {
                    return Err(EngineError::validation(
                        "move note",
                        format!("An item already exists at the target path '{}'.", to_rel),
                    ));
                }
            } else {
                return Err(EngineError::validation(
                    "move note",
                    format!("An item already exists at the target path '{}'.", to_rel),
                ));
            }
        }

        let is_moving_note_dir = tokio::fs::try_exists(&current_fs_path.join("note.md"))
            .await
            .unwrap_or(false);
        if is_descendant_move {
            if !is_moving_note_dir {
                return Err(EngineError::validation(
                    "move note",
                    format!(
                        "Cannot move folder '{}' into one of its own descendants '{}'.",
                        from_rel, to_rel
                    ),
                ));
            }
            return self
                .move_note_into_descendant(
                    from_rel,
                    to_rel,
                    &current_fs_path,
                    &new_fs_path,
                    metadata,
                )
                .await;
        }

        if let Some(parent) = new_fs_path.parent()
            && !tokio::fs::try_exists(parent).await.unwrap_or(false)
            && let Err(error) = tokio::fs::create_dir_all(parent).await
        {
            return Err(EngineError::storage(
                "move note",
                format!(
                    "Failed to create parent directories for new path: {}",
                    error
                ),
            ));
        }

        let previous_notes = metadata.clone();
        if let Err(error) = tokio::fs::rename(&current_fs_path, &new_fs_path).await {
            return Err(EngineError::storage(
                "move note",
                format!(
                    "Failed to move/rename item from '{}' to '{}': {}",
                    from_rel, to_rel, error
                ),
            ));
        }

        let mut updated_metadata = false;
        if is_moving_note_dir {
            if let Some(note) = metadata.iter_mut().find(|note| note.rel_path == from_rel) {
                note.rel_path = to_rel.to_string();
                updated_metadata = true;
            }
        } else {
            let old_prefix = format!("{}/", from_rel);
            let new_prefix = format!("{}/", to_rel);

            for note in metadata.iter_mut() {
                if note.rel_path.starts_with(&old_prefix) {
                    let suffix = note.rel_path.trim_start_matches(&old_prefix);
                    note.rel_path = format!("{}{}", new_prefix, suffix);
                    updated_metadata = true;
                } else if note.rel_path == from_rel {
                    note.rel_path = to_rel.to_string();
                    updated_metadata = true;
                }
            }
        }

        if updated_metadata
            && let Err(metadata_error) = save_metadata(&self.notebook_path, metadata).await
        {
            *metadata = previous_notes;
            if let Err(rollback_error) = rollback_rename(
                &new_fs_path,
                &current_fs_path,
                &self.notebook_path,
                FAIL_MOVE_ROLLBACK_MARKER,
            )
            .await
            {
                return Err(EngineError::recovery(
                    "move note rollback",
                    format!(
                        "{} Rollback failed while restoring filesystem state: {}",
                        metadata_error, rollback_error
                    ),
                ));
            }
            return Err(metadata_error);
        }

        Ok(to_rel.to_string())
    }

    async fn move_note_into_descendant(
        &self,
        from_rel: &str,
        to_rel: &str,
        current_fs_path: &std::path::Path,
        new_fs_path: &std::path::Path,
        metadata: &mut Vec<NoteMetadata>,
    ) -> Result<String, EngineError> {
        tokio::fs::create_dir(new_fs_path).await.map_err(|error| {
            EngineError::storage(
                "move note",
                format!("Failed to create nested note directory: {error}"),
            )
        })?;

        let note_file = current_fs_path.join("note.md");
        let nested_note_file = new_fs_path.join("note.md");
        if let Err(error) = tokio::fs::rename(&note_file, &nested_note_file).await {
            let _ = tokio::fs::remove_dir(new_fs_path).await;
            return Err(EngineError::storage(
                "move note",
                format!("Failed to move note content into nested path: {error}"),
            ));
        }

        let images = current_fs_path.join("images");
        let nested_images = new_fs_path.join("images");
        if tokio::fs::try_exists(&images).await.unwrap_or(false)
            && let Err(error) = tokio::fs::rename(&images, &nested_images).await
        {
            let _ = tokio::fs::rename(&nested_note_file, &note_file).await;
            let _ = tokio::fs::remove_dir(new_fs_path).await;
            return Err(EngineError::storage(
                "move note",
                format!("Failed to move note attachments into nested path: {error}"),
            ));
        }

        let previous_notes = metadata.clone();
        let mut updated_metadata = false;
        if let Some(note) = metadata.iter_mut().find(|note| note.rel_path == from_rel) {
            note.rel_path = to_rel.to_string();
            updated_metadata = true;
        }

        if updated_metadata
            && let Err(metadata_error) = save_metadata(&self.notebook_path, metadata).await
        {
            *metadata = previous_notes;
            if let Err(rollback_error) = self
                .rollback_nested_note_move(
                    current_fs_path,
                    new_fs_path,
                    &nested_note_file,
                    &nested_images,
                )
                .await
            {
                return Err(EngineError::recovery(
                    "move note rollback",
                    format!(
                        "{} Rollback failed while restoring nested note: {}",
                        metadata_error, rollback_error
                    ),
                ));
            }
            return Err(metadata_error);
        }

        Ok(to_rel.to_string())
    }

    async fn rollback_nested_note_move(
        &self,
        current_fs_path: &std::path::Path,
        new_fs_path: &std::path::Path,
        nested_note_file: &std::path::Path,
        nested_images: &std::path::Path,
    ) -> Result<(), EngineError> {
        if tokio::fs::try_exists(self.notebook_path.join(FAIL_MOVE_ROLLBACK_MARKER))
            .await
            .unwrap_or(false)
        {
            return Err(EngineError::storage(
                "move note rollback",
                "simulated move rollback failure",
            ));
        }
        if tokio::fs::try_exists(nested_images).await.unwrap_or(false) {
            tokio::fs::rename(nested_images, current_fs_path.join("images"))
                .await
                .map_err(|error| EngineError::storage("move note rollback", error.to_string()))?;
        }
        tokio::fs::rename(nested_note_file, current_fs_path.join("note.md"))
            .await
            .map_err(|error| EngineError::storage("move note rollback", error.to_string()))?;
        tokio::fs::remove_dir(new_fs_path)
            .await
            .map_err(|error| EngineError::storage("move note rollback", error.to_string()))
    }
}
