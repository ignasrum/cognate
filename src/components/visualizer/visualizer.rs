use crate::notebook::NoteMetadata;
use iced::{
    task::Task,
    Element, Length, Theme,
    widget::{Button, Column, Container, Row, Scrollable, Text},
};

// Import correct styling modules
use iced::widget::{button, container};

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub enum Message {
    UpdateNotes(Vec<NoteMetadata>),
    NoteSelectedInVisualizer(String),
    ToggleLabel(String),
}

#[derive(Debug, Clone, Default)]
struct LabelGroup {
    label: String,
    note_paths: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct GroupedNotesCache {
    notes_without_labels: Vec<String>,
    labels: Vec<LabelGroup>,
}

#[derive(Debug, Default)]
pub struct Visualizer {
    pub notes: Vec<NoteMetadata>,
    pub expanded_labels: HashMap<String, bool>,
    grouped_notes_cache: RefCell<GroupedNotesCache>,
    grouped_notes_cache_key: Cell<Option<u64>>,
}

impl Visualizer {
    pub fn new() -> Self {
        Self {
            notes: Vec::new(),
            expanded_labels: HashMap::new(),
            grouped_notes_cache: RefCell::new(GroupedNotesCache::default()),
            grouped_notes_cache_key: Cell::new(None),
        }
    }

    fn compute_notes_cache_key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();

        for note in &self.notes {
            note.rel_path.hash(&mut hasher);
            for label in &note.labels {
                label.hash(&mut hasher);
            }
        }

        hasher.finish()
    }

    fn build_grouped_notes_cache(notes: &[NoteMetadata]) -> GroupedNotesCache {
        let mut notes_by_label: HashMap<String, Vec<String>> = HashMap::new();
        let mut notes_without_labels: Vec<String> = Vec::new();

        for note in notes {
            if note.labels.is_empty() {
                notes_without_labels.push(note.rel_path.clone());
            } else {
                for label in &note.labels {
                    notes_by_label
                        .entry(label.clone())
                        .or_default()
                        .push(note.rel_path.clone());
                }
            }
        }

        notes_without_labels.sort();

        let mut sorted_labels: Vec<String> = notes_by_label.keys().cloned().collect();
        sorted_labels.sort();

        let mut labels = Vec::with_capacity(sorted_labels.len());
        for label in sorted_labels {
            if let Some(note_paths) = notes_by_label.get_mut(&label) {
                note_paths.sort();
                labels.push(LabelGroup {
                    label,
                    note_paths: note_paths.clone(),
                });
            }
        }

        GroupedNotesCache {
            notes_without_labels,
            labels,
        }
    }

    fn refresh_grouped_notes_cache_if_needed(&self) {
        let cache_key = self.compute_notes_cache_key();
        if self.grouped_notes_cache_key.get() == Some(cache_key) {
            return;
        }

        let grouped = Self::build_grouped_notes_cache(&self.notes);
        *self.grouped_notes_cache.borrow_mut() = grouped;
        self.grouped_notes_cache_key.set(Some(cache_key));
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
                self.refresh_grouped_notes_cache_if_needed();

                // Update expanded_labels to include new labels, keeping existing state.
                let mut new_expanded_labels = HashMap::new();
                for group in &self.grouped_notes_cache.borrow().labels {
                    let is_expanded = *self
                        .expanded_labels
                        .get(&group.label)
                        .unwrap_or(&false);
                    new_expanded_labels.insert(group.label.clone(), is_expanded);
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
        self.refresh_grouped_notes_cache_if_needed();

        let mut content = Column::new().spacing(10);

        if self.notes.is_empty() {
            content = content.push(Text::new(
                "No notes available for visualization. Open a notebook first.",
            ));
        } else {
            content = content.push(Text::new("Notes Grouped by Label:"));
            let grouped_cache = self.grouped_notes_cache.borrow();

            if !grouped_cache.notes_without_labels.is_empty() {
                let mut no_label_column = Column::new().spacing(5);
                no_label_column = no_label_column.push(
                    Text::new("No Labels:")
                        .size(18)
                        .style(|_: &_| iced::widget::text::Style {
                            color: Some(iced::Color::from_rgb(0.7, 0.7, 0.7)),
                        }),
                );

                for note_path in &grouped_cache.notes_without_labels {
                    let note_button = Button::new(Text::new(format!("- {}", note_path)).size(16))
                        .on_press(Message::NoteSelectedInVisualizer(note_path.clone()))
                        .style(button::text);

                    no_label_column = no_label_column.push(note_button);
                }
                content = content.push(
                    Container::new(no_label_column)
                        .style(|theme| container::Style {
                            background: Some(iced::Background::Color(theme.palette().background)),
                            text_color: None,
                            border: iced::Border {
                                radius: 2.0.into(),
                                width: 1.0,
                                color: theme.palette().primary,
                            },
                            shadow: iced::Shadow::default(),
                        })
                        .padding(5)
                        .width(Length::Fill),
                );
            }

            for group in &grouped_cache.labels {
                let label = &group.label;
                let is_expanded = *self.expanded_labels.get(label).unwrap_or(&false);

                let mut label_header_row = Row::new().spacing(5).align_y(iced::Alignment::Center);
                let indicator = if is_expanded { 'v' } else { '>' };

                label_header_row = label_header_row.push(
                    Button::new(
                        Text::new(format!("{} {}", indicator, label))
                            .size(20)
                            .style(|theme: &Theme| iced::widget::text::Style {
                                color: Some(theme.palette().primary),
                            })
                            .shaping(iced::widget::text::Shaping::Advanced),
                    )
                    .on_press(Message::ToggleLabel(label.clone()))
                    .style(button::text),
                );

                let mut label_column = Column::new().spacing(5).push(label_header_row);

                if is_expanded {
                    for note_path in &group.note_paths {
                        let note_button = Button::new(Text::new(format!("- {}", note_path)).size(16))
                            .on_press(Message::NoteSelectedInVisualizer(note_path.clone()))
                            .style(button::text);

                        label_column = label_column.push(note_button);
                    }
                }

                content = content.push(
                    Container::new(label_column)
                        .style(|theme| container::Style {
                            background: Some(iced::Background::Color(theme.palette().background)),
                            text_color: None,
                            border: iced::Border {
                                radius: 2.0.into(),
                                width: 1.0,
                                color: theme.palette().primary,
                            },
                            shadow: iced::Shadow::default(),
                        })
                        .padding(5)
                        .width(Length::Fill),
                );
            }
        }

        let padded_content = content.padding(20);

        Scrollable::new(padded_content).into()
    }
}
