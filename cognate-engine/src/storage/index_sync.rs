use std::collections::HashSet;
use std::path::Path;

use super::fs_utils::{validate_relative_path, write_bytes_file_atomically};
use super::metadata::NoteMetadata;
use crate::{EngineError, NotebookEngineState};

pub(super) async fn load(notebook_path: &Path) -> NotebookEngineState {
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

pub(super) async fn save(
    notebook_path: &Path,
    state: &NotebookEngineState,
) -> Result<(), EngineError> {
    let index_file_path = notebook_path.join(".cognate_index.bin");
    let bytes = state.save_to_bytes()?;
    write_bytes_file_atomically(&index_file_path, &bytes).await
}

pub(super) async fn sync_metadata(
    notebook_path: &Path,
    notes: &[NoteMetadata],
) -> Result<(), EngineError> {
    let validated_notes: Vec<&NoteMetadata> = notes
        .iter()
        .filter(|note| validate_relative_path("metadata note path", &note.rel_path).is_ok())
        .collect();
    let mut engine_state = load(notebook_path).await;
    let mut changed = false;

    let current_paths: HashSet<String> = validated_notes
        .iter()
        .map(|note| note.rel_path.clone())
        .collect();
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

    for note in validated_notes {
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
        save(notebook_path, &engine_state).await?;
    }
    Ok(())
}
