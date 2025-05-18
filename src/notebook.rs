use serde::{Deserialize, Serialize};
use serde_json;
use std::error::Error;
use std::fs;
use std::io::ErrorKind;
use std::path::Path; // Removed PathBuf

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

    // Check if a note with the same relative path already exists in metadata
    if notes.iter().any(|note| note.rel_path == rel_path) {
        return Err(format!(
            "A note with the path '{}' already exists in metadata.",
            rel_path
        ));
    }

    // Check if the directory or file already exists on the filesystem within the notebook path
    let full_notebook_path = Path::new(notebook_path);
    if note_dir_path.exists() || note_file_path.exists() {
        // Further check if the existing path is actually within the notebook directory
        if let Ok(canonical_notebook_path) = full_notebook_path.canonicalize() {
            if let Ok(canonical_note_dir_path) = note_dir_path.canonicalize() {
                if canonical_note_dir_path.starts_with(&canonical_notebook_path) {
                    return Err(format!(
                        "A directory or file already exists at '{}'.",
                        rel_path
                    ));
                }
            }
        } else {
            // If canonicalize fails for notebook path, just rely on exists() which might be less safe
            if note_dir_path.exists() || note_file_path.exists() {
                return Err(format!(
                    "A directory or file already exists at '{}'.",
                    rel_path
                ));
            }
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

    // Before deleting, ensure the path is within the notebook directory
    if let Ok(canonical_notebook_path) = full_notebook_path.canonicalize() {
        if let Ok(canonical_note_dir_path) = note_dir_path.canonicalize() {
            if !canonical_note_dir_path.starts_with(&canonical_notebook_path) {
                return Err(format!(
                    "Cannot delete path outside the notebook directory: '{}'",
                    rel_path
                ));
            }
        } else {
            eprintln!(
                "Warning: Could not canonicalize path '{}'. Proceeding with deletion attempt.",
                rel_path
            );
        }
    } else {
        eprintln!(
            "Warning: Could not canonicalize notebook path '{}'. Skipping thorough path validation.",
            notebook_path
        );
    }

    // Find the note in the metadata
    let _initial_len = notes.len();
    let note_index = notes.iter().position(|note| note.rel_path == rel_path);

    if note_index.is_none() {
        eprintln!(
            "Warning: Note with rel_path '{}' not found in metadata.",
            rel_path
        );
    } else {
        notes.remove(note_index.unwrap());
    }

    // Attempt to delete the note directory recursively
    if note_dir_path.exists() {
        if !note_dir_path.is_dir() {
            eprintln!(
                "Warning: Path '{}' exists but is not a directory. Attempting to delete as file.",
                note_dir_path.display()
            );
            if let Err(e) = fs::remove_file(&note_dir_path) {
                eprintln!("Error deleting file {}: {}", note_dir_path.display(), e);
                return Err(format!("Failed to delete file at note path: {}", e));
            }
        } else {
            if let Err(e) = fs::remove_dir_all(&note_dir_path) {
                eprintln!(
                    "Error deleting note directory {}: {}",
                    note_dir_path.display(),
                    e
                );
                return Err(format!("Failed to delete note directory: {}", e));
            }
            eprintln!(
                "Note directory deleted successfully: {}",
                note_dir_path.display()
            );
        }
    } else {
        eprintln!(
            "Warning: Note directory or file not found on filesystem for rel_path '{}'.",
            rel_path
        );
    }

    // Save the updated metadata file ONLY IF metadata was initially found and removed
    if note_index.is_some() {
        if let Err(e) = save_metadata(notebook_path, notes) {
            eprintln!(
                "Critical Error: Failed to save metadata after deleting note: {}",
                e
            );
            return Err(format!(
                "Failed to save metadata after deleting note: {}",
                e
            ));
        }
        eprintln!("Metadata saved successfully after deleting note.");
    } else {
        eprintln!("Skipping metadata save as note was not found in metadata.");
    }

    eprintln!("Note deletion process completed for: {}", rel_path);
    Ok(())
}

// New function to move a note
pub async fn move_note(
    notebook_path: &str,
    current_rel_path: &str,
    new_rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<String, String> {
    eprintln!(
        "Attempting to move note from '{}' to '{}'",
        current_rel_path, new_rel_path
    );

    let current_note_dir_path = Path::new(notebook_path).join(current_rel_path);
    let new_note_dir_path = Path::new(notebook_path).join(new_rel_path);
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

    // Ensure the current path exists in metadata
    let note_index = notes
        .iter()
        .position(|note| note.rel_path == current_rel_path)
        .ok_or_else(|| {
            format!(
                "Note with path '{}' not found in metadata.",
                current_rel_path
            )
        })?;

    // Ensure the current path exists on the filesystem and is a directory
    if !current_note_dir_path.exists() || !current_note_dir_path.is_dir() {
        return Err(format!(
            "Current note directory '{}' not found or is not a directory.",
            current_rel_path
        ));
    }

    // Ensure the new path does not already exist in metadata
    if notes.iter().any(|note| note.rel_path == new_rel_path) {
        return Err(format!(
            "A note with the target path '{}' already exists in metadata.",
            new_rel_path
        ));
    }

    // Ensure the target path does not already exist on the filesystem within the notebook path
    if new_note_dir_path.exists() {
        if let Ok(canonical_notebook_path) = full_notebook_path.canonicalize() {
            if let Ok(canonical_new_note_dir_path) = new_note_dir_path.canonicalize() {
                if canonical_new_note_dir_path.starts_with(&canonical_notebook_path) {
                    return Err(format!(
                        "A file or directory already exists at the target path '{}'.",
                        new_rel_path
                    ));
                }
            }
        } else {
            // If canonicalize fails for notebook path, just rely on exists()
            if new_note_dir_path.exists() {
                return Err(format!(
                    "A file or directory already exists at the target path '{}'.",
                    new_rel_path
                ));
            }
        }
    }

    // Ensure both current and new paths are within the notebook directory after canonicalization
    if let Ok(canonical_notebook_path) = full_notebook_path.canonicalize() {
        if let Ok(canonical_current_path) = current_note_dir_path.canonicalize() {
            if !canonical_current_path.starts_with(&canonical_notebook_path) {
                return Err(format!(
                    "Cannot move note from path outside the notebook directory: '{}'",
                    current_rel_path
                ));
            }
        } else {
            return Err(format!(
                "Failed to canonicalize current note path: '{}'",
                current_rel_path
            ));
        }

        if let Ok(canonical_new_path) = new_note_dir_path.canonicalize() {
            if !canonical_new_path.starts_with(&canonical_notebook_path) {
                return Err(format!(
                    "Cannot move note to path outside the notebook directory: '{}'",
                    new_rel_path
                ));
            }
        } else {
            eprintln!(
                "Warning: Could not canonicalize new note path '{}'. This is expected if the parent directory doesn't exist yet.",
                new_rel_path
            );
        }
    } else {
        eprintln!(
            "Warning: Could not canonicalize notebook path '{}'. Skipping thorough path validation.",
            notebook_path
        );
    }

    // --- File System Operation ---

    // Create parent directories for the new note path if they don't exist
    if let Some(parent) = new_note_dir_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return Err(format!(
                "Failed to create parent directories for new note path: {}",
                e
            ));
        }
    }

    // Perform the actual move
    if let Err(e) = fs::rename(&current_note_dir_path, &new_note_dir_path) {
        return Err(format!(
            "Failed to move note directory from '{}' to '{}': {}",
            current_rel_path, new_rel_path, e
        ));
    }
    eprintln!("Note directory moved successfully.");

    // --- Metadata Update ---

    // Update the relative path in the notes vector
    notes[note_index].rel_path = new_rel_path.to_string();

    // Save the updated metadata file
    if let Err(e) = save_metadata(notebook_path, notes) {
        eprintln!(
            "Critical Error: Failed to save metadata after moving note: {}",
            e
        );
        return Err(format!("Failed to save metadata after moving note: {}", e));
    }
    eprintln!("Metadata updated successfully after moving note.");

    Ok(new_rel_path.to_string())
}
