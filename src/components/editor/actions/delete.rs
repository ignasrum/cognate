use iced::task::Task;
use iced::widget::text_editor::Content;
use native_dialog::{DialogBuilder, MessageLevel};

use crate::components::editor::Message;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::editor::text_management::undo_manager::UndoManager;
use crate::components::note_explorer;
use crate::components::note_explorer::NoteExplorer;
use crate::notebook::{self, NoteMetadata, NotebookError};

// Handle delete note
pub fn handle_delete_note(state: &mut EditorState) -> Task<Message> {
    if let Some(selected_path) = state.selected_note_path() {
        if !state.show_about_info() {
            let note_path_clone = selected_path.clone();
            state.hide_new_note_dialog();
            state.hide_move_note_dialog();
            state.set_show_visualizer(false);
            state.set_show_about_info(false);

            Task::perform(
                async move {
                    DialogBuilder::message()
                        .set_level(MessageLevel::Warning)
                        .set_title("Confirm Deletion")
                        .set_text(format!(
                            "Are you sure you want to delete the note '{}'?",
                            note_path_clone
                        ))
                        .confirm()
                        .show()
                        .unwrap_or(false)
                },
                Message::ConfirmDeleteNote,
            )
        } else {
            Task::none()
        }
    } else {
        #[cfg(debug_assertions)]
        eprintln!("No note selected to delete.");
        Task::none()
    }
}

// Handle confirm delete note
pub fn handle_confirm_delete_note(
    confirmed: bool,
    state: &mut EditorState,
    current_notes: Vec<NoteMetadata>,
) -> Task<Message> {
    if confirmed {
        if let Some(selected_path) = state.selected_note_path().cloned() {
            let notebook_path = state.notebook_path().to_string();
            let mut notes = current_notes;
            let deleted_path = selected_path.clone();

            Task::perform(
                async move { notebook::delete_note(&notebook_path, &selected_path, &mut notes).await },
                move |result| Message::NoteDeleted(result, deleted_path.clone()),
            )
        } else {
            #[cfg(debug_assertions)]
            eprintln!("ConfirmDeleteNote called with no selected note.");
            Task::none()
        }
    } else {
        #[cfg(debug_assertions)]
        eprintln!("Note deletion cancelled by user.");
        Task::none()
    }
}

// Handle note deleted
pub fn handle_note_deleted(
    result: Result<(), NotebookError>,
    deleted_path: String,
    state: &mut EditorState,
    content: &mut Content,
    markdown_text: &mut String,
    undo_manager: &mut UndoManager,
    note_explorer: &mut NoteExplorer,
) -> Task<Message> {
    match result {
        Ok(()) => {
            #[cfg(debug_assertions)]
            eprintln!("Note deleted successfully.");

            // Clean up history for the deleted note
            undo_manager.remove_history(&deleted_path);

            super::navigation::clear_selected_note_change(state);
            *content = Content::with_text("");
            *markdown_text = String::new();

            note_explorer
                .update(note_explorer::Message::LoadNotes)
                .map(Message::NoteExplorerMsg)
        }
        Err(_err) => {
            #[cfg(debug_assertions)]
            eprintln!("Failed to delete note: {}", _err);
            // Clone _err to be used in the async move block
            let error_message = super::navigation::notebook_error_text(&_err);

            Task::perform(
                async move {
                    let _ = DialogBuilder::message()
                        .set_level(MessageLevel::Error)
                        .set_title("Error Deleting Note")
                        .set_text(&error_message)
                        .alert()
                        .show();
                },
                |_| Message::NoteExplorerMsg(note_explorer::Message::LoadNotes),
            )
        }
    }
}
