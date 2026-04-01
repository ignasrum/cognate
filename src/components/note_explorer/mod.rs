//! Note explorer component.
//!
//! Provides notebook tree state and message types for note/folder navigation.

#[path = "note_explorer.rs"]
mod core;

pub use core::Message;
pub use core::NoteExplorer;
