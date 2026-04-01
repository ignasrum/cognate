use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use super::{NoteMetadata, NoteSearchResult};

#[cfg(test)]
const SEARCH_INDEX_EXTERNAL_REFRESH_INTERVAL: Duration = Duration::from_millis(150);
#[cfg(not(test))]
const SEARCH_INDEX_EXTERNAL_REFRESH_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(test)]
const SEARCH_INDEX_IDLE_EVICTION_INTERVAL: Duration = Duration::from_millis(300);
#[cfg(not(test))]
const SEARCH_INDEX_IDLE_EVICTION_INTERVAL: Duration = Duration::from_secs(15 * 60);
#[cfg(test)]
const SEARCH_INDEX_MAX_CACHED_NOTEBOOKS: usize = 4;
#[cfg(not(test))]
const SEARCH_INDEX_MAX_CACHED_NOTEBOOKS: usize = 24;

#[derive(Debug, Clone, Default)]
struct IndexedNoteContent {
    content: Arc<str>,
    content_lower: Arc<str>,
    modified_time: Option<SystemTime>,
}

#[derive(Debug)]
struct NotebookSearchIndex {
    notes_by_path: HashMap<String, IndexedNoteContent>,
    last_external_refresh: Option<Instant>,
    last_accessed_at: Instant,
}

impl Default for NotebookSearchIndex {
    fn default() -> Self {
        Self {
            notes_by_path: HashMap::new(),
            last_external_refresh: None,
            last_accessed_at: Instant::now(),
        }
    }
}

static SEARCH_INDEXES_BY_NOTEBOOK: OnceLock<Mutex<HashMap<String, NotebookSearchIndex>>> =
    OnceLock::new();

fn search_indexes() -> &'static Mutex<HashMap<String, NotebookSearchIndex>> {
    SEARCH_INDEXES_BY_NOTEBOOK.get_or_init(|| Mutex::new(HashMap::new()))
}

fn prune_search_indexes(search_indexes: &mut HashMap<String, NotebookSearchIndex>) {
    let now = Instant::now();
    search_indexes.retain(|_, index| {
        now.duration_since(index.last_accessed_at) < SEARCH_INDEX_IDLE_EVICTION_INTERVAL
    });

    if search_indexes.len() <= SEARCH_INDEX_MAX_CACHED_NOTEBOOKS {
        return;
    }

    let mut by_last_access: Vec<(String, Instant)> = search_indexes
        .iter()
        .map(|(path, index)| (path.clone(), index.last_accessed_at))
        .collect();
    by_last_access.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));

    let remove_count = by_last_access
        .len()
        .saturating_sub(SEARCH_INDEX_MAX_CACHED_NOTEBOOKS);
    for (path, _) in by_last_access.into_iter().take(remove_count) {
        search_indexes.remove(&path);
    }
}

fn touch_search_index(index: &mut NotebookSearchIndex) {
    index.last_accessed_at = Instant::now();
}

pub fn clear_search_index_for_notebook(notebook_path: &str) {
    let mut search_indexes = search_indexes()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    search_indexes.remove(notebook_path);
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

pub(super) fn note_file_modified_time(note_file_path: &Path) -> Option<SystemTime> {
    fs::metadata(note_file_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
}

fn read_note_content_for_index(notebook_path: &str, rel_path: &str) -> IndexedNoteContent {
    let note_file_path = Path::new(notebook_path).join(rel_path).join("note.md");
    let content = fs::read_to_string(&note_file_path).unwrap_or_default();
    let content_lower = content.to_lowercase();
    let modified_time = note_file_modified_time(&note_file_path);

    IndexedNoteContent {
        content_lower: Arc::from(content_lower),
        content: Arc::from(content),
        modified_time,
    }
}

pub(super) fn cache_upsert_search_index_note_content(
    notebook_path: &str,
    rel_path: &str,
    content: &str,
    modified_time: Option<SystemTime>,
) {
    let mut search_indexes = search_indexes()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    prune_search_indexes(&mut search_indexes);
    let index = search_indexes.entry(notebook_path.to_string()).or_default();
    touch_search_index(index);

    index.notes_by_path.insert(
        rel_path.to_string(),
        IndexedNoteContent {
            content: Arc::from(content.to_string()),
            content_lower: Arc::from(content.to_lowercase()),
            modified_time,
        },
    );
}

pub(super) fn cache_remove_search_index_entries(notebook_path: &str, rel_path: &str) {
    let mut search_indexes = search_indexes()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    prune_search_indexes(&mut search_indexes);

    let mut remove_notebook_index = false;
    if let Some(index) = search_indexes.get_mut(notebook_path) {
        touch_search_index(index);

        let prefix = format!("{rel_path}/");
        index
            .notes_by_path
            .retain(|path, _| path != rel_path && !path.starts_with(&prefix));
        remove_notebook_index = index.notes_by_path.is_empty();
    }

    if remove_notebook_index {
        search_indexes.remove(notebook_path);
    }
}

pub(super) fn cache_rename_search_index_entries(
    notebook_path: &str,
    from_rel_path: &str,
    to_rel_path: &str,
) {
    let mut search_indexes = search_indexes()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    prune_search_indexes(&mut search_indexes);
    let Some(index) = search_indexes.get_mut(notebook_path) else {
        return;
    };
    touch_search_index(index);

    let from_prefix = format!("{from_rel_path}/");
    let to_prefix = format!("{to_rel_path}/");
    let existing_paths: Vec<String> = index.notes_by_path.keys().cloned().collect();
    let mut remapped_entries = Vec::new();

    for existing_path in existing_paths {
        if existing_path == from_rel_path {
            remapped_entries.push((existing_path, to_rel_path.to_string()));
        } else if existing_path.starts_with(&from_prefix) {
            let suffix = existing_path[from_prefix.len()..].to_string();
            remapped_entries.push((existing_path, format!("{to_prefix}{suffix}")));
        }
    }

    for (old_path, new_path) in remapped_entries {
        if let Some(entry) = index.notes_by_path.remove(&old_path) {
            index.notes_by_path.insert(new_path, entry);
        }
    }
}

fn should_refresh_search_index_from_filesystem(last_refresh: Option<Instant>) -> bool {
    match last_refresh {
        Some(last) => last.elapsed() >= SEARCH_INDEX_EXTERNAL_REFRESH_INTERVAL,
        None => true,
    }
}

pub async fn search_notes(
    notebook_path: String,
    notes: Vec<NoteMetadata>,
    query: String,
) -> Vec<NoteSearchResult> {
    let normalized_query = query.trim().to_lowercase();
    if normalized_query.is_empty() {
        return Vec::new();
    }

    let note_paths: HashSet<&str> = notes.iter().map(|note| note.rel_path.as_str()).collect();
    let (missing_paths, refresh_candidates, should_refresh) = {
        let mut search_indexes = search_indexes()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        prune_search_indexes(&mut search_indexes);
        let index = search_indexes.entry(notebook_path.clone()).or_default();
        touch_search_index(index);
        index
            .notes_by_path
            .retain(|rel_path, _| note_paths.contains(rel_path.as_str()));

        let should_refresh =
            should_refresh_search_index_from_filesystem(index.last_external_refresh);
        let mut missing_paths = Vec::new();
        let mut refresh_candidates = Vec::new();

        for note in &notes {
            if let Some(indexed) = index.notes_by_path.get(&note.rel_path) {
                if should_refresh {
                    refresh_candidates.push((note.rel_path.clone(), indexed.modified_time));
                }
            } else {
                missing_paths.push(note.rel_path.clone());
            }
        }

        (missing_paths, refresh_candidates, should_refresh)
    };

    let mut missing_entries = Vec::with_capacity(missing_paths.len());
    for rel_path in missing_paths {
        missing_entries.push((
            rel_path.clone(),
            read_note_content_for_index(&notebook_path, &rel_path),
        ));
    }

    let mut refreshed_entries = Vec::new();
    if should_refresh {
        for (rel_path, previous_modified_time) in refresh_candidates {
            let note_file_path = Path::new(&notebook_path).join(&rel_path).join("note.md");
            let modified_time = note_file_modified_time(&note_file_path);

            if previous_modified_time != modified_time {
                refreshed_entries.push((
                    rel_path.clone(),
                    previous_modified_time,
                    read_note_content_for_index(&notebook_path, &rel_path),
                ));
            }
        }
    }

    let content_snapshot_by_path = {
        let mut search_indexes = search_indexes()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        prune_search_indexes(&mut search_indexes);
        let index = search_indexes.entry(notebook_path.clone()).or_default();
        touch_search_index(index);
        index
            .notes_by_path
            .retain(|rel_path, _| note_paths.contains(rel_path.as_str()));

        for (rel_path, indexed_note) in missing_entries {
            index.notes_by_path.entry(rel_path).or_insert(indexed_note);
        }

        for (rel_path, expected_previous_modified_time, indexed_note) in refreshed_entries {
            let should_apply = index.notes_by_path.get(&rel_path).map_or(true, |existing| {
                existing.modified_time == expected_previous_modified_time
            });

            if should_apply {
                index.notes_by_path.insert(rel_path, indexed_note);
            }
        }

        if should_refresh {
            index.last_external_refresh = Some(Instant::now());
        }

        let mut snapshot = HashMap::with_capacity(notes.len());
        for note in &notes {
            if let Some(indexed) = index.notes_by_path.get(&note.rel_path) {
                snapshot.insert(note.rel_path.clone(), indexed.clone());
            }
        }
        snapshot
    };

    let mut results = Vec::new();

    for note in &notes {
        let rel_path_match = note.rel_path.to_lowercase().contains(&normalized_query);
        let label_match = note
            .labels
            .iter()
            .find(|label| label.to_lowercase().contains(&normalized_query))
            .cloned();

        let content_match = content_snapshot_by_path
            .get(&note.rel_path)
            .and_then(|indexed| {
                if !indexed.content_lower.contains(&normalized_query) {
                    return None;
                }
                find_matching_content_snippet(indexed.content.as_ref(), &normalized_query)
            });

        if rel_path_match || label_match.is_some() || content_match.is_some() {
            let snippet = if let Some(content_snippet) = content_match {
                content_snippet
            } else if let Some(matching_label) = label_match {
                format!(
                    "Label match: {}",
                    truncate_search_snippet(matching_label.as_str(), 100)
                )
            } else {
                "Path match".to_string()
            };

            results.push(NoteSearchResult {
                rel_path: note.rel_path.clone(),
                snippet,
            });
        }
    }

    results.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    results
}

#[cfg(test)]
mod search_index_eviction_tests {
    use super::*;

    #[test]
    fn prune_search_indexes_removes_idle_notebooks() {
        let now = Instant::now();
        let stale_last_access = now
            .checked_sub(SEARCH_INDEX_IDLE_EVICTION_INTERVAL + Duration::from_millis(1))
            .unwrap_or(now);

        let mut indexes = HashMap::new();
        indexes.insert(
            "stale".to_string(),
            NotebookSearchIndex {
                notes_by_path: HashMap::new(),
                last_external_refresh: None,
                last_accessed_at: stale_last_access,
            },
        );
        indexes.insert(
            "active".to_string(),
            NotebookSearchIndex {
                notes_by_path: HashMap::new(),
                last_external_refresh: None,
                last_accessed_at: now,
            },
        );

        prune_search_indexes(&mut indexes);

        assert!(
            !indexes.contains_key("stale"),
            "Expected stale notebook index to be evicted"
        );
        assert!(
            indexes.contains_key("active"),
            "Expected active notebook index to remain cached"
        );
    }

    #[test]
    fn prune_search_indexes_enforces_max_cached_notebooks() {
        let now = Instant::now();
        let total_notebooks = SEARCH_INDEX_MAX_CACHED_NOTEBOOKS + 2;
        let mut indexes = HashMap::new();

        for i in 0..total_notebooks {
            let age = Duration::from_millis((total_notebooks - i) as u64);
            let last_accessed_at = now.checked_sub(age).unwrap_or(now);
            indexes.insert(
                format!("notebook_{i}"),
                NotebookSearchIndex {
                    notes_by_path: HashMap::new(),
                    last_external_refresh: None,
                    last_accessed_at,
                },
            );
        }

        prune_search_indexes(&mut indexes);

        assert_eq!(
            indexes.len(),
            SEARCH_INDEX_MAX_CACHED_NOTEBOOKS,
            "Expected cache size to be capped at SEARCH_INDEX_MAX_CACHED_NOTEBOOKS"
        );
        assert!(
            !indexes.contains_key("notebook_0"),
            "Expected oldest notebook index to be evicted first"
        );
        assert!(
            !indexes.contains_key("notebook_1"),
            "Expected second-oldest notebook index to be evicted when over capacity"
        );
    }
}
