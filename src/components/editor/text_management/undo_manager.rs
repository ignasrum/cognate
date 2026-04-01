use iced::{
    task::Task,
    widget::text_editor::{Content, Cursor, Position},
};
use std::collections::HashMap; // Use Task instead of Command
use std::time::{Duration, Instant};

use crate::components::editor::Message;
use crate::components::editor::state::editor_state::EditorState;
use crate::notebook;

pub struct UndoManager {
    undo_histories: HashMap<String, Vec<UndoSnapshot>>, // Store previous states for undo per note
    undo_indices: HashMap<String, usize>,               // Track position in undo history per note
    redo_histories: HashMap<String, Vec<UndoSnapshot>>, // Store redo states per note
    last_edit_timestamps: HashMap<String, Instant>,     // Debounce rapid edit snapshots per note
}

#[derive(Clone)]
struct UndoSnapshot {
    content: String,
    cursor: Cursor,
}

fn clamp_position_to_content(content: &Content, position: Position) -> Position {
    let line_count = content.line_count();
    if line_count == 0 {
        return Position { line: 0, column: 0 };
    }

    let line = position.line.min(line_count.saturating_sub(1));
    let max_column = content.line(line).map_or(0, |line| line.text.len());

    Position {
        line,
        column: position.column.min(max_column),
    }
}

fn clamp_cursor_to_content(content: &Content, cursor: Cursor) -> Cursor {
    Cursor {
        position: clamp_position_to_content(content, cursor.position),
        selection: cursor
            .selection
            .map(|selection| clamp_position_to_content(content, selection)),
    }
}

fn cursor_at_end(content: &str) -> Cursor {
    let (line, column) = content
        .split('\n')
        .enumerate()
        .last()
        .map_or((0, 0), |(line, text)| (line, text.len()));

    Cursor {
        position: Position { line, column },
        selection: None,
    }
}

#[cfg(test)]
const EDIT_UNDO_DEBOUNCE_WINDOW: Duration = Duration::from_millis(120);
#[cfg(not(test))]
const EDIT_UNDO_DEBOUNCE_WINDOW: Duration = Duration::from_millis(750);

impl UndoManager {
    pub fn new() -> Self {
        Self {
            undo_histories: HashMap::new(),
            undo_indices: HashMap::new(),
            redo_histories: HashMap::new(),
            last_edit_timestamps: HashMap::new(),
        }
    }

    pub fn initialize_history(&mut self, note_path: &str) {
        if !self.undo_histories.contains_key(note_path) {
            self.undo_histories
                .insert(note_path.to_string(), Vec::new());
        }

        let history_index = self
            .undo_histories
            .get(note_path)
            .map_or(0, |history| history.len());

        self.undo_indices
            .insert(note_path.to_string(), history_index);
        self.last_edit_timestamps.remove(note_path);

        #[cfg(debug_assertions)]
        eprintln!(
            "Editor: Setting history index for note '{}' to {}",
            note_path, history_index
        );
    }

    pub fn add_to_history(&mut self, note_path: &str, content: String, cursor: Cursor) {
        let current_index = self.undo_indices.get(note_path).copied().unwrap_or(0);

        let history = self
            .undo_histories
            .entry(note_path.to_string())
            .or_default();

        // A new edit invalidates redo states.
        self.redo_histories.remove(note_path);

        // Remove any future redo states if we're in the middle of the history
        if current_index < history.len() {
            history.truncate(current_index);
        }

        // Avoid creating no-op undo steps when the text did not change.
        // This keeps undo aligned with text edits instead of cursor-only jumps.
        if let Some(last_snapshot) = history.last_mut()
            && last_snapshot.content == content
        {
            last_snapshot.cursor = cursor;
            self.undo_indices
                .insert(note_path.to_string(), history.len());
            return;
        }

        // Add current state to history
        history.push(UndoSnapshot { content, cursor });
        let new_index = history.len();

        // Update the index for this note
        self.undo_indices.insert(note_path.to_string(), new_index);

        #[cfg(debug_assertions)]
        eprintln!(
            "Added state to undo history for note '{}'. History size: {} Index: {}",
            note_path,
            history.len(),
            new_index
        );
    }

    pub fn add_to_history_debounced(&mut self, note_path: &str, content: String, cursor: Cursor) {
        let now = Instant::now();
        let should_create_snapshot = match self.last_edit_timestamps.get(note_path).copied() {
            Some(last_edit) => {
                now.saturating_duration_since(last_edit) >= EDIT_UNDO_DEBOUNCE_WINDOW
            }
            None => true,
        };

        self.last_edit_timestamps.insert(note_path.to_string(), now);

        // Any new edit invalidates redo states, even if the edit is folded into
        // the current debounced typing batch.
        self.redo_histories.remove(note_path);

        if should_create_snapshot {
            self.add_to_history(note_path, content, cursor);
        }
    }

    pub fn reset_edit_debounce(&mut self, note_path: &str) {
        self.last_edit_timestamps.remove(note_path);
    }

    pub fn handle_initial_content(&mut self, note_path: &str, content: &str) {
        self.last_edit_timestamps.remove(note_path);

        // Add underscore to unused variable
        let _history_exists = self.undo_histories.contains_key(note_path);
        let history = self
            .undo_histories
            .entry(note_path.to_string())
            .or_default();

        #[cfg(debug_assertions)]
        eprintln!(
            "Loading note '{}'. History exists: {}, Size: {}",
            note_path,
            _history_exists,
            history.len()
        );

        // Only initialize history if it doesn't exist or is empty
        if history.is_empty() {
            #[cfg(debug_assertions)]
            eprintln!(
                "Initializing history for note '{}' as it's empty or new",
                note_path
            );

            // Add the initial content as the first history entry
            if !content.is_empty() {
                // Add initial content to history
                history.push(UndoSnapshot {
                    content: content.to_string(),
                    cursor: cursor_at_end(content),
                });
                self.undo_indices.insert(note_path.to_string(), 1);
                self.redo_histories.remove(note_path);

                #[cfg(debug_assertions)]
                eprintln!(
                    "Initialized history for note '{}' with first entry. History size: 1, Index: 1",
                    note_path
                );
            } else {
                self.undo_indices.insert(note_path.to_string(), 0);

                #[cfg(debug_assertions)]
                eprintln!("Initialized empty history for note '{}'", note_path);
            }
        } else {
            // Note already has history - verify current content
            let current_index = self.undo_indices.get(note_path).copied().unwrap_or(0);

            // Verify that the loaded content matches what's in the history
            // This handles potential external file changes
            if current_index > 0
                && current_index <= history.len()
                && history[current_index - 1].content != content
            {
                #[cfg(debug_assertions)]
                eprintln!(
                    "Content for note '{}' changed externally, adding to history",
                    note_path
                );

                // Content has changed, add it to history
                history.push(UndoSnapshot {
                    content: content.to_string(),
                    cursor: cursor_at_end(content),
                });
                self.undo_indices
                    .insert(note_path.to_string(), history.len());
                self.redo_histories.remove(note_path);
            }
        }
    }

    fn get_previous_snapshot(&mut self, note_path: &str) -> Option<UndoSnapshot> {
        if let Some(current_index) = self.undo_indices.get(note_path).copied()
            && current_index > 0
            && let Some(history) = self.undo_histories.get(note_path)
            && !history.is_empty()
        {
            let new_index = current_index - 1;
            let previous_snapshot = history[new_index].clone();

            // Update the index
            self.undo_indices.insert(note_path.to_string(), new_index);

            return Some(previous_snapshot);
        }
        None
    }

    fn add_redo_snapshot(&mut self, note_path: &str, snapshot: UndoSnapshot) {
        self.redo_histories
            .entry(note_path.to_string())
            .or_default()
            .push(snapshot);
    }

    fn get_next_redo_snapshot(&mut self, note_path: &str) -> Option<UndoSnapshot> {
        self.redo_histories
            .get_mut(note_path)
            .and_then(|history| history.pop())
    }

    fn advance_undo_index_for_redo(&mut self, note_path: &str) {
        let current_index = self.undo_indices.get(note_path).copied().unwrap_or(0);
        let max_index = self
            .undo_histories
            .get(note_path)
            .map_or(0, |history| history.len());
        let new_index = (current_index + 1).min(max_index);

        self.undo_indices.insert(note_path.to_string(), new_index);
    }

    #[cfg(test)]
    pub fn get_previous_content(&mut self, note_path: &str) -> Option<String> {
        self.get_previous_snapshot(note_path)
            .map(|snapshot| snapshot.content)
    }

    pub fn handle_path_change(&mut self, old_path: &str, new_path: &str) {
        // Update the history collection
        if let Some(history) = self.undo_histories.remove(old_path) {
            self.undo_histories.insert(new_path.to_string(), history);
            #[cfg(debug_assertions)]
            eprintln!(
                "Updated undo history key from '{}' to '{}'",
                old_path, new_path
            );
        }

        // Update redo history collection
        if let Some(history) = self.redo_histories.remove(old_path) {
            self.redo_histories.insert(new_path.to_string(), history);
            #[cfg(debug_assertions)]
            eprintln!(
                "Updated redo history key from '{}' to '{}'",
                old_path, new_path
            );
        }

        // Update the index collection
        if let Some(index) = self.undo_indices.remove(old_path) {
            self.undo_indices.insert(new_path.to_string(), index);
            #[cfg(debug_assertions)]
            eprintln!(
                "Updated undo index key from '{}' to '{}'",
                old_path, new_path
            );
        }

        if let Some(timestamp) = self.last_edit_timestamps.remove(old_path) {
            self.last_edit_timestamps
                .insert(new_path.to_string(), timestamp);
        }
    }

    pub fn remove_history(&mut self, note_path: &str) {
        self.undo_histories.remove(note_path);
        self.undo_indices.remove(note_path);
        self.redo_histories.remove(note_path);
        self.last_edit_timestamps.remove(note_path);
        #[cfg(debug_assertions)]
        eprintln!("Removed undo history and index for note '{}'", note_path);
    }
}

// Handler functions
pub fn handle_undo(
    undo_manager: &mut UndoManager,
    content: &mut Content,
    markdown_text: &mut String,
    selected_note_path: Option<&String>,
    notebook_path: &str,
    state: &EditorState,
) -> Task<Message> {
    if let Some(note_path) = selected_note_path {
        if !state.show_visualizer()
            && !state.show_move_note_input()
            && !state.show_new_note_input()
            && !state.show_about_info()
        {
            if let Some(previous_snapshot) = undo_manager.get_previous_snapshot(note_path) {
                #[cfg(debug_assertions)]
                eprintln!(
                    "Editor: Performing undo to previous state for note: {}",
                    note_path
                );

                undo_manager.add_redo_snapshot(
                    note_path,
                    UndoSnapshot {
                        content: markdown_text.clone(),
                        cursor: content.cursor(),
                    },
                );

                // Update content with the previous state
                *content = Content::with_text(&previous_snapshot.content);
                content.move_to(clamp_cursor_to_content(content, previous_snapshot.cursor));
                *markdown_text = previous_snapshot.content.clone();
                undo_manager.reset_edit_debounce(note_path);

                // Save the content after undo
                let notebook_path_clone = notebook_path.to_string();
                let note_path_clone = note_path.clone();

                return Task::perform(
                    async move {
                        notebook::save_note_content(
                            notebook_path_clone,
                            note_path_clone,
                            previous_snapshot.content,
                        )
                        .await
                    },
                    Message::NoteContentSaved,
                );
            } else {
                #[cfg(debug_assertions)]
                eprintln!("Editor: Cannot undo - no previous state available");
            }
        } else {
            #[cfg(debug_assertions)]
            eprintln!("Editor: Cannot undo - note is in a state that doesn't allow undo");
        }
    } else {
        #[cfg(debug_assertions)]
        eprintln!("Editor: Cannot undo - no note selected");
    }

    Task::none()
}

pub fn handle_redo(
    undo_manager: &mut UndoManager,
    content: &mut Content,
    markdown_text: &mut String,
    selected_note_path: Option<&String>,
    notebook_path: &str,
    state: &EditorState,
) -> Task<Message> {
    if let Some(note_path) = selected_note_path {
        if !state.show_visualizer()
            && !state.show_move_note_input()
            && !state.show_new_note_input()
            && !state.show_about_info()
        {
            if let Some(next_snapshot) = undo_manager.get_next_redo_snapshot(note_path) {
                #[cfg(debug_assertions)]
                eprintln!(
                    "Editor: Performing redo to next state for note: {}",
                    note_path
                );

                undo_manager.advance_undo_index_for_redo(note_path);

                *content = Content::with_text(&next_snapshot.content);
                content.move_to(clamp_cursor_to_content(content, next_snapshot.cursor));
                *markdown_text = next_snapshot.content.clone();
                undo_manager.reset_edit_debounce(note_path);

                let notebook_path_clone = notebook_path.to_string();
                let note_path_clone = note_path.clone();

                return Task::perform(
                    async move {
                        notebook::save_note_content(
                            notebook_path_clone,
                            note_path_clone,
                            next_snapshot.content,
                        )
                        .await
                    },
                    Message::NoteContentSaved,
                );
            } else {
                #[cfg(debug_assertions)]
                eprintln!("Editor: Cannot redo - no next state available");
            }
        } else {
            #[cfg(debug_assertions)]
            eprintln!("Editor: Cannot redo - note is in a state that doesn't allow redo");
        }
    } else {
        #[cfg(debug_assertions)]
        eprintln!("Editor: Cannot redo - no note selected");
    }

    Task::none()
}
