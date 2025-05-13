use iced::widget::{Button, Column, Container, Row, Scrollable, Text, button, text_editor};
use iced::{Application, Command, Element, Length, Theme};
use pulldown_cmark::{Options, Parser, html};

use crate::configuration::Configuration;

#[path = "../../configuration/theme.rs"]
mod local_theme;
#[path = "../note_explorer/note_explorer.rs"]
mod note_explorer;

#[derive(Debug, Clone)]
pub enum Message {
    Edit(text_editor::Action),
    ContentChanged(String),
    NoteExplorerMessage(note_explorer::Message),
    NoteSelected(String), // This message is now sent from NoteExplorer to Editor
    OpenNotebook,
    NewNotebookPathSelected(String),
}

pub struct Editor {
    content: text_editor::Content,
    theme: Theme,
    configuration: Configuration,
    markdown_text: String,
    html_output: String,
    note_explorer: note_explorer::NoteExplorer,
    notebook_path: String,
}

impl Application for Editor {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme; // iced::Theme
    type Flags = Configuration; // This is how you pass your config

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        // `flags` is the Configuration instance passed from main.rs
        let initial_text = "Type something here...".to_string();
        let mut editor_instance = Editor {
            content: text_editor::Content::with_text(&initial_text),
            theme: local_theme::convert_str_to_theme(flags.theme.clone()),
            configuration: flags,
            markdown_text: String::new(),
            html_output: String::new(),
            note_explorer: note_explorer::NoteExplorer::new("example_notebook".to_string()),
            notebook_path: "example_notebook".to_string(), // Default notebook path
        };

        // Load initial notes using the command from note_explorer
        let initial_note_load_command = editor_instance
            .note_explorer
            .update(note_explorer::Message::LoadNotes)
            .map(Message::NoteExplorerMessage);

        let initial_command = Command::batch(vec![
            Command::perform(async { initial_text }, Message::ContentChanged),
            initial_note_load_command,
        ]);
        (editor_instance, initial_command)
    }

    fn title(&self) -> String {
        String::from("Configured Text Editor")
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::Edit(action) => {
                self.content.perform(action);
                self.markdown_text = self.content.text();
                self.html_output = convert_markdown_to_html(self.markdown_text.clone());
                Command::none()
            }
            Message::ContentChanged(new_content) => {
                self.content = text_editor::Content::with_text(&new_content);
                self.markdown_text = new_content;
                self.html_output = convert_markdown_to_html(self.markdown_text.clone());
                Command::none()
            }
            Message::NoteExplorerMessage(note_explorer_message) => {
                // Propagate the update to the note explorer and return its command
                self.note_explorer
                    .update(note_explorer_message)
                    .map(Message::NoteExplorerMessage)
            }
            Message::NoteSelected(note_path) => {
                eprintln!(
                    "Editor: NoteSelected message received for path: {}",
                    note_path
                );
                let notebook_path_clone_editor = self.notebook_path.clone(); // Clone for the first async block
                let notebook_path_clone_explorer = self.notebook_path.clone(); // Clone for the second async block
                let note_path_clone_editor = note_path.clone(); // Clone for the first async block
                let note_path_clone_explorer = note_path.clone(); // Clone for the second async block

                Command::batch(vec![
                    // Load content into the main editor
                    Command::perform(
                        async move {
                            let full_note_path = format!(
                                "{}/{}/{}.md",
                                notebook_path_clone_editor,
                                note_path_clone_editor,
                                note_path_clone_editor
                            );
                            match std::fs::read_to_string(full_note_path) {
                                Ok(content) => content,
                                Err(err) => {
                                    eprintln!("Failed to read note file for editor: {}", err);
                                    String::new() // Return an empty string on error
                                }
                            }
                        },
                        Message::ContentChanged,
                    ),
                    // Load content and display it in the note explorer area
                    Command::perform(
                        async move {
                            let full_note_path_for_explorer = format!(
                                "{}/{}/{}.md",
                                notebook_path_clone_explorer,
                                note_path_clone_explorer,
                                note_path_clone_explorer
                            );
                            match std::fs::read_to_string(full_note_path_for_explorer) {
                                Ok(content) => content,
                                Err(err) => {
                                    eprintln!("Failed to read note file for explorer: {}", err);
                                    String::from("Error loading content.") // Indicate error
                                }
                            }
                        },
                        |content| {
                            Message::NoteExplorerMessage(note_explorer::Message::DisplayContent(
                                content,
                            ))
                        },
                    ),
                ])
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
                            // User cancelled the dialog - tell note explorer to show the list
                            Message::NoteExplorerMessage(note_explorer::Message::ShowList)
                        }
                    },
                )
            }
            Message::NewNotebookPathSelected(path_str) => {
                // Update the notebook path and trigger a load of notes in the explorer
                self.notebook_path = path_str.clone();
                self.note_explorer.notebook_path = path_str;
                // The LoadNotes message in NoteExplorer will automatically switch to list view
                self.note_explorer
                    .update(note_explorer::Message::LoadNotes)
                    .map(Message::NoteExplorerMessage)
            }
        }
    }

    fn view(&self) -> Element<'_, Self::Message, Self::Theme> {
        let note_explorer_view: Element<'_, Self::Message, Self::Theme> = Container::new(
            self.note_explorer
                .view()
                .map(|note_explorer_message| Message::NoteExplorerMessage(note_explorer_message)),
        )
        .width(Length::FillPortion(2))
        .into();

        let editor_widget = text_editor(&self.content)
            .on_action(Message::Edit)
            .height(Length::Fill);
        let editor_widget_element: Element<'_, Self::Message, Self::Theme> = editor_widget.into();

        let editor_container = Container::new(editor_widget_element).width(Length::FillPortion(4));
        let editor_container_element: Element<'_, Self::Message, Self::Theme> =
            editor_container.into();

        // Display the HTML output in a scrollable area
        let html_display = Text::new(self.html_output.clone());
        let html_display_scrollable = Scrollable::new(html_display);
        let html_display_element: Element<'_, Self::Message, Self::Theme> =
            html_display_scrollable.into();

        let html_container = Container::new(html_display_element).width(Length::FillPortion(4));
        let html_container_element: Element<'_, Self::Message, Self::Theme> = html_container.into();

        // Create a top bar with an "Open Notebook" button
        let top_bar = Row::new()
            .push(
                button("Open Notebook")
                    .padding(5)
                    .on_press(Message::OpenNotebook),
            )
            .spacing(10)
            .padding(5)
            .width(Length::Fill);

        let content_row = Row::new()
            .push(note_explorer_view) // Note Explorer is now on the left
            .push(editor_container_element) // Editor is in the middle
            .push(html_container_element) // HTML preview is on the right
            .spacing(10)
            .padding(10)
            .width(Length::Fill)
            .height(Length::Fill);

        let main_content = Column::new().push(top_bar).push(content_row);

        Container::new(main_content)
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

    // Write to String buffer.
    let mut html_output: String = String::new();
    html::push_html(&mut html_output, parser);

    html_output
}
