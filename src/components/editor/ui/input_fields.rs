use iced::widget::{Button, Row, Text, text_input, button};
use iced::Length;

use crate::components::editor::Message;

// Create the labels section
pub fn create_labels_section<'a>(
    selected_note_path: Option<&String>,
    selected_labels: &[String],
    new_label_text: &str,
) -> Row<'a, Message> {
    let mut labels_row = Row::new().spacing(10).padding(5).width(Length::Fill);

    if selected_note_path.is_some() {
        labels_row = labels_row.push(Text::new("Labels: "));
        if selected_labels.is_empty() {
            labels_row = labels_row.push(Text::new("No labels"));
        } else {
            for label in selected_labels {
                labels_row = labels_row.push(
                    button(Text::new(label.clone()))
                        .on_press(Message::RemoveLabel(label.clone())),
                );
            }
        }

        labels_row = labels_row
            .push(
                text_input("New Label", new_label_text)
                    .on_input(Message::NewLabelInputChanged)
                    .on_submit(Message::AddLabel)
                    .width(Length::Fixed(150.0)),
            )
            .push(Button::new(Text::new("Add Label")).padding(5).on_press(Message::AddLabel));
    } else {
        labels_row = labels_row.push(Text::new("Select a note to manage labels."));
    }

    labels_row
}
