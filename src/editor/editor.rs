use iced::widget::{Container, button, column, text_editor};
use iced::{Element, Sandbox, Settings, Theme};

#[derive(Default)]
pub struct Editor {
    content: text_editor::Content,
    theme: Theme,
}

#[derive(Debug, Clone)]
pub enum Message {
    Edit(text_editor::Action),
    ThemeChanged(Theme),
}

impl Sandbox for Editor {
    type Message = Message;

    fn new() -> Self {
        Editor {
            content: text_editor::Content::with_text("Type something here..."),
            theme: Theme::Light,
        }
    }

    fn title(&self) -> String {
        String::from("Simple Text Editor - Iced")
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Edit(action) => {
                self.content.perform(action);
            }
            Message::ThemeChanged(theme) => {
                self.theme = theme;
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let controls = column![
            button("Light Theme").on_press(Message::ThemeChanged(Theme::Light)),
            button("Dark Theme").on_press(Message::ThemeChanged(Theme::Dark)),
        ]
        .spacing(5);

        let editor_widget = text_editor(&self.content)
            .on_action(Message::Edit)
            .height(iced::Length::Fill);

        let content_column = column![controls, editor_widget,].spacing(10).padding(10);

        Container::new(content_column)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .into()
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }
}

impl Editor {
    pub fn run(settings: Settings<()>) -> iced::Result {
        <Self as Sandbox>::run(settings)
    }
}
