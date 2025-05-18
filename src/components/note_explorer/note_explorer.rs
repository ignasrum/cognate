use iced::widget::{Button, Column, Scrollable, Text};
use iced::{Command, Element, Theme};
use serde::{Deserialize, Serialize};
use std::collections::HashMap; // Import HashMap
use std::fs;
use std::path::Path; // Import Path

use crate::notebook::{self, NoteMetadata, NotebookMetadata};

#[derive(Debug, Clone)]
pub enum Message {
    NoteSelected(String),
    LoadNotes,
    NotesLoaded(Vec<NoteMetadata>),
    ToggleFolder(String), // New message for toggling folder visibility
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
            expanded_folders: HashMap::new(), // Initialize the map
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
                // Sorting by rel_path still helps with grouping in the HashMap later
                self.notes.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

                // Initialize expanded state for new folders
                let mut folders_to_initialize = HashMap::new();
                for note in &self.notes {
                    if let Some(parent) = Path::new(&note.rel_path).parent() {
                        let folder_path = parent.to_string_lossy().into_owned();
                        // If the folder path is empty, it's a root note, skip initializing it as a folder
                        if !folder_path.is_empty() {
                            folders_to_initialize.entry(folder_path).or_insert(false);
                        }
                    }
                }
                // Only add new folders, don't overwrite existing expanded states
                for (folder, state) in folders_to_initialize {
                    self.expanded_folders.entry(folder).or_insert(state);
                }

                Command::none()
            }
            Message::NoteSelected(_path) => Command::none(), // Handled by the parent
            Message::ToggleFolder(folder_path) => {
                // Toggle the expanded state for the clicked folder
                let is_expanded = self.expanded_folders.entry(folder_path).or_insert(false);
                *is_expanded = !*is_expanded;
                Command::none()
            }
        }
    }

    pub fn view(&self, selected_note_path: Option<&String>) -> Element<'_, Message> {
        let mut column = Column::new().spacing(5); // Reduced spacing

        if self.notebook_path.is_empty() || self.notes.is_empty() {
            column = column.push(Text::new("No notes found."));
        } else {
            // Group notes by parent directory
            let mut notes_by_folder: HashMap<String, Vec<&NoteMetadata>> = HashMap::new();
            let mut root_notes: Vec<&NoteMetadata> = Vec::new();

            for note in &self.notes {
                if let Some(parent) = Path::new(&note.rel_path).parent() {
                    let folder_path = parent.to_string_lossy().into_owned();
                    if folder_path.is_empty() {
                        root_notes.push(note);
                    } else {
                        notes_by_folder
                            .entry(folder_path)
                            .or_insert_with(Vec::new)
                            .push(note);
                    }
                } else {
                    root_notes.push(note);
                }
            }

            // Sort folders alphabetically
            let mut sorted_folders: Vec<String> = notes_by_folder.keys().cloned().collect();
            sorted_folders.sort();

            // Display root notes first (if any)
            if !root_notes.is_empty() {
                column = column.push(Text::new("Root Notes:").size(18));
                for note in root_notes {
                    let is_selected = Some(&note.rel_path) == selected_note_path;
                    let button_style = if is_selected {
                        iced::theme::Button::Primary
                    } else {
                        iced::theme::Button::Text
                    };
                    column = column.push(
                         Button::new(Text::new(note.rel_path.clone()).size(16)) // Slightly smaller text for notes
                             .on_press(Message::NoteSelected(note.rel_path.clone()))
                             .style(button_style),
                     );
                }
                column = column.push(iced::widget::Space::with_height(iced::Length::Fixed(10.0))); // Add some space after root notes
            }

            // Display folders and their notes
            for folder_path in sorted_folders {
                let is_expanded = *self.expanded_folders.get(&folder_path).unwrap_or(&false);

                // Folder button
                let folder_button_text = if is_expanded {
                    format!("▼ {}", folder_path) // Down arrow when expanded
                } else {
                    format!("► {}", folder_path) // Right arrow when collapsed
                };
                column = column.push(
                    Button::new(Text::new(folder_button_text).size(18)) // Slightly larger text for folders
                        .on_press(Message::ToggleFolder(folder_path.clone()))
                        .style(iced::theme::Button::Text), // Use Text style for folders
                );

                // Display notes if folder is expanded
                if is_expanded {
                    if let Some(notes_in_folder) = notes_by_folder.get(&folder_path) {
                        // Sort notes within the folder alphabetically
                        let mut sorted_notes_in_folder = notes_in_folder.clone();
                        sorted_notes_in_folder.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

                        let mut folder_notes_column = Column::new().spacing(3); // Tighter spacing for notes within a folder
                        for note in sorted_notes_in_folder {
                            let is_selected = Some(&note.rel_path) == selected_note_path;

                            let button_style = if is_selected {
                                iced::theme::Button::Primary
                            } else {
                                iced::theme::Button::Text
                            };

                            // Extract just the note name (file name without the folder path)
                            let note_name = Path::new(&note.rel_path)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned();

                            folder_notes_column = folder_notes_column.push(
                                Button::new(Text::new(format!("  - {}", note_name)).size(16)) // Indent notes and use smaller text
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
