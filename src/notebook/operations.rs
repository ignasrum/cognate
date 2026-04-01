use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::NoteMetadata;
use super::search::{
    cache_remove_search_index_entries, cache_rename_search_index_entries,
    cache_upsert_search_index_note_content, note_file_modified_time,
};
use super::storage::{current_timestamp_rfc3339, save_metadata, write_text_file_atomically};

const FAIL_DELETE_ROLLBACK_MARKER: &str = ".cognate_fail_delete_rollback";
const FAIL_MOVE_ROLLBACK_MARKER: &str = ".cognate_fail_move_rollback";

fn validate_notebook_relative_path(rel_path: &str, path_kind: &str) -> Result<(), String> {
    if rel_path.is_empty()
        || rel_path == "."
        || rel_path == ".."
        || rel_path.starts_with('/')
        || rel_path.contains("..")
    {
        return Err(format!(
            "Invalid {} '{}'. Paths cannot be empty, '.', '..', start with '/', or contain '..'.",
            path_kind, rel_path
        ));
    }
    Ok(())
}

fn ensure_path_within_notebook_if_canonicalizable(
    notebook_path: &Path,
    target_path: &Path,
    rel_path: &str,
    outside_error_prefix: &str,
    _target_canonicalize_warning: &str,
) -> Result<(), String> {
    if let Ok(canonical_notebook_path) = notebook_path.canonicalize() {
        if let Ok(canonical_target_path) = target_path.canonicalize() {
            if !canonical_target_path.starts_with(&canonical_notebook_path) {
                return Err(format!("{} '{}'", outside_error_prefix, rel_path));
            }
        } else {
            #[cfg(debug_assertions)]
            eprintln!("{}", _target_canonicalize_warning);
        }
    } else {
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: Could not canonicalize notebook path '{}'. Skipping thorough path validation.",
            notebook_path.display()
        );
    }

    Ok(())
}

fn remove_note_from_metadata(notes: &mut Vec<NoteMetadata>, rel_path: &str) -> bool {
    if let Some(index) = notes.iter().position(|note| note.rel_path == rel_path) {
        notes.remove(index);
        true
    } else {
        false
    }
}

fn update_metadata_paths_for_move(
    notes: &mut [NoteMetadata],
    current_rel_path: &str,
    new_rel_path: &str,
    is_moving_note_dir: bool,
) -> bool {
    let mut updated_metadata = false;

    if is_moving_note_dir {
        if let Some(note) = notes
            .iter_mut()
            .find(|note| note.rel_path == current_rel_path)
        {
            note.rel_path = new_rel_path.to_string();
            updated_metadata = true;
        }
    } else {
        let old_prefix = if current_rel_path.is_empty() {
            String::new()
        } else {
            format!("{}/", current_rel_path)
        };

        let new_prefix = if new_rel_path.is_empty() {
            String::new()
        } else {
            format!("{}/", new_rel_path)
        };

        for note in notes.iter_mut() {
            if note.rel_path.starts_with(&old_prefix) {
                let suffix = note.rel_path.trim_start_matches(&old_prefix);
                note.rel_path = format!("{}{}", new_prefix, suffix);
                updated_metadata = true;
            } else if note.rel_path == current_rel_path && !current_rel_path.is_empty() {
                note.rel_path = new_rel_path.to_string();
                updated_metadata = true;
            }
        }
    }

    updated_metadata
}

fn persist_metadata_if_changed(
    notebook_path: &str,
    notes: &[NoteMetadata],
    metadata_changed: bool,
    operation_description: &str,
    _rel_path: &str,
) -> Result<(), String> {
    if metadata_changed {
        if let Err(e) = save_metadata(notebook_path, notes) {
            #[cfg(debug_assertions)]
            eprintln!(
                "Critical Error: Failed to save metadata after {}: {}",
                operation_description, e
            );
            return Err(format!(
                "Failed to save metadata after {}: {}",
                operation_description, e
            ));
        }
        #[cfg(debug_assertions)]
        eprintln!(
            "Metadata saved successfully after {}.",
            operation_description
        );
    } else {
        #[cfg(debug_assertions)]
        eprintln!(
            "No relevant metadata found or updated for '{}', skipping metadata save.",
            _rel_path
        );
    }

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
    let sanitized_rel_path = rel_path.replace('/', "__");

    notebook_path.join(format!(
        ".cognate_txn_{}_{}_{}",
        operation, sanitized_rel_path, timestamp
    ))
}

fn remove_empty_parent_directories(notebook_path: &Path, deleted_note_dir_path: &Path) {
    let canonical_notebook_path = notebook_path.canonicalize().ok();
    let mut current_parent = deleted_note_dir_path.parent().map(Path::to_path_buf);

    while let Some(parent_path) = current_parent {
        if parent_path == notebook_path {
            break;
        }

        if let Some(canonical_root) = canonical_notebook_path.as_ref() {
            if let Ok(canonical_parent) = parent_path.canonicalize() {
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

        match fs::remove_dir(&parent_path) {
            Ok(()) => {
                #[cfg(debug_assertions)]
                eprintln!(
                    "Removed empty parent directory after delete: {}",
                    parent_path.display()
                );
                current_parent = parent_path.parent().map(Path::to_path_buf);
            }
            Err(_e) if _e.kind() == ErrorKind::NotFound => {
                current_parent = parent_path.parent().map(Path::to_path_buf);
            }
            Err(_e) if _e.kind() == ErrorKind::DirectoryNotEmpty => {
                break;
            }
            Err(_e) => {
                #[cfg(debug_assertions)]
                eprintln!(
                    "Warning: Failed to remove parent directory '{}' after delete: {}",
                    parent_path.display(),
                    _e
                );
                break;
            }
        }
    }
}

fn rollback_rename(
    staged_or_new_path: &Path,
    original_path: &Path,
    _notebook_root: &Path,
    _fail_marker: &str,
) -> Result<(), String> {
    #[cfg(test)]
    if _notebook_root.join(_fail_marker).exists() {
        return Err(format!(
            "simulated rollback rename failure from '{}' to '{}'",
            staged_or_new_path.display(),
            original_path.display()
        ));
    }

    fs::rename(staged_or_new_path, original_path).map_err(|error| error.to_string())
}

pub async fn create_new_note(
    notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<NoteMetadata, String> {
    #[cfg(debug_assertions)]
    eprintln!("Attempting to create new note with rel_path: {}", rel_path);
    let full_notebook_path = Path::new(notebook_path);
    let note_dir_path = full_notebook_path.join(rel_path);
    let note_file_path = note_dir_path.join("note.md");

    validate_notebook_relative_path(rel_path, "relative path")?;
    ensure_path_within_notebook_if_canonicalizable(
        full_notebook_path,
        &note_dir_path,
        rel_path,
        "Cannot create note outside the notebook directory:",
        &format!(
            "Warning: Could not canonicalize new note path '{}'. This is expected if the parent directory doesn't exist yet.",
            rel_path
        ),
    )?;

    if notes.iter().any(|note| note.rel_path == rel_path) {
        return Err(format!(
            "A note with the path '{}' already exists in metadata.",
            rel_path
        ));
    }

    if (note_dir_path.exists() || note_file_path.exists()) && note_dir_path.exists() {
        return Err(format!(
            "A directory or file already exists at '{}'.",
            rel_path
        ));
    }

    if let Err(e) = fs::create_dir_all(&note_dir_path) {
        return Err(format!("Failed to create directory for new note: {}", e));
    }

    if let Err(e) = write_text_file_atomically(&note_file_path, "") {
        let _ = fs::remove_dir_all(&note_dir_path);
        return Err(format!("Failed to create note file: {}", e));
    }

    let new_note_metadata = NoteMetadata {
        rel_path: rel_path.to_string(),
        labels: Vec::new(),
        last_updated: Some(current_timestamp_rfc3339()),
    };

    let previous_notes = notes.clone();
    notes.push(new_note_metadata.clone());

    if let Err(e) = save_metadata(notebook_path, notes) {
        #[cfg(debug_assertions)]
        eprintln!(
            "Critical Error: Failed to save metadata after creating note: {}",
            e
        );
        *notes = previous_notes;
        let cleanup_result = fs::remove_dir_all(&note_dir_path);
        if let Err(cleanup_error) = cleanup_result {
            return Err(format!(
                "Failed to save metadata after creating note: {}. Rollback cleanup failed: {}",
                e, cleanup_error
            ));
        }
        return Err(format!(
            "Failed to save metadata after creating note: {}",
            e
        ));
    }

    #[cfg(debug_assertions)]
    eprintln!("New note created successfully: {}", rel_path);
    cache_upsert_search_index_note_content(
        notebook_path,
        rel_path,
        "",
        note_file_modified_time(&note_file_path),
    );
    Ok(new_note_metadata)
}

pub async fn delete_note(
    notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<(), String> {
    #[cfg(debug_assertions)]
    eprintln!("Attempting to delete note with rel_path: {}", rel_path);

    validate_notebook_relative_path(rel_path, "relative path")?;

    let note_dir_path = Path::new(notebook_path).join(rel_path);
    let full_notebook_path = Path::new(notebook_path);

    if let Ok(canonical_notebook_path) = full_notebook_path.canonicalize() {
        if let Ok(canonical_note_dir_path) = note_dir_path.canonicalize() {
            if !canonical_note_dir_path.starts_with(&canonical_notebook_path) {
                return Err(format!(
                    "Cannot delete path outside the notebook directory: '{}'",
                    rel_path
                ));
            }
        } else {
            if !Path::new(notebook_path).join(rel_path).exists() {
                return Err(format!(
                    "Path '{}' does not exist within the notebook.",
                    rel_path
                ));
            }
            #[cfg(debug_assertions)]
            eprintln!(
                "Warning: Could not canonicalize path '{}'. Proceeding with deletion attempt based on relative path.",
                rel_path
            );
        }
    } else {
        if !Path::new(notebook_path).join(rel_path).exists() {
            return Err(format!(
                "Path '{}' does not exist within the notebook.",
                rel_path
            ));
        }
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: Could not canonicalize notebook path '{}'. Skipping thorough path validation.",
            notebook_path
        );
    }

    let previous_notes = notes.clone();
    let metadata_changed = remove_note_from_metadata(notes, rel_path);

    if !metadata_changed {
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: Note with rel_path '{}' not found in metadata. Proceeding with filesystem deletion only.",
            rel_path
        );
    }

    let mut staged_delete_path: Option<PathBuf> = None;

    if note_dir_path.exists() {
        let transaction_path =
            build_transaction_staging_path(full_notebook_path, rel_path, "delete");

        if let Err(e) = fs::rename(&note_dir_path, &transaction_path) {
            #[cfg(debug_assertions)]
            eprintln!(
                "Error staging directory {} for deletion: {}",
                note_dir_path.display(),
                e
            );
            return Err(format!(
                "Failed to stage item for deletion on filesystem: {}",
                e
            ));
        }

        staged_delete_path = Some(transaction_path);
        #[cfg(debug_assertions)]
        eprintln!(
            "Item staged successfully for deletion on filesystem: {}",
            note_dir_path.display()
        );
    } else {
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: Item not found on filesystem for rel_path '{}'. Metadata (if it existed) was removed.",
            rel_path
        );
    }

    if let Err(metadata_error) = persist_metadata_if_changed(
        notebook_path,
        notes,
        metadata_changed,
        "deleting item",
        rel_path,
    ) {
        *notes = previous_notes;

        if let Some(staged_path) = staged_delete_path
            && let Err(rollback_error) = rollback_rename(
                &staged_path,
                &note_dir_path,
                full_notebook_path,
                FAIL_DELETE_ROLLBACK_MARKER,
            )
        {
            return Err(format!(
                "{} Rollback failed while restoring filesystem state: {}",
                metadata_error, rollback_error
            ));
        }

        return Err(metadata_error);
    }

    if let Some(staged_path) = staged_delete_path
        && let Err(_e) = fs::remove_dir_all(&staged_path)
    {
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: Metadata commit succeeded, but failed to finalize staged deletion '{}': {}",
            staged_path.display(),
            _e
        );
    }

    remove_empty_parent_directories(full_notebook_path, &note_dir_path);
    cache_remove_search_index_entries(notebook_path, rel_path);

    #[cfg(debug_assertions)]
    eprintln!("Deletion process completed for: {}", rel_path);
    Ok(())
}

pub async fn move_note(
    notebook_path: &str,
    current_rel_path: &str,
    new_rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<String, String> {
    #[cfg(debug_assertions)]
    eprintln!(
        "Attempting to move/rename item from '{}' to '{}'",
        current_rel_path, new_rel_path
    );

    let current_fs_path = Path::new(notebook_path).join(current_rel_path);
    let new_fs_path = Path::new(notebook_path).join(new_rel_path);
    let full_notebook_path = Path::new(notebook_path);

    validate_notebook_relative_path(current_rel_path, "current relative path")?;
    validate_notebook_relative_path(new_rel_path, "new relative path")?;

    if !current_fs_path.exists() {
        return Err(format!(
            "Item at path '{}' not found on the filesystem.",
            current_rel_path
        ));
    }

    if let Ok(canonical_notebook_path) = full_notebook_path.canonicalize() {
        if let Ok(canonical_current_path) = current_fs_path.canonicalize() {
            if !canonical_current_path.starts_with(&canonical_notebook_path) {
                return Err(format!(
                    "Cannot move/rename item from path outside the notebook directory: '{}'",
                    current_rel_path
                ));
            }
        } else {
            return Err(format!(
                "Failed to canonicalize current item path: '{}'",
                current_rel_path
            ));
        }

        ensure_path_within_notebook_if_canonicalizable(
            full_notebook_path,
            &new_fs_path,
            new_rel_path,
            "Cannot move/rename item to path outside the notebook directory:",
            &format!(
                "Warning: Could not canonicalize new item path '{}'. Proceeding with move attempt, but this might indicate a path issue.",
                new_rel_path
            ),
        )?;
    } else {
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: Could not canonicalize notebook path '{}'. Skipping thorough path validation.",
            notebook_path
        );
    }

    if new_fs_path.exists() {
        if let Ok(canonical_notebook_path) = full_notebook_path.canonicalize() {
            if let Ok(canonical_new_fs_path) = new_fs_path.canonicalize()
                && canonical_new_fs_path.starts_with(&canonical_notebook_path)
            {
                return Err(format!(
                    "An item already exists at the target path '{}'.",
                    new_rel_path
                ));
            }
        } else {
            return Err(format!(
                "An item already exists at the target path '{}'.",
                new_rel_path
            ));
        }
    }

    if let Some(parent) = new_fs_path.parent() {
        if !parent.exists() {
            #[cfg(debug_assertions)]
            eprintln!(
                "Creating parent directories for new path: {}",
                parent.display()
            );
            if let Err(e) = fs::create_dir_all(parent) {
                return Err(format!(
                    "Failed to create parent directories for new path: {}",
                    e
                ));
            }
        }
    } else {
        #[cfg(debug_assertions)]
        eprintln!("New path has no parent, attempting rename directly inside notebook root.");
    }

    let previous_notes = notes.clone();

    #[cfg(debug_assertions)]
    eprintln!(
        "Attempting filesystem rename from '{}' to '{}'",
        current_fs_path.display(),
        new_fs_path.display()
    );
    if let Err(e) = fs::rename(&current_fs_path, &new_fs_path) {
        return Err(format!(
            "Failed to move/rename item from '{}' to '{}': {}",
            current_rel_path, new_rel_path, e
        ));
    }
    #[cfg(debug_assertions)]
    eprintln!("Filesystem move/rename successful.");

    let is_moving_note_dir = Path::new(notebook_path)
        .join(current_rel_path)
        .join("note.md")
        .exists();

    let updated_metadata =
        update_metadata_paths_for_move(notes, current_rel_path, new_rel_path, is_moving_note_dir);

    if is_moving_note_dir {
        if updated_metadata {
            #[cfg(debug_assertions)]
            eprintln!("Updated metadata for the moved note.");
        } else {
            #[cfg(debug_assertions)]
            eprintln!(
                "Warning: Moved note directory '{}' not found in metadata. Metadata was not updated for this item.",
                current_rel_path
            );
        }
    } else if updated_metadata {
        #[cfg(debug_assertions)]
        eprintln!("Updated metadata for notes within the moved/renamed folder.");
    }

    if let Err(metadata_error) = persist_metadata_if_changed(
        notebook_path,
        notes,
        updated_metadata,
        "moving/renaming",
        current_rel_path,
    ) {
        *notes = previous_notes;
        if let Err(rollback_error) = rollback_rename(
            &new_fs_path,
            &current_fs_path,
            full_notebook_path,
            FAIL_MOVE_ROLLBACK_MARKER,
        ) {
            return Err(format!(
                "{} Rollback failed while restoring filesystem state: {}",
                metadata_error, rollback_error
            ));
        }
        return Err(metadata_error);
    }

    cache_rename_search_index_entries(notebook_path, current_rel_path, new_rel_path);

    #[cfg(debug_assertions)]
    eprintln!("Move/Rename process completed. New path: {}", new_rel_path);
    Ok(new_rel_path.to_string())
}
