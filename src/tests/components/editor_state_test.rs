#[cfg(test)]
mod tests {
    use crate::components::editor::state::editor_state::EditorState;
    use crate::notebook::NoteMetadata;

    #[test]
    fn dialog_and_path_transitions_work() {
        let mut state = EditorState::new();

        state.show_new_note_dialog();
        assert!(!state.show_new_note_input());

        state.set_notebook_path("notebook".to_string());
        state.show_new_note_dialog();
        assert!(state.show_new_note_input());
        state.update_new_note_path("folder/new_note".to_string());
        assert_eq!(state.new_note_path_input(), "folder/new_note");
        state.hide_new_note_dialog();
        assert!(!state.show_new_note_input());

        state.show_move_note_dialog("a/b".to_string());
        assert!(state.show_move_note_input());
        assert_eq!(state.move_note_current_path(), Some(&"a/b".to_string()));
        assert_eq!(state.move_note_new_path_input(), "a/b");
        state.update_move_note_path("a/c".to_string());
        assert_eq!(state.move_note_new_path_input(), "a/c");
        assert_eq!(state.take_move_note_current_path(), Some("a/b".to_string()));
        assert_eq!(state.move_note_current_path(), None);
        state.hide_move_note_dialog();
        assert!(!state.show_move_note_input());

        state.show_rename_folder_dialog("folder".to_string());
        assert!(state.show_move_note_input());
        assert_eq!(state.move_note_current_path(), Some(&"folder".to_string()));
        assert_eq!(state.move_note_new_path_input(), "folder");
    }

    #[test]
    fn visibility_toggles_reset_other_panels() {
        let mut state = EditorState::new();
        state.set_notebook_path("notebook".to_string());

        state.show_new_note_dialog();
        assert!(state.show_new_note_input());
        state.toggle_visualizer();
        assert!(state.show_visualizer());
        assert!(!state.show_new_note_input());
        assert!(!state.show_move_note_input());
        assert!(!state.show_about_info());

        state.show_move_note_dialog("note".to_string());
        state.toggle_about_info();
        assert!(state.show_about_info());
        assert!(!state.show_visualizer());
        assert!(!state.show_new_note_input());
        assert!(!state.show_move_note_input());
        assert!(state.is_any_dialog_open());
    }

    #[test]
    fn note_and_label_accessors_work() {
        let mut state = EditorState::new();
        state.set_app_version("1.2.3".to_string());
        state.set_selected_note_path(Some("x/y".to_string()));
        state.set_selected_note_labels(vec!["a".to_string(), "b".to_string()]);
        state.set_new_label_text("new".to_string());
        state.set_loading_note(true);

        assert_eq!(state.app_version(), "1.2.3");
        assert_eq!(state.selected_note_path(), Some(&"x/y".to_string()));
        assert_eq!(state.selected_note_labels(), &["a".to_string(), "b".to_string()]);
        assert_eq!(state.new_label_text(), "new");
        assert!(state.is_loading_note());

        state.clear_new_label_text();
        assert_eq!(state.new_label_text(), "");
        assert_eq!(state.take_selected_note_path(), Some("x/y".to_string()));
        assert_eq!(state.selected_note_path(), None);
    }

    #[test]
    fn folder_detection_uses_note_parents() {
        let state = EditorState::new();
        let notes = vec![
            NoteMetadata {
                rel_path: "work/note1".to_string(),
                labels: vec![],
            },
            NoteMetadata {
                rel_path: "work/sub/note2".to_string(),
                labels: vec![],
            },
            NoteMetadata {
                rel_path: "top".to_string(),
                labels: vec![],
            },
        ];

        assert!(state.is_folder_path("work", &notes));
        assert!(state.is_folder_path("work/sub", &notes));
        assert!(!state.is_folder_path("top", &notes));
        assert!(!state.is_folder_path("missing", &notes));
    }
}
