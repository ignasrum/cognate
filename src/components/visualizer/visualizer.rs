use crate::notebook::NoteMetadata;
use iced::{
    Element, Length, Theme,
    widget::{Button, Column, Container, Scrollable, Text},
};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone)]
pub enum Message {
    UpdateNotes(Vec<NoteMetadata>),   // Message to update notes data
    NoteSelectedInVisualizer(String), // New message when a note is clicked in the visualizer
}

#[derive(Debug, Default)]
pub struct Visualizer {
    pub notes: Vec<NoteMetadata>,
}

impl Visualizer {
    pub fn new() -> Self {
        Self { notes: Vec::new() }
    }

    pub fn update(&mut self, message: Message) -> iced::Command<Message> {
        match message {
            Message::UpdateNotes(notes) => {
                #[cfg(debug_assertions)]
                eprintln!(
                    "Visualizer: Received UpdateNotes message with {} notes.",
                    notes.len()
                );
                self.notes = notes;
                iced::Command::none()
            }
            Message::NoteSelectedInVisualizer(_path) => iced::Command::none(),
        }
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
                ));

                let mut sorted_notes_without_labels = notes_without_labels.clone();
                sorted_notes_without_labels.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

                for note in &sorted_notes_without_labels {
                    let note_name = Path::new(&note.rel_path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();

                    let note_button = Button::new(Text::new(format!("- {}", note_name)).size(16))
                        .on_press(Message::NoteSelectedInVisualizer(note.rel_path.clone()))
                        .style(iced::theme::Button::Text);

                    no_label_column = no_label_column.push(note_button);
                }
                content = content.push(
                    Container::new(no_label_column)
                        .style(iced::theme::Container::Box)
                        .padding(5)
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
                        ));

                    let mut sorted_notes_with_label = notes_with_label.clone();
                    sorted_notes_with_label.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

                    for note in sorted_notes_with_label {
                        let note_name = Path::new(&note.rel_path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();

                        let note_button =
                            Button::new(Text::new(format!("- {}", note_name)).size(16))
                                .on_press(Message::NoteSelectedInVisualizer(note.rel_path.clone()))
                                .style(iced::theme::Button::Text);

                        label_column = label_column.push(note_button);
                    }

                    content = content.push(
                        Container::new(label_column)
                            .style(iced::theme::Container::Box)
                            .padding(5)
                            .width(Length::Fill),
                    );
                }
            }
        }

        let padded_content = content.padding(20); // Increased padding from 10 to 20

        Scrollable::new(padded_content).into()
    }
}
