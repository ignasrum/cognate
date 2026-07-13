use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::storage::{NoteMetadata, NotebookManager};
use crate::{EngineError, NotebookEngineState};

#[derive(Debug, Clone)]
struct CachedNote {
    content: Arc<str>,
    content_lower: Arc<str>,
    modified_time: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub struct SearchResultEntry {
    pub rel_path: String,
    pub snippet: String,
    pub score: f32,
}

#[derive(Debug)]
pub struct SearchIndexManager {
    notebook_path: PathBuf,
    notes_cache: HashMap<String, CachedNote>,
    engine_state: Option<NotebookEngineState>,
    last_external_refresh: Option<Instant>,
}

impl SearchIndexManager {
    pub fn new(notebook_path: &Path) -> Self {
        Self {
            notebook_path: notebook_path.to_path_buf(),
            notes_cache: HashMap::new(),
            engine_state: None,
            last_external_refresh: None,
        }
    }

    pub fn cache_upsert(
        &mut self,
        rel_path: &str,
        content: &str,
        modified_time: Option<SystemTime>,
    ) {
        self.notes_cache.insert(
            rel_path.to_string(),
            CachedNote {
                content: Arc::from(content.to_string()),
                content_lower: Arc::from(content.to_lowercase()),
                modified_time,
            },
        );
    }

    pub fn cache_remove(&mut self, rel_path: &str) {
        let prefix = format!("{}/", rel_path);
        self.notes_cache
            .retain(|path, _| path != rel_path && !path.starts_with(&prefix));
    }

    pub fn cache_rename(&mut self, from_rel: &str, to_rel: &str) {
        let from_prefix = format!("{}/", from_rel);
        let to_prefix = format!("{}/", to_rel);
        let existing_paths: Vec<String> = self.notes_cache.keys().cloned().collect();
        let mut remapped = Vec::new();

        for path in existing_paths {
            if path == from_rel {
                remapped.push((path, to_rel.to_string()));
            } else if path.starts_with(&from_prefix) {
                let suffix = path[from_prefix.len()..].to_string();
                remapped.push((path, format!("{}{}", to_prefix, suffix)));
            }
        }

        for (old_path, new_path) in remapped {
            if let Some(entry) = self.notes_cache.remove(&old_path) {
                self.notes_cache.insert(new_path, entry);
            }
        }
    }

    pub async fn search(
        &mut self,
        query: &str,
        notes: &[NoteMetadata],
        refresh_interval: Duration,
    ) -> Result<Vec<SearchResultEntry>, EngineError> {
        let now = Instant::now();
        let should_refresh = match self.last_external_refresh {
            Some(last) => now.duration_since(last) >= refresh_interval,
            None => true,
        };

        let note_paths: HashSet<&str> = notes.iter().map(|n| n.rel_path.as_str()).collect();
        self.notes_cache
            .retain(|rel_path, _| note_paths.contains(rel_path.as_str()));

        let manager = NotebookManager::new(&self.notebook_path);

        // Process missing/changed notes
        for note in notes {
            let in_cache = self.notes_cache.contains_key(&note.rel_path);
            let mut needs_reload = false;

            if in_cache && should_refresh {
                let current_mtime = manager.get_note_modified_time(&note.rel_path).await;
                let cached_mtime = self
                    .notes_cache
                    .get(&note.rel_path)
                    .and_then(|c| c.modified_time);
                if current_mtime != cached_mtime {
                    needs_reload = true;
                }
            }

            if !in_cache || needs_reload {
                let content = manager
                    .load_note_content(&note.rel_path)
                    .await
                    .unwrap_or_default();
                let content_lower = content.to_lowercase();
                let modified_time = manager.get_note_modified_time(&note.rel_path).await;

                self.notes_cache.insert(
                    note.rel_path.clone(),
                    CachedNote {
                        content: Arc::from(content),
                        content_lower: Arc::from(content_lower),
                        modified_time,
                    },
                );
            }
        }

        if should_refresh {
            self.last_external_refresh = Some(now);
        }

        let mut engine_state = match self.engine_state.take() {
            Some(state) => state,
            None => manager.load_engine_state().await,
        };

        let mut changed = false;
        let current_paths: HashSet<&str> = notes.iter().map(|n| n.rel_path.as_str()).collect();

        // Remove deleted documents from index
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

        // Sync with newest snapshot note content and labels
        for note in notes {
            if let Some(indexed) = self.notes_cache.get(&note.rel_path) {
                let needs_indexing = match engine_state.search_index.documents.get(&note.rel_path) {
                    Some(doc_meta) => {
                        doc_meta.last_updated != note.last_updated || doc_meta.labels != note.labels
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
            manager.save_engine_state(&engine_state).await;
        }

        self.engine_state = Some(engine_state.clone());

        let hits = crate::search::execute_search(&engine_state.search_index, query, usize::MAX);
        let mut scores_by_path: HashMap<String, f32> =
            hits.into_iter().map(|hit| (hit.path, hit.score)).collect();

        let mut results = Vec::new();
        let normalized_query = query.trim().to_lowercase();

        for note in notes {
            let rel_path_match = note.rel_path.to_lowercase().contains(&normalized_query);
            let label_match = note
                .labels
                .iter()
                .find(|label| label.to_lowercase().contains(&normalized_query))
                .cloned();

            let content_match = self.notes_cache.get(&note.rel_path).and_then(|indexed| {
                if !indexed.content_lower.contains(&normalized_query) {
                    return None;
                }
                find_matching_content_snippet(&indexed.content, &normalized_query)
            });

            let engine_score = scores_by_path.remove(&note.rel_path);

            if engine_score.is_some()
                || rel_path_match
                || label_match.is_some()
                || content_match.is_some()
            {
                let snippet = if let Some(content_snippet) = content_match {
                    content_snippet
                } else if let Some(matching_label) = label_match {
                    format!(
                        "Label match: {}",
                        truncate_search_snippet(&matching_label, 100)
                    )
                } else {
                    "Path match".to_string()
                };

                results.push(SearchResultEntry {
                    rel_path: note.rel_path.clone(),
                    snippet,
                    score: engine_score.unwrap_or(0.0),
                });
            }
        }

        Ok(results)
    }

    pub fn clear_cache(&mut self) {
        self.notes_cache.clear();
        self.last_external_refresh = None;
        self.engine_state = None;
    }
}

fn truncate_search_snippet(input: &str, max_chars: usize) -> String {
    let char_count = input.chars().count();
    if char_count <= max_chars {
        input.to_string()
    } else {
        let mut truncated: String = input.chars().take(max_chars).collect();
        truncated.push_str("...");
        truncated
    }
}

fn find_matching_content_snippet(content: &str, normalized_query: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.to_lowercase().contains(normalized_query) {
            return Some(truncate_search_snippet(trimmed, 120));
        }
    }

    None
}
