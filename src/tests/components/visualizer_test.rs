#[cfg(test)]
mod tests {
    use crate::components::visualizer::{Message, Visualizer};
    use crate::notebook::NoteMetadata;

    fn sample_notes() -> Vec<NoteMetadata> {
        vec![
            NoteMetadata {
                rel_path: "x/n1".to_string(),
                labels: vec!["urgent".to_string(), "work".to_string()],
            },
            NoteMetadata {
                rel_path: "x/n2".to_string(),
                labels: vec!["work".to_string()],
            },
            NoteMetadata {
                rel_path: "y/n3".to_string(),
                labels: vec![],
            },
        ]
    }

    #[test]
    fn update_notes_sets_labels_and_toggle_changes_state() {
        let mut visualizer = Visualizer::new();
        let _ = visualizer.update(Message::UpdateNotes(sample_notes()));

        assert_eq!(visualizer.notes.len(), 3);
        assert_eq!(visualizer.expanded_labels.get("urgent"), Some(&false));
        assert_eq!(visualizer.expanded_labels.get("work"), Some(&false));

        let _ = visualizer.update(Message::ToggleLabel("work".to_string()));
        assert_eq!(visualizer.expanded_labels.get("work"), Some(&true));

        let _ = visualizer.update(Message::ToggleLabel("work".to_string()));
        assert_eq!(visualizer.expanded_labels.get("work"), Some(&false));
    }

    #[test]
    fn view_renders_with_and_without_notes() {
        let mut visualizer = Visualizer::new();
        {
            let _empty_view = visualizer.view();
        }

        let _ = visualizer.update(Message::UpdateNotes(sample_notes()));
        let _ = visualizer.update(Message::ToggleLabel("work".to_string()));
        {
            let _populated_view = visualizer.view();
        }
    }
}
