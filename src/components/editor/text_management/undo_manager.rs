use std::collections::HashMap;
use iced::{Command, widget::text_editor::Content};

use crate::components::editor::state::editor_state::EditorState;
use crate::components::editor::Message;
use crate::notebook;

pub struct UndoManager {
    undo_histories: HashMap<String, Vec<String>>, // Store previous states for undo per note
    undo_indices: HashMap<String, usize>, // Track position in undo history per note
}

impl UndoManager {
    pub fn new() -> Self {
        Self {
            undo_histories: HashMap::new(),
            undo_indices: HashMap::new(),
        }
    }
    
    pub fn initialize_history(&mut self, note_path: &str) {
        if !self.undo_histories.contains_key(note_path) {
            self.undo_histories.insert(note_path.to_string(), Vec::new());
        }
        
        let history_index = self.undo_histories.get(note_path)
            .map_or(0, |history| history.len());
            
        self.undo_indices.insert(note_path.to_string(), history_index);
        
        #[cfg(debug_assertions)]
        eprintln!(
            "Editor: Setting history index for note '{}' to {}", 
            note_path, history_index
        );
    }
    
    pub fn add_to_history(&mut self, note_path: &str, content: String) {
        let current_index = self.undo_indices.get(note_path).copied().unwrap_or(0);
        
        let history = self.undo_histories
            .entry(note_path.to_string())
            .or_insert_with(Vec::new);
            
        // Remove any future redo states if we're in the middle of the history
        if current_index < history.len() {
            history.truncate(current_index);
        }
        
        // Add current state to history
        history.push(content);
        let new_index = history.len();
        
        // Update the index for this note
        self.undo_indices.insert(note_path.to_string(), new_index);
        
        #[cfg(debug_assertions)]
        eprintln!(
            "Added state to undo history for note '{}'. History size: {} Index: {}",
            note_path, history.len(), new_index
        );
    }
    
    pub fn handle_initial_content(&mut self, note_path: &str, content: &str) {
        // Add underscore to unused variable
        let _history_exists = self.undo_histories.contains_key(note_path);
        let history = self.undo_histories
            .entry(note_path.to_string())
            .or_insert_with(Vec::new);
        
        #[cfg(debug_assertions)]
        eprintln!(
            "Loading note '{}'. History exists: {}, Size: {}",
            note_path, _history_exists, history.len()
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
                history.push(content.to_string());
                self.undo_indices.insert(note_path.to_string(), 1);
                
                #[cfg(debug_assertions)]
                eprintln!(
                    "Initialized history for note '{}' with first entry. History size: 1, Index: 1",
                    note_path
                );
            } else {
                self.undo_indices.insert(note_path.to_string(), 0);
                
                #[cfg(debug_assertions)]
                eprintln!(
                    "Initialized empty history for note '{}'",
                    note_path
                );
            }
        } else {
            // Note already has history - verify current content
            let current_index = self.undo_indices.get(note_path).copied().unwrap_or(0);
            
            // Verify that the loaded content matches what's in the history
            // This handles potential external file changes
            if current_index > 0 && current_index <= history.len() && 
                history[current_index - 1] != content {
                #[cfg(debug_assertions)]
                eprintln!(
                    "Content for note '{}' changed externally, adding to history",
                    note_path
                );
                
                // Content has changed, add it to history
                history.push(content.to_string());
                self.undo_indices.insert(note_path.to_string(), history.len());
            }
        }
    }
    
    pub fn get_previous_content(&mut self, note_path: &str) -> Option<String> {
        if let Some(current_index) = self.undo_indices.get(note_path).copied() {
            if current_index > 0 {
                if let Some(history) = self.undo_histories.get(note_path) {
                    if !history.is_empty() {
                        let new_index = current_index - 1;
                        let previous_content = history[new_index].clone();
                        
                        // Update the index
                        self.undo_indices.insert(note_path.to_string(), new_index);
                        
                        return Some(previous_content);
                    }
                }
            }
        }
        None
    }
    
    pub fn handle_path_change(&mut self, old_path: &str, new_path: &str) {
        // Update the history collection
        if let Some(history) = self.undo_histories.remove(old_path) {
            self.undo_histories.insert(new_path.to_string(), history);
            #[cfg(debug_assertions)]
            eprintln!("Updated undo history key from '{}' to '{}'", old_path, new_path);
        }
        
        // Update the index collection
        if let Some(index) = self.undo_indices.remove(old_path) {
            self.undo_indices.insert(new_path.to_string(), index);
            #[cfg(debug_assertions)]
            eprintln!("Updated undo index key from '{}' to '{}'", old_path, new_path);
        }
    }
    
    pub fn remove_history(&mut self, note_path: &str) {
        self.undo_histories.remove(note_path);
        self.undo_indices.remove(note_path);
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
) -> Command<Message> {
    if let Some(note_path) = selected_note_path {
        if !state.show_visualizer() 
            && !state.show_move_note_input() 
            && !state.show_new_note_input()
            && !state.show_about_info()
        {
            if let Some(previous_content) = undo_manager.get_previous_content(note_path) {
                #[cfg(debug_assertions)]
                eprintln!(
                    "Editor: Performing undo to previous state for note: {}",
                    note_path
                );

                // Update content with the previous state
                *content = Content::with_text(&previous_content);
                *markdown_text = previous_content.clone();
                
                // Save the content after undo
                let notebook_path_clone = notebook_path.to_string();
                let note_path_clone = note_path.clone();

                return Command::perform(
                    async move {
                        notebook::save_note_content(notebook_path_clone, note_path_clone, previous_content).await
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
    
    Command::none()
}
