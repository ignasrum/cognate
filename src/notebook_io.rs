use serde_json;
use std::error::Error;
use std::fs;
use std::path::Path; // Import the Error trait

#[path = "components/note_explorer/note_explorer.rs"]
mod note_explorer;

// Constrain the error to be Send and Sync
pub fn save_metadata(
    notebook_path: &str,
    notes: &[note_explorer::NoteMetadata],
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let metadata_path = Path::new(notebook_path).join("metadata.json");
    eprintln!("Saving metadata to: {}", metadata_path.display());

    let notebook_metadata = note_explorer::NotebookMetadata {
        notes: notes.to_vec(),
    };

    let json_string = serde_json::to_string_pretty(&notebook_metadata)?;

    fs::write(&metadata_path, json_string)?;

    eprintln!("Metadata saved successfully.");
    Ok(())
}
