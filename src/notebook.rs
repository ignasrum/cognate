//! Notebook domain layer.
//!
//! This module defines note metadata and re-exports notebook operations for
//! create/delete/move/search and metadata/content persistence.

#[path = "notebook/backend.rs"]
mod backend;
#[path = "notebook/error.rs"]
mod error;
#[path = "notebook/operations.rs"]
mod operations;
#[path = "notebook/search.rs"]
mod search;
#[path = "notebook/storage.rs"]
mod storage;

pub use cognate_engine::storage::NoteMetadata;

/// Search result surface returned to the editor search UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSearchResult {
    /// Matching note path.
    pub rel_path: String,
    /// User-facing snippet that explains the match.
    pub snippet: String,
}

pub use backend::configure_backend;
pub(crate) use backend::is_api as is_api_backend;
pub(crate) use backend::load_note_content;
#[allow(unused_imports)]
pub use error::{EngineResultExt, NotebookError, NotebookErrorKind};
pub use operations::{create_new_note, delete_note, move_note};
pub use search::{SearchNote, clear_search_index_for_notebook, search_notes_with_snapshot};
pub use storage::{
    MetadataLoadResult, current_timestamp_rfc3339, load_notes_metadata, save_metadata,
    save_note_content,
};
