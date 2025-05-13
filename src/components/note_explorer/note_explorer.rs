use iced::widget::{Button, Column, Text};
use iced::{Command, Element};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone)]
pub enum Message {
    NoteSelected(String),
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
            Message::LoadNotes => Command::perform(
                load_notes_metadata(self.notebook_path.clone()),
                Message::NotesLoaded,
            ),
            Message::NotesLoaded(notes) => {
                self.notes = notes;
                Command::none()
            }
            _ => Command::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut column = Column::new().spacing(10);

        for note in &self.notes {
            column = column.push(
                Button::new(Text::new(note.rel_path.clone()))
                    .on_press(Message::NoteSelected(note.rel_path.clone())),
            );
        }

        column.into()
    }
}

async fn load_notes_metadata(notebook_path: String) -> Vec<NoteMetadata> {
    let file_path = format!("{}/metadata.json", notebook_path);

    let contents = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Error reading metadata file: {}", err);
            return Vec::new();
        }
    };

    let metadata: NotebookMetadata = match serde_json::from_str(&contents) {
        Ok(m) => m,
        Err(err) => {
            eprintln!("Error parsing metadata: {}", err);
            return Vec::new();
        }
    };

    metadata.notes
}
