use serde::{Deserialize, Serialize};

const STAGED_DELETE_PREFIX: &str = ".cognate_txn_delete_";
const STAGED_DELETE_CLEANUP_GRACE_NANOS: u128 = 5 * 60 * 1_000_000_000;

#[path = "notebook/operations.rs"]
mod operations;
#[path = "notebook/search.rs"]
mod search;
#[path = "notebook/storage.rs"]
mod storage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMetadata {
    pub rel_path: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookMetadata {
    pub notes: Vec<NoteMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSearchResult {
    pub rel_path: String,
    pub snippet: String,
}

pub use operations::{create_new_note, delete_note, move_note};
pub use search::{clear_search_index_for_notebook, search_notes};
pub use storage::{
    current_timestamp_rfc3339, load_notes_metadata, save_metadata, save_note_content,
    save_note_content_sync,
};
