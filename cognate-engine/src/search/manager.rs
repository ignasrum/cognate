use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use super::query::{ParsedSearchQuery, parse_query};
use crate::storage::{NoteMetadata, NotebookManager};
use crate::{EngineError, NotebookEngineState};

#[derive(Debug, Clone)]
struct CachedNote {
    content: Arc<str>,
    content_lower: Arc<str>,
    modified_time: Option<SystemTime>,
}

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
        let content = if let Some(note) = self.notes_cache.get(rel_path) {
            note.content.to_string()
        } else {
            NotebookManager::new(&self.notebook_path)
                .load_note_content(rel_path)
                .await?
        };
        self.upsert_note(rel_path, &content, labels, last_updated)
            .await
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
        let mut replacements = Vec::new();
        for path in &paths {
            let target = if path == from_rel_path {
                to_rel_path.to_string()
            } else {
                format!("{to_rel_path}{}", &path[from_rel_path.len()..])
            };
            let content = match self.notes_cache.get(path) {
                Some(note) => note.content.to_string(),
                None => manager.load_note_content(path).await?,
            };
            let metadata = engine_state.search_index.documents.get(path).cloned();
            replacements.push((path.clone(), target, content, metadata));
        }
        for (path, _, _, _) in &replacements {
            engine_state.remove_document(path);
        }
        for (_, target, content, metadata) in replacements {
            if let Some(metadata) = metadata {
                engine_state.process_document(
                    &target,
                    &content,
                    &metadata.labels,
                    metadata.last_updated,
                );
            }
        }
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
        let now = Instant::now();
        let should_refresh = match self.last_external_refresh {
            Some(last) => now.duration_since(last) >= refresh_interval,
            None => true,
        };

        let note_paths: HashSet<&str> = notes.iter().map(|n| n.rel_path.as_str()).collect();
        self.notes_cache
            .retain(|rel_path, _| note_paths.contains(rel_path.as_str()));

        let manager = NotebookManager::new(&self.notebook_path);
        let mut reloaded_paths: HashSet<String> = HashSet::new();

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
                reloaded_paths.insert(note.rel_path.clone());
            }
        }

        if should_refresh {
            self.last_external_refresh = Some(now);
        }

        let mut engine_state = match self.engine_state.take() {
            Some(state) => state,
            None => manager.load_engine_state().await?,
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

            let content_match = self.notes_cache.get(&note.rel_path).and_then(|indexed| {
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

fn find_matching_content_snippet(
    content: &str,
    query: &ParsedSearchQuery,
) -> Option<(String, Vec<SearchHighlight>)> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let lower = trimmed.to_lowercase();
        if query.phrases.iter().all(|phrase| lower.contains(phrase))
            && query.terms.iter().all(|term| lower.contains(term))
        {
            let snippet = truncate_search_snippet(trimmed, 120);
            return Some((snippet.clone(), highlight_ranges(&snippet, query)));
        }
    }

    None
}

fn highlight_ranges(text: &str, query: &ParsedSearchQuery) -> Vec<SearchHighlight> {
    let lower = text.to_lowercase();
    let mut ranges = Vec::new();
    for term in query.terms.iter().chain(query.phrases.iter()) {
        let mut start = 0;
        while let Some(found) = lower[start..].find(term) {
            let start_index = start + found;
            ranges.push(SearchHighlight {
                start: text[..start_index].chars().count(),
                end: text[..start_index + term.len()].chars().count(),
            });
            start = start_index + term.len();
        }
    }
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<SearchHighlight> = Vec::new();
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end
        {
            previous.end = previous.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}
