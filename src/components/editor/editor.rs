use iced::event::Event;
use iced::keyboard::Key;
use iced::widget::text_editor::{Action, Content, Edit, Motion};
use iced::widget::{
    Column, Container, Row, Text, TextInput as IcedTextInput, button, text_editor, text_input,
};
use iced::{Application, Command, Element, Length, Subscription, Theme};
use native_dialog::MessageDialog;
use std::collections::HashSet;
use std::path::Path;

use crate::configuration::Configuration;
use crate::notebook::{self, NoteMetadata};
#[path = "../../configuration/theme.rs"]
mod local_theme;
#[path = "../note_explorer/note_explorer.rs"]
mod note_explorer;
#[path = "../visualizer/visualizer.rs"]
mod visualizer;

#[derive(Debug, Clone)]
pub enum Message {
    EditorAction(text_editor::Action),
    ContentChanged(String),
    NoteExplorerMessage(note_explorer::Message),
    NoteSelected(String),
    NewLabelInputChanged(String),
    AddLabel,
    RemoveLabel(String),
    MetadataSaved(Result<(), String>),
    NoteContentSaved(Result<(), String>),
    ToggleVisualizer,
    VisualizerMessage(visualizer::Message),
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
    InitiateFolderRename(String),
    AboutButtonClicked,
    HandleTabKey,
    SelectAll, // New message for selecting all text
    Undo,      // New message for undo
}

pub struct Editor {
    content: Content,
    theme: Theme,
    markdown_text: String,
    undo_histories: std::collections::HashMap<String, Vec<String>>,    // Store previous states for undo per note
    undo_indices: std::collections::HashMap<String, usize>, // Track position in undo history per note
    loading_note: bool, // Flag to indicate we're loading a new note (not editing)
    note_explorer: note_explorer::NoteExplorer,
    visualizer: visualizer::Visualizer,
    show_visualizer: bool,
    notebook_path: String,
    selected_note_path: Option<String>,
    selected_note_labels: Vec<String>,
    new_label_text: String,
    show_new_note_input: bool,
    new_note_path_input: String,
    show_move_note_input: bool,
    move_note_current_path: Option<String>,
    move_note_new_path_input: String,
    app_version: String,

    show_about_info: bool,
}

impl Application for Editor {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = Configuration;

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        let initial_text = String::new();
        let notebook_path_clone = flags.notebook_path.clone();
        let mut editor_instance = Editor {
            content: Content::with_text(&initial_text),
            theme: local_theme::convert_str_to_theme(flags.theme.clone()),
            notebook_path: notebook_path_clone.clone(),
            markdown_text: String::new(),
            undo_histories: std::collections::HashMap::new(),
            undo_indices: std::collections::HashMap::new(),
            loading_note: false,
            note_explorer: note_explorer::NoteExplorer::new(notebook_path_clone),
            visualizer: visualizer::Visualizer::new(),
            show_visualizer: false,
            selected_note_path: None,
            selected_note_labels: Vec::new(),
            new_label_text: String::new(),
            show_new_note_input: false,
            new_note_path_input: String::new(),
            show_move_note_input: false,
            move_note_current_path: None,
            move_note_new_path_input: String::new(),
            app_version: flags.version,
            show_about_info: false,
        };

        let initial_command = if !editor_instance.notebook_path.is_empty() {
            #[cfg(debug_assertions)]
            eprintln!(
                "Editor: Initializing with notebook: {}",
                editor_instance.notebook_path
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
        String::from("Cognate - Note Taking App")
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::HandleTabKey => {
                if self.selected_note_path.is_some()
                    && !self.show_visualizer
                    && !self.show_move_note_input
                    && !self.show_new_note_input
                    && !self.show_about_info
                {
                    #[cfg(debug_assertions)]
                    eprintln!("Editor: Handling HandleTabKey message by inserting 4 spaces.");

                    self.content.perform(Action::Edit(Edit::Insert(' ')));
                    self.content.perform(Action::Edit(Edit::Insert(' ')));
                    self.content.perform(Action::Edit(Edit::Insert(' ')));
                    self.content.perform(Action::Edit(Edit::Insert(' ')));

                    self.markdown_text = self.content.text();
                    if let Some(selected_path) = &self.selected_note_path {
                        let notebook_path = self.notebook_path.clone();
                        let note_path = selected_path.clone();
                        let content = self.markdown_text.clone();
                        #[cfg(debug_assertions)]
                        eprintln!(
                            "Editor: Handling Tab: Saving content for note: {}",
                            note_path
                        );
                        return Command::perform(
                            async move {
                                notebook::save_note_content(notebook_path, note_path, content).await
                            },
                            Message::NoteContentSaved,
                        );
                    }
                }
                Command::none()
            }
            Message::SelectAll => {
                if self.selected_note_path.is_some()
                    && !self.show_visualizer
                    && !self.show_move_note_input
                    && !self.show_new_note_input
                    && !self.show_about_info
                {
                    #[cfg(debug_assertions)]
                    eprintln!("Editor: Handling SelectAll message.");
                    // Perform the SelectAll action
                    // First move cursor to start, then select to end
                    self.content.perform(Action::Move(Motion::DocumentStart));
                    self.content.perform(Action::Select(Motion::DocumentEnd));

                    // No save needed after selecting all, as content hasn't changed.
                }
                Command::none()
            }
            Message::Undo => {
                if let Some(note_path) = &self.selected_note_path {
                    if !self.show_visualizer
                        && !self.show_move_note_input
                        && !self.show_new_note_input
                        && !self.show_about_info
                    {
                        // Get this note's current history index, if it exists
                        let current_index = self.undo_indices.get(note_path).copied().unwrap_or(0);
                        
                        if current_index > 0 {
                            // Get this note's history, if it exists
                            if let Some(history) = self.undo_histories.get(note_path) {
                                if !history.is_empty() {
                                    #[cfg(debug_assertions)]
                                    eprintln!("Editor: Handling Undo message for note '{}'. History index: {}/{}", 
                                        note_path, current_index, history.len());

                                    // Calculate the previous state index
                                    let new_index = current_index - 1;
                                    let previous_content = history[new_index].clone();
                                    
                                    // Update the index for this note
                                    self.undo_indices.insert(note_path.clone(), new_index);

                                    // Update content with the previous state
                                    self.content = Content::with_text(&previous_content);
                                    self.markdown_text = previous_content;
                                
                                    // Save the content after undo
                                    #[cfg(debug_assertions)]
                                    eprintln!(
                                        "Editor: Performing undo to previous state for note: {}",
                                        note_path
                                    );

                                    let notebook_path = self.notebook_path.clone();
                                    let selected_path = note_path.clone();
                                    let content = self.markdown_text.clone();

                                    return Command::perform(
                                        async move {
                                            notebook::save_note_content(notebook_path, selected_path, content).await
                                        },
                                        Message::NoteContentSaved,
                                    );
                                } else {
                                    #[cfg(debug_assertions)]
                                    eprintln!("Editor: Cannot undo - history for note '{}' is empty", note_path);
                                }
                            } else {
                                #[cfg(debug_assertions)]
                                eprintln!("Editor: Cannot undo - no history found for note '{}'", note_path);
                            }
                        } else {
                            #[cfg(debug_assertions)]
                            eprintln!("Editor: Cannot undo - index is 0, cannot undo further");
                        }
                    } else {
                        #[cfg(debug_assertions)]
                        eprintln!("Editor: Cannot undo - note is in a state that doesn't allow undo");
                    }
                } else {
                    #[cfg(debug_assertions)]
                    eprintln!("Editor: Cannot undo - no note selected");
                }
                
                Command::none()
            }
            Message::EditorAction(action) => {
                if self.selected_note_path.is_some()
                    && !self.show_visualizer
                    && !self.show_move_note_input
                    && !self.show_new_note_input
                    && !self.show_about_info
                {
                    // Save the current state to history before performing the action
                    // Only save if this is a modifying action (Edit)
                    if matches!(action, Action::Edit(_)) && self.selected_note_path.is_some() {
                        let note_path = self.selected_note_path.as_ref().unwrap().clone();
                        
                        // Get the current index for this note
                        let current_index = self.undo_indices.get(&note_path).copied().unwrap_or(0);
                        
                        // Get the history for this note or create a new one
                        let history = self.undo_histories
                            .entry(note_path.clone())
                            .or_insert_with(Vec::new);
                            
                        // Remove any future redo states if we're in the middle of the history
                        if current_index < history.len() {
                            history.truncate(current_index);
                        }
                        
                        // Add current state to history
                        history.push(self.markdown_text.clone());
                        let new_index = history.len();
                        
                        // Update the index for this note
                        self.undo_indices.insert(note_path.clone(), new_index);
                        
                        #[cfg(debug_assertions)]
                        eprintln!("Added state to undo history for note '{}' before Edit action. History size: {} Index: {}", 
                            note_path, history.len(), new_index);
                    }
                    
                    #[cfg(debug_assertions)]
                    eprintln!("Editor: Performing other EditorAction: {:?}", action);
                    self.content.perform(action);

                    self.markdown_text = self.content.text();

                    if let Some(selected_path) = &self.selected_note_path {
                        let notebook_path = self.notebook_path.clone();
                        let note_path = selected_path.clone();
                        let content = self.markdown_text.clone();
                        #[cfg(debug_assertions)]
                        eprintln!(
                            "Editor: Performing other EditorAction: Saving content for note: {}",
                            note_path
                        );
                        return Command::perform(
                            async move {
                                notebook::save_note_content(notebook_path, note_path, content).await
                            },
                            Message::NoteContentSaved,
                        );
                    }
                }
                Command::none()
            }
            Message::ContentChanged(new_content) => {
                if !self.show_visualizer
                    && !self.show_move_note_input
                    && !self.show_new_note_input
                    && !self.show_about_info
                {
                    if let Some(note_path) = &self.selected_note_path {
                        // Check if we're loading a note (switching between notes)
                        if self.loading_note {
                            // Get the current history for this note, if it exists
                            let history_exists = self.undo_histories.contains_key(note_path);
                            let history = self.undo_histories
                                .entry(note_path.clone())
                                .or_insert_with(Vec::new);
                            
                            #[cfg(debug_assertions)]
                            eprintln!(
                                "Loading note '{}'. History exists: {}, Size: {}",
                                note_path, history_exists, history.len()
                            );
                            
                            // Only initialize history if it doesn't exist or is empty
                            if history.is_empty() {
                                #[cfg(debug_assertions)]
                                eprintln!(
                                    "Initializing history for note '{}' as it's empty or new",
                                    note_path
                                );
                                
                                // Add the initial content as the first history entry
                                if !new_content.is_empty() {
                                    // Add initial content to history
                                    history.push(new_content.clone());
                                    self.undo_indices.insert(note_path.clone(), 1);
                                    
                                    #[cfg(debug_assertions)]
                                    eprintln!(
                                        "Initialized history for note '{}' with first entry. History size: 1, Index: 1",
                                        note_path
                                    );
                                } else {
                                    self.undo_indices.insert(note_path.clone(), 0);
                                    
                                    #[cfg(debug_assertions)]
                                    eprintln!(
                                        "Initialized empty history for note '{}'",
                                        note_path
                                    );
                                }
                            } else {
                                // Note already has history - use existing history and verify current content
                                let current_index = self.undo_indices.get(note_path).copied().unwrap_or(0);
                                
                                // Verify that the loaded content matches what's in the history
                                // This handles potential external file changes
                                if current_index > 0 && current_index <= history.len() && 
                                   history[current_index - 1] != new_content {
                                    #[cfg(debug_assertions)]
                                    eprintln!(
                                        "Content for note '{}' changed externally, adding to history",
                                        note_path
                                    );
                                    
                                    // Content has changed, add it to history
                                    history.push(new_content.clone());
                                    self.undo_indices.insert(note_path.clone(), history.len());
                                }
                                
                                #[cfg(debug_assertions)]
                                eprintln!(
                                    "Preserving existing history for note '{}'. History size: {}, Index: {}",
                                    note_path, history.len(), self.undo_indices.get(note_path).unwrap_or(&0)
                                );
                            }
                            
                            // Reset the loading flag
                            self.loading_note = false;
                            
                        } else if !self.markdown_text.is_empty() && self.markdown_text != new_content {
                            // This is a regular content change, not a note switch
                            // Get the current index for this note
                            let current_index = self.undo_indices.get(note_path).copied().unwrap_or(0);
                            
                            // Get the history for this note, or create a new one
                            let history = self.undo_histories
                                .entry(note_path.clone())
                                .or_insert_with(Vec::new);
                                
                            // Remove any future redo states if we're in the middle of the history
                            if current_index < history.len() {
                                history.truncate(current_index);
                            }

                            // Add current state to history before updating
                            history.push(self.markdown_text.clone());
                            let new_index = history.len();
                            
                            // Update the index for this note
                            self.undo_indices.insert(note_path.clone(), new_index);

                            #[cfg(debug_assertions)]
                            eprintln!(
                                "Added state to undo history for note '{}'. History size: {} Index: {}",
                                note_path, history.len(), new_index
                            );
                        }
                    }

                    self.content = Content::with_text(&new_content);
                    self.markdown_text = new_content;
                }
                Command::none()
            }
            Message::NoteExplorerMessage(note_explorer_message) => {
                #[cfg(debug_assertions)]
                eprintln!(
                    "Editor: Received NoteExplorerMessage: {:?}",
                    note_explorer_message
                );
                let note_explorer_command = self
                    .note_explorer
                    .update(note_explorer_message.clone())
                    .map(Message::NoteExplorerMessage);

                let mut editor_command = Command::none();
                if let note_explorer::Message::NotesLoaded(_loaded_notes) = note_explorer_message {
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "Editor: NoteExplorer finished loading {} notes. Updating editor state.",
                        _loaded_notes.len()
                    );
                    // Update the visualizer with the new notes data
                    let _ = self.visualizer.update(visualizer::Message::UpdateNotes(
                        self.note_explorer.notes.clone(),
                    ));

                    if let Some(selected_path) = &self.selected_note_path {
                        if !self
                            .note_explorer
                            .notes
                            .iter()
                            .any(|n| &n.rel_path == selected_path)
                        {
                            #[cfg(debug_assertions)]
                            eprintln!(
                                "Editor: Selected note no longer exists. Clearing editor state."
                            );
                            self.selected_note_path = None;
                            self.selected_note_labels = Vec::new();
                            self.content = Content::with_text("");
                            self.markdown_text = String::new();
                            self.show_move_note_input = false;
                            self.move_note_current_path = None;
                            self.move_note_new_path_input = String::new();
                        } else if let Some(note) = self
                            .note_explorer
                            .notes
                            .iter()
                            .find(|n| &n.rel_path == selected_path)
                        {
                            self.selected_note_labels = note.labels.clone();
                        }
                    } else if !self.note_explorer.notes.is_empty() {
                        let first_note_path = self.note_explorer.notes[0].rel_path.clone();
                        #[cfg(debug_assertions)]
                        eprintln!(
                            "Editor: No note selected, selecting first note: {}",
                            first_note_path
                        );
                        editor_command =
                            Command::perform(async { first_note_path }, Message::NoteSelected);
                    }
                }

                Command::batch(vec![note_explorer_command, editor_command])
            }
            Message::NoteSelected(note_path) => {
                #[cfg(debug_assertions)]
                eprintln!(
                    "Editor: NoteSelected message received for path: {}",
                    note_path
                );
                self.selected_note_path = Some(note_path.clone());
                self.new_label_text = String::new();
                self.show_move_note_input = false;
                self.move_note_current_path = None;
                self.move_note_new_path_input = String::new();
                self.show_about_info = false;
                self.show_new_note_input = false;
                
                // Initialize history containers for this note if they don't exist yet
                if !self.undo_histories.contains_key(&note_path) {
                    // If this note has no history yet, create a new empty history
                    self.undo_histories.insert(note_path.clone(), Vec::new());
                }
                
                // Initialize or retrieve the history index for this note
                let history_index = self.undo_histories.get(&note_path)
                    .map_or(0, |history| history.len());
                    
                // Update the index for this note
                self.undo_indices.insert(note_path.clone(), history_index);
                
                #[cfg(debug_assertions)]
                eprintln!(
                    "Editor: Setting history index for note '{}' to {}", 
                    note_path, history_index
                );

                if let Some(note) = self
                    .note_explorer
                    .notes
                    .iter()
                    .find(|n| n.rel_path == note_path)
                {
                    self.selected_note_labels = note.labels.clone();
                } else {
                    self.selected_note_labels = Vec::new();
                }

                let mut commands = Vec::new();

                // Send the message to collapse all and then expand to the selected note
                commands.push(
                    self.note_explorer
                        .update(note_explorer::Message::CollapseAllAndExpandToNote(
                            note_path.clone(),
                        ))
                        .map(Message::NoteExplorerMessage),
                );

                if !self.show_visualizer && !self.notebook_path.is_empty() {
                    // Set loading flag before requesting content for the new note
                    self.loading_note = true;
                    
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "Setting loading_note flag to true for note '{}'",
                        note_path
                    );
                    
                    let notebook_path_clone = self.notebook_path.clone();
                    let note_path_clone = note_path.clone();

                    commands.push(Command::perform(
                        async move {
                            let full_note_path =
                                format!("{}/{}/note.md", notebook_path_clone, note_path_clone);
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
            Message::NewLabelInputChanged(text) => {
                if !self.show_about_info {
                    self.new_label_text = text;
                }
                Command::none()
            }
            Message::AddLabel => {
                if !self.show_about_info {
                    if let Some(selected_path) = &self.selected_note_path {
                        let label = self.new_label_text.trim().to_string();
                        if !label.is_empty() && !self.selected_note_labels.contains(&label) {
                            self.selected_note_labels.push(label.clone());

                            if let Some(note) = self
                                .note_explorer
                                .notes
                                .iter_mut()
                                .find(|n| n.rel_path == *selected_path)
                            {
                                note.labels.push(label);
                            }

                            let _ = self.visualizer.update(visualizer::Message::UpdateNotes(
                                self.note_explorer.notes.clone(),
                            ));

                            self.new_label_text = String::new();

                            let notebook_path = self.notebook_path.clone();
                            let notes_to_save = self.note_explorer.notes.clone();
                            return Command::perform(
                                async move {
                                    notebook::save_metadata(&notebook_path, &notes_to_save[..])
                                        .map_err(|e| e.to_string())
                                },
                                Message::MetadataSaved,
                            );
                        }
                    }
                }
                Command::none()
            }
            Message::RemoveLabel(label_to_remove) => {
                if let Some(selected_path) = &self.selected_note_path {
                    if !self.show_about_info {
                        self.selected_note_labels
                            .retain(|label| label != &label_to_remove);

                        if let Some(note) = self
                            .note_explorer
                            .notes
                            .iter_mut()
                            .find(|n| n.rel_path == *selected_path)
                        {
                            note.labels.retain(|label| label != &label_to_remove);
                        }

                        let _ = self.visualizer.update(visualizer::Message::UpdateNotes(
                            self.note_explorer.notes.clone(),
                        ));

                        let notebook_path = self.notebook_path.clone();
                        let notes_to_save = self.note_explorer.notes.clone();
                        return Command::perform(
                            async move {
                                notebook::save_metadata(&notebook_path, &notes_to_save[..])
                                    .map_err(|e| e.to_string())
                            },
                            Message::MetadataSaved,
                        );
                    }
                }
                Command::none()
            }
            Message::MetadataSaved(result) => {
                if let Err(_err) = result {
                    #[cfg(debug_assertions)]
                    eprintln!("Error saving metadata: {}", _err);
                } else {
                    #[cfg(debug_assertions)]
                    eprintln!("Metadata saved successfully.");
                }
                Command::none()
            }
            Message::NoteContentSaved(result) => {
                if let Err(_err) = result {
                    #[cfg(debug_assertions)]
                    eprintln!("Error saving note content: {}", _err);
                } else {
                    #[cfg(debug_assertions)]
                    eprintln!("Note content saved successfully.");
                }
                Command::none()
            }
            Message::ToggleVisualizer => {
                if !self.notebook_path.is_empty() {
                    self.show_visualizer = !self.show_visualizer;
                    if self.show_visualizer {
                        self.show_new_note_input = false;
                        self.show_move_note_input = false;
                        self.show_about_info = false;
                    }
                    #[cfg(debug_assertions)]
                    eprintln!("Toggled visualizer visibility to: {}", self.show_visualizer);
                    if self.show_visualizer {
                        let _ = self.visualizer.update(visualizer::Message::UpdateNotes(
                            self.note_explorer.notes.clone(),
                        ));
                    }
                } else {
                    #[cfg(debug_assertions)]
                    eprintln!("Cannot show visualizer: No notebook is open.");
                }
                Command::none()
            }
            Message::VisualizerMessage(visualizer_message) => {
                let mut commands_to_return: Vec<Command<Self::Message>> = Vec::new();

                // Update the visualizer state and get the command it might return
                // Map the command from the visualizer to an editor message
                commands_to_return.push(
                    self.visualizer
                        .update(visualizer_message.clone())
                        .map(Message::VisualizerMessage),
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
                        self.show_visualizer = false; // Hide visualizer
                        self.show_new_note_input = false;
                        self.show_move_note_input = false;
                        self.show_about_info = false;

                        self.selected_note_path = Some(note_path.clone());
                        self.new_label_text = String::new();
                        self.move_note_current_path = None;
                        self.move_note_new_path_input = String::new();

                        if let Some(note) = self
                            .note_explorer
                            .notes
                            .iter()
                            .find(|n| n.rel_path == note_path)
                        {
                            self.selected_note_labels = note.labels.clone();
                        } else {
                            self.selected_note_labels = Vec::new();
                        }

                        // Commands to update the note explorer and load content
                        commands_to_return.push(
                            self.note_explorer
                                .update(note_explorer::Message::CollapseAllAndExpandToNote(
                                    note_path.clone(),
                                ))
                                .map(Message::NoteExplorerMessage),
                        );

                        if !self.show_visualizer && !self.notebook_path.is_empty() {
                            // Set loading flag before requesting content for the new note
                            self.loading_note = true;
                            
                            #[cfg(debug_assertions)]
                            eprintln!(
                                "Setting loading_note flag to true for note '{}' from visualizer",
                                note_path
                            );
                            
                            let notebook_path_clone = self.notebook_path.clone();
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
            Message::NewNote => {
                if self.notebook_path.is_empty() {
                    #[cfg(debug_assertions)]
                    eprintln!("Cannot create a new note: No notebook is open.");
                    Command::none()
                } else {
                    self.show_new_note_input = true;
                    self.new_note_path_input = String::new();
                    self.show_visualizer = false;
                    self.show_move_note_input = false;
                    self.show_about_info = false;
                    Command::none()
                }
            }
            Message::NewNoteInputChanged(text) => {
                if self.show_new_note_input {
                    self.new_note_path_input = text;
                }
                Command::none()
            }
            Message::CreateNote => {
                if self.show_new_note_input {
                    let new_note_rel_path = self.new_note_path_input.trim().to_string();
                    if new_note_rel_path.is_empty() {
                        #[cfg(debug_assertions)]
                        eprintln!("New note name cannot be empty.");
                        Command::none()
                    } else {
                        self.show_new_note_input = false;
                        let notebook_path = self.notebook_path.clone();
                        let mut current_notes = self.note_explorer.notes.clone();

                        Command::perform(
                            async move {
                                notebook::create_new_note(
                                    &notebook_path,
                                    &new_note_rel_path,
                                    &mut current_notes,
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
            Message::CancelNewNote => {
                self.show_new_note_input = false;
                self.new_note_path_input = String::new();
                Command::none()
            }
            Message::NoteCreated(result) => match result {
                Ok(new_note_metadata) => {
                    #[cfg(debug_assertions)]
                    eprintln!("Note created successfully: {}", new_note_metadata.rel_path);
                    let reload_command = self
                        .note_explorer
                        .update(note_explorer::Message::LoadNotes)
                        .map(Message::NoteExplorerMessage);

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
            },
            Message::DeleteNote => {
                if let Some(selected_path) = &self.selected_note_path {
                    if !self.show_about_info {
                        let note_path_clone = selected_path.clone();
                        self.show_new_note_input = false;
                        self.show_move_note_input = false;
                        self.show_visualizer = false;
                        self.show_about_info = false;

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
            Message::ConfirmDeleteNote(confirmed) => {
                if confirmed {
                    if let Some(selected_path) = self.selected_note_path.take() {
                        let notebook_path_clone = self.notebook_path.clone();
                        let mut current_notes = self.note_explorer.notes.clone();

                        Command::perform(
                            async move {
                                notebook::delete_note(
                                    &notebook_path_clone,
                                    &selected_path,
                                    &mut current_notes,
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
            Message::NoteDeleted(result) => match result {
                Ok(()) => {
                    #[cfg(debug_assertions)]
                    eprintln!("Note deleted successfully.");
                    
                    // Clean up history for the deleted note
                    if let Some(path) = &self.selected_note_path {
                        self.undo_histories.remove(path);
                        self.undo_indices.remove(path);
                        #[cfg(debug_assertions)]
                        eprintln!("Removed undo history and index for deleted note '{}'", path);
                    }
                    
                    self.selected_note_path = None;
                    self.selected_note_labels = Vec::new();
                    self.content = Content::with_text("");
                    self.markdown_text = String::new();
                    self.show_move_note_input = false;
                    self.move_note_current_path = None;
                    self.move_note_new_path_input = String::new();

                    self.note_explorer
                        .update(note_explorer::Message::LoadNotes)
                        .map(Message::NoteExplorerMessage)
                }
                Err(_err) => {
                    #[cfg(debug_assertions)]
                    eprintln!("Failed to delete note: {}", _err);
                    // Clone _err to be used in the async move block
                    let error_message = _err.clone();
                    let error_message_clone = error_message.clone();
                    let dialog_command = Command::perform(
                        async move {
                            // _err is moved here
                            let _ = MessageDialog::new()
                                .set_type(native_dialog::MessageType::Error)
                                .set_title("Error Deleting Note")
                                .set_text(&error_message) // Use the cloned variable
                                .show_alert();
                        },
                        move |()| Message::NoteDeleted(Err(error_message_clone)), // Use cloned message
                    );
                    let reload_command = self
                        .note_explorer
                        .update(note_explorer::Message::LoadNotes)
                        .map(Message::NoteExplorerMessage);
                    Command::batch(vec![dialog_command, reload_command])
                }
            },
            Message::MoveNote => {
                if let Some(current_path) = &self.selected_note_path {
                    if !self.show_about_info {
                        self.show_new_note_input = false;
                        self.show_visualizer = false;
                        self.show_about_info = false;

                        self.show_move_note_input = true;
                        self.move_note_current_path = Some(current_path.clone());
                        self.move_note_new_path_input = current_path.clone();
                        #[cfg(debug_assertions)]
                        eprintln!("Showing move note input for: {}", current_path);
                    }
                } else {
                    #[cfg(debug_assertions)]
                    eprintln!("No note selected to move.");
                }
                Command::none()
            }
            Message::InitiateFolderRename(folder_path) => {
                if !self.notebook_path.is_empty() && !self.show_about_info {
                    self.show_new_note_input = false;
                    self.show_visualizer = false;
                    self.show_about_info = false;

                    self.show_move_note_input = true;
                    self.move_note_current_path = Some(folder_path.clone());
                    self.move_note_new_path_input = folder_path.clone();
                    self.selected_note_path = None;

                    #[cfg(debug_assertions)]
                    eprintln!("Initiating folder rename for: {}", folder_path);
                } else if self.notebook_path.is_empty() {
                    #[cfg(debug_assertions)]
                    eprintln!("Cannot rename folder: No notebook is open.");
                }
                Command::none()
            }
            Message::MoveNoteInputChanged(text) => {
                if self.show_move_note_input {
                    self.move_note_new_path_input = text;
                }
                Command::none()
            }
            Message::ConfirmMoveNote => {
                if self.show_move_note_input {
                    if let Some(current_path) = self.move_note_current_path.take() {
                        let new_path = self.move_note_new_path_input.trim().to_string();
                        self.show_move_note_input = false;
                        self.move_note_new_path_input = String::new();

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
                            let first_note_path =
                                self.note_explorer.notes.get(0).map(|n| n.rel_path.clone());
                            if let Some(path) = first_note_path {
                                return Command::perform(async { path }, Message::NoteSelected);
                            } else {
                                return Command::none();
                            }
                        }

                        let notebook_path = self.notebook_path.clone();
                        let mut current_notes = self.note_explorer.notes.clone();

                        Command::perform(
                            async move {
                                notebook::move_note(
                                    &notebook_path,
                                    &current_path,
                                    &new_path,
                                    &mut current_notes,
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
                        self.show_move_note_input = false;
                        self.move_note_new_path_input = String::new();
                        Command::none()
                    }
                } else {
                    Command::none()
                }
            }
            Message::CancelMoveNote => {
                self.show_move_note_input = false;
                self.move_note_current_path = None;
                self.move_note_new_path_input = String::new();
                #[cfg(debug_assertions)]
                eprintln!("Move/Rename cancelled by user.");

                let command = if let Some(selected_path) = self.selected_note_path.clone() {
                    Command::perform(async move { selected_path }, Message::NoteSelected)
                } else {
                    let first_note_path =
                        self.note_explorer.notes.get(0).map(|n| n.rel_path.clone());
                    if let Some(path) = first_note_path {
                        Command::perform(async { path }, Message::NoteSelected)
                    } else {
                        Command::none()
                    }
                };

                command
            }
            Message::NoteMoved(result) => match result {
                Ok(new_rel_path) => {
                    #[cfg(debug_assertions)]
                    eprintln!("Item moved/renamed successfully to: {}", new_rel_path);
                    
                    // If we're moving a note that had an undo history, update the key
                    if let Some(old_path) = &self.move_note_current_path {
                        // Update the history collection
                        if let Some(history) = self.undo_histories.remove(old_path) {
                            self.undo_histories.insert(new_rel_path.clone(), history);
                            #[cfg(debug_assertions)]
                            eprintln!("Updated undo history key from '{}' to '{}'", old_path, new_rel_path);
                        }
                        
                        // Update the index collection
                        if let Some(index) = self.undo_indices.remove(old_path) {
                            self.undo_indices.insert(new_rel_path.clone(), index);
                            #[cfg(debug_assertions)]
                            eprintln!("Updated undo index key from '{}' to '{}'", old_path, new_rel_path);
                        }
                    }
                    
                    let reload_command = self
                        .note_explorer
                        .update(note_explorer::Message::LoadNotes)
                        .map(Message::NoteExplorerMessage);

                    reload_command
                }
                Err(_err) => {
                    #[cfg(debug_assertions)]
                    eprintln!("Failed to move/rename item: {}", _err);
                    // Clone _err to be used in the async move block
                    let error_message = _err.clone();
                    let error_message_clone = error_message.clone();
                    let dialog_command = Command::perform(
                        async move {
                            // _err is moved here
                            let _ = MessageDialog::new()
                                .set_type(native_dialog::MessageType::Error)
                                .set_title("Error Moving/Renaming")
                                .set_text(&error_message) // Use the cloned variable
                                .show_alert();
                        },
                        move |()| Message::NoteMoved(Err(error_message_clone)), // Use cloned message
                    );
                    let reload_command = self
                        .note_explorer
                        .update(note_explorer::Message::LoadNotes)
                        .map(Message::NoteExplorerMessage);
                    Command::batch(vec![dialog_command, reload_command])
                }
            },
            Message::AboutButtonClicked => {
                #[cfg(debug_assertions)]
                eprintln!("About button clicked. Toggling about info visibility.");
                self.show_about_info = !self.show_about_info;
                if self.show_about_info {
                    self.show_visualizer = false;
                    self.show_new_note_input = false;
                    self.show_move_note_input = false;
                }
                Command::none()
            }
        }
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        iced::event::listen_with(|event, _status| {
            // Only handle key presses that could be shortcuts when the editor is logically active/visible.
            // We can't directly check if the text_editor widget has focus here,
            // so we'll rely on the state checks in the HandleTabKey and SelectAll message handlers.
            match event {
                Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    // Handle Ctrl+A for Select All
                    if modifiers.control() {
                        if let Key::Character(c) = &key {
                            if c == "a" || c == "A" {
                                #[cfg(debug_assertions)]
                                eprintln!(
                                    "Ctrl+A pressed detected via subscription. Sending SelectAll message."
                                );
                                return Some(Message::SelectAll); // Consume the event
                            }
                            if c == "z" || c == "Z" {
                                #[cfg(debug_assertions)]
                                eprintln!(
                                    "Ctrl+Z pressed detected via subscription. Sending Undo message."
                                );
                                return Some(Message::Undo); // Consume the event
                            }
                        }
                    }

                    // Handle Tab key press (no modifiers)
                    if key == Key::Named(iced::keyboard::key::Named::Tab) && modifiers.is_empty() {
                        #[cfg(debug_assertions)]
                        eprintln!(
                            "Tab key (Named::Tab) pressed detected via subscription. Sending HandleTabKey message."
                        );
                        return Some(Message::HandleTabKey); // Consume the event
                    }

                    // For any other key press, let the event propagate.
                    None
                }
                _ => None, // Ignore other events
            }
        })
    }

    fn view(&self) -> Element<'_, Self::Message, Self::Theme> {
        let mut top_bar = Row::new().spacing(10).padding(5).width(Length::Fill);

        let is_dialog_open =
            self.show_new_note_input || self.show_move_note_input || self.show_about_info;

        if !is_dialog_open && !self.show_visualizer {
            let about_button_text = if self.show_about_info {
                "Back"
            } else {
                "About"
            };
            top_bar = top_bar.push(
                button(about_button_text)
                    .padding(5)
                    .on_press(Message::AboutButtonClicked),
            );
        } else if self.show_about_info {
            top_bar = top_bar.push(
                button("Back")
                    .padding(5)
                    .on_press(Message::AboutButtonClicked),
            );
        }

        if !self.notebook_path.is_empty() {
            if !is_dialog_open && !self.show_visualizer {
                let visualizer_button_text = if self.show_visualizer {
                    "Hide Visualizer"
                } else {
                    "Show Visualizer"
                };
                top_bar = top_bar.push(
                    button(visualizer_button_text)
                        .padding(5)
                        .on_press(Message::ToggleVisualizer),
                );
            } else if self.show_visualizer && !is_dialog_open {
                top_bar = top_bar.push(
                    button("Hide Visualizer")
                        .padding(5)
                        .on_press(Message::ToggleVisualizer),
                );
            }

            if !self.show_visualizer
                && !self.show_new_note_input
                && !self.show_move_note_input
                && !self.show_about_info
            {
                top_bar = top_bar.push(button("New Note").padding(5).on_press(Message::NewNote));
                if self.selected_note_path.is_some() {
                    top_bar = top_bar.push(
                        button("Delete Note")
                            .padding(5)
                            .on_press(Message::DeleteNote),
                    );
                    top_bar =
                        top_bar.push(button("Move Note").padding(5).on_press(Message::MoveNote));
                }
            } else if self.show_new_note_input {
                top_bar = top_bar.push(Text::new("Creating New Note..."));
            } else if self.show_move_note_input {
                let mut all_folders_in_notes: HashSet<String> = HashSet::new();
                for note in &self.note_explorer.notes {
                    if let Some(parent) = Path::new(&note.rel_path).parent() {
                        let folder_path = parent.to_string_lossy().into_owned();
                        if !folder_path.is_empty() && folder_path != "." {
                            all_folders_in_notes.insert(folder_path);
                        }
                    }
                }

                let is_renaming_folder = self
                    .move_note_current_path
                    .as_deref()
                    .map_or(false, |p| all_folders_in_notes.contains(p));

                let operation_text = if is_renaming_folder {
                    "Renaming Folder"
                } else {
                    "Moving Note"
                };
                top_bar = top_bar.push(Text::new(format!(
                    "{} '{}'...",
                    operation_text,
                    self.move_note_current_path.as_deref().unwrap_or("")
                )));
            }
        } else {
            if !self.show_about_info {
                top_bar = top_bar.push(Text::new(
                    "Please configure the 'notebook_path' in your config.json file to open a notebook.",
                ));
            }
        }

        let main_content: Element<'_, Self::Message, Self::Theme> = if self.show_about_info {
            let about_info_column = Column::new()
                .spacing(10)
                .align_items(iced::Alignment::Center)
                .push(Text::new("Cognate Note Taking App").size(30))
                .push(Text::new(format!("Version: {}", self.app_version)).size(20));

            Container::new(about_info_column)
                .center_x()
                .center_y()
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else if self.show_visualizer {
            Container::new(self.visualizer.view().map(Message::VisualizerMessage))
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else if self.show_new_note_input {
            Column::new()
                .push(Text::new(
                    "Enter new note name/relative path (e.g., folder/note_name):",
                ))
                .push(
                    IcedTextInput::new("Note name...", &self.new_note_path_input)
                        .on_input(Message::NewNoteInputChanged)
                        .on_submit(Message::CreateNote)
                        .width(Length::Fixed(300.0)),
                )
                .push(
                    Row::new()
                        .push(button("Create").padding(5).on_press(Message::CreateNote))
                        .push(button("Cancel").padding(5).on_press(Message::CancelNewNote))
                        .spacing(10),
                )
                .spacing(10)
                .padding(20)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_items(iced::Alignment::Center)
                .into()
        } else if self.show_move_note_input {
            let mut all_folders_in_notes: HashSet<String> = HashSet::new();
            for note in &self.note_explorer.notes {
                if let Some(parent) = Path::new(&note.rel_path).parent() {
                    let folder_path = parent.to_string_lossy().into_owned();
                    if !folder_path.is_empty() && folder_path != "." {
                        all_folders_in_notes.insert(folder_path);
                    }
                }
            }

            let is_renaming_folder = self
                .move_note_current_path
                .as_deref()
                .map_or(false, |p| all_folders_in_notes.contains(p));

            let prompt_text = if is_renaming_folder {
                format!(
                    "Enter new relative path/name for folder '{}':",
                    self.move_note_current_path.as_deref().unwrap_or("")
                )
            } else {
                format!(
                    "Enter new relative path/name for note '{}':",
                    self.move_note_current_path.as_deref().unwrap_or("")
                )
            };

            Column::new()
                .push(Text::new(prompt_text))
                .push(
                    IcedTextInput::new("New path...", &self.move_note_new_path_input)
                        .on_input(Message::MoveNoteInputChanged)
                        .on_submit(Message::ConfirmMoveNote)
                        .width(Length::Fixed(400.0)),
                )
                .push(
                    Row::new()
                        .push(
                            button("Confirm")
                                .padding(5)
                                .on_press(Message::ConfirmMoveNote),
                        )
                        .push(
                            button("Cancel")
                                .padding(5)
                                .on_press(Message::CancelMoveNote),
                        )
                        .spacing(10),
                )
                .spacing(10)
                .padding(20)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_items(iced::Alignment::Center)
                .into()
        } else if self.notebook_path.is_empty() {
            Container::new(
                Text::new("Please configure the 'notebook_path' in your config.json file to open a notebook.")
                    .size(20)
                    .style(iced::theme::Text::Color(iced::Color::from_rgb(0.7, 0.2, 0.2)))
            )
             .center_x()
             .center_y()
             .width(Length::Fill)
             .height(Length::Fill)
             .into()
        } else {
            let note_explorer_view: Element<'_, Self::Message, Self::Theme> = Container::new(
                self.note_explorer
                    .view(self.selected_note_path.as_ref())
                    .map(|note_explorer_message| match note_explorer_message {
                        note_explorer::Message::NoteSelected(path) => Message::NoteSelected(path),
                        note_explorer::Message::ToggleFolder(path) => {
                            Message::NoteExplorerMessage(note_explorer::Message::ToggleFolder(path))
                        }
                        note_explorer::Message::InitiateFolderRename(path) => {
                            Message::InitiateFolderRename(path)
                        }
                        other_msg => Message::NoteExplorerMessage(other_msg),
                    }),
            )
            .width(Length::FillPortion(2))
            .into();

            let mut editor_widget = text_editor(&self.content).height(Length::Fill);

            if self.selected_note_path.is_some() {
                editor_widget = editor_widget.on_action(Message::EditorAction);
            }

            let editor_widget_element: Element<'_, Self::Message, Self::Theme> =
                editor_widget.into();

            let editor_container =
                Container::new(editor_widget_element).width(Length::FillPortion(8));
            let editor_container_element: Element<'_, Self::Message, Self::Theme> =
                editor_container.into();

            let content_row = Row::new()
                .push(note_explorer_view)
                .push(editor_container_element)
                .spacing(10)
                .padding(10)
                .width(Length::Fill)
                .height(Length::FillPortion(10));

            let mut labels_row = Row::new().spacing(10).padding(5).width(Length::Fill);

            if self.selected_note_path.is_some() {
                labels_row = labels_row.push(Text::new("Labels: "));
                if self.selected_note_labels.is_empty() {
                    labels_row = labels_row.push(Text::new("No labels"));
                } else {
                    for label in &self.selected_note_labels {
                        labels_row = labels_row.push(
                            button(Text::new(label.clone()))
                                .on_press(Message::RemoveLabel(label.clone())),
                        );
                    }
                }

                labels_row = labels_row
                    .push(
                        text_input("New Label", &self.new_label_text)
                            .on_input(Message::NewLabelInputChanged)
                            .on_submit(Message::AddLabel)
                            .width(Length::Fixed(150.0)),
                    )
                    .push(button("Add Label").padding(5).on_press(Message::AddLabel));
            } else {
                labels_row = labels_row.push(Text::new("Select a note to manage labels."));
            }

            let bottom_bar: Element<'_, Self::Message, Self::Theme> = Container::new(labels_row)
                .width(Length::Fill)
                .height(Length::FillPortion(1))
                .into();

            Column::new().push(content_row).push(bottom_bar).into()
        };

        Container::new(Column::new().push(top_bar).push(main_content))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }
}
