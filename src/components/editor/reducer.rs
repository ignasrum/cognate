use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MessageDomain {
    Text,
    Selection,
    Label,
    Search,
    DebouncedMetadata,
    Shutdown,
    SaveFeedback,
    Visualizer,
    NoteLifecycle,
    Ui,
}

fn message_domain(message: &Message) -> MessageDomain {
    match message {
        Message::HandleTabKey
        | Message::SelectAll
        | Message::Undo
        | Message::Redo
        | Message::PasteFromClipboard
        | Message::EditorAction(_)
        | Message::LoadedNoteContent(_, _, _) => MessageDomain::Text,

        Message::NoteExplorerMsg(_) | Message::NoteSelected(_) => MessageDomain::Selection,

        Message::NewLabelInputChanged(_) | Message::AddLabel | Message::RemoveLabel(_) => {
            MessageDomain::Label
        }

        Message::SearchQueryChanged(_)
        | Message::RunSearch
        | Message::SearchCompleted(_)
        | Message::ClearSearch => MessageDomain::Search,

        Message::DebouncedMetadataSaveElapsed(_)
        | Message::DebouncedMetadataSaveCompleted(_, _) => MessageDomain::DebouncedMetadata,

        Message::WindowCloseRequested(_) | Message::ShutdownFlushCompleted(_, _) => {
            MessageDomain::Shutdown
        }

        Message::MetadataSaved(_) | Message::NoteContentSaved(_) | Message::ScaleSaved(_) => {
            MessageDomain::SaveFeedback
        }

        Message::ToggleVisualizer | Message::VisualizerMsg(_) => MessageDomain::Visualizer,

        Message::NewNote
        | Message::NewNoteInputChanged(_)
        | Message::CreateNote
        | Message::CancelNewNote
        | Message::NoteCreated(_)
        | Message::DeleteNote
        | Message::ConfirmDeleteNote(_)
        | Message::ConfirmDeleteEmbeddedImages(_)
        | Message::NoteDeleted(_, _)
        | Message::MoveNote
        | Message::MoveNoteInputChanged(_)
        | Message::ConfirmMoveNote
        | Message::CancelMoveNote
        | Message::NoteMoved(_, _) => MessageDomain::NoteLifecycle,

        Message::InitiateFolderRename(_)
        | Message::AboutButtonClicked
        | Message::IncreaseScale
        | Message::DecreaseScale
        | Message::MarkdownLinkClicked(_) => MessageDomain::Ui,
    }
}

pub(super) fn route_message(state: &mut Editor, message: Message) -> Task<Message> {
    match message_domain(&message) {
        MessageDomain::Text => Editor::handle_text_messages(state, message),
        MessageDomain::Selection => Editor::handle_selection_messages(state, message),
        MessageDomain::Label => Editor::handle_label_messages(state, message),
        MessageDomain::Search => Editor::handle_search_messages(state, message),
        MessageDomain::DebouncedMetadata => {
            Editor::handle_debounced_metadata_messages(state, message)
        }
        MessageDomain::Shutdown => Editor::handle_shutdown_messages(state, message),
        MessageDomain::SaveFeedback => Editor::handle_save_feedback_messages(message),
        MessageDomain::Visualizer => Editor::handle_visualizer_messages(state, message),
        MessageDomain::NoteLifecycle => Editor::handle_note_lifecycle_messages(state, message),
        MessageDomain::Ui => Editor::handle_ui_messages(state, message),
    }
}
