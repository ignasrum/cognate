// Make submodules public
pub mod state;
pub mod text_management;
pub mod ui;
pub mod actions;

#[path = "editor.rs"]
mod core;

// Re-export the main types directly from their definition here
pub use core::Editor;
pub use core::Message;
