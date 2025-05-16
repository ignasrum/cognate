use serde::{Deserialize, Serialize};
use serde_json;
use std::error::Error;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf}; // Import PathBuf // Import ErrorKind

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
    notes: &[NoteMetadata], // Now uses the NoteMetadata defined in this module
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let metadata_path = Path::new(notebook_path).join("metadata.json");
    eprintln!("Saving metadata to: {}", metadata_path.display());

    let notebook_metadata = NotebookMetadata {
        notes: notes.to_vec(), // Uses the NotebookMetadata defined in this module
    };

    let json_string = serde_json::to_string_pretty(&notebook_metadata)?;

    fs::write(&metadata_path, json_string)?;

    eprintln!("Metadata saved successfully.");
    Ok(())
}

// The load_notes_metadata function from note_explorer.rs should also probably live here
// to keep metadata logic together. I'll move it and adjust note_explorer.rs accordingly.
pub async fn load_notes_metadata(notebook_path: String) -> Vec<NoteMetadata> {
    let file_path = format!("{}/metadata.json", notebook_path);
    eprintln!(
        "load_notes_metadata: Attempting to read file: {}",
        file_path
    );

    let contents = match fs::read_to_string(&file_path) {
        Ok(c) => {
            eprintln!("load_notes_metadata: Successfully read file: {}", file_path);
            c
        }
        Err(err) => {
            eprintln!(
                "load_notes_metadata: Error reading metadata file {}: {}",
                file_path, err
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
                file_path, err
            );
            // This might happen if the file is empty or malformed initially
            // Consider handling this case more gracefully, maybe by returning an empty NotebookMetadata
            // For now, we return an empty vec of notes
            return Vec::new();
        }
    };

    metadata.notes
}

// New function to save note content
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

    // Check if a note with the same relative path already exists
    if notes.iter().any(|note| note.rel_path == rel_path) {
        return Err(format!(
            "A note with the path '{}' already exists.",
            rel_path
        ));
    }

    // Check if the directory or file already exists on the filesystem
    if note_dir_path.exists() || note_file_path.exists() {
        return Err(format!(
            "A directory or file already exists at '{}'.",
            rel_path
        ));
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
    notes.push(new_note_metadata.clone()); // Clone to return it later

    // Save the updated metadata file
    if let Err(e) = save_metadata(notebook_path, notes) {
        // This is a critical error, the metadata file is out of sync
        eprintln!(
            "Critical Error: Failed to save metadata after creating note: {}",
            e
        );
        // We might want to delete the created note directory here to avoid inconsistency
        // For simplicity now, just log the error.
        return Err(format!(
            "Failed to save metadata after creating note: {}",
            e
        ));
    }

    eprintln!("New note created successfully: {}", rel_path);
    Ok(new_note_metadata) // Return the metadata of the newly created note
}

// New function to delete a note
pub async fn delete_note(
    notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>, // Pass the notes vector to update
) -> Result<(), String> {
    eprintln!("Attempting to delete note with rel_path: {}", rel_path);
    let note_dir_path = Path::new(notebook_path).join(rel_path);

    // Remove the note metadata from the in-memory notes vector
    let initial_len = notes.len();
    notes.retain(|note| note.rel_path != rel_path);
    if notes.len() == initial_len {
        // Note with this rel_path was not found in the metadata
        eprintln!(
            "Warning: Note with rel_path '{}' not found in metadata.",
            rel_path
        );
        // We can still attempt to delete the directory on the filesystem
    }

    // Attempt to delete the note directory recursively
    if note_dir_path.exists() {
        if let Err(e) = fs::remove_dir_all(&note_dir_path) {
            eprintln!(
                "Error deleting note directory {}: {}",
                note_dir_path.display(),
                e
            );
            // We might want to add the note back to the in-memory vector if fs deletion failed
            // For now, we'll just return an error.
            return Err(format!("Failed to delete note directory: {}", e));
        }
        eprintln!(
            "Note directory deleted successfully: {}",
            note_dir_path.display()
        );
    } else {
        eprintln!(
            "Warning: Note directory not found on filesystem for rel_path '{}'.",
            rel_path
        );
        // If the directory didn't exist but it was in metadata, we proceed to save metadata
    }

    // Save the updated metadata file
    if let Err(e) = save_metadata(notebook_path, notes) {
        eprintln!(
            "Critical Error: Failed to save metadata after deleting note: {}",
            e
        );
        // This is a critical error, the metadata file is out of sync
        return Err(format!(
            "Failed to save metadata after deleting note: {}",
            e
        ));
    }

    eprintln!("Note deletion process completed for: {}", rel_path);
    Ok(())
}
