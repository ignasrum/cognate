use iced::widget::{Column, Container, Row, Scrollable, Text, button, text_editor, text_input};
use iced::{Application, Command, Element, Length, Theme};
use pulldown_cmark::{Options, Parser, html};

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
    OpenNotebook,
    NewNotebookPathSelected(String),
    NewLabelInputChanged(String),
    AddLabel,
    RemoveLabel(String),
    MetadataSaved(Result<(), String>),
    NoteContentSaved(Result<(), String>),
    ToggleVisualizer,
    VisualizerMessage(visualizer::Message), // New message to handle visualizer actions
}

pub struct Editor {
    content: text_editor::Content,
    theme: Theme,
    configuration: Configuration,
    markdown_text: String,
    html_output: String,
    note_explorer: note_explorer::NoteExplorer,
    visualizer: visualizer::Visualizer,
    show_visualizer: bool,
    notebook_path: String,
    selected_note_path: Option<String>,
    selected_note_labels: Vec<String>,
    new_label_text: String,
}

impl Application for Editor {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = Configuration;

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        let initial_text = String::new();
        let mut editor_instance = Editor {
            content: text_editor::Content::with_text(&initial_text),
            theme: local_theme::convert_str_to_theme(flags.theme.clone()),
            configuration: flags,
            markdown_text: String::new(),
            html_output: String::new(),
            note_explorer: note_explorer::NoteExplorer::new("".to_string()),
            visualizer: visualizer::Visualizer::new(),
            show_visualizer: false,
            notebook_path: "".to_string(),
            selected_note_path: None,
            selected_note_labels: Vec::new(),
            new_label_text: String::new(),
        };

        let initial_note_load_command = editor_instance
            .note_explorer
            .update(note_explorer::Message::LoadNotes)
            .map(Message::NoteExplorerMessage);

        let initial_command = Command::batch(vec![initial_note_load_command]);
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
                if !self.show_visualizer {
                    self.content = text_editor::Content::with_text(&new_content);
                    self.markdown_text = new_content;
                    self.html_output = convert_markdown_to_html(self.markdown_text.clone());
                }
                Command::none()
            }
            Message::NoteExplorerMessage(note_explorer_message) => {
                if let note_explorer::Message::NotesLoaded(notes) = note_explorer_message.clone() {
                    eprintln!(
                        "Editor: Received NotesLoaded from NoteExplorer. Clearing editor state."
                    );
                    self.selected_note_path = None;
                    self.selected_note_labels = Vec::new();
                    self.content = text_editor::Content::with_text("");
                    self.markdown_text = String::new();
                    self.html_output = String::new();

                    self.visualizer
                        .update(visualizer::Message::UpdateNotes(notes));
                }

                self.note_explorer
                    .update(note_explorer_message)
                    .map(Message::NoteExplorerMessage)
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

                if !self.show_visualizer {
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
            Message::OpenNotebook => {
                use native_dialog::FileDialog;

                Command::perform(
                    async move {
                        let folder = FileDialog::new().show_open_single_dir().unwrap();
                        folder
                    },
                    |folder: Option<std::path::PathBuf>| {
                        if let Some(path) = folder {
                            let path_str = path.to_string_lossy().to_string();
                            Message::NewNotebookPathSelected(path_str)
                        } else {
                            Message::NewNotebookPathSelected(String::new())
                        }
                    },
                )
            }
            Message::NewNotebookPathSelected(path_str) => {
                self.notebook_path = path_str.clone();
                self.note_explorer.notebook_path = path_str;

                self.content = text_editor::Content::with_text("");
                self.markdown_text = String::new();
                self.html_output = String::new();
                self.selected_note_path = None;
                self.selected_note_labels = Vec::new();
                self.new_label_text = String::new();
                self.show_visualizer = false;
                self.visualizer
                    .update(visualizer::Message::UpdateNotes(Vec::new()));

                if !self.notebook_path.is_empty() {
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

                        if let Some(note) = self
                            .note_explorer
                            .notes
                            .iter_mut()
                            .find(|n| n.rel_path == *selected_path)
                        {
                            note.labels.push(label);
                        }

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

                    if let Some(note) = self
                        .note_explorer
                        .notes
                        .iter_mut()
                        .find(|n| n.rel_path == *selected_path)
                    {
                        note.labels.retain(|label| label != &label_to_remove);
                    }

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
                self.show_visualizer = !self.show_visualizer;
                eprintln!("Toggled visualizer visibility to: {}", self.show_visualizer);
                if self.show_visualizer {
                    self.visualizer.update(visualizer::Message::UpdateNotes(
                        self.note_explorer.notes.clone(),
                    ));
                }
                Command::none()
            }
            Message::VisualizerMessage(visualizer_message) => {
                // Handle messages from the visualizer if it were to emit any
                self.visualizer.update(visualizer_message);
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Self::Message, Self::Theme> {
        let top_bar = Row::new()
            .push(
                button("Open Notebook")
                    .padding(5)
                    .on_press(Message::OpenNotebook),
            )
            .push(
                button("Show Visualizer")
                    .padding(5)
                    .on_press(Message::ToggleVisualizer),
            )
            .spacing(10)
            .padding(5)
            .width(Length::Fill);

        let main_content: Element<'_, Self::Message, Self::Theme> = if self.show_visualizer {
            // Display the visualizer
            Container::new(self.visualizer.view().map(Message::VisualizerMessage)) // Correctly map visualizer messages
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            // Display the standard editor/preview layout
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
                labels_row = labels_row.push(Text::new(""));
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
