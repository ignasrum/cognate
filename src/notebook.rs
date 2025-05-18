use serde::{Deserialize, Serialize};
use serde_json;
use std::error::Error;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

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

// The save_metadata function also lives here
pub fn save_metadata(
    notebook_path: &str,
    notes: &[NoteMetadata],
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let metadata_path = Path::new(notebook_path).join("metadata.json");
    eprintln!("Saving metadata to: {}", metadata_path.display());

    // Ensure the notebook directory exists before saving metadata
    if let Some(parent) = metadata_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("Failed to create parent directory for metadata file: {}", e);
            return Err(Box::new(e));
        }
    }

    let notebook_metadata = NotebookMetadata {
        notes: notes.to_vec(),
    };

    let json_string = serde_json::to_string_pretty(&notebook_metadata)?;

    fs::write(&metadata_path, json_string)?;

    eprintln!("Metadata saved successfully.");
    Ok(())
}

// The load_notes_metadata function from note_explorer.rs should also probably live here
// to keep metadata logic together. I'll move it and adjust note_explorer.rs accordingly.
pub async fn load_notes_metadata(notebook_path: String) -> Vec<NoteMetadata> {
    let file_path = Path::new(&notebook_path).join("metadata.json");
    eprintln!(
        "load_notes_metadata: Attempting to read file: {}",
        file_path.display()
    );

    let contents = match fs::read_to_string(&file_path) {
        Ok(c) => {
            eprintln!(
                "load_notes_metadata: Successfully read file: {}",
                file_path.display()
            );
            c
        }
        Err(err) => {
            eprintln!(
                "load_notes_metadata: Error reading metadata file {}: {}",
                file_path.display(),
                err
            );
            // If the file doesn't exist, assume it's a new notebook and return empty notes
            if err.kind() == ErrorKind::NotFound {
                eprintln!("Metadata file not found, assuming new notebook.");
                return Vec::new();
            }
            return Vec::new(); // Return empty vector on other errors
        }
    };

    let metadata: NotebookMetadata = match serde_json::from_str(&contents) {
        Ok(m) => {
            eprintln!("load_notes_metadata: Successfully parsed metadata.");
            m
        }
        Err(err) => {
            eprintln!(
                "load_notes_metadata: Error parsing metadata from {}: {}",
                file_path.display(),
                err
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
    eprintln!("Attempting to save note to: {}", full_note_path.display());

    // Ensure the directory exists before writing the file
    if let Some(parent) = full_note_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return Err(format!("Failed to create directory for note: {}", e));
        }
    }

    fs::write(&full_note_path, content).map_err(|e| format!("Failed to save note: {}", e))
}

// Function to create a new note
pub async fn create_new_note(
    notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>, // Pass the notes vector to update
) -> Result<NoteMetadata, String> {
    eprintln!("Attempting to create new note with rel_path: {}", rel_path);
    let note_dir_path = Path::new(notebook_path).join(rel_path);
    let note_file_path = note_dir_path.join("note.md");
    let full_notebook_path = Path::new(notebook_path);

    // Basic validation for the new path
    if rel_path.is_empty()
        || rel_path == "."
        || rel_path == ".."
        || rel_path.starts_with('/')
        || rel_path.contains("..")
    {
        return Err(format!(
            "Invalid relative path '{}'. Paths cannot be empty, '.', '..', start with '/', or contain '..'.",
            rel_path
        ));
    }

    // Ensure the new path is within the notebook directory after canonicalization
    if let Ok(canonical_notebook_path) = full_notebook_path.canonicalize() {
        if let Ok(canonical_note_dir_path) = Path::new(notebook_path).join(rel_path).canonicalize()
        {
            if !canonical_note_dir_path.starts_with(&canonical_notebook_path) {
                return Err(format!(
                    "Cannot create note outside the notebook directory: '{}'",
                    rel_path
                ));
            }
        } else {
            eprintln!(
                "Warning: Could not canonicalize new note path '{}'. This is expected if the parent directory doesn't exist yet.",
                rel_path
            );
        }
    } else {
        eprintln!(
            "Warning: Could not canonicalize notebook path '{}'. Skipping thorough path validation.",
            notebook_path
        );
    }

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

    // Add the new note metadata to the in-memory notes vector
    notes.push(new_note_metadata.clone());

    // Save the updated metadata file
    if let Err(e) = save_metadata(notebook_path, notes) {
        eprintln!(
            "Critical Error: Failed to save metadata after creating note: {}",
            e
        );
        // Attempt to clean up the filesystem changes to avoid inconsistency
        let _ = fs::remove_dir_all(&note_dir_path);
        return Err(format!(
            "Failed to save metadata after creating note: {}",
            e
        ));
    }

    eprintln!("New note created successfully: {}", rel_path);
    Ok(new_note_metadata)
}

// Function to delete a note
pub async fn delete_note(
    notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<(), String> {
    eprintln!("Attempting to delete note with rel_path: {}", rel_path);
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
        eprintln!(
            "Warning: Could not canonicalize notebook path '{}'. Skipping thorough path validation.",
            notebook_path
        );
    }

    // Find the note in the metadata
    let note_index = notes.iter().position(|note| note.rel_path == rel_path);

    if note_index.is_none() {
        eprintln!(
            "Warning: Note with rel_path '{}' not found in metadata. Proceeding with filesystem deletion only.",
            rel_path
        );
        // If not found in metadata, we still attempt to delete the directory on disk
    } else {
        notes.remove(note_index.unwrap());
    }

    // Attempt to delete the note directory recursively
    if note_dir_path.exists() {
        if let Err(e) = fs::remove_dir_all(&note_dir_path) {
            eprintln!(
                "Error deleting directory {}: {}",
                note_dir_path.display(),
                e
            );
            return Err(format!("Failed to delete item on filesystem: {}", e));
        }
        eprintln!(
            "Item deleted successfully from filesystem: {}",
            note_dir_path.display()
        );
    } else {
        eprintln!(
            "Warning: Item not found on filesystem for rel_path '{}'. Metadata (if it existed) was removed.",
            rel_path
        );
    }

    // Save the updated metadata file ONLY IF metadata was initially found
    if note_index.is_some() {
        if let Err(e) = save_metadata(notebook_path, notes) {
            eprintln!(
                "Critical Error: Failed to save metadata after deleting note: {}",
                e
            );
            // Note: File system action succeeded, but metadata save failed.
            // This leaves the state inconsistent. Recovery would be complex.
            return Err(format!(
                "Failed to save metadata after deleting item: {}",
                e
            ));
        }
        eprintln!("Metadata saved successfully after deleting item.");
    } else {
        eprintln!(
            "Metadata was already absent for '{}', skipping metadata save.",
            rel_path
        );
    }

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
    eprintln!(
        "Attempting to move/rename item from '{}' to '{}'",
        current_rel_path, new_rel_path
    );

    let current_fs_path = Path::new(notebook_path).join(current_rel_path);
    let new_fs_path = Path::new(notebook_path).join(new_rel_path);
    let full_notebook_path = Path::new(notebook_path);

    // --- Validation ---

    // Basic validation for the new path
    if new_rel_path.is_empty()
        || new_rel_path == "."
        || new_rel_path == ".."
        || new_rel_path.starts_with('/')
        || new_rel_path.contains("..")
    {
        return Err(format!(
            "Invalid new relative path '{}'. Paths cannot be empty, '.', '..', start with '/', or contain '..'.",
            new_rel_path
        ));
    }

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

        if let Ok(canonical_new_path) = new_fs_path.canonicalize() {
            // The new path might not exist yet, so canonicalize might fail.
            // If it succeeds, ensure it's within the notebook path.
            if canonical_new_path.exists()
                && !canonical_new_path.starts_with(&canonical_notebook_path)
            {
                return Err(format!(
                    "Cannot move/rename item to path outside the notebook directory: '{}'",
                    new_rel_path
                ));
            }
        } else {
            // If canonicalize of new path fails, it might be because the path doesn't exist (which is fine).
            // We'll do a simpler check to see if the path formed by joining notebook_path and new_rel_path
            // would resolve to something outside the notebook root, but this is tricky without canonicalize.
            // For now, we'll rely on the filesystem rename failing if the target is invalid.
            eprintln!(
                "Warning: Could not canonicalize new item path '{}'. Proceeding with move attempt, but this might indicate a path issue.",
                new_rel_path
            );
        }
    } else {
        eprintln!(
            "Warning: Could not canonicalize notebook path '{}'. Skipping thorough path validation.",
            notebook_path
        );
    }

    // Check if the target path already exists on the filesystem within the notebook path
    if new_fs_path.exists() {
        // Re-check canonicalization safety if exists() is true
        if let Ok(canonical_notebook_path) = full_notebook_path.canonicalize() {
            if let Ok(canonical_new_fs_path) = new_fs_path.canonicalize() {
                if canonical_new_fs_path.starts_with(&canonical_notebook_path) {
                    return Err(format!(
                        "An item already exists at the target path '{}'.",
                        new_rel_path
                    ));
                }
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
        eprintln!("New path has no parent, attempting rename directly inside notebook root.");
    }

    // Perform the actual move/rename
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
    eprintln!("Filesystem move/rename successful.");

    // --- Metadata Update ---

    // Update the relative paths in the notes vector
    let mut updated_metadata = false;

    // Check if the current path corresponds to a note directory (contains note.md)
    let is_moving_note_dir = Path::new(notebook_path)
        .join(current_rel_path)
        .join("note.md")
        .exists();

    if is_moving_note_dir {
        // If moving a single note directory, update its path if found in metadata
        if let Some(note) = notes
            .iter_mut()
            .find(|note| note.rel_path == current_rel_path)
        {
            note.rel_path = new_rel_path.to_string();
            updated_metadata = true;
            eprintln!("Updated metadata for the moved note.");
        } else {
            eprintln!(
                "Warning: Moved note directory '{}' not found in metadata. Metadata was not updated for this item.",
                current_rel_path
            );
        }
    } else {
        // Assume it's a folder move/rename, update paths of notes within it
        let old_prefix = if current_rel_path.is_empty() {
            // Moving the root? (Shouldn't happen with current UI, but defensive)
            String::new()
        } else {
            format!("{}/", current_rel_path)
        };

        let new_prefix = if new_rel_path.is_empty() {
            // Moving to root?
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
                // Handle the edge case where the current_rel_path itself IS a note path
                note.rel_path = new_rel_path.to_string();
                updated_metadata = true;
            }
        }
        if updated_metadata {
            eprintln!("Updated metadata for notes within the moved/renamed folder.");
        } else {
            eprintln!(
                "No notes found within the old path '{}' to update metadata for.",
                current_rel_path
            );
        }
    }

    // Save the updated metadata file ONLY IF any metadata was updated
    if updated_metadata {
        if let Err(e) = save_metadata(notebook_path, notes) {
            eprintln!(
                "Critical Error: Failed to save metadata after moving/renaming: {}",
                e
            );
            // File system action succeeded, but metadata save failed. Leaves inconsistent state.
            return Err(format!(
                "Failed to save metadata after moving/renaming: {}",
                e
            ));
        }
        eprintln!("Metadata saved successfully after moving/renaming.");
    } else {
        eprintln!(
            "No relevant metadata found or updated for '{}', skipping metadata save.",
            current_rel_path
        );
    }

    eprintln!("Move/Rename process completed. New path: {}", new_rel_path);
    // Return the new relative path of the item that was moved/renamed
    Ok(new_rel_path.to_string())
}
