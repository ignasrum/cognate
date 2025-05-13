use serde::{Deserialize, Serialize};
use serde_json;
use std::error::Error;
use std::fs;
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
            return Vec::new();
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
