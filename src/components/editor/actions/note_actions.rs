use iced::Command;
use native_dialog::MessageDialog;
use iced::widget::text_editor::Content;

// Use root-level imports that avoid circular references
use crate::components::editor::Message;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::editor::text_management::undo_manager::UndoManager;
use crate::components::note_explorer::NoteExplorer;
use crate::components::note_explorer;
use crate::components::visualizer::Visualizer;
use crate::components::visualizer;
use crate::notebook::{self, NoteMetadata};

// Handle note explorer messages
pub fn handle_note_explorer_message(
    note_explorer: &mut NoteExplorer,
    visualizer: &mut Visualizer,
    state: &mut EditorState,
    content: &mut Content,
    markdown_text: &mut String,
    note_explorer_message: note_explorer::Message,
) -> Command<Message> {
    #[cfg(debug_assertions)]
    eprintln!(
        "Editor: Received NoteExplorerMessage: {:?}",
        note_explorer_message
    );
    
    let note_explorer_command = note_explorer
        .update(note_explorer_message.clone())
        .map(|msg| Message::NoteExplorerMessage(msg));

    let mut editor_command = Command::none();
    
    if let note_explorer::Message::NotesLoaded(_loaded_notes) = note_explorer_message {
        #[cfg(debug_assertions)]
        eprintln!(
            "Editor: NoteExplorer finished loading {} notes. Updating editor state.",
            _loaded_notes.len()
        );
        
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
                eprintln!(
                    "Editor: Selected note no longer exists. Clearing editor state."
                );
                
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
            editor_command = Command::perform(async { first_note_path }, Message::NoteSelected);
        }
    }

    Command::batch(vec![note_explorer_command, editor_command])
}

// Handle note selection
pub fn handle_note_selected(
    note_explorer: &mut NoteExplorer,
    undo_manager: &mut UndoManager,
    state: &mut EditorState,
    _content: &mut Content,    // Added underscore
    _markdown_text: &mut String,    // Added underscore
    note_path: String,
) -> Command<Message> {
    #[cfg(debug_assertions)]
    eprintln!(
        "Editor: NoteSelected message received for path: {}",
        note_path
    );
    
    state.set_selected_note_path(Some(note_path.clone()));
    state.clear_new_label_text();
    state.hide_move_note_dialog();
    
    // Don't directly access private fields
    state.set_show_about_info(false);
    state.set_show_new_note_input(false);
    
    // Initialize history for this note
    undo_manager.initialize_history(&note_path);

    if let Some(note) = note_explorer
        .notes
        .iter()
        .find(|n| n.rel_path == note_path)
    {
        state.set_selected_note_labels(note.labels.clone());
    } else {
        state.set_selected_note_labels(Vec::new());
    }

    let mut commands = Vec::new();

    // Send the message to collapse all and then expand to the selected note
    commands.push(
        note_explorer
            .update(note_explorer::Message::CollapseAllAndExpandToNote(
                note_path.clone(),
            ))
            .map(|msg| Message::NoteExplorerMessage(msg)),
    );

    if !state.show_visualizer() && !state.notebook_path().is_empty() {
        // Set loading flag before requesting content for the new note
        state.set_loading_note(true);
        
        #[cfg(debug_assertions)]
        eprintln!(
            "Setting loading_note flag to true for note '{}'",
            note_path
        );
        
        let notebook_path = state.notebook_path().to_string();
        let note_path_clone = note_path;

        commands.push(Command::perform(
            async move {
                let full_note_path = format!("{}/{}/note.md", notebook_path, note_path_clone);
                match std::fs::read_to_string(full_note_path) {
                    Ok(content) => content,
                    Err(_err) => {
                        #[cfg(debug_assertions)]
                        eprintln!("Failed to read note file for editor: {}", _err);
                        String::new()
                    }
                }
            },
            Message::ContentChanged,
        ));
    }

    Command::batch(commands)
}

// Handle visualizer messages
pub fn handle_visualizer_message(
    visualizer: &mut Visualizer,
    note_explorer: &mut NoteExplorer,
    state: &mut EditorState,
    _content: &mut Content,    // Added underscore
    _markdown_text: &mut String,    // Added underscore
    undo_manager: &mut UndoManager,
    visualizer_message: visualizer::Message,
) -> Command<Message> {
    let mut commands_to_return: Vec<Command<Message>> = Vec::new();

    // Update visualizer state and map the command
    commands_to_return.push(
        visualizer
            .update(visualizer_message.clone())
            .map(|msg| Message::VisualizerMessage(msg)),
    );

    match visualizer_message {
        visualizer::Message::UpdateNotes(_) => {
            // No additional editor commands needed when visualizer just updates notes
        }
        visualizer::Message::ToggleLabel(_) => {
            // No additional editor commands needed when a label is toggled in the visualizer
        }
        visualizer::Message::NoteSelectedInVisualizer(note_path) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "Editor: Received NoteSelectedInVisualizer for path: {}",
                note_path
            );
            
            // Trigger the logic to select the note in the editor
            state.set_selected_note_path(Some(note_path.clone()));
            state.clear_new_label_text();
            state.hide_move_note_dialog();
            state.set_show_visualizer(false); // Use setter instead of direct field access
            state.set_show_new_note_input(false);
            state.set_show_about_info(false);

            // Initialize history for this note
            undo_manager.initialize_history(&note_path);

            if let Some(note) = note_explorer
                .notes
                .iter()
                .find(|n| n.rel_path == note_path)
            {
                state.set_selected_note_labels(note.labels.clone());
            } else {
                state.set_selected_note_labels(Vec::new());
            }

            // Commands to update the note explorer and load content
            commands_to_return.push(
                note_explorer
                    .update(note_explorer::Message::CollapseAllAndExpandToNote(
                        note_path.clone(),
                    ))
                    .map(|msg| Message::NoteExplorerMessage(msg)),
            );

            if !state.show_visualizer() && !state.notebook_path().is_empty() {
                // Set loading flag before requesting content for the new note
                state.set_loading_note(true);
                
                #[cfg(debug_assertions)]
                eprintln!(
                    "Setting loading_note flag to true for note '{}' from visualizer",
                    note_path
                );
                
                let notebook_path_clone = state.notebook_path().to_string();
                let note_path_clone = note_path.clone();

                commands_to_return.push(Command::perform(
                    async move {
                        let full_note_path = format!(
                            "{}/{}/note.md",
                            notebook_path_clone, note_path_clone
                        );
                        match std::fs::read_to_string(full_note_path) {
                            Ok(content) => content,
                            Err(_err) => {
                                #[cfg(debug_assertions)]
                                eprintln!(
                                    "Failed to read note file for editor: {}",
                                    _err
                                );
                                String::new()
                            }
                        }
                    },
                    Message::ContentChanged,
                ));
            }
        }
    }
    
    // Batch all collected commands
    Command::batch(commands_to_return)
}

// Get command to select a note
pub fn get_select_note_command(
    selected_note_path: Option<&String>,
    notes: &[NoteMetadata],
) -> Command<Message> {
    if let Some(selected_path) = selected_note_path.cloned() {
        Command::perform(async { selected_path }, Message::NoteSelected)
    } else {
        let first_note_path = notes.get(0).map(|n| n.rel_path.clone());
        if let Some(path) = first_note_path {
            Command::perform(async { path }, Message::NoteSelected)
        } else {
            Command::none()
        }
    }
}

// Handle create note
pub fn handle_create_note(
    state: &mut EditorState,
    current_notes: Vec<NoteMetadata>,
) -> Command<Message> {
    if state.show_new_note_input() {
        let new_note_rel_path = state.new_note_path_input().trim().to_string();
        if new_note_rel_path.is_empty() {
            #[cfg(debug_assertions)]
            eprintln!("New note name cannot be empty.");
            Command::none()
        } else {
            state.hide_new_note_dialog();
            let notebook_path = state.notebook_path().to_string();
            let mut notes = current_notes;

            Command::perform(
                async move {
                    notebook::create_new_note(
                        &notebook_path,
                        &new_note_rel_path,
                        &mut notes,
                    )
                    .await
                },
                Message::NoteCreated,
            )
        }
    } else {
        Command::none()
    }
}

// Handle note created
pub fn handle_note_created(
    result: Result<NoteMetadata, String>,
    note_explorer: &mut NoteExplorer,
) -> Command<Message> {
    match result {
        Ok(new_note_metadata) => {
            #[cfg(debug_assertions)]
            eprintln!("Note created successfully: {}", new_note_metadata.rel_path);
            let reload_command = note_explorer
                .update(note_explorer::Message::LoadNotes)
                .map(|msg| Message::NoteExplorerMessage(msg));

            let select_command = Command::perform(
                async { new_note_metadata.rel_path },
                Message::NoteSelected,
            );

            Command::batch(vec![reload_command, select_command])
        }
        Err(_err) => {
            #[cfg(debug_assertions)]
            eprintln!("Failed to create note: {}", _err);
            // Clone _err to be used in the async move block
            let error_message = _err.clone();
            let dialog_command = Command::perform(
                async move {
                    // _err is moved here
                    let _ = MessageDialog::new()
                        .set_type(native_dialog::MessageType::Error)
                        .set_title("Error Creating Note")
                        .set_text(&error_message) // Use the cloned variable
                        .show_alert();
                },
                |()| Message::NoteExplorerMessage(note_explorer::Message::LoadNotes),
            );
            dialog_command
        }
    }
}

// Handle delete note
pub fn handle_delete_note(state: &mut EditorState) -> Command<Message> {
    if let Some(selected_path) = state.selected_note_path() {
        if !state.show_about_info() {
            let note_path_clone = selected_path.clone();
            state.hide_new_note_dialog();
            state.hide_move_note_dialog();
            state.set_show_visualizer(false);
            state.set_show_about_info(false);

            Command::perform(
                async move {
                    MessageDialog::new()
                        .set_type(native_dialog::MessageType::Warning)
                        .set_title("Confirm Deletion")
                        .set_text(&format!(
                            "Are you sure you want to delete the note '{}'?",
                            note_path_clone
                        ))
                        .show_confirm()
                        .unwrap_or(false)
                },
                Message::ConfirmDeleteNote,
            )
        } else {
            Command::none()
        }
    } else {
        #[cfg(debug_assertions)]
        eprintln!("No note selected to delete.");
        Command::none()
    }
}

// Handle confirm delete note
pub fn handle_confirm_delete_note(
    confirmed: bool,
    state: &mut EditorState,
    current_notes: Vec<NoteMetadata>,
) -> Command<Message> {
    if confirmed {
        if let Some(selected_path) = state.take_selected_note_path() {
            let notebook_path = state.notebook_path().to_string();
            let mut notes = current_notes;

            Command::perform(
                async move {
                    notebook::delete_note(
                        &notebook_path,
                        &selected_path,
                        &mut notes,
                    )
                    .await
                },
                Message::NoteDeleted,
            )
        } else {
            #[cfg(debug_assertions)]
            eprintln!("ConfirmDeleteNote called with no selected note.");
            Command::none()
        }
    } else {
        #[cfg(debug_assertions)]
        eprintln!("Note deletion cancelled by user.");
        Command::none()
    }
}

// Handle note deleted
pub fn handle_note_deleted(
    result: Result<(), String>,
    state: &mut EditorState,
    content: &mut Content,
    markdown_text: &mut String,
    undo_manager: &mut UndoManager,
    note_explorer: &mut NoteExplorer,
) -> Command<Message> {
    match result {
        Ok(()) => {
            #[cfg(debug_assertions)]
            eprintln!("Note deleted successfully.");
            
            // Clean up history for the deleted note
            if let Some(path) = state.selected_note_path() {
                undo_manager.remove_history(path);
            }
            
            state.set_selected_note_path(None);
            state.set_selected_note_labels(Vec::new());
            *content = Content::with_text("");
            *markdown_text = String::new();
            state.hide_move_note_dialog();

            note_explorer
                .update(note_explorer::Message::LoadNotes)
                .map(|msg| Message::NoteExplorerMessage(msg))
        }
        Err(_err) => {
            #[cfg(debug_assertions)]
            eprintln!("Failed to delete note: {}", _err);
            // Clone _err to be used in the async move block
            let error_message = _err.clone();
            let error_message_clone = error_message.clone();
            let dialog_command = Command::perform(
                async move {
                    let _ = MessageDialog::new()
                        .set_type(native_dialog::MessageType::Error)
                        .set_title("Error Deleting Note")
                        .set_text(&error_message)
                        .show_alert();
                },
                move |()| Message::NoteDeleted(Err(error_message_clone)),
            );
            let reload_command = note_explorer
                .update(note_explorer::Message::LoadNotes)
                .map(|msg| Message::NoteExplorerMessage(msg));
            Command::batch(vec![dialog_command, reload_command])
        }
    }
}

// Handle confirm move note
pub fn handle_confirm_move_note(
    state: &mut EditorState,
    current_notes: Vec<NoteMetadata>,
) -> Command<Message> {
    if state.show_move_note_input() {
        if let Some(current_path) = state.take_move_note_current_path() {
            let new_path = state.move_note_new_path_input().trim().to_string();
            state.hide_move_note_dialog();

            if new_path.is_empty() {
                #[cfg(debug_assertions)]
                eprintln!("New path cannot be empty for moving/renaming.");
                let dialog_command = Command::perform(
                    async move {
                        let _ = MessageDialog::new()
                            .set_type(native_dialog::MessageType::Error)
                            .set_title("Error Moving/Renaming")
                            .set_text("New path cannot be empty.")
                            .show_alert();
                    },
                    |()| Message::NoteMoved(Err(String::new())),
                );
                return dialog_command;
            }

            if new_path == current_path {
                #[cfg(debug_assertions)]
                eprintln!(
                    "New path is the same as the current path. No action needed."
                );
                return get_select_note_command(None, &current_notes);
            }

            let notebook_path = state.notebook_path().to_string();
            let mut notes = current_notes;

            Command::perform(
                async move {
                    notebook::move_note(
                        &notebook_path,
                        &current_path,
                        &new_path,
                        &mut notes,
                    )
                    .await
                },
                Message::NoteMoved,
            )
        } else {
            #[cfg(debug_assertions)]
            eprintln!(
                "ConfirmMoveNote called with no current item selected to move/rename."
            );
            state.hide_move_note_dialog();
            Command::none()
        }
    } else {
        Command::none()
    }
}

// Handle note moved
pub fn handle_note_moved(
    result: Result<String, String>,
    state: &mut EditorState,
    undo_manager: &mut UndoManager,
    note_explorer: &mut NoteExplorer,
) -> Command<Message> {
    match result {
        Ok(new_rel_path) => {
            #[cfg(debug_assertions)]
            eprintln!("Item moved/renamed successfully to: {}", new_rel_path);
            
            // If we're moving a note that had an undo history, update the key
            if let Some(old_path) = state.move_note_current_path() {
                undo_manager.handle_path_change(old_path, &new_rel_path);
            }
            
            note_explorer
                .update(note_explorer::Message::LoadNotes)
                .map(|msg| Message::NoteExplorerMessage(msg))
        }
        Err(_err) => {
            #[cfg(debug_assertions)]
            eprintln!("Failed to move/rename item: {}", _err);
            // Clone _err to be used in the async move block
            let error_message = _err.clone();
            let error_message_clone = error_message.clone();
            let dialog_command = Command::perform(
                async move {
                    let _ = MessageDialog::new()
                        .set_type(native_dialog::MessageType::Error)
                        .set_title("Error Moving/Renaming")
                        .set_text(&error_message)
                        .show_alert();
                },
                move |()| Message::NoteMoved(Err(error_message_clone)),
            );
            let reload_command = note_explorer
                .update(note_explorer::Message::LoadNotes)
                .map(|msg| Message::NoteExplorerMessage(msg));
            Command::batch(vec![dialog_command, reload_command])
        }
    }
}
