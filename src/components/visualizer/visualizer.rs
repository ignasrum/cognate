use crate::notebook::NoteMetadata;
use iced::{
    Element, Length, Theme,
    widget::{Column, Container, Row, Scrollable, Text},
};
use std::collections::{HashMap, HashSet}; // Import HashMap and HashSet

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
                // No complex preprocessing needed here, grouping happens in view
            } // Handle other messages if defined
              // Message::SelectNote(path) => { /* logic for selecting a note */ }
        }
        iced::Command::none()
    }

    pub fn view(&self) -> Element<'_, Message, Theme> {
        let mut content = Column::new().spacing(10);

        if self.notes.is_empty() {
            content = content.push(Text::new(
                "No notes available for visualization. Open a notebook first.",
            ));
        } else {
            content = content.push(Text::new("Notes Grouped by Label:"));

            // Group notes by label
            let mut notes_by_label: HashMap<String, Vec<NoteMetadata>> = HashMap::new();
            let mut notes_without_labels: Vec<NoteMetadata> = Vec::new();
            let mut all_labels: HashSet<String> = HashSet::new();

            for note in &self.notes {
                if note.labels.is_empty() {
                    notes_without_labels.push(note.clone());
                } else {
                    for label in &note.labels {
                        notes_by_label
                            .entry(label.clone())
                            .or_insert_with(Vec::new)
                            .push(note.clone());
                        all_labels.insert(label.clone());
                    }
                }
            }

            // Display notes without labels first
            if !notes_without_labels.is_empty() {
                let mut no_label_column = Column::new().spacing(5);
                no_label_column = no_label_column.push(Text::new("No Labels:").size(18).style(
                    iced::theme::Text::Color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                )); // Slightly greyed out title

                for note in &notes_without_labels {
                    no_label_column =
                        no_label_column.push(Text::new(format!("- {}", note.rel_path)));
                }
                content = content.push(
                    Container::new(no_label_column)
                        .style(iced::theme::Container::Box)
                        .padding(10)
                        .width(Length::Fill),
                );
            }

            // Sort labels for consistent display
            let mut sorted_labels: Vec<String> = all_labels.into_iter().collect();
            sorted_labels.sort();

            // Display notes grouped by label
            for label in sorted_labels {
                if let Some(notes_with_label) = notes_by_label.get(&label) {
                    let mut label_column = Column::new().spacing(5);
                    label_column =
                        label_column.push(Text::new(format!("Label: {}", label)).size(20).style(
                            iced::theme::Text::Color(iced::Color::from_rgb(0.1, 0.5, 0.9)),
                        )); // Highlight label

                    for note in notes_with_label {
                        label_column = label_column.push(Text::new(format!("- {}", note.rel_path)));
                    }

                    content = content.push(
                        Container::new(label_column)
                            .style(iced::theme::Container::Box)
                            .padding(10)
                            .width(Length::Fill),
                    );
                }
            }
        }

        // Apply padding to the content column
        let padded_content = content.padding(10);

        // Wrap the padded content in a scrollable widget
        Scrollable::new(padded_content).into()
    }
}
