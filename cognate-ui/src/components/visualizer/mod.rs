//! Visualizer component.
//!
//! Renders the label-relationship graph and emits note-focused interaction messages.

#[path = "visualizer.rs"]
mod core;

pub use core::Message;
pub use core::Visualizer;
