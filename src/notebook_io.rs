// This file seems redundant as save_metadata has been moved to notebook.rs.
// It should probably be removed or updated if more IO-specific functions are added.
// For now, I will leave it as is but note its current state.

use serde_json;
use std::error::Error;
use std::fs;
use std::path::Path; // Import the Error trait

#[path = "components/note_explorer/note_explorer.rs"]
mod note_explorer;

// Constrain the error to be Send and Sync
// Note: This function is now a duplicate of the one in notebook.rs
// and should likely be removed. Keeping it for now to avoid breaking
// any potential external references, but it's a discrepancy.
#[allow(dead_code)] // Allow dead code for this function as it seems unused
pub fn save_metadata(
    notebook_path: &str,
    notes: &[note_explorer::NoteMetadata],
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let metadata_path = Path::new(notebook_path).join("metadata.json");
    #[cfg(debug_assertions)]
    eprintln!("Saving metadata to: {}", metadata_path.display());

    let notebook_metadata = note_explorer::NotebookMetadata {
        notes: notes.to_vec(),
    };

    let json_string = serde_json::to_string_pretty(&notebook_metadata)?;

    fs::write(&metadata_path, json_string)?;

    #[cfg(debug_assertions)]
    eprintln!("Metadata saved successfully.");
    Ok(())
}
