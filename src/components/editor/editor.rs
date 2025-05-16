use iced::widget::{
    Column, Container, Row, Scrollable, Text, TextInput as IcedTextInput, button, text_editor,
    text_input,
};
use iced::{Application, Command, Element, Length, Theme};
use native_dialog::FileDialog; // Keep FileDialog import
use native_dialog::MessageDialog;
use pulldown_cmark::{Options, Parser, html}; // Import MessageDialog for confirmation

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
    // Remove OpenNotebook message
    NewNotebookPathSelected(String), // This message is now used for the initial load
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
}

pub struct Editor {
    content: text_editor::Content,
    theme: Theme,
    configuration: Configuration, // Keep configuration to access other settings if needed
    markdown_text: String,
    html_output: String,
    note_explorer: note_explorer::NoteExplorer,
    visualizer: visualizer::Visualizer,
    show_visualizer: bool,
    notebook_path: String, // This will now be initialized from config
    selected_note_path: Option<String>,
    selected_note_labels: Vec<String>,
    new_label_text: String,
    show_new_note_input: bool,
    new_note_path_input: String,
}

impl Application for Editor {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = Configuration; // Accept Configuration struct as flags

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        let initial_text = String::new();
        let notebook_path_clone = flags.notebook_path.clone(); // Clone before moving flags
        let mut editor_instance = Editor {
            content: text_editor::Content::with_text(&initial_text),
            theme: local_theme::convert_str_to_theme(flags.theme.clone()),
            notebook_path: notebook_path_clone.clone(), // Initialize from cloned value
            configuration: flags,                       // Store the original flags (now moved)
            markdown_text: String::new(),
            html_output: String::new(),
            note_explorer: note_explorer::NoteExplorer::new(notebook_path_clone), // Initialize note_explorer with the cloned path
            visualizer: visualizer::Visualizer::new(),
            show_visualizer: false,
            selected_note_path: None,
            selected_note_labels: Vec::new(),
            new_label_text: String::new(),
            show_new_note_input: false,
            new_note_path_input: String::new(),
        };

        // Trigger note loading immediately if a notebook path is provided
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
                if self.selected_note_path.is_some() && !self.show_visualizer {
                    self.content.perform(action);
                    self.markdown_text = self.content.text();
                    self.html_output = convert_markdown_to_html(self.markdown_text.clone());

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
                // This message can now also be triggered by initial note loading
                if !self.show_visualizer {
                    self.content = text_editor::Content::with_text(&new_content);
                    self.markdown_text = new_content;
                    self.html_output = convert_markdown_to_html(self.markdown_text.clone());
                }
                Command::none()
            }
            Message::NoteExplorerMessage(note_explorer_message) => {
                eprintln!(
                    "Editor: Received NoteExplorerMessage: {:?}",
                    note_explorer_message
                );
                // First, let the NoteExplorer update its internal state
                let note_explorer_command = self
                    .note_explorer
                    .update(note_explorer_message.clone()) // Clone to inspect later if needed, but the original goes to update
                    .map(Message::NoteExplorerMessage);

                // Then, react based on the specific NoteExplorer message received
                let mut editor_command = Command::none();
                if let note_explorer::Message::NotesLoaded(loaded_notes) = note_explorer_message {
                    eprintln!(
                        "Editor: NoteExplorer finished loading {} notes. Updating editor state.",
                        loaded_notes.len() // Use the notes from the original message for count reporting
                    );
                    // Update the visualizer with the now loaded notes from the NoteExplorer's state
                    self.visualizer.update(visualizer::Message::UpdateNotes(
                        self.note_explorer.notes.clone(), // Use the notes from the NoteExplorer's state
                    ));

                    // If no note was selected and notes were loaded, select the first one
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
                        // If the currently selected note was deleted, clear the editor state
                        eprintln!("Editor: Selected note no longer exists. Clearing editor state.");
                        self.selected_note_path = None;
                        self.selected_note_labels = Vec::new();
                        self.content = text_editor::Content::with_text("");
                        self.markdown_text = String::new();
                        self.html_output = String::new();
                        // No command needed here, just state update
                    } else if let Some(selected_path) = &self.selected_note_path {
                        // If the selected note still exists, ensure labels are up to date
                        if let Some(note) = self
                            .note_explorer
                            .notes
                            .iter()
                            .find(|n| &n.rel_path == selected_path)
                        {
                            self.selected_note_labels = note.labels.clone();
                        }
                    }
                    // If no notes loaded, clear editor state (handled by the else if for selected_note_path)
                }

                // Combine the command from NoteExplorer update and any commands generated in Editor
                Command::batch(vec![note_explorer_command, editor_command])
            }
            Message::NoteSelected(note_path) => {
                eprintln!(
                    "Editor: NoteSelected message received for path: {}",
                    note_path
                );
                self.selected_note_path = Some(note_path.clone());
                self.new_label_text = String::new();

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

                // Load content only if not in visualizer mode
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
                                    String::new() // Return empty content on error
                                }
                            }
                        },
                        Message::ContentChanged,
                    )
                } else {
                    Command::none()
                }
            }
            Message::NewNotebookPathSelected(path_str) => {
                // This message is now only used internally after the initial load.
                // Keep the logic for completeness, although the OpenNotebook button is removed.
                self.notebook_path = path_str.clone();
                self.note_explorer.notebook_path = path_str;

                // Clear editor state and reload notes when a new notebook is selected
                self.content = text_editor::Content::with_text("");
                self.markdown_text = String::new();
                self.html_output = String::new();
                self.selected_note_path = None;
                self.selected_note_labels = Vec::new();
                self.new_label_text = String::new();
                self.show_visualizer = false; // Go back to editor view
                self.visualizer
                    .update(visualizer::Message::UpdateNotes(Vec::new())); // Clear visualizer notes

                if !self.notebook_path.is_empty() {
                    eprintln!("Editor: Loading notes for notebook: {}", self.notebook_path);
                    return self
                        .note_explorer
                        .update(note_explorer::Message::LoadNotes)
                        .map(Message::NoteExplorerMessage);
                }
                Command::none()
            }
            Message::NewLabelInputChanged(text) => {
                self.new_label_text = text;
                Command::none()
            }
            Message::AddLabel => {
                if let Some(selected_path) = &self.selected_note_path {
                    let label = self.new_label_text.trim().to_string();
                    if !label.is_empty() && !self.selected_note_labels.contains(&label) {
                        self.selected_note_labels.push(label.clone());

                        // Update the label in the note_explorer's internal notes list
                        if let Some(note) = self
                            .note_explorer
                            .notes
                            .iter_mut()
                            .find(|n| n.rel_path == *selected_path)
                        {
                            note.labels.push(label);
                        }

                        // Update visualizer with the new label information
                        self.visualizer.update(visualizer::Message::UpdateNotes(
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

                    // Update the label in the note_explorer's internal notes list
                    if let Some(note) = self
                        .note_explorer
                        .notes
                        .iter_mut()
                        .find(|n| n.rel_path == *selected_path)
                    {
                        note.labels.retain(|label| label != &label_to_remove);
                    }

                    // Update visualizer with the removed label information
                    self.visualizer.update(visualizer::Message::UpdateNotes(
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
                // Can only toggle visualizer if a notebook is open
                if !self.notebook_path.is_empty() {
                    self.show_visualizer = !self.show_visualizer;
                    eprintln!("Toggled visualizer visibility to: {}", self.show_visualizer);
                    if self.show_visualizer {
                        self.visualizer.update(visualizer::Message::UpdateNotes(
                            self.note_explorer.notes.clone(),
                        ));
                    }
                } else {
                    eprintln!("Cannot show visualizer: No notebook is open.");
                }
                Command::none()
            }
            Message::VisualizerMessage(visualizer_message) => {
                match visualizer_message {
                    visualizer::Message::UpdateNotes(notes) => {
                        self.visualizer
                            .update(visualizer::Message::UpdateNotes(notes));
                        Command::none()
                    }
                    visualizer::Message::NoteSelectedInVisualizer(note_path) => {
                        eprintln!(
                            "Editor: Received NoteSelectedInVisualizer for path: {}",
                            note_path
                        );
                        // Handle the note selection logic, similar to Message::NoteSelected
                        self.selected_note_path = Some(note_path.clone());
                        self.new_label_text = String::new();

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

                        // Also, switch back to the editor view when a note is selected in the visualizer
                        self.show_visualizer = false;

                        // Load note content if notebook path is available
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
                                            String::new() // Return empty content on error
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
                    Command::none() // Cannot create a note if no notebook is open
                } else {
                    // Show the new note input fields
                    self.show_new_note_input = true;
                    self.new_note_path_input = String::new(); // Clear previous input
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
                    // Hide the new note input fields
                    self.show_new_note_input = false;
                    let notebook_path = self.notebook_path.clone();
                    let mut current_notes = self.note_explorer.notes.clone(); // Clone current notes to pass

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
                // Hide the new note input fields and clear input
                self.show_new_note_input = false;
                self.new_note_path_input = String::new();
                Command::none()
            }
            Message::NoteCreated(result) => {
                match result {
                    Ok(new_note_metadata) => {
                        eprintln!("Note created successfully: {}", new_note_metadata.rel_path);
                        // Reload the notes in NoteExplorer and Visualizer
                        let reload_command = self
                            .note_explorer
                            .update(note_explorer::Message::LoadNotes)
                            .map(Message::NoteExplorerMessage);

                        // Select the newly created note after reloading
                        let select_command = Command::perform(
                            async { new_note_metadata.rel_path },
                            Message::NoteSelected,
                        );

                        Command::batch(vec![reload_command, select_command])
                    }
                    Err(err) => {
                        eprintln!("Failed to create note: {}", err);
                        Command::none()
                    }
                }
            }
            Message::DeleteNote => {
                if let Some(selected_path) = &self.selected_note_path {
                    let note_path_clone = selected_path.clone();
                    // Show confirmation dialog
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
                                .unwrap_or(false) // Handle potential error showing dialog
                        },
                        Message::ConfirmDeleteNote,
                    )
                } else {
                    // Should not happen if button is only shown when a note is selected
                    Command::none()
                }
            }
            Message::ConfirmDeleteNote(confirmed) => {
                if confirmed {
                    if let Some(selected_path) = self.selected_note_path.take() {
                        // Use take() to move ownership
                        let notebook_path_clone = self.notebook_path.clone();
                        let mut current_notes = self.note_explorer.notes.clone(); // Clone for the async block

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
                        Command::none() // No note was selected to delete
                    }
                } else {
                    eprintln!("Note deletion cancelled by user.");
                    Command::none() // User cancelled
                }
            }
            Message::NoteDeleted(result) => {
                match result {
                    Ok(()) => {
                        eprintln!("Note deleted successfully.");
                        // Clear the editor state and reload notes
                        self.selected_note_path = None;
                        self.selected_note_labels = Vec::new();
                        self.content = text_editor::Content::with_text("");
                        self.markdown_text = String::new();
                        self.html_output = String::new();

                        // Reload notes in NoteExplorer and Visualizer
                        self.note_explorer
                            .update(note_explorer::Message::LoadNotes)
                            .map(Message::NoteExplorerMessage)
                    }
                    Err(err) => {
                        eprintln!("Failed to delete note: {}", err);
                        // Maybe show an error dialog here
                        Command::none()
                    }
                }
            }
        }
    }

    fn view(&self) -> Element<'_, Self::Message, Self::Theme> {
        // Remove the "Open Notebook" button
        let mut top_bar = Row::new();

        // Add "New Note" button only if no new note input is currently shown AND a notebook is open
        if !self.show_new_note_input && !self.notebook_path.is_empty() {
            top_bar = top_bar.push(button("New Note").padding(5).on_press(Message::NewNote));
        }

        // Add "Delete Note" button only if a note is selected, not showing new note input, AND a notebook is open
        if self.selected_note_path.is_some()
            && !self.show_new_note_input
            && !self.notebook_path.is_empty()
        {
            top_bar = top_bar.push(
                button("Delete Note")
                    .padding(5)
                    .on_press(Message::DeleteNote),
            );
        }

        // Only show "Show Visualizer" button if a notebook is open
        if !self.notebook_path.is_empty() {
            top_bar = top_bar.push(
                button("Show Visualizer")
                    .padding(5)
                    .on_press(Message::ToggleVisualizer),
            );
        } else {
            top_bar = top_bar.push(Text::new(
                "No notebook opened. Configure 'notebook_path' in config.json",
            ));
        }

        top_bar = top_bar.spacing(10).padding(5).width(Length::Fill);

        let main_content: Element<'_, Self::Message, Self::Theme> = if self.show_visualizer {
            Container::new(self.visualizer.view().map(Message::VisualizerMessage))
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else if self.show_new_note_input {
            // Display input fields for new note creation
            Column::new()
                .push(Text::new("Enter new note name/relative path:"))
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
        } else if self.notebook_path.is_empty() {
            // Show a message if no notebook is open and not creating a new note
            Container::new(
                Text::new("Please configure the 'notebook_path' in your config.json file to open a notebook.")
                    .size(20)
                     // Correctly set the color using style
                    .style(iced::theme::Text::Color(iced::Color::from_rgb(0.7, 0.2, 0.2)))
            )
             .center_x()
             .center_y()
             .width(Length::Fill)
             .height(Length::Fill)
             .into()
        } else {
            // Display the standard editor/preview layout if a notebook is open
            let note_explorer_view: Element<'_, Self::Message, Self::Theme> = Container::new(
                self.note_explorer
                    .view(self.selected_note_path.as_ref())
                    .map(|note_explorer_message| match note_explorer_message {
                        note_explorer::Message::NoteSelected(path) => Message::NoteSelected(path),
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
                Container::new(editor_widget_element).width(Length::FillPortion(4));
            let editor_container_element: Element<'_, Self::Message, Self::Theme> =
                editor_container.into();

            let html_display = Text::new(self.html_output.clone());
            let html_display_scrollable = Scrollable::new(html_display);
            let html_display_element: Element<'_, Self::Message, Self::Theme> =
                html_display_scrollable.into();

            let html_container = Container::new(html_display_element).width(Length::FillPortion(4));
            let html_container_element: Element<'_, Self::Message, Self::Theme> =
                html_container.into();

            let content_row = Row::new()
                .push(note_explorer_view)
                .push(editor_container_element)
                .push(html_container_element)
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
                // If no note is selected, just add an empty text element for spacing consistency or a placeholder
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

fn convert_markdown_to_html(markdown_input: String) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(&markdown_input, options);

    let mut html_output: String = String::new();
    html::push_html(&mut html_output, parser);

    html_output
}
