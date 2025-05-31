use iced::event::Event;
use iced::keyboard::Key;
use iced::{Application, Command, Element, Subscription, Theme};

use crate::configuration::Configuration;
use crate::notebook::NoteMetadata;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::editor::text_management::undo_manager::UndoManager;
use crate::components::editor::text_management::content_handler;
use crate::components::editor::text_management::undo_manager;
use crate::components::editor::actions::{label_actions, note_actions};
use crate::components::editor::ui::layout;

// These are now imported through the components module
use crate::components::note_explorer;
use crate::components::visualizer;

#[path = "../../configuration/theme.rs"]
mod local_theme;

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

pub struct Editor {
    // Core state management
    state: EditorState,
    
    // Text management
    content: iced::widget::text_editor::Content,
    markdown_text: String,
    
    // Undo/redo management
    undo_manager: UndoManager,
    
    // UI components and state
    theme: Theme,
    note_explorer: note_explorer::NoteExplorer,
    visualizer: visualizer::Visualizer,
}

impl Application for Editor {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = Configuration;

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        let notebook_path_clone = flags.notebook_path.clone();
        
        // Make editor_instance mutable so we can modify its fields
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
            #[cfg(debug_assertions)]
            eprintln!(
                "Editor: Initializing with notebook: {}",
                editor_instance.state.notebook_path()
            );
            editor_instance
                .note_explorer
                .update(note_explorer::Message::LoadNotes)
                .map(Message::NoteExplorerMessage)
        } else {
            #[cfg(debug_assertions)]
            eprintln!("Editor: No notebook path provided in config. Starting without a notebook.");
            Command::none()
        };

        (editor_instance, initial_command)
    }

    fn title(&self) -> String {
        String::from("Cognate")
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            // Handle text editing operations
            Message::HandleTabKey => {
                return content_handler::handle_tab_key(
                    &mut self.content, 
                    &mut self.markdown_text, 
                    self.state.selected_note_path(),
                    self.state.notebook_path(), 
                    &self.state
                );
            },
            Message::SelectAll => {
                return content_handler::handle_select_all(
                    &mut self.content,
                    &self.state
                );
            },
            Message::Undo => {
                return undo_manager::handle_undo(
                    &mut self.undo_manager,
                    &mut self.content,
                    &mut self.markdown_text,
                    self.state.selected_note_path(),
                    self.state.notebook_path(),
                    &self.state
                );
            },
            Message::EditorAction(action) => {
                return content_handler::handle_editor_action(
                    &mut self.content,
                    &mut self.markdown_text,
                    &mut self.undo_manager,
                    action,
                    self.state.selected_note_path(),
                    self.state.notebook_path(),
                    &self.state
                );
            },
            Message::ContentChanged(new_content) => {
                return content_handler::handle_content_changed(
                    &mut self.content,
                    &mut self.markdown_text,
                    &mut self.undo_manager,
                    &mut self.state,
                    new_content
                );
            },

            // Handle note explorer interactions
            Message::NoteExplorerMessage(note_explorer_message) => {
                return note_actions::handle_note_explorer_message(
                    &mut self.note_explorer,
                    &mut self.visualizer,
                    &mut self.state,
                    &mut self.content,
                    &mut self.markdown_text,
                    note_explorer_message
                );
            },
            Message::NoteSelected(note_path) => {
                return note_actions::handle_note_selected(
                    &mut self.note_explorer,
                    &mut self.undo_manager,
                    &mut self.state,
                    &mut self.content,
                    &mut self.markdown_text,
                    note_path
                );
            },

            // Handle label management
            Message::NewLabelInputChanged(text) => {
                label_actions::handle_label_input_changed(&mut self.state, text);
                Command::none()
            },
            Message::AddLabel => {
                return label_actions::handle_add_label(
                    &mut self.state,
                    &mut self.note_explorer,
                    &mut self.visualizer
                );
            },
            Message::RemoveLabel(label) => {
                return label_actions::handle_remove_label(
                    &mut self.state,
                    &mut self.note_explorer,
                    &mut self.visualizer,
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
                Command::none()
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
                Command::none()
            },

            // Handle visualizer
            Message::ToggleVisualizer => {
                self.state.toggle_visualizer();
                
                if self.state.show_visualizer() && !self.state.notebook_path().is_empty() {
                    let _ = self.visualizer.update(visualizer::Message::UpdateNotes(
                        self.note_explorer.notes.clone(),
                    ));
                }
                
                Command::none()
            },
            Message::VisualizerMessage(visualizer_message) => {
                return note_actions::handle_visualizer_message(
                    &mut self.visualizer,
                    &mut self.note_explorer,
                    &mut self.state,
                    &mut self.content,
                    &mut self.markdown_text,
                    &mut self.undo_manager,
                    visualizer_message
                );
            },

            // Handle note operations
            Message::NewNote => {
                self.state.show_new_note_dialog();
                Command::none()
            },
            Message::NewNoteInputChanged(text) => {
                self.state.update_new_note_path(text);
                Command::none()
            },
            Message::CreateNote => {
                return note_actions::handle_create_note(&mut self.state, self.note_explorer.notes.clone());
            },
            Message::CancelNewNote => {
                self.state.hide_new_note_dialog();
                Command::none()
            },
            Message::NoteCreated(result) => {
                return note_actions::handle_note_created(result, &mut self.note_explorer);
            },
            Message::DeleteNote => {
                return note_actions::handle_delete_note(&mut self.state);
            },
            Message::ConfirmDeleteNote(confirmed) => {
                return note_actions::handle_confirm_delete_note(
                    confirmed,
                    &mut self.state,
                    self.note_explorer.notes.clone()
                );
            },
            Message::NoteDeleted(result) => {
                return note_actions::handle_note_deleted(
                    result,
                    &mut self.state,
                    &mut self.content,
                    &mut self.markdown_text,
                    &mut self.undo_manager,
                    &mut self.note_explorer
                );
            },
            Message::MoveNote => {
                if let Some(current_path) = self.state.selected_note_path() {
                    self.state.show_move_note_dialog(current_path.clone());
                }
                Command::none()
            },
            Message::MoveNoteInputChanged(text) => {
                self.state.update_move_note_path(text);
                Command::none()
            },
            Message::ConfirmMoveNote => {
                return note_actions::handle_confirm_move_note(
                    &mut self.state,
                    self.note_explorer.notes.clone()
                );
            },
            Message::CancelMoveNote => {
                self.state.hide_move_note_dialog();
                let command = note_actions::get_select_note_command(
                    self.state.selected_note_path(),
                    &self.note_explorer.notes
                );
                command
            },
            Message::NoteMoved(result) => {
                return note_actions::handle_note_moved(
                    result,
                    &mut self.state,
                    &mut self.undo_manager,
                    &mut self.note_explorer
                );
            },

            // Handle folder operations
            Message::InitiateFolderRename(folder_path) => {
                self.state.show_rename_folder_dialog(folder_path);
                Command::none()
            },

            // Handle UI interactions
            Message::AboutButtonClicked => {
                self.state.toggle_about_info();
                Command::none()
            },
        }
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        iced::event::listen_with(|event, _status| {
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

    fn view(&self) -> Element<'_, Self::Message, Self::Theme> {
        layout::generate_layout(
            &self.state,
            &self.content,
            &self.note_explorer,
            &self.visualizer,
        )
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }
}
