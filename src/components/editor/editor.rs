use iced::widget::{
    Column, Container, Row, Text, TextInput as IcedTextInput, button, text_editor, text_input,
};
use iced::{Application, Command, Element, Length, Theme};
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
    // Added new message for initiating folder rename
    InitiateFolderRename(String),

    // Message for About button click
    AboutButtonClicked,
}

pub struct Editor {
    content: text_editor::Content,
    theme: Theme,
    markdown_text: String,
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
    app_version: String, // Storing the app version

    // New state to control visibility of the about information
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
            content: text_editor::Content::with_text(&initial_text),
            theme: local_theme::convert_str_to_theme(flags.theme.clone()),
            notebook_path: notebook_path_clone.clone(),
            markdown_text: String::new(),
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
            // Initialize app version from flags
            app_version: flags.version,
            // Initialize new about info state
            show_about_info: false,
        };

        let initial_command = if !editor_instance.notebook_path.is_empty() {
            eprintln!(
                "Editor: Initializing with notebook: {}",
                editor_instance.notebook_path
            );
            editor_instance
                .note_explorer
                .update(note_explorer::Message::LoadNotes)
                .map(Message::NoteExplorerMessage)
        } else {
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
            Message::EditorAction(action) => {
                // Only perform editor actions if not showing visualizer, new note input, or move note input, AND not showing about info
                if self.selected_note_path.is_some()
                    && !self.show_visualizer
                    && !self.show_move_note_input
                    && !self.show_new_note_input
                    && !self.show_about_info
                // Added check for about info
                {
                    self.content.perform(action);
                    self.markdown_text = self.content.text();

                    if let Some(selected_path) = &self.selected_note_path {
                        let notebook_path = self.notebook_path.clone();
                        let note_path = selected_path.clone();
                        let content = self.markdown_text.clone();
                        eprintln!("Editor: Saving content for note: {}", note_path);
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
                // Only update content if not showing visualizer, new note input, or move note input, AND not showing about info
                if !self.show_visualizer
                    && !self.show_move_note_input
                    && !self.show_new_note_input
                    && !self.show_about_info
                // Added check for about info
                {
                    self.content = text_editor::Content::with_text(&new_content);
                    self.markdown_text = new_content;
                }
                Command::none()
            }
            Message::NoteExplorerMessage(note_explorer_message) => {
                eprintln!(
                    "Editor: Received NoteExplorerMessage: {:?}",
                    note_explorer_message
                );
                let note_explorer_command = self
                    .note_explorer
                    .update(note_explorer_message.clone())
                    .map(Message::NoteExplorerMessage);

                let mut editor_command = Command::none();
                if let note_explorer::Message::NotesLoaded(loaded_notes) = note_explorer_message {
                    eprintln!(
                        "Editor: NoteExplorer finished loading {} notes. Updating editor state.",
                        loaded_notes.len()
                    );
                    let _ = self.visualizer.update(visualizer::Message::UpdateNotes(
                        self.note_explorer.notes.clone(),
                    ));

                    // Handle case where the previously selected note was moved/deleted
                    if let Some(selected_path) = &self.selected_note_path {
                        if !self
                            .note_explorer
                            .notes
                            .iter()
                            .any(|n| &n.rel_path == selected_path)
                        {
                            eprintln!(
                                "Editor: Selected note no longer exists. Clearing editor state."
                            );
                            self.selected_note_path = None;
                            self.selected_note_labels = Vec::new();
                            self.content = text_editor::Content::with_text("");
                            self.markdown_text = String::new();
                            self.show_move_note_input = false; // Hide move/rename input if the item is gone
                            self.move_note_current_path = None;
                            self.move_note_new_path_input = String::new();
                        } else if let Some(note) = self
                            .note_explorer
                            .notes
                            .iter()
                            .find(|n| &n.rel_path == selected_path)
                        {
                            // Update labels if the note still exists (labels might have changed)
                            self.selected_note_labels = note.labels.clone();
                        }
                    } else if !self.note_explorer.notes.is_empty() {
                        // If no note was selected but there are notes, select the first one
                        let first_note_path = self.note_explorer.notes[0].rel_path.clone();
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
                eprintln!(
                    "Editor: NoteSelected message received for path: {}",
                    note_path
                );
                self.selected_note_path = Some(note_path.clone());
                self.new_label_text = String::new();
                self.show_move_note_input = false;
                self.move_note_current_path = None;
                self.move_note_new_path_input = String::new();
                self.show_about_info = false; // Hide about info when a note is selected
                self.show_new_note_input = false; // Hide new note input

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

                if !self.show_visualizer && !self.notebook_path.is_empty() {
                    let notebook_path_clone = self.notebook_path.clone();
                    let note_path_clone = note_path.clone();

                    Command::perform(
                        async move {
                            let full_note_path =
                                format!("{}/{}/note.md", notebook_path_clone, note_path_clone);
                            match std::fs::read_to_string(full_note_path) {
                                Ok(content) => content,
                                Err(err) => {
                                    eprintln!("Failed to read note file for editor: {}", err);
                                    String::new()
                                }
                            }
                        },
                        Message::ContentChanged,
                    )
                } else {
                    Command::none()
                }
            }
            Message::NewLabelInputChanged(text) => {
                if !self.show_about_info {
                    // Prevent input change if about info is showing
                    self.new_label_text = text;
                }
                Command::none()
            }
            Message::AddLabel => {
                // Fixed: Use if let to correctly access selected_note_path
                if !self.show_about_info {
                    // Prevent adding labels if about info is showing
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
                        // Prevent removing labels if about info is showing
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
                if let Err(err) = result {
                    eprintln!("Error saving metadata: {}", err);
                } else {
                    eprintln!("Metadata saved successfully.");
                }
                Command::none()
            }
            Message::NoteContentSaved(result) => {
                if let Err(err) = result {
                    eprintln!("Error saving note content: {}", err);
                } else {
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
                        self.show_about_info = false; // Hide about info when visualizer is shown
                    }
                    eprintln!("Toggled visualizer visibility to: {}", self.show_visualizer);
                    if self.show_visualizer {
                        let _ = self.visualizer.update(visualizer::Message::UpdateNotes(
                            self.note_explorer.notes.clone(),
                        ));
                    }
                } else {
                    eprintln!("Cannot show visualizer: No notebook is open.");
                }
                Command::none()
            }
            Message::VisualizerMessage(visualizer_message) => {
                let _ = self.visualizer.update(visualizer_message.clone());

                match visualizer_message {
                    visualizer::Message::UpdateNotes(_) => Command::none(),
                    visualizer::Message::NoteSelectedInVisualizer(note_path) => {
                        eprintln!(
                            "Editor: Received NoteSelectedInVisualizer for path: {}",
                            note_path
                        );
                        // When selecting from visualizer, hide other UI elements
                        self.show_visualizer = false;
                        self.show_new_note_input = false;
                        self.show_move_note_input = false;
                        self.show_about_info = false; // Hide about info when a note is selected in visualizer

                        // Then handle selecting the note as usual
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

                        if !self.notebook_path.is_empty() {
                            let notebook_path_clone = self.notebook_path.clone();
                            let note_path_clone = note_path.clone();

                            Command::perform(
                                async move {
                                    let full_note_path = format!(
                                        "{}/{}/note.md",
                                        notebook_path_clone, note_path_clone
                                    );
                                    match std::fs::read_to_string(full_note_path) {
                                        Ok(content) => content,
                                        Err(err) => {
                                            eprintln!(
                                                "Failed to read note file for editor: {}",
                                                err
                                            );
                                            String::new()
                                        }
                                    }
                                },
                                Message::ContentChanged,
                            )
                        } else {
                            Command::none()
                        }
                    }
                }
            }
            Message::NewNote => {
                if self.notebook_path.is_empty() {
                    eprintln!("Cannot create a new note: No notebook is open.");
                    Command::none()
                } else {
                    self.show_new_note_input = true;
                    self.new_note_path_input = String::new();
                    self.show_visualizer = false;
                    self.show_move_note_input = false;
                    self.show_about_info = false; // Hide about info when creating a new note
                    Command::none()
                }
            }
            Message::NewNoteInputChanged(text) => {
                if self.show_new_note_input {
                    // Only allow input change if new note input is showing
                    self.new_note_path_input = text;
                }
                Command::none()
            }
            Message::CreateNote => {
                if self.show_new_note_input {
                    // Only allow create note if new note input is showing
                    let new_note_rel_path = self.new_note_path_input.trim().to_string();
                    if new_note_rel_path.is_empty() {
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
                Err(err) => {
                    eprintln!("Failed to create note: {}", err);
                    let dialog_command = Command::perform(
                        async move {
                            let _ = MessageDialog::new()
                                .set_type(native_dialog::MessageType::Error)
                                .set_title("Error Creating Note")
                                .set_text(&err)
                                .show_alert();
                        },
                        |()| {
                            // We need a message here to satisfy the type signature.
                            // Ideally, this would trigger a state where the error is displayed,
                            // but for simplicity, we'll just acknowledge it.
                            // A unit struct or an ignored variant could be better.
                            // For now, we'll just reload the notes in case the error
                            // caused an inconsistent state, and the user can try again.
                            Message::NoteExplorerMessage(note_explorer::Message::LoadNotes)
                        },
                    );
                    dialog_command
                }
            },
            Message::DeleteNote => {
                if let Some(selected_path) = &self.selected_note_path {
                    if !self.show_about_info {
                        // Prevent deleting if about info is showing
                        let note_path_clone = selected_path.clone();
                        self.show_new_note_input = false;
                        self.show_move_note_input = false;
                        self.show_visualizer = false;
                        self.show_about_info = false; // Hide about info

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
                        eprintln!("ConfirmDeleteNote called with no selected note.");
                        Command::none()
                    }
                } else {
                    eprintln!("Note deletion cancelled by user.");
                    Command::none()
                }
            }
            Message::NoteDeleted(result) => {
                match result {
                    Ok(()) => {
                        eprintln!("Note deleted successfully.");
                        self.selected_note_path = None;
                        self.selected_note_labels = Vec::new();
                        self.content = text_editor::Content::with_text("");
                        self.markdown_text = String::new();
                        self.show_move_note_input = false;
                        self.move_note_current_path = None;
                        self.move_note_new_path_input = String::new();

                        self.note_explorer
                            .update(note_explorer::Message::LoadNotes)
                            .map(Message::NoteExplorerMessage)
                    }
                    Err(err) => {
                        eprintln!("Failed to delete note: {}", err);
                        let dialog_command = Command::perform(
                            async move {
                                let _ = MessageDialog::new()
                                    .set_type(native_dialog::MessageType::Error)
                                    .set_title("Error Deleting Note")
                                    .set_text(&err)
                                    .show_alert();
                            },
                            |()| Message::NoteDeleted(Ok(())), // Dummy message to satisfy type
                        );
                        let reload_command = self
                            .note_explorer
                            .update(note_explorer::Message::LoadNotes)
                            .map(Message::NoteExplorerMessage);
                        Command::batch(vec![dialog_command, reload_command])
                    }
                }
            }
            Message::MoveNote => {
                if let Some(current_path) = &self.selected_note_path {
                    if !self.show_about_info {
                        // Prevent moving if about info is showing
                        self.show_new_note_input = false;
                        self.show_visualizer = false;
                        self.show_about_info = false; // Hide about info when moving a note

                        self.show_move_note_input = true;
                        self.move_note_current_path = Some(current_path.clone());
                        self.move_note_new_path_input = current_path.clone();
                        eprintln!("Showing move note input for: {}", current_path);
                    }
                } else {
                    eprintln!("No note selected to move.");
                }
                Command::none()
            }
            Message::InitiateFolderRename(folder_path) => {
                if !self.notebook_path.is_empty() && !self.show_about_info {
                    // Prevent initiating rename if about info is showing
                    self.show_new_note_input = false;
                    self.show_visualizer = false;
                    self.show_about_info = false; // Hide about info

                    self.show_move_note_input = true;
                    self.move_note_current_path = Some(folder_path.clone());
                    self.move_note_new_path_input = folder_path.clone();
                    self.selected_note_path = None; // Deselect note when renaming folder

                    eprintln!("Initiating folder rename for: {}", folder_path);
                } else if self.notebook_path.is_empty() {
                    eprintln!("Cannot rename folder: No notebook is open.");
                }
                Command::none()
            }
            Message::MoveNoteInputChanged(text) => {
                if self.show_move_note_input {
                    // Only allow input change if move note input is showing
                    self.move_note_new_path_input = text;
                }
                Command::none()
            }
            Message::ConfirmMoveNote => {
                if self.show_move_note_input {
                    // Only allow confirm move if move note input is showing
                    if let Some(current_path) = self.move_note_current_path.take() {
                        let new_path = self.move_note_new_path_input.trim().to_string();
                        self.show_move_note_input = false;
                        self.move_note_new_path_input = String::new();

                        if new_path.is_empty() {
                            eprintln!("New path cannot be empty for moving/renaming.");
                            let dialog_command = Command::perform(
                                async move {
                                    let _ = MessageDialog::new()
                                        .set_type(native_dialog::MessageType::Error)
                                        .set_title("Error Moving/Renaming")
                                        .set_text("New path cannot be empty.")
                                        .show_alert();
                                },
                                |()| Message::NoteMoved(Err(String::new())), // Dummy message
                            );
                            return dialog_command;
                        }

                        // Check if the new path is the same as the current path
                        if new_path == current_path {
                            eprintln!(
                                "New path is the same as the current path. No action needed."
                            );
                            // If we were renaming a folder, re-select the first note if available
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
                eprintln!("Move/Rename cancelled by user.");

                // If we were renaming a folder, re-select the first note if available
                let first_note_path = self.note_explorer.notes.get(0).map(|n| n.rel_path.clone());
                if let Some(path) = first_note_path {
                    // Select the first note if cancelling a folder rename/move
                    Command::perform(async { path }, Message::NoteSelected)
                } else if let Some(selected_path) = self.selected_note_path.clone() {
                    // Otherwise, if a note was selected before the move prompt, re-select it
                    Command::perform(async move { selected_path }, Message::NoteSelected)
                } else {
                    Command::none() // No notes available to select
                }
            }
            Message::NoteMoved(result) => match result {
                Ok(new_rel_path) => {
                    eprintln!("Item moved/renamed successfully to: {}", new_rel_path);
                    let reload_command = self
                        .note_explorer
                        .update(note_explorer::Message::LoadNotes)
                        .map(Message::NoteExplorerMessage);

                    // After a move/rename, the note list is reloaded.
                    // The NoteExplorerMessage handler will re-select the correct note
                    // or the first note if the selected one was part of a moved folder.
                    // So, we just need to trigger the reload.
                    reload_command
                }
                Err(err) => {
                    eprintln!("Failed to move/rename item: {}", err);
                    let dialog_command = Command::perform(
                        async move {
                            let _ = MessageDialog::new()
                                .set_type(native_dialog::MessageType::Error)
                                .set_title("Error Moving/Renaming")
                                .set_text(&err)
                                .show_alert();
                        },
                        |()| Message::NoteMoved(Err(String::new())), // Dummy message to satisfy type
                    );
                    let reload_command = self
                        .note_explorer
                        .update(note_explorer::Message::LoadNotes)
                        .map(Message::NoteExplorerMessage);
                    Command::batch(vec![dialog_command, reload_command])
                }
            },
            Message::AboutButtonClicked => {
                eprintln!("About button clicked. Toggling about info visibility.");
                self.show_about_info = !self.show_about_info;
                // Hide other transient UI elements when showing About
                if self.show_about_info {
                    self.show_visualizer = false;
                    self.show_new_note_input = false;
                    self.show_move_note_input = false;
                }
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Self::Message, Self::Theme> {
        let mut top_bar = Row::new().spacing(10).padding(5).width(Length::Fill);

        // Determine if any of the transient dialogs are open
        let is_dialog_open =
            self.show_new_note_input || self.show_move_note_input || self.show_about_info;

        // Conditionally add the About button
        // Only show if no dialog is open AND visualizer is not shown
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
            // If About dialog IS open, still show the "Back" button
            top_bar = top_bar.push(
                button("Back")
                    .padding(5)
                    .on_press(Message::AboutButtonClicked),
            );
        }

        if !self.notebook_path.is_empty() {
            // Conditionally add the Visualizer toggle button
            // Only show if no dialog is open AND visualizer is not shown
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
                // If Visualizer IS shown AND no other dialog is open, show "Hide Visualizer"
                top_bar = top_bar.push(
                    button("Hide Visualizer")
                        .padding(5)
                        .on_press(Message::ToggleVisualizer),
                );
            }

            // Show other action buttons only if no other input/visualizer is active AND not showing about info
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
                // Corrected heuristic to determine if it's a folder rename or note move
                let mut all_folders_in_notes: HashSet<String> = HashSet::new();
                for note in &self.note_explorer.notes {
                    if let Some(parent) = Path::new(&note.rel_path).parent() {
                        let folder_path = parent.to_string_lossy().into_owned();
                        if !folder_path.is_empty() && folder_path != "." {
                            all_folders_in_notes.insert(folder_path);
                        }
                    }
                }

                let is_renaming_folder =
                    self.move_note_current_path.as_deref().map_or(false, |p| {
                        // Check if the current path is one of the known folder paths
                        all_folders_in_notes.contains(p)
                    });

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
            // No notebook open message
            // Hide this message if about info is showing
            if !self.show_about_info {
                top_bar = top_bar.push(Text::new(
                    "No notebook opened. Configure 'notebook_path' in config.json",
                ));
            }
        }

        let main_content: Element<'_, Self::Message, Self::Theme> = if self.show_about_info {
            // Display about information
            let about_info_column = Column::new()
                .spacing(10)
                .align_items(iced::Alignment::Center)
                .push(Text::new("Cognate Note Taking App").size(30))
                .push(Text::new(format!("Version: {}", self.app_version)).size(20));
            // Could add more info here later, like license or authors

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
            // Corrected heuristic to determine if it's a folder rename or note move
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
            // Display the "No notebook" message when no notebook is open and not showing about
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
            // Display the standard editor layout
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
                        } // Map the new message
                        // Handle other messages if any, including the updated NoteExplorerMessage
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
