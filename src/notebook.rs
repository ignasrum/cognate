use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const STAGED_DELETE_PREFIX: &str = ".cognate_txn_delete_";
const STAGED_DELETE_CLEANUP_GRACE_NANOS: u128 = 5 * 60 * 1_000_000_000;

// These structs are now defined once in this common module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMetadata {
    pub rel_path: String,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookMetadata {
    pub notes: Vec<NoteMetadata>,
}

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
    target_canonicalize_warning: &str,
) -> Result<(), String> {
    if let Ok(canonical_notebook_path) = notebook_path.canonicalize() {
        if let Ok(canonical_target_path) = target_path.canonicalize() {
            if !canonical_target_path.starts_with(&canonical_notebook_path) {
                return Err(format!("{} '{}'", outside_error_prefix, rel_path));
            }
        } else {
            #[cfg(debug_assertions)]
            eprintln!("{}", target_canonicalize_warning);
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
    rel_path: &str,
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
            rel_path
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

fn cleanup_stale_staged_delete_entries(notebook_path: &Path) {
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    let read_dir = match fs::read_dir(notebook_path) {
        Ok(entries) => entries,
        Err(e) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "Warning: Failed to scan notebook directory '{}' for stale staged deletes: {}",
                notebook_path.display(),
                e
            );
            return;
        }
    };

    for entry_result in read_dir {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(e) => {
                #[cfg(debug_assertions)]
                eprintln!("Warning: Failed to read notebook directory entry: {}", e);
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

        if let Err(e) = removal_result {
            #[cfg(debug_assertions)]
            eprintln!(
                "Warning: Failed to remove stale staged delete '{}': {}",
                staged_path.display(),
                e
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

// The save_metadata function also lives here
pub fn save_metadata(
    notebook_path: &str,
    notes: &[NoteMetadata],
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    #[cfg(debug_assertions)]
    eprintln!(
        "Saving metadata to: {}",
        Path::new(notebook_path).join("metadata.json").display()
    );

    let metadata_path = Path::new(notebook_path).join("metadata.json");

    // Ensure the notebook directory exists before saving metadata
    if let Some(parent) = metadata_path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        #[cfg(debug_assertions)]
        eprintln!("Failed to create parent directory for metadata file: {}", e);
        return Err(Box::new(e));
    }

    let notebook_metadata = NotebookMetadata {
        notes: notes.to_vec(),
    };

    let json_string = serde_json::to_string_pretty(&notebook_metadata)?;

    fs::write(&metadata_path, json_string)?;

    #[cfg(debug_assertions)]
    eprintln!("Metadata saved successfully.");
    Ok(())
}

// The load_notes_metadata function from note_explorer.rs should also probably live here
// to keep metadata logic together. I'll move it and adjust note_explorer.rs accordingly.
pub async fn load_notes_metadata(notebook_path: String) -> Vec<NoteMetadata> {
    let file_path = Path::new(&notebook_path).join("metadata.json");
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
            // Added underscore here
            #[cfg(debug_assertions)]
            eprintln!(
                "load_notes_metadata: Error reading metadata file {}: {}",
                file_path.display(),
                _err // Use the underscore version here too
            );
            // If the file doesn't exist, assume it's a new notebook and return empty notes
            if _err.kind() == ErrorKind::NotFound {
                // Use the underscore version here too
                #[cfg(debug_assertions)]
                eprintln!("Metadata file not found, assuming new notebook.");
                return Vec::new();
            }
            return Vec::new(); // Return empty vector on other errors
        }
    };

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
            return Vec::new();
        }
    };

    metadata.notes
}

// Function to save note content
pub async fn save_note_content(
    notebook_path: String,
    rel_note_path: String,
    content: String,
) -> Result<(), String> {
    let full_note_path = Path::new(&notebook_path)
        .join(&rel_note_path)
        .join("note.md");
    #[cfg(debug_assertions)]
    eprintln!("Attempting to save note to: {}", full_note_path.display());

    // Ensure the directory exists before writing the file
    if let Some(parent) = full_note_path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        return Err(format!("Failed to create directory for note: {}", e));
    }

    fs::write(&full_note_path, content).map_err(|e| format!("Failed to save note: {}", e))
}

// Function to create a new note
pub async fn create_new_note(
    notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>, // Pass the notes vector to update
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

    // Check if a note with the same relative path already exists in metadata
    if notes.iter().any(|note| note.rel_path == rel_path) {
        return Err(format!(
            "A note with the path '{}' already exists in metadata.",
            rel_path
        ));
    }

    // Check if the directory or file already exists on the filesystem within the notebook path
    if note_dir_path.exists() || note_file_path.exists() {
        // We've already done canonicalize check above, but repeating the exists check is fine.
        if note_dir_path.exists() {
            return Err(format!(
                "A directory or file already exists at '{}'.",
                rel_path
            ));
        }
    }

    // Create the note directory and the note.md file
    if let Err(e) = fs::create_dir_all(&note_dir_path) {
        return Err(format!("Failed to create directory for new note: {}", e));
    }

    if let Err(e) = fs::write(&note_file_path, "") {
        // Clean up the created directory if file creation fails
        let _ = fs::remove_dir_all(&note_dir_path);
        return Err(format!("Failed to create note file: {}", e));
    }

    // Create metadata for the new note
    let new_note_metadata = NoteMetadata {
        rel_path: rel_path.to_string(),
        labels: Vec::new(),
    };

    let previous_notes = notes.clone();

    // Add the new note metadata to the in-memory notes vector
    notes.push(new_note_metadata.clone());

    // Save the updated metadata file
    if let Err(e) = save_metadata(notebook_path, notes) {
        #[cfg(debug_assertions)]
        eprintln!(
            "Critical Error: Failed to save metadata after creating note: {}",
            e
        );
        // Roll back in-memory metadata and filesystem changes to avoid inconsistency.
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
    Ok(new_note_metadata)
}

// Function to delete a note
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

    // Ensure the path is within the notebook directory
    if let Ok(canonical_notebook_path) = full_notebook_path.canonicalize() {
        if let Ok(canonical_note_dir_path) = note_dir_path.canonicalize() {
            if !canonical_note_dir_path.starts_with(&canonical_notebook_path) {
                return Err(format!(
                    "Cannot delete path outside the notebook directory: '{}'",
                    rel_path
                ));
            }
        } else {
            // If canonicalize fails, check if the path exists relative to the notebook root.
            // This is less safe but better than nothing if canonicalize fails.
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
        // If canonicalize fails for notebook path, just rely on relative path existence check.
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
        // If not found in metadata, we still attempt to delete the directory on disk
    }

    let mut staged_delete_path: Option<PathBuf> = None;

    // Stage deletion by renaming first so we can roll back if metadata persistence fails.
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
            && let Err(rollback_error) = fs::rename(&staged_path, &note_dir_path)
        {
            return Err(format!(
                "{} Rollback failed while restoring filesystem state: {}",
                metadata_error, rollback_error
            ));
        }

        return Err(metadata_error);
    }

    if let Some(staged_path) = staged_delete_path
        && let Err(e) = fs::remove_dir_all(&staged_path)
    {
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: Metadata commit succeeded, but failed to finalize staged deletion '{}': {}",
            staged_path.display(),
            e
        );
    }

    #[cfg(debug_assertions)]
    eprintln!("Deletion process completed for: {}", rel_path);
    Ok(())
}

// Function to move/rename a note or folder
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

    // --- Validation ---

    validate_notebook_relative_path(current_rel_path, "current relative path")?;
    validate_notebook_relative_path(new_rel_path, "new relative path")?;

    // Ensure the current path exists on the filesystem
    if !current_fs_path.exists() {
        return Err(format!(
            "Item at path '{}' not found on the filesystem.",
            current_rel_path
        ));
    }

    // Ensure both current and new paths are within the notebook directory after canonicalization
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

    // Check if the target path already exists on the filesystem within the notebook path
    if new_fs_path.exists() {
        // Re-check canonicalization safety if exists() is true
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
            // If canonicalize fails for notebook path, just rely on exists()
            return Err(format!(
                "An item already exists at the target path '{}'.",
                new_rel_path
            ));
        }
    }

    // --- File System Operation ---

    // Create parent directories for the new path if they don't exist
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
        // This case means new_rel_path is just a name (e.g., "new_folder" or "new_note")
        // and new_fs_path is directly inside notebook_path. No parent directory creation needed beyond the notebook root.
        #[cfg(debug_assertions)]
        eprintln!("New path has no parent, attempting rename directly inside notebook root.");
    }

    // Perform the actual move/rename
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

    // --- Metadata Update ---

    // Check if the current path corresponds to a note directory (contains note.md)
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
        if let Err(rollback_error) = fs::rename(&new_fs_path, &current_fs_path) {
            return Err(format!(
                "{} Rollback failed while restoring filesystem state: {}",
                metadata_error, rollback_error
            ));
        }
        return Err(metadata_error);
    }

    #[cfg(debug_assertions)]
    eprintln!("Move/Rename process completed. New path: {}", new_rel_path);
    // Return the new relative path of the item that was moved/renamed
    Ok(new_rel_path.to_string())
}
