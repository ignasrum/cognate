use iced::task::Task;
use iced::widget::text_editor::Content;
#[cfg(not(test))]
use native_dialog::{DialogBuilder, MessageLevel};

use crate::components::editor::Message;
use crate::components::editor::note_coordinator;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::editor::text_management::undo_manager::UndoManager;
use crate::components::note_explorer;
use crate::components::note_explorer::NoteExplorer;
use crate::components::visualizer;
use crate::components::visualizer::Visualizer;
use crate::notebook::{NoteMetadata, NotebookError};

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

pub(crate) fn notebook_error_text(error: &NotebookError) -> String {
    error.ui_message()
}

fn sync_selected_note_labels(
    state: &mut EditorState,
    note_explorer: &NoteExplorer,
    selected_note_path: Option<&str>,
) {
    let labels = selected_note_path
        .and_then(|path| {
            note_explorer
                .notes
                .iter()
                .find(|note| note.rel_path == path)
        })
        .map(|note| note.labels.clone())
        .unwrap_or_default();
    state.set_selected_note_labels(labels);
}

fn apply_selected_note_change(
    note_explorer: &NoteExplorer,
    undo_manager: &mut UndoManager,
    state: &mut EditorState,
    note_path: &str,
    hide_visualizer: bool,
) {
    state.set_selected_note_path(Some(note_path.to_string()));
    state.clear_new_label_text();
    state.hide_move_note_dialog();
    if hide_visualizer {
        state.set_show_visualizer(false);
    }
    state.set_show_new_note_input(false);
    state.set_show_about_info(false);
    undo_manager.initialize_history(note_path);
    sync_selected_note_labels(state, note_explorer, Some(note_path));
}

pub(crate) fn clear_selected_note_change(state: &mut EditorState) {
    state.set_selected_note_path(None);
    state.set_selected_note_labels(Vec::new());
    state.hide_move_note_dialog();
    state.clear_new_label_text();
}

fn handle_note_selection_internal(
    note_explorer: &mut NoteExplorer,
    undo_manager: &mut UndoManager,
    state: &mut EditorState,
    note_path: String,
    hide_visualizer: bool,
) -> Task<Message> {
    apply_selected_note_change(
        note_explorer,
        undo_manager,
        state,
        &note_path,
        hide_visualizer,
    );
    state.clear_note_load_error();

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
            Message::LoadedNoteContent,
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

    let notes_loaded_feedback = match &note_explorer_message {
        note_explorer::Message::NotesLoaded(load_result) => Some(match load_result {
            Ok(load_result) => Ok(load_result.warning.clone()),
            Err(load_error) => Err(notebook_error_text(load_error)),
        }),
        _ => None,
    };

    let note_explorer_command = note_explorer
        .update(note_explorer_message)
        .map(Message::NoteExplorerMsg);

    let mut editor_command = Task::none();

    if let Some(load_feedback) = notes_loaded_feedback {
        let connection_was_lost = state.connection_error().is_some();
        match load_feedback {
            Ok(load_warning) => {
                state.clear_connection_error();
                #[cfg(debug_assertions)]
                eprintln!(
                    "Editor: NoteExplorer finished loading {} notes. Updating editor state.",
                    note_explorer.notes.len()
                );

                if let Some(load_warning) = load_warning {
                    report_metadata_load_issue("Notebook Metadata Recovered", &load_warning);
                }
            }
            Err(load_error) => {
                let error_detail = format!(
                    "Cognate could not read notebook metadata safely:\n\n{}",
                    load_error
                );
                eprintln!("[cognate] could not connect to server: {load_error}");
                state.set_connection_error(error_detail);
            }
        }

        // Update the visualizer with the new notes data
        visualizer.sync_notes(&note_explorer.notes);

        if let Some(selected_path) = state.selected_note_path().cloned() {
            if !note_explorer
                .notes
                .iter()
                .any(|n| n.rel_path == selected_path)
            {
                #[cfg(debug_assertions)]
                eprintln!("Editor: Selected note no longer exists. Clearing editor state.");

                clear_selected_note_change(state);
                *content = Content::with_text("");
                *markdown_text = String::new();
            } else {
                sync_selected_note_labels(state, note_explorer, Some(selected_path.as_str()));
                if connection_was_lost {
                    editor_command =
                        Task::perform(async move { selected_path }, Message::NoteSelected);
                }
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
    let selection_change = match &visualizer_message {
        visualizer::Message::FocusOnNote(note_path) => {
            note_path.as_ref().map(|path| (path.clone(), false))
        }
        visualizer::Message::NoteSelectedInVisualizer(note_path) => Some((note_path.clone(), true)),
    };

    // Update visualizer state and map the command
    commands_to_return.push(
        visualizer
            .update(visualizer_message)
            .map(Message::VisualizerMsg),
    );

    if let Some((note_path, hide_visualizer)) = selection_change {
        if hide_visualizer {
            #[cfg(debug_assertions)]
            eprintln!(
                "Editor: Received NoteSelectedInVisualizer for path: {}",
                note_path
            );
        }

        commands_to_return.push(handle_note_selection_internal(
            note_explorer,
            undo_manager,
            state,
            note_path,
            hide_visualizer,
        ));
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
