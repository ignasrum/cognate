//! Notebook persistence boundaries and filesystem safety invariants.
//!
//! Mutating operations validate relative paths, acquire the notebook or note
//! lock before touching disk, stage writes where needed, and update metadata
//! atomically. Callers should use [`NotebookManager`] rather than bypassing
//! these lock, rollback, and revision boundaries.

pub mod attachments;
pub mod concurrency;
pub mod fs_utils;
mod index_sync;
pub(crate) mod metadata;
mod metadata_persistence;
pub mod notebook;
mod notebook_content;
mod notebook_lifecycle;
pub(crate) mod transactions;

pub use attachments::{AttachmentManager, AttachmentMetadata, attachment_revision};
pub use concurrency::{ConcurrencyManager, FileLockGuard};
pub use metadata::{MetadataLoadResult, NoteMetadata, NotebookMetadata, current_timestamp_rfc3339};
pub use notebook::{NoteContentSaveResult, NotebookManager, note_content_revision};
