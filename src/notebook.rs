//! Notebook domain layer.
//!
//! This module defines note metadata and re-exports notebook operations for
//! create/delete/move/search and metadata/content persistence.

use serde::{Deserialize, Serialize};

const STAGED_DELETE_PREFIX: &str = ".cognate_txn_delete_";
const STAGED_DELETE_CLEANUP_GRACE_NANOS: u128 = 5 * 60 * 1_000_000_000;

#[path = "notebook/operations.rs"]
mod operations;
#[path = "notebook/search.rs"]
mod search;
#[path = "notebook/storage.rs"]
mod storage;

/// Metadata persisted for a single note directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMetadata {
    /// Note directory path relative to the notebook root.
    pub rel_path: String,
    /// User-defined labels attached to this note.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Last update timestamp in RFC3339 format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

/// Root metadata object stored in `metadata.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookMetadata {
    /// All known notes in the notebook.
    pub notes: Vec<NoteMetadata>,
}

/// Search result surface returned to the editor search UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSearchResult {
    /// Matching note path.
    pub rel_path: String,
    /// User-facing snippet that explains the match.
    pub snippet: String,
}

pub use operations::{create_new_note, delete_note, move_note};
pub use search::{clear_search_index_for_notebook, search_notes};
pub use storage::{
    current_timestamp_rfc3339, load_notes_metadata, save_metadata, save_note_content,
    save_note_content_sync,
};
