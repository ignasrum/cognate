use iced::task::Task; // Use Task instead of Command
use iced::widget::text_editor::Content;
use native_dialog::{DialogBuilder, MessageLevel};

// Use root-level imports that avoid circular references
use crate::components::editor::Message;
use crate::components::editor::note_coordinator;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::editor::text_management::undo_manager::UndoManager;
use crate::components::note_explorer;
use crate::components::note_explorer::NoteExplorer;
use crate::components::visualizer;
use crate::components::visualizer::Visualizer;
use crate::notebook::{self, NoteMetadata};

fn report_metadata_load_issue(title: &str, detail: &str) {
    eprintln!("{}: {}", title, detail);

    #[cfg(not(test))]
    {
        let _ = DialogBuilder::message()
            .set_level(MessageLevel::Warning)
            .set_title(title)
            .set_text(detail)
            .alert()
            .show();
    }
}

fn handle_note_selection_internal(
    note_explorer: &mut NoteExplorer,
    undo_manager: &mut UndoManager,
    state: &mut EditorState,
    note_path: String,
    hide_visualizer: bool,
) -> Task<Message> {
    state.set_selected_note_path(Some(note_path.clone()));
    state.clear_new_label_text();
    state.hide_move_note_dialog();
    if hide_visualizer {
        state.set_show_visualizer(false);
    }
    state.set_show_new_note_input(false);
    state.set_show_about_info(false);

    undo_manager.initialize_history(&note_path);

    if let Some(note) = note_explorer.notes.iter().find(|n| n.rel_path == note_path) {
        state.set_selected_note_labels(note.labels.clone());
    } else {
        state.set_selected_note_labels(Vec::new());
    }

    let mut commands = vec![
        note_explorer
            .update(note_explorer::Message::CollapseAllAndExpandToNote(
                note_path.clone(),
            ))
            .map(Message::NoteExplorerMsg),
    ];

    if !state.show_visualizer() && !state.notebook_path().is_empty() {
        state.set_loading_note(true);

        #[cfg(debug_assertions)]
        eprintln!("Setting loading_note flag to true for note '{}'", note_path);

        let notebook_path = state.notebook_path().to_string();
        let selected_note_path = note_path;

        commands.push(Task::perform(
            async move { note_coordinator::load_note_payload(notebook_path, selected_note_path).await },
            |payload| Message::LoadedNoteContent(payload.note_path, payload.content, payload.images),
        ));
    }

    Task::batch(commands)
}

// Handle note explorer messages
pub fn handle_note_explorer_message(
    note_explorer: &mut NoteExplorer,
    visualizer: &mut Visualizer,
    state: &mut EditorState,
    content: &mut Content,
    markdown_text: &mut String,
    note_explorer_message: note_explorer::Message,
) -> Task<Message> {
    #[cfg(debug_assertions)]
    eprintln!(
        "Editor: Received NoteExplorerMsg: {:?}",
        note_explorer_message
    );

    let note_explorer_command = note_explorer
        .update(note_explorer_message.clone())
        .map(Message::NoteExplorerMsg);

    let mut editor_command = Task::none();

    if let note_explorer::Message::NotesLoaded(load_result) = note_explorer_message {
        match load_result {
            Ok(load_result) => {
                #[cfg(debug_assertions)]
                eprintln!(
                    "Editor: NoteExplorer finished loading {} notes. Updating editor state.",
                    load_result.notes.len()
                );

                if let Some(load_warning) = load_result.warning {
                    report_metadata_load_issue("Notebook Metadata Recovered", &load_warning);
                }
            }
            Err(load_error) => {
                report_metadata_load_issue(
                    "Failed to Load Notebook Metadata",
                    &format!(
                        "Cognate could not read notebook metadata safely:\n\n{}",
                        load_error
                    ),
                );
            }
        }

        // Update the visualizer with the new notes data
        let _ = visualizer.update(visualizer::Message::UpdateNotes(
            note_explorer.notes.clone(),
        ));

        if let Some(selected_path) = state.selected_note_path() {
            if !note_explorer
                .notes
                .iter()
                .any(|n| &n.rel_path == selected_path)
            {
                #[cfg(debug_assertions)]
                eprintln!("Editor: Selected note no longer exists. Clearing editor state.");

                state.set_selected_note_path(None);
                state.set_selected_note_labels(Vec::new());
                *content = Content::with_text("");
                *markdown_text = String::new();
                state.hide_move_note_dialog();
            } else if let Some(note) = note_explorer
                .notes
                .iter()
                .find(|n| &n.rel_path == selected_path)
            {
                state.set_selected_note_labels(note.labels.clone());
            }
        } else if !note_explorer.notes.is_empty() {
            let first_note_path = note_explorer.notes[0].rel_path.clone();
            #[cfg(debug_assertions)]
            eprintln!(
                "Editor: No note selected, selecting first note: {}",
                first_note_path
            );
            editor_command = Task::perform(async { first_note_path }, Message::NoteSelected);
        }
    }

    Task::batch(vec![note_explorer_command, editor_command])
}

// Handle note selection
pub fn handle_note_selected(
    note_explorer: &mut NoteExplorer,
    undo_manager: &mut UndoManager,
    state: &mut EditorState,
    note_path: String,
) -> Task<Message> {
    #[cfg(debug_assertions)]
    eprintln!(
        "Editor: NoteSelected message received for path: {}",
        note_path
    );

    handle_note_selection_internal(note_explorer, undo_manager, state, note_path, false)
}

// Handle visualizer messages
pub fn handle_visualizer_message(
    visualizer: &mut Visualizer,
    note_explorer: &mut NoteExplorer,
    state: &mut EditorState,
    undo_manager: &mut UndoManager,
    visualizer_message: visualizer::Message,
) -> Task<Message> {
    let mut commands_to_return: Vec<Task<Message>> = Vec::new();

    // Update visualizer state and map the command
    commands_to_return.push(
        visualizer
            .update(visualizer_message.clone())
            .map(Message::VisualizerMsg),
    );

    match visualizer_message {
        visualizer::Message::UpdateNotes(_) => {
            // No additional editor commands needed when visualizer just updates notes
        }
        visualizer::Message::FocusOnNote(note_path) => {
            if let Some(note_path) = note_path {
                commands_to_return.push(handle_note_selection_internal(
                    note_explorer,
                    undo_manager,
                    state,
                    note_path,
                    false,
                ));
            }
        }
        visualizer::Message::NoteSelectedInVisualizer(note_path) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "Editor: Received NoteSelectedInVisualizer for path: {}",
                note_path
            );

            commands_to_return.push(handle_note_selection_internal(
                note_explorer,
                undo_manager,
                state,
                note_path,
                true,
            ));
        }
    }

    // Batch all collected commands
    Task::batch(commands_to_return)
}

// Get command to select a note
pub fn get_select_note_command(
    selected_note_path: Option<&String>,
    notes: &[NoteMetadata],
) -> Task<Message> {
    if let Some(selected_path) = selected_note_path.cloned() {
        Task::perform(async { selected_path }, Message::NoteSelected)
    } else {
        let first_note_path = notes.first().map(|n| n.rel_path.clone());
        if let Some(path) = first_note_path {
            Task::perform(async { path }, Message::NoteSelected)
        } else {
            Task::none()
        }
    }
}

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
    result: Result<NoteMetadata, String>,
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
            let error_message = _err.clone();
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
    result: Result<(), String>,
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

            state.set_selected_note_path(None);
            state.set_selected_note_labels(Vec::new());
            *content = Content::with_text("");
            *markdown_text = String::new();
            state.hide_move_note_dialog();

            note_explorer
                .update(note_explorer::Message::LoadNotes)
                .map(Message::NoteExplorerMsg)
        }
        Err(_err) => {
            #[cfg(debug_assertions)]
            eprintln!("Failed to delete note: {}", _err);
            // Clone _err to be used in the async move block
            let error_message = _err.clone();

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
    result: Result<String, String>,
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
            let error_message = _err.clone();

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
