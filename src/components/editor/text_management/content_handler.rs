use iced::widget::text_editor::{Action, Content, Edit, Motion};
use iced::Command;

use crate::components::editor::Message;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::editor::text_management::undo_manager::UndoManager;
use crate::notebook;

// Handler for tab key press
pub fn handle_tab_key(
    content: &mut Content,
    markdown_text: &mut String,
    selected_note_path: Option<&String>,
    notebook_path: &str,
    state: &EditorState,
) -> Command<Message> {
    if selected_note_path.is_some()
        && !state.show_visualizer()
        && !state.show_move_note_input()
        && !state.show_new_note_input()
        && !state.show_about_info()
    {
        #[cfg(debug_assertions)]
        eprintln!("Editor: Handling HandleTabKey message by inserting 4 spaces.");

        content.perform(Action::Edit(Edit::Insert(' ')));
        content.perform(Action::Edit(Edit::Insert(' ')));
        content.perform(Action::Edit(Edit::Insert(' ')));
        content.perform(Action::Edit(Edit::Insert(' ')));

        *markdown_text = content.text();
        if let Some(selected_path) = selected_note_path {
            let notebook_path = notebook_path.to_string();
            let note_path = selected_path.clone();
            let content_text = markdown_text.clone();
            #[cfg(debug_assertions)]
            eprintln!(
                "Editor: Handling Tab: Saving content for note: {}",
                note_path
            );
            return Command::perform(
                async move {
                    notebook::save_note_content(notebook_path, note_path, content_text).await
                },
                Message::NoteContentSaved,
            );
        }
    }
    Command::none()
}

// Handler for select all action
pub fn handle_select_all(
    content: &mut Content,
    state: &EditorState,
) -> Command<Message> {
    if state.selected_note_path().is_some()
        && !state.show_visualizer()
        && !state.show_move_note_input()
        && !state.show_new_note_input()
        && !state.show_about_info()
    {
        #[cfg(debug_assertions)]
        eprintln!("Editor: Handling SelectAll message.");
        
        // Perform the SelectAll action
        // First move cursor to start, then select to end
        content.perform(Action::Move(Motion::DocumentStart));
        content.perform(Action::Select(Motion::DocumentEnd));
    }
    Command::none()
}

// Handler for editor actions
pub fn handle_editor_action(
    content: &mut Content,
    markdown_text: &mut String,
    undo_manager: &mut UndoManager,
    action: Action,
    selected_note_path: Option<&String>,
    notebook_path: &str,
    state: &EditorState,
) -> Command<Message> {
    if selected_note_path.is_some()
        && !state.show_visualizer()
        && !state.show_move_note_input()
        && !state.show_new_note_input()
        && !state.show_about_info()
    {
        // Save the current state to history before performing the action
        // Only save if this is a modifying action (Edit)
        if matches!(action, Action::Edit(_)) && selected_note_path.is_some() {
            let note_path = selected_note_path.unwrap().clone();
            undo_manager.add_to_history(&note_path, markdown_text.clone());
        }
        
        #[cfg(debug_assertions)]
        eprintln!("Editor: Performing EditorAction: {:?}", action);
        content.perform(action);

        *markdown_text = content.text();

        if let Some(selected_path) = selected_note_path {
            let notebook_path_clone = notebook_path.to_string();
            let note_path_clone = selected_path.clone();
            let content_text = markdown_text.clone();
            #[cfg(debug_assertions)]
            eprintln!(
                "Editor: Performing EditorAction: Saving content for note: {}",
                note_path_clone
            );
            return Command::perform(
                async move {
                    notebook::save_note_content(notebook_path_clone, note_path_clone, content_text).await
                },
                Message::NoteContentSaved,
            );
        }
    }
    Command::none()
}

// Handler for content changed
pub fn handle_content_changed(
    content: &mut Content,
    markdown_text: &mut String,
    undo_manager: &mut UndoManager,
    state: &mut EditorState,
    new_content: String,
) -> Command<Message> {
    if !state.show_visualizer()
        && !state.show_move_note_input()
        && !state.show_new_note_input()
        && !state.show_about_info()
    {
        if let Some(note_path) = state.selected_note_path() {
            // Check if we're loading a note (switching between notes)
            if state.is_loading_note() {
                undo_manager.handle_initial_content(note_path, &new_content);
                // Reset the loading flag
                state.set_loading_note(false);
            } else if !markdown_text.is_empty() && *markdown_text != new_content {
                // This is a regular content change, not a note switch
                undo_manager.add_to_history(note_path, markdown_text.clone());
            }
        }

        *content = Content::with_text(&new_content);
        *markdown_text = new_content;
    }
    Command::none()
}
