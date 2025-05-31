use iced::widget::{Column, Container, Row, Text, TextInput as IcedTextInput, button};
use iced::{Element, Length};

use crate::components::editor::Message;

// About dialog
pub fn about_dialog<'a>(app_version: &str) -> Element<'a, Message> {
    let about_info_column = Column::new()
        .spacing(10)
        .align_items(iced::Alignment::Center)
        .push(Text::new("Cognate - Note Taking App").size(30))
        .push(Text::new(format!("Version: {}", app_version)).size(20));

    Container::new(about_info_column)
        .center_x()
        .center_y()
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

// New note dialog
pub fn new_note_dialog<'a>(new_note_path_input: &str) -> Element<'a, Message> {
    Column::new()
        .push(Text::new(
            "Enter new note name/relative path (e.g., folder/note_name):",
        ))
        .push(
            IcedTextInput::new("Note name...", new_note_path_input)
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
}

// Move note dialog
pub fn move_note_dialog<'a>(current_path: &str, new_path_input: &str, is_folder: bool) -> Element<'a, Message> {
    let prompt_text = if is_folder {
        format!(
            "Enter new relative path/name for folder '{}':",
            current_path
        )
    } else {
        format!(
            "Enter new relative path/name for note '{}':",
            current_path
        )
    };

    Column::new()
        .push(Text::new(prompt_text))
        .push(
            IcedTextInput::new("New path...", new_path_input)
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
}
