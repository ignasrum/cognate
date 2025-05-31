use crate::notebook::NoteMetadata;
use iced::{
    task::Task,
    Element, Length, Theme,
    widget::{Button, Column, Container, Row, Scrollable, Text},
};

// Import correct styling modules
use iced::widget::{button, container};

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub enum Message {
    UpdateNotes(Vec<NoteMetadata>),
    NoteSelectedInVisualizer(String),
    ToggleLabel(String),
}

#[derive(Debug, Default)]
pub struct Visualizer {
    pub notes: Vec<NoteMetadata>,
    pub expanded_labels: HashMap<String, bool>,
}

impl Visualizer {
    pub fn new() -> Self {
        Self {
            notes: Vec::new(),
            expanded_labels: HashMap::new(),
        }
    }

    // Update method signatures
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::UpdateNotes(notes) => {
                #[cfg(debug_assertions)]
                eprintln!(
                    "Visualizer: Received UpdateNotes message with {} notes.",
                    notes.len()
                );
                self.notes = notes;
                // Update expanded_labels to include new labels, keeping existing state
                let mut all_labels: HashSet<String> = HashSet::new();
                for note in &self.notes {
                    for label in &note.labels {
                        all_labels.insert(label.clone());
                    }
                }
                let mut new_expanded_labels = HashMap::new();
                for label in all_labels {
                    // Change default from true to false here
                    let is_expanded = *self.expanded_labels.get(&label).unwrap_or(&false);
                    new_expanded_labels.insert(label, is_expanded);
                }
                self.expanded_labels = new_expanded_labels;

                Task::none()
            }
            Message::NoteSelectedInVisualizer(_path) => Task::none(),
            Message::ToggleLabel(label) => {
                if let Some(is_expanded) = self.expanded_labels.get_mut(&label) {
                    *is_expanded = !*is_expanded;
                    #[cfg(debug_assertions)]
                    eprintln!("Toggled label '{}' to expanded: {}", label, *is_expanded);
                } else {
                    #[cfg(debug_assertions)]
                    eprintln!("Attempted to toggle non-existent label: {}", label);
                }
                Task::none()
            }
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
                no_label_column = no_label_column.push(
                    Text::new("No Labels:")
                        .size(18)
                        .style(|_: &_| iced::widget::text::Style {
                            color: Some(iced::Color::from_rgb(0.7, 0.7, 0.7)),
                            ..Default::default()
                        }),
                );

                let mut sorted_notes_without_labels = notes_without_labels.clone();
                sorted_notes_without_labels.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

                for note in sorted_notes_without_labels {
                    // Use rel_path instead of file_name()
                    let note_path = note.rel_path.clone();

                    let note_button = Button::new(Text::new(format!("- {}", note_path)).size(16))
                        .on_press(Message::NoteSelectedInVisualizer(note.rel_path.clone()))
                        .style(button::text); // Use button styling function

                    no_label_column = no_label_column.push(note_button);
                }
                content = content.push(
                    Container::new(no_label_column)
                        .style(|theme| container::Style {
                            background: Some(iced::Background::Color(theme.palette().background)),
                            border: iced::Border {
                                radius: 2.0.into(),
                                width: 1.0,
                                color: theme.palette().primary,
                            },
                            // No text_color field is needed
                            ..container::Style::default()
                        })
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
                    let is_expanded = *self.expanded_labels.get(&label).unwrap_or(&false); // Default to collapsed

                    let mut label_header_row = Row::new().spacing(5).align_y(iced::Alignment::Center);
                    let indicator = if is_expanded { 'v' } else { '>' };

                    label_header_row = label_header_row.push(
                        Button::new(
                            Text::new(format!("{} {}", indicator, label))
                                .size(20)
                                .style(|_: &_| iced::widget::text::Style {
                                    color: Some(iced::Color::from_rgb(0.0, 0.9, 1.0)),
                                    ..Default::default()
                                })
                                .shaping(iced::widget::text::Shaping::Advanced),
                        )
                        .on_press(Message::ToggleLabel(label.clone()))
                        .style(button::text), // Use button styling function
                    );

                    let mut label_column = Column::new().spacing(5).push(label_header_row);

                    if is_expanded {
                        let mut sorted_notes_with_label = notes_with_label.clone();
                        sorted_notes_with_label.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

                        for note in sorted_notes_with_label {
                            // Use rel_path instead of file_name()
                            let note_path = note.rel_path.clone();

                            let note_button = Button::new(Text::new(format!("- {}", note_path)).size(16))
                                .on_press(Message::NoteSelectedInVisualizer(note.rel_path.clone()))
                                .style(button::text); // Use button styling function

                            label_column = label_column.push(note_button);
                        }
                    }

                    content = content.push(
                        Container::new(label_column)
                            .style(|theme| container::Style {
                                background: Some(iced::Background::Color(theme.palette().background)),
                                border: iced::Border {
                                    radius: 2.0.into(),
                                    width: 1.0,
                                    color: theme.palette().primary,
                                },
                                ..container::Style::default()
                            })
                            .padding(5)
                            .width(Length::Fill),
                    );
                }
            }
        }

        let padded_content = content.padding(20);

        Scrollable::new(padded_content).into()
    }
}
