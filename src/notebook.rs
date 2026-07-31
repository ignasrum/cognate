//! Notebook domain layer.
//!
//! This module defines note metadata and re-exports notebook operations for
//! create/delete/move/search and metadata/content persistence.

#[path = "notebook/backend.rs"]
mod backend;
#[path = "notebook/embedded_api.rs"]
mod embedded_api;
#[path = "notebook/error.rs"]
mod error;
#[path = "notebook/offline_queue.rs"]
mod offline_queue;
#[path = "notebook/operations.rs"]
mod operations;
#[path = "notebook/storage.rs"]
mod storage;
#[path = "notebook/write_coordinator.rs"]
mod write_coordinator;

pub use cognate_engine::storage::NoteMetadata;

/// Search result surface returned to the editor search UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSearchResult {
    /// Matching note path.
    pub rel_path: String,
    /// User-facing snippet that explains the match.
    pub snippet: String,
    /// Search field that produced the match.
    pub match_type: cognate_engine::search::SearchMatchType,
    /// Character ranges within the snippet that matched the query.
    pub highlights: Vec<cognate_engine::search::SearchHighlight>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSearchPage {
    pub results: Vec<NoteSearchResult>,
    pub next_cursor: Option<String>,
    pub total: usize,
}

pub use backend::configure_backend;
pub(crate) use backend::is_api as is_api_backend;
pub(crate) use backend::load_note_content;
pub(crate) use backend::replay_offline_queue;
pub(crate) use backend::set_note_revision;
pub(crate) use backend::shutdown_backend;
pub(crate) use backend::{delete_attachment, download_attachment, upload_attachment};
#[allow(unused_imports)]
pub use error::{NotebookError, NotebookErrorKind};
pub use operations::{create_new_note, delete_note, move_note};
pub use storage::{
    MetadataLoadResult, current_timestamp_rfc3339, load_notes_metadata, save_metadata,
    save_note_content,
};

pub(crate) async fn search_notes_page(
    notebook_path: String,
    query: String,
    limit: usize,
    cursor: Option<String>,
) -> Result<NoteSearchPage, NotebookError> {
    backend::search_page(notebook_path, query, limit, cursor).await
}
