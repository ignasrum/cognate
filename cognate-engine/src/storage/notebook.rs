use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{EngineError, NotebookEngineState};

const METADATA_FILE_NAME: &str = "metadata.json";
const METADATA_BACKUP_FILE_NAME: &str = "metadata.json.bak";
const STAGED_DELETE_PREFIX: &str = ".cognate_txn_delete_";
const STAGED_DELETE_CLEANUP_GRACE_NANOS: u128 = 5 * 60 * 1_000_000_000;
#[allow(dead_code)]
const FAIL_ATOMIC_RENAME_MARKER: &str = ".cognate_fail_atomic_rename";
#[allow(dead_code)]
const FAIL_DELETE_ROLLBACK_MARKER: &str = ".cognate_fail_delete_rollback";
#[allow(dead_code)]
const FAIL_MOVE_ROLLBACK_MARKER: &str = ".cognate_fail_move_rollback";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteMetadata {
    pub rel_path: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookMetadata {
    pub notes: Vec<NoteMetadata>,
}

#[derive(Debug, Clone)]
pub struct MetadataLoadResult {
    pub notes: Vec<NoteMetadata>,
    pub warning: Option<String>,
}

pub fn current_timestamp_rfc3339() -> String {
    OffsetDateTime::from_unix_timestamp(OffsetDateTime::now_utc().unix_timestamp())
        .ok()
        .and_then(|timestamp| timestamp.format(&Rfc3339).ok())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

fn format_system_time_rfc3339(timestamp: SystemTime) -> Option<String> {
    OffsetDateTime::from_unix_timestamp(OffsetDateTime::from(timestamp).unix_timestamp())
        .ok()
        .and_then(|dt| dt.format(&Rfc3339).ok())
}

fn normalize_rfc3339_to_seconds(timestamp: &str) -> String {
    if let Some(dot_index) = timestamp.find('.') {
        let base = &timestamp[..dot_index];
        let remainder = &timestamp[dot_index + 1..];
        if let Some(tz_index) = remainder.find(['Z', '+', '-']) {
            return format!("{}{}", base, &remainder[tz_index..]);
        }
        return base.to_string();
    }
    timestamp.to_string()
}

fn parse_rfc3339_timestamp(timestamp: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(timestamp, &Rfc3339).ok()
}

fn reconcile_last_updated_timestamp(
    existing_timestamp: Option<&str>,
    note_file_modified_time: Option<SystemTime>,
) -> Option<String> {
    let file_timestamp = note_file_modified_time.and_then(|modified_time| {
        format_system_time_rfc3339(modified_time).and_then(|formatted| {
            parse_rfc3339_timestamp(&formatted).map(|parsed| (parsed, formatted))
        })
    });

    match existing_timestamp {
        Some(existing_timestamp) => {
            let normalized = normalize_rfc3339_to_seconds(existing_timestamp);
            let existing_parsed = parse_rfc3339_timestamp(&normalized);

            if let Some((file_parsed, file_formatted)) = file_timestamp
                && existing_parsed.is_none_or(|existing| file_parsed > existing)
            {
                return Some(file_formatted);
            }

            Some(normalized)
        }
        None => file_timestamp.map(|(_, formatted)| formatted),
    }
}

fn validate_relative_path(path_kind: &'static str, value: &str) -> Result<PathBuf, EngineError> {
    if value.trim().is_empty() {
        return Err(EngineError::validation(
            "path validation",
            format!("Invalid {} '{}': path cannot be empty.", path_kind, value),
        ));
    }

    let mut has_normal_component = false;
    for component in Path::new(value).components() {
        match component {
            Component::Normal(_) => has_normal_component = true,
            Component::CurDir => {
                return Err(EngineError::validation(
                    "path validation",
                    format!(
                        "Invalid {} '{}': '.' path components are not allowed.",
                        path_kind, value
                    ),
                ));
            }
            Component::ParentDir => {
                return Err(EngineError::validation(
                    "path validation",
                    format!(
                        "Invalid {} '{}': '..' path components are not allowed.",
                        path_kind, value
                    ),
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(EngineError::validation(
                    "path validation",
                    format!(
                        "Invalid {} '{}': absolute paths are not allowed.",
                        path_kind, value
                    ),
                ));
            }
        }
    }

    if !has_normal_component {
        return Err(EngineError::validation(
            "path validation",
            format!(
                "Invalid {} '{}': path must contain at least one normal component.",
                path_kind, value
            ),
        ));
    }

    Ok(PathBuf::from(value))
}

async fn ensure_path_within_notebook_if_canonicalizable(
    notebook_path: &Path,
    target_path: &Path,
    rel_path: &str,
    outside_error_prefix: &str,
) -> Result<(), EngineError> {
    if let Ok(canonical_notebook_path) = tokio::fs::canonicalize(notebook_path).await
        && let Ok(canonical_target_path) = tokio::fs::canonicalize(target_path).await
        && !canonical_target_path.starts_with(&canonical_notebook_path)
    {
        return Err(EngineError::validation(
            "path containment",
            format!("{} '{}'", outside_error_prefix, rel_path),
        ));
    }
    Ok(())
}

async fn cleanup_stale_staged_delete_entries(notebook_path: &Path) {
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    let mut entries = match tokio::fs::read_dir(notebook_path).await {
        Ok(dir) => dir,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();

        if !file_name.starts_with(STAGED_DELETE_PREFIX) {
            continue;
        }

        let timestamp_nanos = match file_name.rsplit('_').next() {
            Some(ts) => match ts.parse::<u128>() {
                Ok(parsed) => parsed,
                Err(_) => continue,
            },
            None => continue,
        };

        let age_nanos = now_nanos.saturating_sub(timestamp_nanos);
        if age_nanos < STAGED_DELETE_CLEANUP_GRACE_NANOS {
            continue;
        }

        let staged_path = entry.path();
        if let Ok(meta) = tokio::fs::metadata(&staged_path).await {
            let _ = if meta.is_dir() {
                tokio::fs::remove_dir_all(&staged_path).await
            } else {
                tokio::fs::remove_file(&staged_path).await
            };
        }
    }
}

fn build_atomic_temp_path(target_path: &Path) -> Result<PathBuf, std::io::Error> {
    let parent = target_path.parent().ok_or_else(|| {
        std::io::Error::new(
            ErrorKind::InvalidInput,
            format!(
                "Cannot atomically write '{}': target has no parent directory.",
                target_path.display()
            ),
        )
    })?;

    let target_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("cognate_tmp");
    let timestamp_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    Ok(parent.join(format!(
        ".{}.cognate_tmp_{}_{}",
        target_name,
        process::id(),
        timestamp_nanos
    )))
}

async fn atomic_rename(from: &Path, to: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = to.parent()
        && tokio::fs::try_exists(&parent.join(FAIL_ATOMIC_RENAME_MARKER))
            .await
            .unwrap_or(false)
        && to.file_name().and_then(|name| name.to_str()) != Some(METADATA_BACKUP_FILE_NAME)
    {
        return Err(std::io::Error::other(format!(
            "Simulated atomic rename failure for '{}'",
            to.display()
        )));
    }

    tokio::fs::rename(from, to).await
}

async fn atomic_write_string(target_path: &Path, content: &str) -> Result<(), std::io::Error> {
    let temp_path = build_atomic_temp_path(target_path)?;
    tokio::fs::write(&temp_path, content).await?;

    if let Err(rename_error) = atomic_rename(&temp_path, target_path).await {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(rename_error);
    }

    Ok(())
}

async fn write_text_file_atomically(target_path: &Path, content: &str) -> Result<(), EngineError> {
    if let Some(parent) = target_path.parent()
        && let Err(error) = tokio::fs::create_dir_all(parent).await
    {
        return Err(EngineError::storage(
            "atomic write",
            format!(
                "Failed to create parent directory for '{}': {}",
                target_path.display(),
                error
            ),
        ));
    }

    atomic_write_string(target_path, content)
        .await
        .map_err(|error| {
            EngineError::storage(
                "atomic write",
                format!(
                    "Failed to atomically write '{}': {}",
                    target_path.display(),
                    error
                ),
            )
        })
}

async fn atomic_write_bytes(target_path: &Path, content: &[u8]) -> Result<(), std::io::Error> {
    let temp_path = build_atomic_temp_path(target_path)?;
    tokio::fs::write(&temp_path, content).await?;

    if let Err(rename_error) = atomic_rename(&temp_path, target_path).await {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(rename_error);
    }

    Ok(())
}

async fn write_bytes_file_atomically(
    target_path: &Path,
    content: &[u8],
) -> Result<(), EngineError> {
    if let Some(parent) = target_path.parent()
        && let Err(error) = tokio::fs::create_dir_all(parent).await
    {
        return Err(EngineError::storage(
            "atomic write",
            format!(
                "Failed to create parent directory for '{}': {}",
                target_path.display(),
                error
            ),
        ));
    }

    atomic_write_bytes(target_path, content)
        .await
        .map_err(|error| {
            EngineError::storage(
                "atomic write",
                format!(
                    "Failed to atomically write '{}': {}",
                    target_path.display(),
                    error
                ),
            )
        })
}

async fn load_engine_state_from_disk(notebook_path: &Path) -> NotebookEngineState {
    let index_file_path = notebook_path.join(".cognate_index.bin");
    if tokio::fs::try_exists(&index_file_path)
        .await
        .unwrap_or(false)
        && let Ok(bytes) = tokio::fs::read(&index_file_path).await
        && let Ok(state) = NotebookEngineState::load_from_bytes(&bytes)
    {
        return state;
    }
    NotebookEngineState::new()
}

async fn save_engine_state_to_disk(notebook_path: &Path, state: &NotebookEngineState) {
    let index_file_path = notebook_path.join(".cognate_index.bin");
    if let Ok(bytes) = state.save_to_bytes() {
        let bytes_ref: &[u8] = &bytes;
        let _ = write_bytes_file_atomically(&index_file_path, bytes_ref).await;
    }
}

async fn sync_engine_metadata(
    notebook_path: &Path,
    notes: &[NoteMetadata],
) -> Result<(), EngineError> {
    let mut engine_state = load_engine_state_from_disk(notebook_path).await;
    let mut changed = false;

    let current_paths: HashSet<String> = notes.iter().map(|n| n.rel_path.clone()).collect();

    // Remove deleted documents
    let existing_paths: Vec<String> = engine_state
        .search_index
        .documents
        .keys()
        .cloned()
        .collect();
    for path in existing_paths {
        if !current_paths.contains(&path) {
            engine_state.remove_document(&path);
            changed = true;
        }
    }

    // Sync labels and timestamps
    for note in notes {
        let needs_indexing = match engine_state.search_index.documents.get(&note.rel_path) {
            Some(doc_meta) => {
                doc_meta.last_updated != note.last_updated || doc_meta.labels != note.labels
            }
            None => true,
        };

        if needs_indexing {
            let note_file_path = notebook_path.join(&note.rel_path).join("note.md");
            if tokio::fs::try_exists(&note_file_path)
                .await
                .unwrap_or(false)
            {
                let content = tokio::fs::read_to_string(&note_file_path)
                    .await
                    .unwrap_or_default();
                engine_state.process_document(
                    &note.rel_path,
                    &content,
                    &note.labels,
                    note.last_updated.clone(),
                );
                changed = true;
            }
        }
    }

    if changed {
        save_engine_state_to_disk(notebook_path, &engine_state).await;
    }

    Ok(())
}

async fn snapshot_known_good_metadata(
    metadata_path: &Path,
    backup_path: &Path,
) -> Result<(), EngineError> {
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

async fn save_metadata(notebook_path: &Path, notes: &[NoteMetadata]) -> Result<(), EngineError> {
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

    snapshot_known_good_metadata(&metadata_path, &backup_path).await?;

    let json_string = serde_json::to_string_pretty(&notebook_metadata).map_err(|error| {
        EngineError::storage(
            "save metadata",
            format!("Failed to serialize metadata.json: {}", error),
        )
    })?;

    write_text_file_atomically(&metadata_path, &json_string).await?;

    // Sync engine state
    sync_engine_metadata(notebook_path, notes).await?;

    Ok(())
}

fn build_transaction_staging_path(
    notebook_path: &Path,
    rel_path: &str,
    operation: &str,
) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sanitized_rel_path = Path::new(rel_path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(component) => Some(component.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<String>>()
        .join("__");

    notebook_path.join(format!(
        ".cognate_txn_{}_{}_{}",
        operation, sanitized_rel_path, timestamp
    ))
}

async fn remove_empty_parent_directories(notebook_path: &Path, deleted_note_dir_path: &Path) {
    let canonical_notebook_path = tokio::fs::canonicalize(notebook_path).await.ok();
    let mut current_parent = deleted_note_dir_path.parent().map(Path::to_path_buf);

    while let Some(parent_path) = current_parent {
        if parent_path == notebook_path {
            break;
        }

        if let Some(canonical_root) = canonical_notebook_path.as_ref() {
            if let Ok(canonical_parent) = tokio::fs::canonicalize(&parent_path).await {
                if canonical_parent == *canonical_root
                    || !canonical_parent.starts_with(canonical_root)
                {
                    break;
                }
            } else {
                break;
            }
        } else if parent_path == notebook_path || !parent_path.starts_with(notebook_path) {
            break;
        }

        match tokio::fs::remove_dir(&parent_path).await {
            Ok(()) => {
                current_parent = parent_path.parent().map(Path::to_path_buf);
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                current_parent = parent_path.parent().map(Path::to_path_buf);
            }
            Err(e) if e.kind() == ErrorKind::DirectoryNotEmpty => {
                break;
            }
            Err(_) => {
                break;
            }
        }
    }
}

async fn rollback_rename(
    staged_or_new_path: &Path,
    original_path: &Path,
    notebook_root: &Path,
    fail_marker: &str,
) -> Result<(), EngineError> {
    if tokio::fs::try_exists(&notebook_root.join(fail_marker))
        .await
        .unwrap_or(false)
    {
        return Err(EngineError::recovery(
            "rollback rename",
            format!(
                "simulated rollback rename failure from '{}' to '{}'",
                staged_or_new_path.display(),
                original_path.display()
            ),
        ));
    }

    tokio::fs::rename(staged_or_new_path, original_path)
        .await
        .map_err(|error| {
            EngineError::recovery(
                "rollback rename",
                format!(
                    "Failed to rollback filesystem rename from '{}' to '{}': {}",
                    staged_or_new_path.display(),
                    original_path.display(),
                    error
                ),
            )
        })
}

pub struct NotebookManager {
    notebook_path: PathBuf,
}

impl NotebookManager {
    pub fn new(path: &Path) -> Self {
        Self {
            notebook_path: path.to_path_buf(),
        }
    }

    pub fn notebook_path(&self) -> &Path {
        &self.notebook_path
    }

    pub async fn load_engine_state(&self) -> NotebookEngineState {
        load_engine_state_from_disk(&self.notebook_path).await
    }

    pub async fn save_engine_state(&self, state: &NotebookEngineState) {
        save_engine_state_to_disk(&self.notebook_path, state).await
    }

    pub async fn get_note_modified_time(&self, rel_path: &str) -> Option<SystemTime> {
        let note_file_path = self.notebook_path.join(rel_path).join("note.md");
        tokio::fs::metadata(note_file_path)
            .await
            .ok()
            .and_then(|metadata| metadata.modified().ok())
    }

    pub async fn load_metadata(&self) -> Result<MetadataLoadResult, EngineError> {
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

        let mut notes = metadata.notes;
        let mut metadata_changed = false;

        for note in &mut notes {
            let note_file_path = self.notebook_path.join(&note.rel_path).join("note.md");
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
            if let Err(error) = sync_engine_metadata(&self.notebook_path, &notes).await {
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
        save_metadata(&self.notebook_path, notes).await
    }

    pub async fn load_note_content(&self, rel_path: &str) -> Result<String, EngineError> {
        let rel_path_buf = validate_relative_path("note path", rel_path)?;
        let note_file_path = self.notebook_path.join(rel_path_buf).join("note.md");
        tokio::fs::read_to_string(&note_file_path)
            .await
            .map_err(|err| {
                EngineError::storage(
                    "load note content",
                    format!(
                        "Failed to read note file '{}': {}",
                        note_file_path.display(),
                        err
                    ),
                )
            })
    }

    pub async fn create_note(
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
        let rel_path_buf = validate_relative_path("relative path", rel_path)?;
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

        if tokio::fs::try_exists(&note_dir_path).await.unwrap_or(false) {
            let transaction_path =
                build_transaction_staging_path(&self.notebook_path, rel_path, "delete");

            if let Err(error) = tokio::fs::rename(&note_dir_path, &transaction_path).await {
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

            if let Some(staged_path) = staged_delete_path
                && let Err(rollback_error) = rollback_rename(
                    &staged_path,
                    &note_dir_path,
                    &self.notebook_path,
                    FAIL_DELETE_ROLLBACK_MARKER,
                )
                .await
            {
                return Err(EngineError::recovery(
                    "delete note rollback",
                    format!(
                        "{} Rollback failed while restoring filesystem state: {}",
                        metadata_error, rollback_error
                    ),
                ));
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

    pub async fn move_note(
        &self,
        from_rel: &str,
        to_rel: &str,
        metadata: &mut Vec<NoteMetadata>,
    ) -> Result<String, EngineError> {
        let from_rel_buf = validate_relative_path("current relative path", from_rel)?;
        let to_rel_buf = validate_relative_path("new relative path", to_rel)?;

        let current_fs_path = self.notebook_path.join(&from_rel_buf);
        let new_fs_path = self.notebook_path.join(&to_rel_buf);

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
        let is_moving_note_dir = tokio::fs::try_exists(&current_fs_path.join("note.md"))
            .await
            .unwrap_or(false);

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

    pub async fn save_note_content(
        &self,
        rel_path: &str,
        content: &str,
    ) -> Result<(), EngineError> {
        let rel_path_buf = validate_relative_path("note path", rel_path)?;
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

        if existing_content.as_deref() == Some(content) {
            return Ok(());
        }

        write_text_file_atomically(&full_note_path, content).await?;

        // Sync engine state
        let last_updated = current_timestamp_rfc3339();
        let mut engine_state = load_engine_state_from_disk(&self.notebook_path).await;
        let labels = engine_state
            .search_index
            .documents
            .get(rel_path)
            .map(|doc| doc.labels.clone())
            .unwrap_or_default();
        engine_state.process_document(rel_path, content, &labels, Some(last_updated));
        save_engine_state_to_disk(&self.notebook_path, &engine_state).await;

        Ok(())
    }
}
