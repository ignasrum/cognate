use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
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

#[derive(Debug, Clone)]
pub struct SearchNote {
    pub rel_path: String,
    pub labels: Vec<String>,
    pub last_updated: Option<String>,
}

impl From<&NoteMetadata> for SearchNote {
    fn from(note: &NoteMetadata) -> Self {
        Self {
            rel_path: note.rel_path.clone(),
            labels: note.labels.clone(),
            last_updated: note.last_updated.clone(),
        }
    }
}

#[derive(Debug)]
struct NotebookSearchIndex {
    manager: cognate_engine::search::SearchIndexManager,
    last_accessed_at: Instant,
}

impl NotebookSearchIndex {
    fn new(notebook_path: &str) -> Self {
        Self {
            manager: cognate_engine::search::SearchIndexManager::new(Path::new(notebook_path)),
            last_accessed_at: Instant::now(),
        }
    }
}

static SEARCH_INDEXES_BY_NOTEBOOK: OnceLock<Mutex<HashMap<String, NotebookSearchIndex>>> =
    OnceLock::new();

fn search_indexes() -> &'static Mutex<HashMap<String, NotebookSearchIndex>> {
    SEARCH_INDEXES_BY_NOTEBOOK.get_or_init(|| Mutex::new(HashMap::new()))
}

fn with_search_indexes<R>(f: impl FnOnce(&mut HashMap<String, NotebookSearchIndex>) -> R) -> R {
    let mut search_indexes = search_indexes()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut search_indexes)
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
    with_search_indexes(|search_indexes| {
        search_indexes.remove(notebook_path);
    });
}

pub(super) fn cache_upsert_search_index_note_content(
    notebook_path: &str,
    rel_path: &str,
    content: &str,
    modified_time: Option<SystemTime>,
) {
    with_search_indexes(|search_indexes| {
        prune_search_indexes(search_indexes);
        let index = search_indexes
            .entry(notebook_path.to_string())
            .or_insert_with(|| NotebookSearchIndex::new(notebook_path));
        touch_search_index(index);

        index.manager.cache_upsert(rel_path, content, modified_time);
    });
}

pub(super) fn cache_remove_search_index_entries(notebook_path: &str, rel_path: &str) {
    with_search_indexes(|search_indexes| {
        prune_search_indexes(search_indexes);

        if let Some(index) = search_indexes.get_mut(notebook_path) {
            touch_search_index(index);
            index.manager.cache_remove(rel_path);
        }
    });
}

pub(super) fn cache_rename_search_index_entries(
    notebook_path: &str,
    from_rel_path: &str,
    to_rel_path: &str,
) {
    with_search_indexes(|search_indexes| {
        prune_search_indexes(search_indexes);
        if let Some(index) = search_indexes.get_mut(notebook_path) {
            touch_search_index(index);
            index.manager.cache_rename(from_rel_path, to_rel_path);
        }
    });
}

pub async fn search_notes_with_snapshot(
    notebook_path: String,
    notes: Vec<SearchNote>,
    query: String,
) -> Vec<NoteSearchResult> {
    let normalized_query = query.trim().to_lowercase();
    if normalized_query.is_empty() {
        return Vec::new();
    }

    let engine_notes: Vec<cognate_engine::storage::NoteMetadata> = notes
        .iter()
        .map(|n| cognate_engine::storage::NoteMetadata {
            rel_path: n.rel_path.clone(),
            labels: n.labels.clone(),
            last_updated: n.last_updated.clone(),
        })
        .collect();

    let engine_results = with_search_indexes(|search_indexes| {
        prune_search_indexes(search_indexes);
        let index = search_indexes
            .entry(notebook_path.clone())
            .or_insert_with(|| NotebookSearchIndex::new(&notebook_path));
        touch_search_index(index);

        index.manager.search(
            &query,
            &engine_notes,
            SEARCH_INDEX_EXTERNAL_REFRESH_INTERVAL,
        )
    });

    let mut results = match engine_results {
        Ok(res) => res,
        Err(_) => return Vec::new(),
    };

    // Sort by BM25 score descending, fallback to alphabetical order of paths
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.rel_path.cmp(&b.rel_path))
    });

    results
        .into_iter()
        .map(|res| NoteSearchResult {
            rel_path: res.rel_path,
            snippet: res.snippet,
        })
        .collect()
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
                manager: cognate_engine::search::SearchIndexManager::new(Path::new("stale")),
                last_accessed_at: stale_last_access,
            },
        );
        indexes.insert(
            "active".to_string(),
            NotebookSearchIndex {
                manager: cognate_engine::search::SearchIndexManager::new(Path::new("active")),
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
                    manager: cognate_engine::search::SearchIndexManager::new(Path::new(&format!("notebook_{i}"))),
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
