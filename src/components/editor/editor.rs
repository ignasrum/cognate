use iced::widget::{Container, Text, row, text_editor};
use iced::{Application, Command, Element, Length, Theme};
use pulldown_cmark::{Options, Parser, html};

use crate::configuration::Configuration;

#[path = "../../configuration/theme.rs"]
mod local_theme;

#[derive(Debug, Clone)]
pub enum Message {
    Edit(text_editor::Action),
    ContentChanged(String),
}

pub struct Editor {
    content: text_editor::Content,
    theme: Theme,
    configuration: Configuration,
    markdown_text: String,
    html_output: String,
}

impl Application for Editor {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme; // iced::Theme
    type Flags = Configuration; // This is how you pass your config

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        // `flags` is the Configuration instance passed from main.rs
        let editor_instance = Editor {
            content: text_editor::Content::with_text("Type something here..."),
            theme: local_theme::convert_str_to_theme(flags.theme.clone()),
            configuration: flags,
            markdown_text: String::new(),
            html_output: String::new(),
        };
        (editor_instance, Command::none())
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
        }
        Command::none()
    }

    fn view(&self) -> Element<'_, Self::Message, Self::Theme> {
        let editor_widget = text_editor(&self.content)
            .on_action(Message::Edit)
            .height(Length::Fill);

        // Display the HTML output
        let html_display = Text::new(self.html_output.clone());

        let content_row = row![editor_widget, html_display]
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
