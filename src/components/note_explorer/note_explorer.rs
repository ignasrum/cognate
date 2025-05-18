use iced::widget::{Button, Column, Row, Scrollable, Text};
use iced::{Command, Element};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::notebook::{self, NoteMetadata};

#[derive(Debug, Clone)]
pub enum Message {
    NoteSelected(String),
    LoadNotes,
    NotesLoaded(Vec<NoteMetadata>),
    ToggleFolder(String), // Message for toggling folder visibility
    // Added message to initiate folder rename from the explorer
    InitiateFolderRename(String),
}

#[derive(Debug, Default)]
pub struct NoteExplorer {
    pub notes: Vec<NoteMetadata>,
    pub notebook_path: String,
    expanded_folders: HashMap<String, bool>, // Keep track of expanded folders
}

impl NoteExplorer {
    pub fn new(notebook_path: String) -> Self {
        Self {
            notes: Vec::new(),
            notebook_path,
            expanded_folders: HashMap::new(),
        }
    }

    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::LoadNotes => {
                eprintln!(
                    "NoteExplorer: Received LoadNotes message. Loading from path: {}",
                    self.notebook_path
                );
                let notebook_path = self.notebook_path.clone();
                Command::perform(
                    notebook::load_notes_metadata(notebook_path),
                    Message::NotesLoaded,
                )
            }
            Message::NotesLoaded(notes) => {
                eprintln!(
                    "NoteExplorer: Received NotesLoaded message with {} notes.",
                    notes.len()
                );
                self.notes = notes;
                self.notes.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

                // Initialize expanded state for new folders, preserving existing states
                let mut current_folders: HashMap<String, bool> =
                    self.expanded_folders.drain().collect();
                for note in &self.notes {
                    if let Some(parent) = Path::new(&note.rel_path).parent() {
                        let folder_path = parent.to_string_lossy().into_owned();
                        if !folder_path.is_empty() && folder_path != "." {
                            current_folders.entry(folder_path).or_insert(false);
                        }
                    }
                }
                self.expanded_folders = current_folders;

                Command::none()
            }
            Message::NoteSelected(_path) => Command::none(),
            Message::ToggleFolder(folder_path) => {
                let is_expanded = self
                    .expanded_folders
                    .entry(folder_path.clone())
                    .or_insert(false);
                *is_expanded = !*is_expanded;
                eprintln!(
                    "Toggled folder '{}' to expanded: {}",
                    folder_path, *is_expanded
                );
                Command::none()
            }
            Message::InitiateFolderRename(_folder_path) => {
                // This message is handled by the Editor to manage UI state.
                // We just need to pass it up.
                Command::none()
            }
        }
    }

    pub fn view(&self, selected_note_path: Option<&String>) -> Element<'_, Message> {
        let mut column = Column::new().spacing(5);

        if self.notebook_path.is_empty() || self.notes.is_empty() {
            column = column.push(Text::new("No notes found."));
        } else {
            // Group notes by parent directory
            let mut notes_by_folder: HashMap<String, Vec<&NoteMetadata>> = HashMap::new();
            let mut root_notes: Vec<&NoteMetadata> = Vec::new();
            let mut all_folders: HashSet<String> = HashSet::new();

            for note in &self.notes {
                if let Some(parent) = Path::new(&note.rel_path).parent() {
                    let folder_path = parent.to_string_lossy().into_owned();
                    if folder_path.is_empty() || folder_path == "." {
                        root_notes.push(note);
                    } else {
                        notes_by_folder
                            .entry(folder_path.clone())
                            .or_insert_with(Vec::new)
                            .push(note);
                        all_folders.insert(folder_path);
                    }
                } else {
                    root_notes.push(note);
                }
            }

            // Sort folders alphabetically
            let mut sorted_folders: Vec<String> = all_folders.into_iter().collect();
            sorted_folders.sort();

            // Display root notes first (if any)
            if !root_notes.is_empty() {
                column = column.push(Text::new("Root Notes:").size(18));
                let mut sorted_root_notes = root_notes.clone();
                sorted_root_notes.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

                for note in sorted_root_notes {
                    let is_selected = Some(&note.rel_path) == selected_note_path;
                    let button_style = if is_selected {
                        iced::theme::Button::Primary
                    } else {
                        iced::theme::Button::Text
                    };
                    column = column.push(
                        Button::new(Text::new(note.rel_path.clone()).size(16))
                            .on_press(Message::NoteSelected(note.rel_path.clone()))
                            .style(button_style),
                    );
                }
                column = column.push(iced::widget::Space::with_height(iced::Length::Fixed(10.0)));
            }

            // Display folders and their notes
            for folder_path in sorted_folders {
                let is_expanded = *self.expanded_folders.get(&folder_path).unwrap_or(&false);

                let folder_indicator = if is_expanded { 'v' } else { '>' };
                let folder_button_text = format!("{} {}", folder_indicator, folder_path);

                let folder_row = Row::new()
                    .push(
                        Button::new(Text::new(folder_button_text).size(18))
                            .on_press(Message::ToggleFolder(folder_path.clone()))
                            .style(iced::theme::Button::Text),
                    )
                    .push(
                        Button::new(Text::new("Rename").size(14))
                            .on_press(Message::InitiateFolderRename(folder_path.clone())) // Added rename button
                            .style(iced::theme::Button::Secondary)
                            .padding(3),
                    )
                    .spacing(5)
                    .align_items(iced::Alignment::Center);

                column = column.push(folder_row);

                // Display notes if folder is expanded
                if is_expanded {
                    if let Some(notes_in_folder) = notes_by_folder.get(&folder_path) {
                        let mut sorted_notes_in_folder = notes_in_folder.clone();
                        sorted_notes_in_folder.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

                        let mut folder_notes_column = Column::new().spacing(3);
                        for note in sorted_notes_in_folder {
                            let is_selected = Some(&note.rel_path) == selected_note_path;

                            let button_style = if is_selected {
                                iced::theme::Button::Primary
                            } else {
                                iced::theme::Button::Text
                            };

                            let note_name = Path::new(&note.rel_path)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned();

                            folder_notes_column = folder_notes_column.push(
                                Button::new(Text::new(format!("  - {}", note_name)).size(16))
                                    .on_press(Message::NoteSelected(note.rel_path.clone()))
                                    .style(button_style),
                            );
                        }
                        column = column.push(folder_notes_column);
                    }
                }
            }
        }

        Scrollable::new(column).into()
    }
}
