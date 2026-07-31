use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::storage::{NoteMetadata, NotebookManager};
use crate::{EngineError, NotebookEngineState};

use super::cache::NoteCache;

pub(super) async fn synchronize_index(
    notebook_path: &Path,
    cache: &mut NoteCache,
    mut engine_state: Option<NotebookEngineState>,
    notes: &[NoteMetadata],
    refresh_interval: Duration,
    last_external_refresh: &mut Option<Instant>,
) -> Result<(NotebookEngineState, HashSet<String>), EngineError> {
    let now = Instant::now();
    let should_refresh = match *last_external_refresh {
        Some(last) => now.duration_since(last) >= refresh_interval,
        None => true,
    };

    let note_paths: HashSet<&str> = notes.iter().map(|note| note.rel_path.as_str()).collect();
    cache
        .notes
        .retain(|rel_path, _| note_paths.contains(rel_path.as_str()));

    let manager = NotebookManager::new(notebook_path);
    let mut reloaded_paths = HashSet::new();

    for note in notes {
        let in_cache = cache.notes.contains_key(&note.rel_path);
        let needs_reload = if in_cache && should_refresh {
            let current_mtime = manager.get_note_modified_time(&note.rel_path).await;
            let cached_mtime = cache
                .notes
                .get(&note.rel_path)
                .and_then(|cached| cached.modified_time);
            current_mtime != cached_mtime
        } else {
            false
        };

        if !in_cache || needs_reload {
            let content = manager
                .load_note_content(&note.rel_path)
                .await
                .unwrap_or_default();
            let modified_time = manager.get_note_modified_time(&note.rel_path).await;
            cache.upsert(&note.rel_path, &content, modified_time);
            reloaded_paths.insert(note.rel_path.clone());
        }
    }

    if should_refresh {
        *last_external_refresh = Some(now);
    }

    let mut engine_state = match engine_state.take() {
        Some(state) => state,
        None => manager.load_engine_state().await?,
    };
    let current_paths: HashSet<&str> = notes.iter().map(|note| note.rel_path.as_str()).collect();
    let mut changed = false;

    let existing_paths: Vec<String> = engine_state
        .search_index
        .documents
        .keys()
        .cloned()
        .collect();
    for path in existing_paths {
        if !current_paths.contains(path.as_str()) {
            engine_state.remove_document(&path);
            changed = true;
        }
    }

    for note in notes {
        if let Some(indexed) = cache.notes.get(&note.rel_path) {
            let needs_indexing = match engine_state.search_index.documents.get(&note.rel_path) {
                Some(doc_meta) => {
                    reloaded_paths.contains(&note.rel_path)
                        || doc_meta.last_updated != note.last_updated
                        || doc_meta.labels != note.labels
                }
                None => true,
            };
            if needs_indexing {
                engine_state.process_document(
                    &note.rel_path,
                    &indexed.content,
                    &note.labels,
                    note.last_updated.clone(),
                );
                changed = true;
            }
        }
    }

    if changed {
        manager.save_engine_state(&engine_state).await?;
    }

    Ok((engine_state, reloaded_paths))
}
