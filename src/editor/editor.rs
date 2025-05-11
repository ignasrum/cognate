use iced::widget::{Container, column, text_editor};
use iced::{Application, Command, Element, Theme};

use crate::configuration::Configuration;

#[path = "../configuration/theme.rs"]
mod local_theme;

#[derive(Debug, Clone)]
pub enum Message {
    Edit(text_editor::Action),
}

pub struct Editor {
    content: text_editor::Content,
    theme: Theme,
    configuration: Configuration,
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
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<'_, Self::Message, Self::Theme> {
        let editor_widget = text_editor(&self.content)
            .on_action(Message::Edit)
            .height(iced::Length::Fill);

        let content_column = column![editor_widget].spacing(10).padding(10);

        Container::new(content_column)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .into()
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }
}
