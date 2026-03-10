#[cfg(test)]
mod tests {
    use crate::components::editor::state::editor_state::EditorState;
    use crate::components::editor::text_management::content_handler;
    use crate::components::editor::text_management::undo_manager::{self, UndoManager};
    use iced::widget::text_editor::{Action, Content, Edit};

    #[test]
    fn undo_manager_history_and_path_changes_work() {
        let mut undo = UndoManager::new();
        undo.initialize_history("note/a");
        undo.add_to_history("note/a", "v1".to_string());
        undo.add_to_history("note/a", "v2".to_string());

        assert_eq!(undo.get_previous_content("note/a"), Some("v2".to_string()));
        assert_eq!(undo.get_previous_content("note/a"), Some("v1".to_string()));
        assert_eq!(undo.get_previous_content("note/a"), None);

        undo.handle_path_change("note/a", "note/b");
        assert_eq!(undo.get_previous_content("note/b"), None);

        undo.add_to_history("note/b", "v3".to_string());
        undo.remove_history("note/b");
        assert_eq!(undo.get_previous_content("note/b"), None);
    }

    #[test]
    fn undo_manager_initial_content_tracks_external_changes() {
        let mut undo = UndoManager::new();
        undo.initialize_history("note/x");
        undo.handle_initial_content("note/x", "first");
        undo.handle_initial_content("note/x", "changed");

        assert_eq!(undo.get_previous_content("note/x"), Some("changed".to_string()));
        assert_eq!(undo.get_previous_content("note/x"), Some("first".to_string()));
    }

    #[test]
    fn content_handlers_modify_text_and_history() {
        let mut state = EditorState::new();
        state.set_notebook_path("dummy".to_string());
        state.set_selected_note_path(Some("note/a".to_string()));

        let mut content = Content::with_text("");
        let mut markdown = String::new();
        let mut undo = UndoManager::new();
        undo.initialize_history("note/a");

        let _ = content_handler::handle_tab_key(
            &mut content,
            &mut markdown,
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert!(markdown.starts_with("    "));

        let _ = content_handler::handle_select_all(&mut content, &state);

        let _ = content_handler::handle_editor_action(
            &mut content,
            &mut markdown,
            &mut undo,
            Action::Edit(Edit::Insert('x')),
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert!(undo.get_previous_content("note/a").is_some());

        state.set_loading_note(true);
        let _ = content_handler::handle_loaded_note_content(
            &mut content,
            &mut markdown,
            &mut undo,
            &mut state,
            "note/a".to_string(),
            "loaded".to_string(),
        );
        assert!(!state.is_loading_note());
        assert_eq!(markdown, "loaded");

        let _ = content_handler::handle_editor_action(
            &mut content,
            &mut markdown,
            &mut undo,
            Action::Edit(Edit::Insert('!')),
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert_ne!(markdown, "loaded");
        assert!(undo.get_previous_content("note/a").is_some());
    }

    #[test]
    fn loaded_note_content_ignores_stale_results_and_applies_current_selection() {
        let mut state = EditorState::new();
        state.set_notebook_path("dummy".to_string());
        state.set_selected_note_path(Some("note/b".to_string()));
        state.set_loading_note(true);

        let mut content = Content::with_text("existing");
        let mut markdown = "existing".to_string();
        let mut undo = UndoManager::new();
        undo.initialize_history("note/b");

        let _ = content_handler::handle_loaded_note_content(
            &mut content,
            &mut markdown,
            &mut undo,
            &mut state,
            "note/a".to_string(),
            "stale".to_string(),
        );

        assert_eq!(markdown, "existing");
        assert!(state.is_loading_note());

        let _ = content_handler::handle_loaded_note_content(
            &mut content,
            &mut markdown,
            &mut undo,
            &mut state,
            "note/b".to_string(),
            "fresh".to_string(),
        );

        assert_eq!(markdown, "fresh");
        assert!(!state.is_loading_note());
    }

    #[test]
    fn handle_undo_replaces_editor_text_when_history_exists() {
        let mut state = EditorState::new();
        state.set_notebook_path("dummy".to_string());
        state.set_selected_note_path(Some("note/u".to_string()));

        let mut undo = UndoManager::new();
        undo.initialize_history("note/u");
        undo.add_to_history("note/u", "previous".to_string());

        let mut content = Content::with_text("current");
        let mut markdown = "current".to_string();
        let _ = undo_manager::handle_undo(
            &mut undo,
            &mut content,
            &mut markdown,
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );

        assert_eq!(markdown, "previous");
    }
}
