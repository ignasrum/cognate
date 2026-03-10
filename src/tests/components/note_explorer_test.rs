#[cfg(test)]
mod tests {
    use crate::components::note_explorer::{Message, NoteExplorer};
    use crate::notebook::NoteMetadata;

    fn sample_notes() -> Vec<NoteMetadata> {
        vec![
            NoteMetadata {
                rel_path: "b/note2".to_string(),
                labels: vec![],
            },
            NoteMetadata {
                rel_path: "a/note1".to_string(),
                labels: vec![],
            },
            NoteMetadata {
                rel_path: "a/sub/note3".to_string(),
                labels: vec![],
            },
        ]
    }

    #[test]
    fn notes_loaded_sorts_and_tracks_folders() {
        let mut explorer = NoteExplorer::new("dummy".to_string());
        let _ = explorer.update(Message::NotesLoaded(sample_notes()));

        let sorted_paths: Vec<String> = explorer.notes.iter().map(|n| n.rel_path.clone()).collect();
        assert_eq!(
            sorted_paths,
            vec![
                "a/note1".to_string(),
                "a/sub/note3".to_string(),
                "b/note2".to_string()
            ]
        );

        assert!(explorer.expanded_folders.contains_key(""));
        assert!(explorer.expanded_folders.contains_key("a"));
        assert!(explorer.expanded_folders.contains_key("a/sub"));
        assert!(explorer.expanded_folders.contains_key("b"));
    }

    #[test]
    fn toggle_and_expand_messages_update_state() {
        let mut explorer = NoteExplorer::new("dummy".to_string());
        let _ = explorer.update(Message::NotesLoaded(sample_notes()));

        assert_eq!(explorer.expanded_folders.get("a"), Some(&false));
        let _ = explorer.update(Message::ToggleFolder("a".to_string()));
        assert_eq!(explorer.expanded_folders.get("a"), Some(&true));

        let _ = explorer.update(Message::CollapseAllAndExpandToNote("a/sub/note3".to_string()));
        assert_eq!(explorer.expanded_folders.get("a"), Some(&true));
        assert_eq!(explorer.expanded_folders.get("a/sub"), Some(&true));
        assert_eq!(explorer.expanded_folders.get("b"), Some(&false));
    }

    #[test]
    fn view_builds_for_empty_and_non_empty_state() {
        let mut explorer = NoteExplorer::new("dummy".to_string());

        {
            let _empty_view = explorer.view(None);
        }

        let _ = explorer.update(Message::NotesLoaded(sample_notes()));
        let _ = explorer.update(Message::CollapseAllAndExpandToNote("a/sub/note3".to_string()));
        let selected = "a/sub/note3".to_string();
        {
            let _populated_view = explorer.view(Some(&selected));
        }
    }
}
