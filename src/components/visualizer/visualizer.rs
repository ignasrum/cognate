use crate::notebook::NoteMetadata;
use iced::{
    Element, Length, Theme,
    widget::{Column, Container, Row, Scrollable, Text},
}; // Import necessary widgets

#[derive(Debug, Default)]
pub struct Visualizer {
    pub notes: Vec<NoteMetadata>, // Field to hold note metadata
}

#[derive(Debug, Clone)]
pub enum Message {
    UpdateNotes(Vec<NoteMetadata>), // Message to update notes data
                                    // Add other messages here if you want to add interaction, e.g.,
                                    // SelectNote(String),
}

impl Visualizer {
    pub fn new() -> Self {
        Self {
            notes: Vec::new(), // Initialize with empty notes
        }
    }

    pub fn update(&mut self, message: Message) -> iced::Command<Message> {
        match message {
            Message::UpdateNotes(notes) => {
                eprintln!(
                    "Visualizer: Received UpdateNotes message with {} notes.",
                    notes.len()
                );
                self.notes = notes;
                // In a real implementation, you might need to process notes for visualization layout here
            } // Handle other messages if defined
              // Message::SelectNote(path) => { /* logic for selecting a note */ }
        }
        iced::Command::none()
    }

    pub fn view(&self) -> Element<'_, Message, Theme> {
        let mut notes_list = Column::new().spacing(10);

        if self.notes.is_empty() {
            notes_list = notes_list.push(Text::new(
                "No notes available for visualization. Open a notebook first.",
            ));
        } else {
            notes_list = notes_list.push(Text::new("Notes in Notebook:"));
            for note in &self.notes {
                // Create the labels element separately to help with type inference
                let labels_element: Element<'_, Message, Theme> = if note.labels.is_empty() {
                    Text::new("None").into()
                } else {
                    Text::new(note.labels.join(", ")).into()
                };

                let note_element = Column::new()
                    .push(Text::new(format!("Path: {}", note.rel_path)))
                    .push(
                        Row::new()
                            .push(Text::new("Labels: "))
                            .push(labels_element) // Push the explicitly typed element
                            .spacing(5),
                    )
                    .spacing(5)
                    .padding(5)
                    .width(Length::Fill);

                notes_list = notes_list.push(
                    Container::new(note_element)
                        // Add some styling or borders to differentiate notes visually
                        .style(iced::theme::Container::Box)
                        .width(Length::Fill),
                );
            }
        }

        // Apply padding to the column before wrapping it in a scrollable widget
        let padded_notes_list = notes_list.padding(10);

        // Wrap the padded content in a scrollable widget
        Scrollable::new(padded_notes_list).into()
    }
}
