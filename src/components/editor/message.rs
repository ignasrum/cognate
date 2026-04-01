use iced::widget::text_editor::Action;
use iced::window;
use std::collections::HashMap;

use crate::components::note_explorer;
use crate::components::visualizer;
use crate::notebook::{self, NoteMetadata, NotebookError};

#[derive(Debug, Clone)]
pub enum Message {
    // Text editing operations
    EditorAction(Action),
    PasteFromClipboard,
    LoadedNoteContent(String, String, HashMap<String, String>),
    HandleTabKey,
    SelectAll,
    Undo,
    Redo,

    // Note explorer interaction
    NoteExplorerMsg(note_explorer::Message),
    NoteSelected(String),

    // Label management
    NewLabelInputChanged(String),
    AddLabel,
    RemoveLabel(String),
    MetadataSaved(Result<(), NotebookError>),

    // Search
    SearchQueryChanged(String),
    RunSearch,
    SearchCompleted(u64, Vec<notebook::NoteSearchResult>),
    ClearSearch,

    // Content management
    NoteContentSaved(Result<(), NotebookError>),
    DebouncedMetadataSaveElapsed(u64),
    DebouncedMetadataSaveCompleted(u64, Result<(), NotebookError>),
    WindowCloseRequested(window::Id),
    ShutdownFlushCompleted(window::Id, Result<(), NotebookError>),

    // Visualizer
    ToggleVisualizer,
    VisualizerMsg(visualizer::Message),

    // Note operations
    NewNote,
    NewNoteInputChanged(String),
    CreateNote,
    NoteCreated(Result<NoteMetadata, NotebookError>),
    CancelNewNote,
    DeleteNote,
    ConfirmDeleteNote(bool),
    ConfirmDeleteEmbeddedImages(bool),
    NoteDeleted(Result<(), NotebookError>, String),
    MoveNote,
    MoveNoteInputChanged(String),
    ConfirmMoveNote,
    CancelMoveNote,
    NoteMoved(Result<String, NotebookError>, String),

    // Folder operations
    InitiateFolderRename(String),

    // UI interactions
    AboutButtonClicked,
    IncreaseScale,
    DecreaseScale,
    MarkdownLinkClicked(String),
    ScaleSaved(Result<(), String>),
}
