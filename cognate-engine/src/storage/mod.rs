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
