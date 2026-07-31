pub mod links;
pub mod metrics;
pub mod search;
pub mod storage;
pub mod tasks;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EngineError {
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Deserialization error: {0}")]
    Deserialization(String),
    #[error("Validation error inside {context}: {detail}")]
    Validation {
        context: &'static str,
        detail: String,
    },
    #[error("Storage error inside {context}: {detail}")]
    Storage {
        context: &'static str,
        detail: String,
    },
    #[error("Recovery error inside {context}: {detail}")]
    Recovery {
        context: &'static str,
        detail: String,
    },
    #[error("Lock unavailable for {resource} inside {context}: {detail}")]
    LockUnavailable {
        context: &'static str,
        resource: String,
        detail: String,
    },
    #[error("conflict inside {context}: {detail}")]
    Conflict {
        context: &'static str,
        detail: String,
    },
}

impl EngineError {
    pub fn validation(context: &'static str, detail: impl Into<String>) -> Self {
        Self::Validation {
            context,
            detail: detail.into(),
        }
    }

    pub fn storage(context: &'static str, detail: impl Into<String>) -> Self {
        Self::Storage {
            context,
            detail: detail.into(),
        }
    }

    pub fn recovery(context: &'static str, detail: impl Into<String>) -> Self {
        Self::Recovery {
            context,
            detail: detail.into(),
        }
    }

    pub fn lock_unavailable(
        context: &'static str,
        resource: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::LockUnavailable {
            context,
            resource: resource.into(),
            detail: detail.into(),
        }
    }

    pub fn conflict(context: &'static str, detail: impl Into<String>) -> Self {
        Self::Conflict {
            context,
            detail: detail.into(),
        }
    }
}

/// Central state manager holding all indexed databases for a notebook.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct NotebookEngineState {
    pub search_index: search::InvertedIndex,
    pub link_graph: links::LinkGraph,
    pub task_register: tasks::TaskRegister,
    pub metrics_register: metrics::MetricsRegister,
}

impl NotebookEngineState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_from_bytes(bytes: &[u8]) -> Result<Self, EngineError> {
        serde_json::from_slice(bytes).map_err(|err| EngineError::Deserialization(err.to_string()))
    }

    pub fn save_to_bytes(&self) -> Result<Vec<u8>, EngineError> {
        serde_json::to_vec(self).map_err(|err| EngineError::Serialization(err.to_string()))
    }

    pub fn process_document(
        &mut self,
        path: &str,
        content: &str,
        labels: &[String],
        last_updated: Option<String>,
    ) {
        self.search_index
            .index_document(path, content, labels, last_updated);
        self.task_register.update_tasks_for_note(path, content);
        self.metrics_register.update_metrics_for_note(path, content);
    }

    pub fn remove_document(&mut self, path: &str) {
        self.search_index.remove_document(path);
        self.task_register.remove_note(path);
        self.metrics_register.remove_note(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_and_relevance() {
        let mut state = NotebookEngineState::new();

        // Index a recipe about apple pie, with specific labels and headers
        state.process_document(
            "recipes/apple-pie",
            "# Apple Pie Recipe\nThis is a sweet dessert baked in the oven.",
            &["baking".to_string(), "sweet".to_string()],
            None,
        );

        // Index another note that just mentions apples in the body
        state.process_document(
            "general/notes",
            "I bought some apples and bananas today at the market.",
            &[],
            None,
        );

        // 1. Check basic search
        let hits = search::execute_search(&state.search_index, "apple", 10);
        assert_eq!(hits.len(), 2);
        // The recipe has "Apple" in a header and "baking"/"sweet" tags, so it should rank higher
        assert_eq!(hits[0].path, "recipes/apple-pie");

        // 2. Check label weighting
        let hits_tag = search::execute_search(&state.search_index, "sweet", 10);
        assert_eq!(hits_tag.len(), 1);
        assert_eq!(hits_tag[0].path, "recipes/apple-pie");

        // 3. Check Boolean negative filter
        let hits_filtered = search::execute_search(&state.search_index, "apple NOT dessert", 10);
        assert_eq!(hits_filtered.len(), 1);
        assert_eq!(hits_filtered[0].path, "general/notes");
    }

    #[test]
    fn test_serialization_cycle() {
        let mut state = NotebookEngineState::new();
        state.process_document(
            "recipes/apple-pie",
            "# Apple Pie\nSweet dessert.",
            &["baking".to_string()],
            None,
        );

        let bytes = state.save_to_bytes().unwrap();
        let restored = NotebookEngineState::load_from_bytes(&bytes).unwrap();

        assert_eq!(restored.search_index.documents.len(), 1);
        assert!(
            restored
                .search_index
                .documents
                .contains_key("recipes/apple-pie")
        );
    }

    #[test]
    fn test_tasks_and_metrics() {
        let mut state = NotebookEngineState::new();

        // Process note with tasks and content
        state.process_document(
            "todos/work",
            "# Work Tasks\n\n- [ ] Finish search indexing module due: 2026-07-06\n- [x] Read architecture documentation\n- [ ] Write progress report @due(2026-07-10)\n\nThis is a simple paragraph to test readability.",
            &["work".to_string()],
            None,
        );

        // Check pending tasks
        let pending = state.task_register.get_pending_tasks();
        assert_eq!(pending.len(), 2);

        let task_1 = pending
            .iter()
            .find(|t| t.text.contains("Finish search"))
            .unwrap();
        assert_eq!(task_1.due_date, Some("2026-07-06".to_string()));
        assert!(!task_1.is_completed);

        let task_2 = pending
            .iter()
            .find(|t| t.text.contains("Write progress"))
            .unwrap();
        assert_eq!(task_2.due_date, Some("2026-07-10".to_string()));

        // Check document metrics
        let metrics = state
            .metrics_register
            .metrics_by_note
            .get("todos/work")
            .unwrap();
        assert!(metrics.word_count > 10);
        assert!(metrics.character_count > 50);
        assert!(metrics.sentence_count >= 1);
        assert!(metrics.readability_score > 0.0);
    }
}
