use iced::event::Event;
use iced::keyboard::Key;
use iced::{Element, Subscription, Theme};
use iced::task::Task;

// Import required types and modules
use crate::configuration::Configuration;
use crate::notebook::NoteMetadata;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::editor::text_management::undo_manager::UndoManager;
use crate::components::editor::text_management::content_handler;
use crate::components::editor::text_management::undo_manager;
use crate::components::editor::actions::{label_actions, note_actions};
use crate::components::editor::ui::layout;

// Import re-exported components
use crate::components::note_explorer::NoteExplorer;
use crate::components::note_explorer;
use crate::components::visualizer::Visualizer;
use crate::components::visualizer;

#[path = "../../configuration/theme.rs"]
mod local_theme;

// Define the Message enum in this module
#[derive(Debug, Clone)]
pub enum Message {
    // Text editing operations
    EditorAction(iced::widget::text_editor::Action),
    ContentChanged(String),
    HandleTabKey,
    SelectAll,
    Undo,
    
    // Note explorer interaction
    NoteExplorerMessage(note_explorer::Message),
    NoteSelected(String),
    
    // Label management
    NewLabelInputChanged(String),
    AddLabel,
    RemoveLabel(String),
    MetadataSaved(Result<(), String>),
    
    // Content management
    NoteContentSaved(Result<(), String>),
    
    // Visualizer
    ToggleVisualizer,
    VisualizerMessage(visualizer::Message),
    
    // Note operations
    NewNote,
    NewNoteInputChanged(String),
    CreateNote,
    NoteCreated(Result<NoteMetadata, String>),
    CancelNewNote,
    DeleteNote,
    ConfirmDeleteNote(bool),
    NoteDeleted(Result<(), String>),
    MoveNote,
    MoveNoteInputChanged(String),
    ConfirmMoveNote,
    CancelMoveNote,
    NoteMoved(Result<String, String>),
    
    // Folder operations
    InitiateFolderRename(String),
    
    // UI interactions
    AboutButtonClicked,
}

// Define the Editor struct
pub struct Editor {
    // Core state management
    state: EditorState,
    
    // Text management
    content: iced::widget::text_editor::Content,
    markdown_text: String,
    
    // Undo/redo management
    undo_manager: UndoManager,
    
    // UI components and state
    #[allow(dead_code)] // Explicitly allow this field as it's used during initialization
    theme: Theme,
    note_explorer: NoteExplorer,
    visualizer: Visualizer,
}

// Implement static methods for Editor to work with iced::application
impl Editor {
    // Keep create method for internal use
    pub fn create(flags: Configuration) -> (Self, Task<Message>) {
        let notebook_path_clone = flags.notebook_path.clone();
        
        let mut editor_instance = Editor {
            content: iced::widget::text_editor::Content::with_text(""),
            markdown_text: String::new(),
            undo_manager: UndoManager::new(),
            state: EditorState::new(),
            theme: local_theme::convert_str_to_theme(flags.theme.clone()),
            note_explorer: note_explorer::NoteExplorer::new(notebook_path_clone.clone()),
            visualizer: visualizer::Visualizer::new(),
        };
        
        editor_instance.state.set_notebook_path(notebook_path_clone);
        editor_instance.state.set_app_version(flags.version);

        let initial_command = if !editor_instance.state.notebook_path().is_empty() {
            editor_instance
                .note_explorer
                .update(note_explorer::Message::LoadNotes)
                .map(Message::NoteExplorerMessage)
        } else {
            Task::none()
        };

        (editor_instance, initial_command)
    }

    // Update the update method to match the signature expected by iced::application
    pub fn update(state: &mut Self, message: Message) -> Task<Message> {
        match message {
            // Handle text editing operations
            Message::HandleTabKey => {
                return content_handler::handle_tab_key(
                    &mut state.content, 
                    &mut state.markdown_text, 
                    state.state.selected_note_path(),
                    state.state.notebook_path(), 
                    &state.state
                );
            },
            Message::SelectAll => {
                return content_handler::handle_select_all(
                    &mut state.content,
                    &state.state
                );
            },
            Message::Undo => {
                return undo_manager::handle_undo(
                    &mut state.undo_manager,
                    &mut state.content,
                    &mut state.markdown_text,
                    state.state.selected_note_path(),
                    state.state.notebook_path(),
                    &state.state
                );
            },
            Message::EditorAction(action) => {
                return content_handler::handle_editor_action(
                    &mut state.content,
                    &mut state.markdown_text,
                    &mut state.undo_manager,
                    action,
                    state.state.selected_note_path(),
                    state.state.notebook_path(),
                    &state.state
                );
            },
            Message::ContentChanged(new_content) => {
                return content_handler::handle_content_changed(
                    &mut state.content,
                    &mut state.markdown_text,
                    &mut state.undo_manager,
                    &mut state.state,
                    new_content
                );
            },

            // Handle note explorer interactions
            Message::NoteExplorerMessage(note_explorer_message) => {
                return note_actions::handle_note_explorer_message(
                    &mut state.note_explorer,
                    &mut state.visualizer,
                    &mut state.state,
                    &mut state.content,
                    &mut state.markdown_text,
                    note_explorer_message
                );
            },
            Message::NoteSelected(note_path) => {
                return note_actions::handle_note_selected(
                    &mut state.note_explorer,
                    &mut state.undo_manager,
                    &mut state.state,
                    &mut state.content,
                    &mut state.markdown_text,
                    note_path
                );
            },

            // Handle label management
            Message::NewLabelInputChanged(text) => {
                label_actions::handle_label_input_changed(&mut state.state, text);
                Task::none()
            },
            Message::AddLabel => {
                return label_actions::handle_add_label(
                    &mut state.state,
                    &mut state.note_explorer,
                    &mut state.visualizer
                );
            },
            Message::RemoveLabel(label) => {
                return label_actions::handle_remove_label(
                    &mut state.state,
                    &mut state.note_explorer,
                    &mut state.visualizer,
                    label
                );
            },
            Message::MetadataSaved(result) => {
                if let Err(_err) = result {
                    #[cfg(debug_assertions)]
                    eprintln!("Error saving metadata: {}", _err);
                } else {
                    #[cfg(debug_assertions)]
                    eprintln!("Metadata saved successfully.");
                }
                Task::none()
            },

            // Handle content management
            Message::NoteContentSaved(result) => {
                if let Err(_err) = result {
                    #[cfg(debug_assertions)]
                    eprintln!("Error saving note content: {}", _err);
                } else {
                    #[cfg(debug_assertions)]
                    eprintln!("Note content saved successfully.");
                }
                Task::none()
            },

            // Handle visualizer
            Message::ToggleVisualizer => {
                state.state.toggle_visualizer();
                
                if state.state.show_visualizer() && !state.state.notebook_path().is_empty() {
                    let _ = state.visualizer.update(visualizer::Message::UpdateNotes(
                        state.note_explorer.notes.clone(),
                    ));
                }
                
                Task::none()
            },
            Message::VisualizerMessage(visualizer_message) => {
                return note_actions::handle_visualizer_message(
                    &mut state.visualizer,
                    &mut state.note_explorer,
                    &mut state.state,
                    &mut state.content,
                    &mut state.markdown_text,
                    &mut state.undo_manager,
                    visualizer_message
                );
            },

            // Handle note operations
            Message::NewNote => {
                state.state.show_new_note_dialog();
                Task::none()
            },
            Message::NewNoteInputChanged(text) => {
                state.state.update_new_note_path(text);
                Task::none()
            },
            Message::CreateNote => {
                return note_actions::handle_create_note(&mut state.state, state.note_explorer.notes.clone());
            },
            Message::CancelNewNote => {
                state.state.hide_new_note_dialog();
                Task::none()
            },
            Message::NoteCreated(result) => {
                return note_actions::handle_note_created(result, &mut state.note_explorer);
            },
            Message::DeleteNote => {
                return note_actions::handle_delete_note(&mut state.state);
            },
            Message::ConfirmDeleteNote(confirmed) => {
                return note_actions::handle_confirm_delete_note(
                    confirmed,
                    &mut state.state,
                    state.note_explorer.notes.clone()
                );
            },
            Message::NoteDeleted(result) => {
                return note_actions::handle_note_deleted(
                    result,
                    &mut state.state,
                    &mut state.content,
                    &mut state.markdown_text,
                    &mut state.undo_manager,
                    &mut state.note_explorer
                );
            },
            Message::MoveNote => {
                if let Some(current_path) = state.state.selected_note_path() {
                    state.state.show_move_note_dialog(current_path.clone());
                }
                Task::none()
            },
            Message::MoveNoteInputChanged(text) => {
                state.state.update_move_note_path(text);
                Task::none()
            },
            Message::ConfirmMoveNote => {
                return note_actions::handle_confirm_move_note(
                    &mut state.state,
                    state.note_explorer.notes.clone()
                );
            },
            Message::CancelMoveNote => {
                state.state.hide_move_note_dialog();
                let command = note_actions::get_select_note_command(
                    state.state.selected_note_path(),
                    &state.note_explorer.notes
                );
                command
            },
            Message::NoteMoved(result) => {
                return note_actions::handle_note_moved(
                    result,
                    &mut state.state,
                    &mut state.undo_manager,
                    &mut state.note_explorer
                );
            },

            // Handle folder operations
            Message::InitiateFolderRename(folder_path) => {
                state.state.show_rename_folder_dialog(folder_path);
                Task::none()
            },

            // Handle UI interactions
            Message::AboutButtonClicked => {
                state.state.toggle_about_info();
                Task::none()
            },
        }
    }

    // Keep view method as is, but fix the state reference
    pub fn view(state: &Self) -> Element<Message> {
        layout::generate_layout(
            &state.state,
            &state.content,
            &state.note_explorer,
            &state.visualizer,
        )
    }

    // Keep subscription method as is
    pub fn subscription(_state: &Self) -> Subscription<Message> {
        iced::event::listen_with(|event, _status, _shell| {
            match event {
                Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    // Handle Ctrl+A for Select All
                    if modifiers.control() {
                        if let Key::Character(c) = &key {
                            if c == "a" || c == "A" {
                                return Some(Message::SelectAll);
                            }
                            if c == "z" || c == "Z" {
                                return Some(Message::Undo);
                            }
                        }
                    }

                    // Handle Tab key press (no modifiers)
                    if key == Key::Named(iced::keyboard::key::Named::Tab) && modifiers.is_empty() {
                        return Some(Message::HandleTabKey);
                    }

                    None
                }
                _ => None,
            }
        })
    }
}

// Keep Default impl for Editor
impl Default for Editor {
    fn default() -> Self {
        Self {
            content: iced::widget::text_editor::Content::with_text(""),
            markdown_text: String::new(),
            undo_manager: UndoManager::new(),
            state: EditorState::new(),
            theme: Theme::Dark, // Default theme
            note_explorer: note_explorer::NoteExplorer::new(String::new()),
            visualizer: visualizer::Visualizer::new(),
        }
    }
}
