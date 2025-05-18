use iced::widget::{
    Column, Container, Row, Text, TextInput as IcedTextInput, button, text_editor, text_input,
};
use iced::{Application, Command, Element, Length, Theme};
use native_dialog::MessageDialog;

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
    // Removed: NewNotebookPathSelected(String), // This variant was never constructed by the current UI
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
    // New messages for moving notes
    MoveNote,
    MoveNoteInputChanged(String),
    ConfirmMoveNote,
    CancelMoveNote,
    NoteMoved(Result<String, String>), // Result contains the new rel_path on success
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
    // New state for moving notes
    show_move_note_input: bool,
    move_note_current_path: Option<String>,
    move_note_new_path_input: String,
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
            // Initialize new state for moving notes
            show_move_note_input: false,
            move_note_current_path: None,
            move_note_new_path_input: String::new(),
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
                if self.selected_note_path.is_some()
                    && !self.show_visualizer
                    && !self.show_move_note_input
                    && !self.show_new_note_input
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
                if !self.show_visualizer && !self.show_move_note_input && !self.show_new_note_input
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

                    if self.selected_note_path.is_none() && !self.note_explorer.notes.is_empty() {
                        let first_note_path = self.note_explorer.notes[0].rel_path.clone();
                        eprintln!(
                            "Editor: No note selected, selecting first note: {}",
                            first_note_path
                        );
                        editor_command =
                            Command::perform(async { first_note_path }, Message::NoteSelected);
                    } else if self.selected_note_path.is_some()
                        && !self
                            .note_explorer
                            .notes
                            .iter()
                            .any(|n| Some(&n.rel_path) == self.selected_note_path.as_ref())
                    {
                        eprintln!("Editor: Selected note no longer exists. Clearing editor state.");
                        self.selected_note_path = None;
                        self.selected_note_labels = Vec::new();
                        self.content = text_editor::Content::with_text("");
                        self.markdown_text = String::new();
                        self.show_move_note_input = false;
                        self.move_note_current_path = None;
                        self.move_note_new_path_input = String::new();
                    } else if let Some(selected_path) = &self.selected_note_path {
                        if let Some(note) = self
                            .note_explorer
                            .notes
                            .iter()
                            .find(|n| &n.rel_path == selected_path)
                        {
                            self.selected_note_labels = note.labels.clone();
                        }
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
            // Removed: Message::NewNotebookPathSelected(path_str) => { ... }
            Message::NewLabelInputChanged(text) => {
                self.new_label_text = text;
                Command::none()
            }
            Message::AddLabel => {
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
                Command::none()
            }
            Message::RemoveLabel(label_to_remove) => {
                if let Some(selected_path) = &self.selected_note_path {
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
                        self.selected_note_path = Some(note_path.clone());
                        self.new_label_text = String::new();
                        self.show_move_note_input = false;
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

                        self.show_visualizer = false;

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

                    Command::none()
                }
            }
            Message::NewNoteInputChanged(text) => {
                self.new_note_path_input = text;
                Command::none()
            }
            Message::CreateNote => {
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
                            Message::NoteCreated(Ok(NoteMetadata {
                                rel_path: String::new(),
                                labels: Vec::new(),
                            }))
                        }, // Dummy message to satisfy type
                    );
                    dialog_command
                }
            },
            Message::DeleteNote => {
                if let Some(selected_path) = &self.selected_note_path {
                    let note_path_clone = selected_path.clone();
                    self.show_new_note_input = false;
                    self.show_move_note_input = false;
                    self.show_visualizer = false;

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
                    self.show_new_note_input = false;
                    self.show_visualizer = false;

                    self.show_move_note_input = true;
                    self.move_note_current_path = Some(current_path.clone());
                    self.move_note_new_path_input = current_path.clone();
                    eprintln!("Showing move note input for: {}", current_path);
                } else {
                    eprintln!("No note selected to move.");
                }
                Command::none()
            }
            Message::MoveNoteInputChanged(text) => {
                self.move_note_new_path_input = text;
                Command::none()
            }
            Message::ConfirmMoveNote => {
                if let Some(current_path) = self.move_note_current_path.take() {
                    let new_path = self.move_note_new_path_input.trim().to_string();
                    self.show_move_note_input = false;
                    self.move_note_new_path_input = String::new();

                    if new_path.is_empty() {
                        eprintln!("New path cannot be empty for moving note.");
                        let dialog_command = Command::perform(
                            async move {
                                let _ = MessageDialog::new()
                                    .set_type(native_dialog::MessageType::Error)
                                    .set_title("Error Moving Note")
                                    .set_text("New path cannot be empty.")
                                    .show_alert();
                            },
                            |()| Message::NoteMoved(Err(String::new())), // Dummy message
                        );
                        return dialog_command;
                    }

                    if new_path == current_path {
                        eprintln!("New path is the same as the current path.");
                        self.selected_note_path = Some(current_path.clone());
                        return Command::none();
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
                    eprintln!("ConfirmMoveNote called with no current note selected.");
                    self.show_move_note_input = false;
                    self.move_note_new_path_input = String::new();
                    Command::none()
                }
            }
            Message::CancelMoveNote => {
                self.show_move_note_input = false;
                self.move_note_current_path = None;
                self.move_note_new_path_input = String::new();
                eprintln!("Move note cancelled by user.");
                Command::none()
            }
            Message::NoteMoved(result) => match result {
                Ok(new_rel_path) => {
                    eprintln!("Note moved successfully to: {}", new_rel_path);
                    let reload_command = self
                        .note_explorer
                        .update(note_explorer::Message::LoadNotes)
                        .map(Message::NoteExplorerMessage);

                    let select_command =
                        Command::perform(async { new_rel_path }, Message::NoteSelected);

                    Command::batch(vec![reload_command, select_command])
                }
                Err(err) => {
                    eprintln!("Failed to move note: {}", err);
                    let dialog_command = Command::perform(
                        async move {
                            let _ = MessageDialog::new()
                                .set_type(native_dialog::MessageType::Error)
                                .set_title("Error Moving Note")
                                .set_text(&err)
                                .show_alert();
                        },
                        |()| Message::NoteMoved(Err(String::new())), // Dummy message
                    );
                    let reload_command = self
                        .note_explorer
                        .update(note_explorer::Message::LoadNotes)
                        .map(Message::NoteExplorerMessage);
                    Command::batch(vec![dialog_command, reload_command])
                }
            },
        }
    }

    fn view(&self) -> Element<'_, Self::Message, Self::Theme> {
        let mut top_bar = Row::new().spacing(10).padding(5).width(Length::Fill);

        if !self.notebook_path.is_empty() {
            // Always show visualizer toggle when notebook is open
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

            // Show other action buttons only if no other input/visualizer is active
            if !self.show_visualizer && !self.show_new_note_input && !self.show_move_note_input {
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
                top_bar = top_bar.push(Text::new(format!(
                    "Moving Note '{}'...",
                    self.move_note_current_path.as_deref().unwrap_or("")
                )));
            }
        } else {
            // No notebook open message
            top_bar = top_bar.push(Text::new(
                "No notebook opened. Configure 'notebook_path' in config.json",
            ));
        }

        let main_content: Element<'_, Self::Message, Self::Theme> = if self.show_visualizer {
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
            Column::new()
                .push(Text::new(format!(
                    "Enter new relative path for '{}':",
                    self.move_note_current_path.as_deref().unwrap_or("")
                )))
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
            // Display the standard editor layout (without HTML preview)
            let note_explorer_view: Element<'_, Self::Message, Self::Theme> = Container::new(
                self.note_explorer
                    .view(self.selected_note_path.as_ref())
                    .map(|note_explorer_message| match note_explorer_message {
                        note_explorer::Message::NoteSelected(path) => Message::NoteSelected(path),
                        note_explorer::Message::ToggleFolder(path) => {
                            Message::NoteExplorerMessage(note_explorer::Message::ToggleFolder(path))
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
