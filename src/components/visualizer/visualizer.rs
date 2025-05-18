use crate::notebook::NoteMetadata;
use iced::{
    Element, Length, Theme,
    widget::{Button, Column, Container, Row, Scrollable, Text},
}; // Import Button
use std::collections::{HashMap, HashSet}; // Import HashMap and HashSet
use std::path::Path; // Import Path

#[derive(Debug, Clone)]
pub enum Message {
    UpdateNotes(Vec<NoteMetadata>),   // Message to update notes data
    NoteSelectedInVisualizer(String), // New message when a note is clicked in the visualizer
}

#[derive(Debug, Default)]
pub struct Visualizer {
    pub notes: Vec<NoteMetadata>, // Field to hold note metadata
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
                iced::Command::none() // Return a command here
            }
            Message::NoteSelectedInVisualizer(_path) => {
                // This message is primarily handled by the parent (Editor)
                iced::Command::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message, Theme> {
        let mut content = Column::new().spacing(10); // Keep 10px spacing between containers

        if self.notes.is_empty() {
            content = content.push(Text::new(
                "No notes available for visualization. Open a notebook first.",
            ));
        } else {
            content = content.push(Text::new("Notes Grouped by Label:"));

            // Group notes by label
            let mut notes_by_label: HashMap<String, Vec<&NoteMetadata>> = HashMap::new();
            let mut notes_without_labels: Vec<&NoteMetadata> = Vec::new();
            let mut all_labels: HashSet<String> = HashSet::new();

            for note in &self.notes {
                if note.labels.is_empty() {
                    notes_without_labels.push(note);
                } else {
                    for label in &note.labels {
                        notes_by_label
                            .entry(label.clone())
                            .or_insert_with(Vec::new)
                            .push(note);
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

                // Sort notes without labels by rel_path
                let mut sorted_notes_without_labels = notes_without_labels.clone();
                sorted_notes_without_labels.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

                for note in &sorted_notes_without_labels {
                    // Wrap note text in a button
                    let note_button = Button::new(
                        Text::new(format!("- {}", note.rel_path)).size(16),
                    ) // Slightly smaller text for notes
                    .on_press(Message::NoteSelectedInVisualizer(note.rel_path.clone()))
                    .style(iced::theme::Button::Text); // Use Text style to make it look like plain text initially

                    no_label_column = no_label_column.push(note_button);
                }
                content = content.push(
                    Container::new(no_label_column)
                        .style(iced::theme::Container::Box)
                        .padding(5) // Reduced padding inside the container
                        .width(Length::Fill),
                );
                // Removed the extra Space widget here, relying on column spacing
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

                    // Sort notes within the label by rel_path
                    let mut sorted_notes_with_label = notes_with_label.clone();
                    sorted_notes_with_label.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

                    for note in sorted_notes_with_label {
                        // Wrap note text in a button
                        // Extract just the note name (file name without the folder path)
                        let note_name = Path::new(&note.rel_path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();

                        let note_button = Button::new(
                            Text::new(format!("- {}", note_name)).size(16),
                        ) // Indent notes and use smaller text
                        .on_press(Message::NoteSelectedInVisualizer(note.rel_path.clone()))
                        .style(iced::theme::Button::Text); // Use Text style

                        label_column = label_column.push(note_button);
                    }

                    content = content.push(
                        Container::new(label_column)
                            .style(iced::theme::Container::Box)
                            .padding(5) // Reduced padding inside the container
                            .width(Length::Fill),
                    );
                    // Removed the extra Space widget here, relying on column spacing
                }
            }
        }

        // Apply uniform padding to the content column (padding between scrollable edge and content)
        let padded_content = content.padding(10); // 10px padding on all sides of the scrollable content area

        // Wrap the padded content in a scrollable widget
        Scrollable::new(padded_content).into()
    }
}
