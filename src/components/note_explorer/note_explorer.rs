use iced::widget::{Button, Column, Scrollable, Text};
use iced::{Command, Element};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone)]
pub enum Message {
    NoteSelected(String),
    LoadNotes,
    NotesLoaded(Vec<NoteMetadata>),
    // New messages for displaying content
    DisplayContent(String),
    ShowList,
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
    // New fields to manage view state and displayed content
    displayed_content: Option<String>,
    show_list: bool,
}

impl NoteExplorer {
    pub fn new(notebook_path: String) -> Self {
        Self {
            notes: Vec::new(),
            notebook_path,
            displayed_content: None,
            show_list: true, // Start by showing the list
        }
    }

    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::LoadNotes => {
                eprintln!(
                    "NoteExplorer: Received LoadNotes message. Loading from path: {}",
                    self.notebook_path
                );
                // Ensure we show the list when loading notes
                self.show_list = true;
                self.displayed_content = None;

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
            Message::NoteSelected(path) => {
                // When a note is selected in the list, hide the list temporarily
                // The Editor will handle loading and sending DisplayContent, which will set show_list to false
                self.show_list = false;
                // Pass the selection up to the Editor
                Command::none() // The Editor will handle loading and sending DisplayContent
            }
            Message::DisplayContent(content) => {
                // When content is received from the Editor, display it
                eprintln!("NoteExplorer: Received DisplayContent message.");
                self.displayed_content = Some(content);
                self.show_list = false;
                Command::none()
            }
            Message::ShowList => {
                // When ShowList is received, clear content and show the list
                eprintln!("NoteExplorer: Received ShowList message.");
                self.displayed_content = None;
                self.show_list = true;
                Command::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        if self.show_list {
            // Display the list of notes
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
        } else {
            // Display the note content and a back button
            let mut column = Column::new().spacing(10);

            column =
                column.push(Button::new(Text::new("Back to Notes")).on_press(Message::ShowList));

            if let Some(content) = &self.displayed_content {
                column = column.push(Text::new(content.clone()));
            } else {
                column = column.push(Text::new("Loading note content..."));
            }

            Scrollable::new(column).into()
        }
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
