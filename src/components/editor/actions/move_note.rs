use iced::task::Task;
use native_dialog::{DialogBuilder, MessageLevel};

use crate::components::editor::Message;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::editor::text_management::undo_manager::UndoManager;
use crate::components::note_explorer;
use crate::components::note_explorer::NoteExplorer;
use crate::notebook::{self, NoteMetadata, NotebookError};

// Handle confirm move note
pub fn handle_confirm_move_note(
    state: &mut EditorState,
    current_notes: Vec<NoteMetadata>,
) -> Task<Message> {
    if state.show_move_note_input() {
        if let Some(current_path) = state.move_note_current_path().cloned() {
            let new_path = state.move_note_new_path_input().trim().to_string();
            state.hide_move_note_dialog();

            if new_path.is_empty() {
                #[cfg(debug_assertions)]
                eprintln!("New path cannot be empty for moving/renaming.");
                let dialog_command = Task::perform(
                    async move {
                        let _ = DialogBuilder::message()
                            .set_level(MessageLevel::Error)
                            .set_title("Error Moving/Renaming")
                            .set_text("New path cannot be empty.")
                            .alert()
                            .show();
                    },
                    |()| Message::NoteExplorerMsg(note_explorer::Message::LoadNotes),
                );
                return dialog_command;
            }

            if new_path == current_path {
                #[cfg(debug_assertions)]
                eprintln!(
                    "New path is the same as the current path. No action needed; preserving current selection."
                );
                return Task::none();
            }

            let notebook_path = state.notebook_path().to_string();
            let mut notes = current_notes;
            let old_path = current_path.clone();

            Task::perform(
                async move {
                    notebook::move_note(&notebook_path, &current_path, &new_path, &mut notes).await
                },
                move |result| Message::NoteMoved(result, old_path.clone()),
            )
        } else {
            #[cfg(debug_assertions)]
            eprintln!("ConfirmMoveNote called with no current item selected to move/rename.");
            state.hide_move_note_dialog();
            Task::none()
        }
    } else {
        Task::none()
    }
}

// Handle note moved
pub fn handle_note_moved(
    result: Result<String, NotebookError>,
    old_path: String,
    _state: &mut EditorState,
    undo_manager: &mut UndoManager,
    note_explorer: &mut NoteExplorer,
) -> Task<Message> {
    match result {
        Ok(new_rel_path) => {
            #[cfg(debug_assertions)]
            eprintln!("Item moved/renamed successfully to: {}", new_rel_path);

            // If we're moving a note that had an undo history, update the key
            undo_manager.handle_path_change(&old_path, &new_rel_path);

            note_explorer
                .update(note_explorer::Message::LoadNotes)
                .map(Message::NoteExplorerMsg)
        }
        Err(_err) => {
            #[cfg(debug_assertions)]
            eprintln!("Failed to move/rename item: {}", _err);

            // Clone _err to be used in the async move block
            let error_message = super::navigation::notebook_error_text(&_err);

            Task::perform(
                async move {
                    let _ = DialogBuilder::message()
                        .set_level(MessageLevel::Error)
                        .set_title("Error Moving/Renaming")
                        .set_text(&error_message)
                        .alert()
                        .show();
                },
                |_| Message::NoteExplorerMsg(note_explorer::Message::LoadNotes),
            )
        }
    }
}
