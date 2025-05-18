use iced::widget::{Button, Column, Scrollable, Text};
use iced::{Command, Element, Theme};
use serde::{Deserialize, Serialize};
use std::fs;

use crate::notebook::{self, NoteMetadata, NotebookMetadata};

#[derive(Debug, Clone)]
pub enum Message {
    NoteSelected(String),
    LoadNotes,
    NotesLoaded(Vec<NoteMetadata>),
}

#[derive(Debug, Default)]
pub struct NoteExplorer {
    pub notes: Vec<NoteMetadata>,
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
                // Sort notes by relative path to group them by directory in the view
                self.notes.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
                Command::none()
            }
            Message::NoteSelected(_path) => Command::none(),
        }
    }

    // Modified view function to accept selected_note_path
    pub fn view(&self, selected_note_path: Option<&String>) -> Element<'_, Message> {
        let mut column = Column::new().spacing(10);

        if self.notes.is_empty() {
            column = column.push(Text::new("No notes found."));
        } else {
            for note in &self.notes {
                let is_selected = Some(&note.rel_path) == selected_note_path;

                // Choose style based on whether the note is selected
                let button_style = if is_selected {
                    iced::theme::Button::Primary // Use Primary theme for selected notes
                } else {
                    iced::theme::Button::Text // Use Text theme for unselected notes
                };

                // Display the full relative path in the button
                column = column.push(
                    Button::new(Text::new(note.rel_path.clone()))
                        .on_press(Message::NoteSelected(note.rel_path.clone()))
                        .style(button_style), // Apply the determined style
                );
            }
        }

        Scrollable::new(column).into()
    }
}
