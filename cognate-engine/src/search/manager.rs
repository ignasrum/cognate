use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use super::cache::NoteCache;
use super::maintenance::synchronize_index;
use super::matching::{find_matching_content_snippet, highlight_ranges, truncate_search_snippet};
use super::query::parse_query;
use crate::storage::{NoteMetadata, NotebookManager};
use crate::{EngineError, NotebookEngineState};

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct SearchResultEntry {
    pub rel_path: String,
    pub snippet: String,
    pub score: f32,
    pub match_type: SearchMatchType,
    pub highlights: Vec<SearchHighlight>,
}

#[derive(Debug, Clone, Copy, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchMatchType {
    Content,
    Label,
    Path,
    Multiple,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SearchHighlight {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SearchRequest {
    pub query: String,
    pub limit: usize,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct SearchResponse {
    pub results: Vec<SearchResultEntry>,
    pub next_cursor: Option<String>,
    pub total: usize,
}

#[derive(Debug)]
pub struct SearchIndexManager {
    notebook_path: PathBuf,
    notes_cache: NoteCache,
    engine_state: Option<NotebookEngineState>,
    last_external_refresh: Option<Instant>,
}

impl SearchIndexManager {
    pub fn new(notebook_path: &Path) -> Self {
        Self {
            notebook_path: notebook_path.to_path_buf(),
            notes_cache: NoteCache::default(),
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
        self.notes_cache.upsert(rel_path, content, modified_time);
    }

    pub fn cache_remove(&mut self, rel_path: &str) {
        self.notes_cache.remove(rel_path);
    }

    pub fn cache_rename(&mut self, from_rel: &str, to_rel: &str) {
        self.notes_cache.rename(from_rel, to_rel);
    }

    pub async fn upsert_note(
        &mut self,
        rel_path: &str,
        content: &str,
        labels: &[String],
        last_updated: Option<String>,
    ) -> Result<(), EngineError> {
        let manager = NotebookManager::new(&self.notebook_path);
        let modified_time = manager.get_note_modified_time(rel_path).await;
        self.cache_upsert(rel_path, content, modified_time);
        let mut engine_state = match self.engine_state.take() {
            Some(state) => state,
            None => manager.load_engine_state().await?,
        };
        engine_state.process_document(rel_path, content, labels, last_updated);
        manager.save_engine_state(&engine_state).await?;
        self.engine_state = Some(engine_state);
        Ok(())
    }

    pub async fn update_note_metadata(
        &mut self,
        rel_path: &str,
        labels: &[String],
        last_updated: Option<String>,
    ) -> Result<(), EngineError> {
        let content = if let Some(note) = self.notes_cache.notes.get(rel_path) {
            note.content.to_string()
        } else {
            NotebookManager::new(&self.notebook_path)
                .load_note_content(rel_path)
                .await?
        };
        self.upsert_note(rel_path, &content, labels, last_updated)
            .await
    }

    pub async fn update_metadata(&mut self, notes: &[NoteMetadata]) -> Result<(), EngineError> {
        let manager = NotebookManager::new(&self.notebook_path);
        let mut engine_state = match self.engine_state.take() {
            Some(state) => state,
            None => manager.load_engine_state().await?,
        };

        let note_paths = notes
            .iter()
            .map(|note| note.rel_path.as_str())
            .collect::<std::collections::HashSet<_>>();
        let indexed_paths = engine_state
            .search_index
            .documents
            .keys()
            .filter(|path| !note_paths.contains(path.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let mut changed = false;

        for path in indexed_paths {
            engine_state.remove_document(&path);
            self.notes_cache.remove(&path);
            changed = true;
        }

        for note in notes {
            let metadata_changed = match engine_state.search_index.documents.get(&note.rel_path) {
                Some(indexed) => {
                    indexed.labels != note.labels || indexed.last_updated != note.last_updated
                }
                None => true,
            };
            if !metadata_changed {
                continue;
            }

            let content = if let Some(cached) = self.notes_cache.notes.get(&note.rel_path) {
                cached.content.to_string()
            } else {
                manager.load_note_content(&note.rel_path).await?
            };
            if !self.notes_cache.notes.contains_key(&note.rel_path) {
                self.notes_cache.upsert(&note.rel_path, &content, None);
            }
            engine_state.process_search_document(
                &note.rel_path,
                &content,
                &note.labels,
                note.last_updated.clone(),
            );
            changed = true;
        }

        if changed {
            manager.save_engine_state(&engine_state).await?;
        }
        self.engine_state = Some(engine_state);
        Ok(())
    }

    pub async fn remove_note(&mut self, rel_path: &str) -> Result<(), EngineError> {
        let manager = NotebookManager::new(&self.notebook_path);
        let mut engine_state = match self.engine_state.take() {
            Some(state) => state,
            None => manager.load_engine_state().await?,
        };
        let prefix = format!("{rel_path}/");
        let paths = engine_state
            .search_index
            .documents
            .keys()
            .filter(|path| *path == rel_path || path.starts_with(&prefix))
            .cloned()
            .collect::<Vec<_>>();
        for path in paths {
            engine_state.remove_document(&path);
        }
        manager.save_engine_state(&engine_state).await?;
        self.cache_remove(rel_path);
        self.engine_state = Some(engine_state);
        Ok(())
    }

    pub async fn rename_note(
        &mut self,
        from_rel_path: &str,
        to_rel_path: &str,
    ) -> Result<(), EngineError> {
        let manager = NotebookManager::new(&self.notebook_path);
        let mut engine_state = match self.engine_state.take() {
            Some(state) => state,
            None => manager.load_engine_state().await?,
        };
        let from_prefix = format!("{from_rel_path}/");
        let paths = engine_state
            .search_index
            .documents
            .keys()
            .filter(|path| *path == from_rel_path || path.starts_with(&from_prefix))
            .cloned()
            .collect::<Vec<_>>();
        if paths.is_empty() {
            self.cache_rename(from_rel_path, to_rel_path);
            self.engine_state = Some(engine_state);
            return Ok(());
        }
        engine_state.rename_note_paths(from_rel_path, to_rel_path);
        manager.save_engine_state(&engine_state).await?;
        self.cache_rename(from_rel_path, to_rel_path);
        self.engine_state = Some(engine_state);
        Ok(())
    }

    pub async fn search(
        &mut self,
        query: &str,
        notes: &[NoteMetadata],
        refresh_interval: Duration,
    ) -> Result<Vec<SearchResultEntry>, EngineError> {
        Ok(self
            .search_request(
                &SearchRequest {
                    query: query.to_string(),
                    limit: usize::MAX,
                    cursor: None,
                },
                notes,
                refresh_interval,
            )
            .await?
            .results)
    }

    pub async fn search_request(
        &mut self,
        request: &SearchRequest,
        notes: &[NoteMetadata],
        refresh_interval: Duration,
    ) -> Result<SearchResponse, EngineError> {
        let parsed = parse_query(&request.query)
            .map_err(|error| EngineError::validation("search query", error.to_string()))?;
        let query_key = blake3::hash(request.query.trim().as_bytes())
            .to_hex()
            .to_string();
        let offset = match request.cursor.as_deref() {
            None => 0,
            Some(cursor) => {
                let Some((cursor_key, offset)) = cursor.split_once(':') else {
                    return Err(EngineError::validation("search cursor", "invalid cursor"));
                };
                if cursor_key != query_key {
                    return Err(EngineError::validation(
                        "search cursor",
                        "cursor does not belong to this query",
                    ));
                }
                offset
                    .parse::<usize>()
                    .map_err(|_| EngineError::validation("search cursor", "invalid cursor"))?
            }
        };
        let limit = request.limit.clamp(1, 100);
        let (engine_state, _reloaded_paths) = synchronize_index(
            &self.notebook_path,
            &mut self.notes_cache,
            self.engine_state.take(),
            notes,
            refresh_interval,
            &mut self.last_external_refresh,
        )
        .await?;
        self.engine_state = Some(engine_state.clone());

        let hits =
            crate::search::execute_search(&engine_state.search_index, &parsed.text, usize::MAX);
        let mut scores_by_path: HashMap<String, f32> =
            hits.into_iter().map(|hit| (hit.path, hit.score)).collect();

        let mut results = Vec::new();
        for note in notes {
            let path_lower = note.rel_path.to_lowercase();
            let text_fields = parsed
                .terms
                .iter()
                .chain(parsed.phrases.iter())
                .collect::<Vec<_>>();
            let rel_path_match = if parsed.path_filters.is_empty() {
                text_fields.iter().any(|term| path_lower.contains(*term))
            } else {
                parsed
                    .path_filters
                    .iter()
                    .all(|filter| path_lower.contains(filter))
            };
            let label_match = note
                .labels
                .iter()
                .find(|label| {
                    let lower = label.to_lowercase();
                    if parsed.label_filters.is_empty() {
                        text_fields.iter().any(|term| lower.contains(*term))
                    } else {
                        parsed
                            .label_filters
                            .iter()
                            .all(|filter| lower.contains(filter))
                    }
                })
                .cloned();
            let has_label_match = label_match.is_some();
            let excluded = parsed
                .excluded_paths
                .iter()
                .any(|value| path_lower.contains(value))
                || parsed.excluded_labels.iter().any(|value| {
                    note.labels
                        .iter()
                        .any(|label| label.to_lowercase().contains(value))
                });
            if excluded
                || (!parsed.path_filters.is_empty() && !rel_path_match)
                || (!parsed.label_filters.is_empty() && label_match.is_none())
                || parsed.updated_range.as_ref().is_some_and(|(from, to)| {
                    note.last_updated.as_deref().is_none_or(|date| {
                        let date = date.get(..10).unwrap_or(date);
                        date < from.as_str() || date > to.as_str()
                    })
                })
            {
                continue;
            }

            let content_match = self
                .notes_cache
                .notes
                .get(&note.rel_path)
                .and_then(|indexed| {
                    if !parsed
                        .phrases
                        .iter()
                        .all(|phrase| indexed.content_lower.contains(phrase))
                        || !parsed
                            .terms
                            .iter()
                            .all(|term| indexed.content_lower.contains(term))
                        || parsed
                            .excluded_terms
                            .iter()
                            .any(|term| indexed.content_lower.contains(term))
                    {
                        return None;
                    }
                    find_matching_content_snippet(&indexed.content, &parsed)
                });

            let engine_score = scores_by_path.remove(&note.rel_path);

            if engine_score.is_some()
                || rel_path_match
                || label_match.is_some()
                || content_match.is_some()
            {
                let has_content_match = content_match.is_some();
                let (snippet, highlights, _) = if let Some(content_snippet) = content_match {
                    (
                        content_snippet.0,
                        content_snippet.1,
                        SearchMatchType::Content,
                    )
                } else if let Some(matching_label) = label_match {
                    let snippet = truncate_search_snippet(&matching_label, 100);
                    let highlights = highlight_ranges(&snippet, &parsed);
                    (
                        format!("Label match: {snippet}"),
                        highlights,
                        SearchMatchType::Label,
                    )
                } else {
                    let snippet = if parsed.path_filters.is_empty() {
                        "Path match".to_string()
                    } else {
                        truncate_search_snippet(&note.rel_path, 120)
                    };
                    let highlights = highlight_ranges(&snippet, &parsed);
                    (snippet, highlights, SearchMatchType::Path)
                };

                let match_type = match [has_content_match, has_label_match, rel_path_match]
                    .into_iter()
                    .filter(|matched| *matched)
                    .count()
                {
                    0 | 1 if has_content_match => SearchMatchType::Content,
                    0 | 1 if has_label_match => SearchMatchType::Label,
                    _ if has_content_match || has_label_match => SearchMatchType::Multiple,
                    _ => SearchMatchType::Path,
                };

                results.push(SearchResultEntry {
                    rel_path: note.rel_path.clone(),
                    snippet,
                    score: engine_score.unwrap_or(0.0)
                        + if rel_path_match { 10.0 } else { 0.0 }
                        + if has_label_match { 8.0 } else { 0.0 },
                    match_type,
                    highlights,
                });
            }
        }

        results.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.rel_path.cmp(&right.rel_path))
        });
        let total = results.len();
        let end = offset.saturating_add(limit).min(total);
        let page = if offset >= total {
            Vec::new()
        } else {
            results[offset..end].to_vec()
        };
        Ok(SearchResponse {
            results: page,
            next_cursor: (end < total).then(|| format!("{query_key}:{end}")),
            total,
        })
    }

    pub fn clear_cache(&mut self) {
        self.notes_cache.clear();
        self.last_external_refresh = None;
        self.engine_state = None;
    }
}
