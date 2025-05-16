use crate::notebook::NoteMetadata;
use iced::{Element, Theme, widget::Column, widget::Text}; // Import NoteMetadata

#[derive(Debug, Default)]
pub struct Visualizer {
    // Add fields here to hold data needed for visualization,
    // like note positions, connections, etc.
    // For a placeholder, no fields are strictly needed.
    pub notes: Vec<NoteMetadata>, // Field to hold note metadata
}

#[derive(Debug, Clone)]
pub enum Message {
    // Define messages for interaction with the visualizer,
    // e.g., zooming, panning, clicking on nodes.
    UpdateNotes(Vec<NoteMetadata>), // Message to update notes data
}

impl Visualizer {
    pub fn new() -> Self {
        Self {
            notes: Vec::new(), // Initialize with empty notes
        }
    }

    pub fn update(&mut self, message: Message) -> iced::Command<Message> {
        match message {
            Message::UpdateNotes(notes) => {
                eprintln!(
                    "Visualizer: Received UpdateNotes message with {} notes.",
                    notes.len()
                );
                self.notes = notes;
                // In a real implementation, you would update the 3D scene based on these notes
            }
        }
        iced::Command::none()
    }

    pub fn view(&self) -> Element<'_, Message, Theme> {
        // This is where the 3D rendering logic would go.
        // For now, it's a placeholder showing the loaded notes.
        let mut column = Column::new().spacing(10).push(Text::new(
            "3D Visualizer Placeholder (Rendering logic to be implemented)",
        ));

        if self.notes.is_empty() {
            column = column.push(Text::new("No notes loaded in the visualizer."));
        } else {
            column = column.push(Text::new("Notes loaded:"));
            for note in &self.notes {
                column = column.push(Text::new(format!("- {}", note.rel_path)));
            }
        }

        column.into()
    }
}
