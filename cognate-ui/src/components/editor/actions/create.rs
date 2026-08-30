use iced::task::Task;
use native_dialog::{DialogBuilder, MessageLevel};

use crate::components::editor::Message;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::note_explorer;
use crate::components::note_explorer::NoteExplorer;
use crate::notebook::{self, NoteMetadata, NotebookError};

// Handle create note
pub fn handle_create_note(
    state: &mut EditorState,
    current_notes: Vec<NoteMetadata>,
) -> Task<Message> {
    if state.show_new_note_input() {
        let new_note_rel_path = state.new_note_path_input().trim().to_string();
        if new_note_rel_path.is_empty() {
            #[cfg(debug_assertions)]
            eprintln!("New note name cannot be empty.");
            Task::none()
        } else {
            state.hide_new_note_dialog();
            let notebook_path = state.notebook_path().to_string();
            let mut notes = current_notes;

            Task::perform(
                async move {
                    notebook::create_new_note(&notebook_path, &new_note_rel_path, &mut notes).await
                },
                Message::NoteCreated,
            )
        }
    } else {
        Task::none()
    }
}

// Handle note created
pub fn handle_note_created(
    result: Result<NoteMetadata, NotebookError>,
    note_explorer: &mut NoteExplorer,
) -> Task<Message> {
    match result {
        Ok(new_note_metadata) => {
            #[cfg(debug_assertions)]
            eprintln!("Note created successfully: {}", new_note_metadata.rel_path);
            let reload_command = note_explorer
                .update(note_explorer::Message::LoadNotes)
                .map(Message::NoteExplorerMsg);

            let select_command =
                Task::perform(async { new_note_metadata.rel_path }, Message::NoteSelected);

            Task::batch(vec![reload_command, select_command])
        }
        Err(_err) => {
            #[cfg(debug_assertions)]
            eprintln!("Failed to create note: {}", _err);
            // Clone _err to be used in the async move block
            let error_message = super::navigation::notebook_error_text(&_err);
            Task::perform(
                async move {
                    let _ = DialogBuilder::message()
                        .set_level(MessageLevel::Error)
                        .set_title("Error Creating Note")
                        .set_text(&error_message) // Use the cloned variable
                        .alert()
                        .show();
                },
                |()| Message::NoteExplorerMsg(note_explorer::Message::LoadNotes),
            )
        }
    }
}
