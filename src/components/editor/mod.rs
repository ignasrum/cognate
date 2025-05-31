// Make submodules public
pub mod state;
pub mod text_management;
pub mod ui;
pub mod actions;

// Import the actual editor module
pub mod editor;

// Re-export the main types directly from their definition here
pub use editor::Editor;
pub use editor::Message;
