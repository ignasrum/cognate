use crate::notebook::NoteMetadata;
use iced::{
    Element, Length, Theme,
    widget::{Column, Container, Row, Scrollable, Text},
}; // Import necessary widgets
use std::collections::HashSet; // To easily get unique connected notes

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
                // Find notes connected to the current note via shared labels
                let mut connected_note_paths = HashSet::new();
                for label in &note.labels {
                    for other_note in &self.notes {
                        // Ensure it's a different note and it shares the label
                        if other_note.rel_path != note.rel_path && other_note.labels.contains(label)
                        {
                            connected_note_paths.insert(other_note.rel_path.clone());
                        }
                    }
                }

                // Create the labels element separately to help with type inference
                let labels_element: Element<'_, Message, Theme> = if note.labels.is_empty() {
                    Text::new("None").into()
                } else {
                    Text::new(note.labels.join(", ")).into()
                };

                // Create the connected notes element
                let connected_notes_element: Element<'_, Message, Theme> =
                    if connected_note_paths.is_empty() {
                        Text::new("None").into()
                    } else {
                        // Convert the HashSet to a sorted Vec for consistent display
                        let mut sorted_connected_notes: Vec<_> =
                            connected_note_paths.into_iter().collect();
                        sorted_connected_notes.sort();
                        Text::new(sorted_connected_notes.join(", ")).into()
                    };

                let note_element = Column::new()
                    .push(Text::new(format!("Path: {}", note.rel_path)))
                    .push(
                        Row::new()
                            .push(Text::new("Labels: "))
                            .push(labels_element) // Push the explicitly typed element
                            .spacing(5),
                    )
                    .push(
                        // Add the connected notes row
                        Row::new()
                            .push(Text::new("Connected to: "))
                            .push(connected_notes_element)
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
