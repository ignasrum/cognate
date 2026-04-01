use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::search::{cache_upsert_search_index_note_content, note_file_modified_time};
use super::{
    NoteMetadata, NotebookError, NotebookMetadata, NotebookRelativePath,
    STAGED_DELETE_CLEANUP_GRACE_NANOS, STAGED_DELETE_PREFIX,
};

const METADATA_FILE_NAME: &str = "metadata.json";
const METADATA_BACKUP_FILE_NAME: &str = "metadata.json.bak";
#[cfg(test)]
const FAIL_ATOMIC_RENAME_MARKER: &str = ".cognate_fail_atomic_rename";

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

fn cleanup_stale_staged_delete_entries(notebook_path: &Path) {
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    let read_dir = match fs::read_dir(notebook_path) {
        Ok(entries) => entries,
        Err(_e) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "Warning: Failed to scan notebook directory '{}' for stale staged deletes: {}",
                notebook_path.display(),
                _e
            );
            return;
        }
    };

    for entry_result in read_dir {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(_e) => {
                #[cfg(debug_assertions)]
                eprintln!("Warning: Failed to read notebook directory entry: {}", _e);
                continue;
            }
        };

        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();

        if !file_name.starts_with(STAGED_DELETE_PREFIX) {
            continue;
        }

        let timestamp_nanos = match file_name.rsplit('_').next() {
            Some(ts) => match ts.parse::<u128>() {
                Ok(parsed) => parsed,
                Err(_) => {
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "Warning: Could not parse staged-delete timestamp from '{}'; skipping cleanup for this entry.",
                        file_name
                    );
                    continue;
                }
            },
            None => continue,
        };

        let age_nanos = now_nanos.saturating_sub(timestamp_nanos);
        if age_nanos < STAGED_DELETE_CLEANUP_GRACE_NANOS {
            continue;
        }

        let staged_path = entry.path();
        let removal_result = if staged_path.is_dir() {
            fs::remove_dir_all(&staged_path)
        } else {
            fs::remove_file(&staged_path)
        };

        if let Err(_e) = removal_result {
            #[cfg(debug_assertions)]
            eprintln!(
                "Warning: Failed to remove stale staged delete '{}': {}",
                staged_path.display(),
                _e
            );
        } else {
            #[cfg(debug_assertions)]
            eprintln!(
                "Cleaned up stale staged delete '{}'.",
                staged_path.display()
            );
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

fn atomic_rename(from: &Path, to: &Path) -> Result<(), std::io::Error> {
    #[cfg(test)]
    if let Some(parent) = to.parent()
        && parent.join(FAIL_ATOMIC_RENAME_MARKER).exists()
        && to.file_name().and_then(|name| name.to_str()) != Some(METADATA_BACKUP_FILE_NAME)
    {
        return Err(std::io::Error::other(format!(
            "Simulated atomic rename failure for '{}'",
            to.display()
        )));
    }

    fs::rename(from, to)
}

fn atomic_write_string(target_path: &Path, content: &str) -> Result<(), std::io::Error> {
    let temp_path = build_atomic_temp_path(target_path)?;
    fs::write(&temp_path, content)?;

    if let Err(rename_error) = atomic_rename(&temp_path, target_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(rename_error);
    }

    Ok(())
}

pub(super) fn write_text_file_atomically(
    target_path: &Path,
    content: &str,
) -> Result<(), NotebookError> {
    if let Some(parent) = target_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        return Err(NotebookError::storage(
            "atomic write",
            format!(
                "Failed to create parent directory for '{}': {}",
                target_path.display(),
                error
            ),
        ));
    }

    atomic_write_string(target_path, content).map_err(|error| {
        NotebookError::storage(
            "atomic write",
            format!(
                "Failed to atomically write '{}': {}",
                target_path.display(),
                error
            ),
        )
    })
}

fn metadata_backup_path(notebook_path: &str) -> PathBuf {
    Path::new(notebook_path).join(METADATA_BACKUP_FILE_NAME)
}

fn snapshot_known_good_metadata(
    metadata_path: &Path,
    backup_path: &Path,
) -> Result<(), NotebookError> {
    if !metadata_path.exists() {
        return Ok(());
    }

    let existing_metadata = fs::read_to_string(metadata_path).map_err(|error| {
        NotebookError::recovery(
            "metadata snapshot",
            format!(
                "Failed to read existing metadata at '{}' before backup: {}",
                metadata_path.display(),
                error
            ),
        )
    })?;

    serde_json::from_str::<NotebookMetadata>(&existing_metadata).map_err(|error| {
        NotebookError::recovery(
            "metadata snapshot",
            format!(
                "Refusing to overwrite invalid metadata at '{}': {}",
                metadata_path.display(),
                error
            ),
        )
    })?;

    write_text_file_atomically(backup_path, &existing_metadata).map_err(|error| {
        NotebookError::recovery(
            "metadata snapshot",
            format!(
                "Failed to update metadata recovery copy at '{}': {}",
                backup_path.display(),
                error
            ),
        )
    })
}

fn append_warning(current: &mut Option<String>, warning: String) {
    if let Some(existing) = current {
        existing.push_str("\n\n");
        existing.push_str(&warning);
    } else {
        *current = Some(warning);
    }
}

pub fn save_metadata(notebook_path: &str, notes: &[NoteMetadata]) -> Result<(), NotebookError> {
    #[cfg(debug_assertions)]
    eprintln!(
        "Saving metadata to: {}",
        Path::new(notebook_path).join("metadata.json").display()
    );

    let metadata_path = Path::new(notebook_path).join(METADATA_FILE_NAME);
    let backup_path = metadata_backup_path(notebook_path);

    if let Some(parent) = metadata_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        #[cfg(debug_assertions)]
        eprintln!(
            "Failed to create parent directory for metadata file: {}",
            error
        );
        return Err(NotebookError::storage(
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

    snapshot_known_good_metadata(&metadata_path, &backup_path)?;

    let json_string = serde_json::to_string_pretty(&notebook_metadata).map_err(|error| {
        NotebookError::storage(
            "save metadata",
            format!("Failed to serialize metadata.json: {}", error),
        )
    })?;

    write_text_file_atomically(&metadata_path, &json_string)?;

    #[cfg(debug_assertions)]
    eprintln!("Metadata saved successfully.");
    Ok(())
}

pub async fn load_notes_metadata(
    notebook_path: String,
) -> Result<MetadataLoadResult, NotebookError> {
    let file_path = Path::new(&notebook_path).join(METADATA_FILE_NAME);
    let backup_path = metadata_backup_path(&notebook_path);
    cleanup_stale_staged_delete_entries(Path::new(&notebook_path));
    #[cfg(debug_assertions)]
    eprintln!(
        "load_notes_metadata: Attempting to read file: {}",
        file_path.display()
    );

    let contents = match fs::read_to_string(&file_path) {
        Ok(c) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "load_notes_metadata: Successfully read file: {}",
                file_path.display()
            );
            c
        }
        Err(_err) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "load_notes_metadata: Error reading metadata file {}: {}",
                file_path.display(),
                _err
            );
            if _err.kind() == ErrorKind::NotFound {
                #[cfg(debug_assertions)]
                eprintln!("Metadata file not found, assuming new notebook.");
                return Ok(MetadataLoadResult {
                    notes: Vec::new(),
                    warning: None,
                });
            }
            return Err(NotebookError::storage(
                "load metadata",
                format!(
                    "Failed to read metadata file '{}': {}",
                    file_path.display(),
                    _err
                ),
            ));
        }
    };

    let mut warning: Option<String> = None;
    let metadata: NotebookMetadata = match serde_json::from_str(&contents) {
        Ok(m) => {
            #[cfg(debug_assertions)]
            eprintln!("load_notes_metadata: Successfully parsed metadata.");
            m
        }
        Err(_err) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "load_notes_metadata: Error parsing metadata from {}: {}",
                file_path.display(),
                _err
            );

            let backup_contents = fs::read_to_string(&backup_path).map_err(|backup_error| {
                NotebookError::recovery(
                    "metadata recovery",
                    format!(
                        "Failed to parse metadata at '{}': {}. Also failed to read backup '{}': {}",
                        file_path.display(),
                        _err,
                        backup_path.display(),
                        backup_error
                    ),
                )
            })?;

            let backup_metadata = serde_json::from_str::<NotebookMetadata>(&backup_contents)
                .map_err(|backup_parse_error| {
                    NotebookError::recovery(
                        "metadata recovery",
                        format!(
                            "Failed to parse metadata at '{}': {}. Backup '{}' is also invalid: {}",
                            file_path.display(),
                            _err,
                            backup_path.display(),
                            backup_parse_error
                        ),
                    )
                })?;

            write_text_file_atomically(&file_path, &backup_contents).map_err(|restore_error| {
                NotebookError::recovery(
                    "metadata recovery",
                    format!(
                        "Failed to parse metadata at '{}': {}. Backup '{}' was valid, but restore failed: {}",
                        file_path.display(),
                        _err,
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
        if let Some(existing_timestamp) = note.last_updated.clone() {
            let normalized = normalize_rfc3339_to_seconds(&existing_timestamp);
            if normalized != existing_timestamp {
                note.last_updated = Some(normalized);
                metadata_changed = true;
            }
        } else {
            let note_file_path = Path::new(&notebook_path)
                .join(&note.rel_path)
                .join("note.md");

            if let Ok(file_metadata) = fs::metadata(note_file_path)
                && let Ok(modified_time) = file_metadata.modified()
                && let Some(formatted_time) = format_system_time_rfc3339(modified_time)
            {
                note.last_updated = Some(formatted_time);
                metadata_changed = true;
            }
        }
    }

    if metadata_changed && let Err(_error) = save_metadata(&notebook_path, &notes) {
        append_warning(
            &mut warning,
            format!(
                "Loaded metadata but failed to persist normalized timestamps: {}",
                _error.ui_message()
            ),
        );
    }

    Ok(MetadataLoadResult { notes, warning })
}

pub async fn save_note_content(
    notebook_path: String,
    rel_note_path: String,
    content: String,
) -> Result<(), NotebookError> {
    save_note_content_sync(&notebook_path, &rel_note_path, &content)
}

pub fn save_note_content_sync(
    notebook_path: &str,
    rel_note_path: &str,
    content: &str,
) -> Result<(), NotebookError> {
    let rel_note_path = NotebookRelativePath::parse("note path", rel_note_path)?;
    let full_note_path = rel_note_path
        .join_under(Path::new(notebook_path))
        .join("note.md");
    #[cfg(debug_assertions)]
    eprintln!("Attempting to save note to: {}", full_note_path.display());

    if let Some(parent) = full_note_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        return Err(NotebookError::storage(
            "save note content",
            format!("Failed to create directory for note: {}", error),
        ));
    }

    let existing_content = match fs::read_to_string(&full_note_path) {
        Ok(existing) => Some(existing),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            return Err(NotebookError::storage(
                "save note content",
                format!("Failed to read existing note before save: {}", error),
            ));
        }
    };

    if existing_content.as_deref() == Some(content) {
        cache_upsert_search_index_note_content(
            notebook_path,
            rel_note_path.as_str(),
            content,
            note_file_modified_time(&full_note_path),
        );
        return Ok(());
    }

    write_text_file_atomically(&full_note_path, content)?;
    cache_upsert_search_index_note_content(
        notebook_path,
        rel_note_path.as_str(),
        content,
        note_file_modified_time(&full_note_path),
    );

    Ok(())
}
