//! Editor component module.
//!
//! Exposes editor state/actions/ui modules and re-exports the `Editor`
//! application type with its top-level message enum.

pub mod actions;
pub mod state;
pub mod text_management;
pub mod ui;

#[path = "editor.rs"]
mod core;

pub use core::Editor;
pub use core::Message;
