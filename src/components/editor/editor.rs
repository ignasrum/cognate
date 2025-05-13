use iced::widget::{Container, Row, Text, row, text_editor};
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
    NoteSelected(String),
}

pub struct Editor {
    content: text_editor::Content,
    theme: Theme,
    configuration: Configuration,
    markdown_text: String,
    html_output: String,
    note_explorer: note_explorer::NoteExplorer,
}

impl Application for Editor {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme; // iced::Theme
    type Flags = Configuration; // This is how you pass your config

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        // `flags` is the Configuration instance passed from main.rs
        let initial_text = "Type something here...".to_string();
        let editor_instance = Editor {
            content: text_editor::Content::with_text(&initial_text),
            theme: local_theme::convert_str_to_theme(flags.theme.clone()),
            configuration: flags,
            markdown_text: String::new(),
            html_output: String::new(),
            note_explorer: note_explorer::NoteExplorer::new(),
        };
        let initial_command = Command::batch(vec![
            Command::perform(async { initial_text }, Message::ContentChanged),
            Command::perform(async {}, |_| {
                Message::NoteExplorerMessage(note_explorer::Message::LoadNotes)
            }),
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
            }
            Message::ContentChanged(new_content) => {
                self.content = text_editor::Content::with_text(&new_content);
                self.markdown_text = new_content;
                self.html_output = convert_markdown_to_html(self.markdown_text.clone());
            }
            Message::NoteExplorerMessage(note_explorer_message) => {
                self.note_explorer.update(note_explorer_message);
            }
            Message::NoteSelected(note_path) => {
                let full_note_path = format!("example_notebook/{}/note1.md", note_path); // Assuming note1.md inside each directory
                let load_command = Command::perform(
                    async {
                        match std::fs::read_to_string(full_note_path) {
                            Ok(content) => content,
                            Err(err) => {
                                eprintln!("Failed to read note file: {}", err);
                                String::new() // Return an empty string on error
                            }
                        }
                    },
                    Message::ContentChanged,
                );
                return load_command;
            }
        }
        Command::none()
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

        // Display the HTML output
        let html_display = Text::new(self.html_output.clone()).width(Length::FillPortion(4));
        let html_display_element: Element<'_, Self::Message, Self::Theme> = html_display.into();

        let content_row = Row::new()
            .push(note_explorer_view)
            .push(editor_container_element)
            .push(html_display_element)
            .spacing(10)
            .padding(10)
            .width(Length::Fill)
            .height(Length::Fill);

        Container::new(content_row)
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
