// Re-export everything publicly from the editor module
pub mod editor;
pub use editor::Editor;
pub use editor::Message;

// Make these modules public so they can be referenced from editor.rs
pub mod state;
pub mod text_management;
pub mod ui;
pub mod actions;
