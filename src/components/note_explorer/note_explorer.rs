use iced::widget::{Button, Column, Scrollable, Text};
use iced::{Command, Element};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone)]
pub enum Message {
    NoteSelected(String), // This message will now only trigger the Editor to load
    LoadNotes,
    NotesLoaded(Vec<NoteMetadata>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMetadata {
    pub rel_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookMetadata {
    pub notes: Vec<NoteMetadata>,
}

#[derive(Debug, Default)]
pub struct NoteExplorer {
    notes: Vec<NoteMetadata>,
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
                Command::perform(load_notes_metadata(notebook_path), Message::NotesLoaded)
            }
            Message::NotesLoaded(notes) => {
                eprintln!(
                    "NoteExplorer: Received NotesLoaded message with {} notes.",
                    notes.len()
                );
                self.notes = notes;
                Command::none()
            }
            Message::NoteSelected(_path) => {
                // Marked as unused
                // When a note is selected, just pass the message up to the Editor
                // The NoteExplorer's view will remain as the list.
                Command::none()
            }
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

async fn load_notes_metadata(notebook_path: String) -> Vec<NoteMetadata> {
    let file_path = format!("{}/metadata.json", notebook_path);
    eprintln!(
        "load_notes_metadata: Attempting to read file: {}",
        file_path
    );

    let contents = match fs::read_to_string(&file_path) {
        Ok(c) => {
            eprintln!("load_notes_metadata: Successfully read file: {}", file_path);
            c
        }
        Err(err) => {
            eprintln!(
                "load_notes_metadata: Error reading metadata file {}: {}",
                file_path, err
            );
            return Vec::new();
        }
    };

    let metadata: NotebookMetadata = match serde_json::from_str(&contents) {
        Ok(m) => {
            eprintln!("load_notes_metadata: Successfully parsed metadata.");
            m
        }
        Err(err) => {
            eprintln!(
                "load_notes_metadata: Error parsing metadata from {}: {}",
                file_path, err
            );
            return Vec::new();
        }
    };

    metadata.notes
}
