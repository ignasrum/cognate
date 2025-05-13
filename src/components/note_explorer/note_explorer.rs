use iced::widget::{Button, Column, Scrollable, Text};
use iced::{Command, Element};
use serde::{Deserialize, Serialize}; // Keep imports for structs if they were still here, but they moved
use std::fs; // Keep imports for file system operations if needed, but load logic moved

// Import structs and load_notes_metadata from the notebook module
use crate::notebook::{self, NoteMetadata, NotebookMetadata};

#[derive(Debug, Clone)]
pub enum Message {
    NoteSelected(String),
    LoadNotes,
    NotesLoaded(Vec<NoteMetadata>), // Now uses NoteMetadata from the notebook module
}

// NoteMetadata and NotebookMetadata structs definitions are removed from here

#[derive(Debug, Default)]
pub struct NoteExplorer {
    pub notes: Vec<NoteMetadata>, // Now uses NoteMetadata from the notebook module
    pub notebook_path: String,
}

impl NoteExplorer {
    pub fn new(notebook_path: String) -> Self {
        Self {
            notes: Vec::new(),
            notebook_path,
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
                // Call the load_notes_metadata function from the notebook module
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
                Command::none()
            }
            Message::NoteSelected(_path) => Command::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut column = Column::new().spacing(10);

        if self.notes.is_empty() {
            column = column.push(Text::new("No notes found."));
        } else {
            for note in &self.notes {
                column = column.push(
                    Button::new(Text::new(note.rel_path.clone()))
                        .on_press(Message::NoteSelected(note.rel_path.clone())),
                );
            }
        }

        Scrollable::new(column).into()
    }
}

// load_notes_metadata function definition is removed from here
