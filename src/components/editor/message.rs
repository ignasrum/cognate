use crate::components::editor::note_coordinator::LoadedNotePayload;
use crate::components::note_explorer;
use crate::components::visualizer;
use crate::notebook::{self, NoteMetadata, NotebookError};
use iced::widget::text_editor::Action;
use iced::window;

#[derive(Debug, Clone)]
pub struct LabelMutationRollback {
    pub note_path: String,
    pub note_labels: Vec<String>,
    pub selected_labels: Vec<String>,
    pub input_text: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    // Text editing operations
    EditorAction(Action),
    PasteFromClipboard,
    LoadedNoteContent(Result<LoadedNotePayload, NotebookError>),
    HandleTabKey,
    SelectAll,
    Undo,
    Redo,

    // Note explorer interaction
    NoteExplorerMsg(note_explorer::Message),
    NoteSelected(String),
    ConnectionChecked(Result<(), NotebookError>),

    // Label management
    NewLabelInputChanged(String),
    AddLabel,
    RemoveLabel(String),
    MetadataSaved(Result<(), NotebookError>, Option<LabelMutationRollback>),

    // Search
    SearchQueryChanged(String),
    SearchDebounced(u64),
    RunSearch,
    #[allow(dead_code)]
    SearchCompleted(u64, Vec<notebook::NoteSearchResult>),
    SearchPageCompleted(u64, bool, Result<notebook::NoteSearchPage, NotebookError>),
    LoadMoreSearchResults,
    ClearSearch,

    // Content management
    NoteContentSaved(Result<(), NotebookError>),
    OfflineReplayCompleted(Result<(), NotebookError>),
    ConflictKeepServer,
    ConflictRetryLocal,
    ConflictSaveCopy,
    ConflictDismiss,
    ConflictCopySaved(Result<String, NotebookError>),
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
    RetryConnection,
    AboutButtonClicked,
    IncreaseScale,
    DecreaseScale,
    MarkdownLinkClicked(String),
    ScaleSaved(Result<(), String>),

    // Async attachments
    AttachmentLoaded(String, Result<Vec<u8>, String>),
    PastedImageSaved(Result<String, String>),
    Dummy,
}
