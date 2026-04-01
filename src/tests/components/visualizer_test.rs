#[cfg(test)]
mod tests {
    use crate::components::visualizer::{Message, Visualizer};
    use crate::notebook::NoteMetadata;

    fn sample_notes() -> Vec<NoteMetadata> {
        vec![
            NoteMetadata {
                rel_path: "x/n1".to_string(),
                labels: vec!["urgent".to_string(), "work".to_string()],
                last_updated: None,
            },
            NoteMetadata {
                rel_path: "x/n2".to_string(),
                labels: vec!["work".to_string()],
                last_updated: None,
            },
            NoteMetadata {
                rel_path: "y/n3".to_string(),
                labels: vec![],
                last_updated: None,
            },
        ]
    }

    fn sample_notes_with_messy_labels() -> Vec<NoteMetadata> {
        vec![
            NoteMetadata {
                rel_path: "a/n1".to_string(),
                labels: vec![" work ".to_string(), "work".to_string(), "".to_string()],
                last_updated: None,
            },
            NoteMetadata {
                rel_path: "a/n2".to_string(),
                labels: vec![
                    "work".to_string(),
                    "urgent".to_string(),
                    " urgent ".to_string(),
                ],
                last_updated: None,
            },
            NoteMetadata {
                rel_path: "b/n3".to_string(),
                labels: vec!["urgent".to_string()],
                last_updated: None,
            },
        ]
    }

    #[test]
    fn update_notes_builds_label_connected_graph() {
        let mut visualizer = Visualizer::new();
        let _ = visualizer.update(Message::UpdateNotes(sample_notes()));

        assert_eq!(visualizer.notes.len(), 3);
        assert_eq!(visualizer.debug_graph_stats(), (3, 1, 2));

        let _ = visualizer.update(Message::FocusOnNote(Some("x/n2".to_string())));
        assert_eq!(visualizer.debug_graph_stats(), (3, 1, 2));
    }

    #[test]
    fn view_renders_with_and_without_notes() {
        let mut visualizer = Visualizer::new();
        {
            let _empty_view = visualizer.view();
        }

        let _ = visualizer.update(Message::UpdateNotes(sample_notes()));
        {
            let _populated_view = visualizer.view();
        }
    }

    #[test]
    fn update_notes_normalizes_and_deduplicates_labels_for_graph_stats() {
        let mut visualizer = Visualizer::new();
        let _ = visualizer.update(Message::UpdateNotes(sample_notes_with_messy_labels()));

        // Expected:
        // - labels normalize to "urgent" and "work" (2 unique labels)
        // - edges are n1<->n2 (work) and n2<->n3 (urgent) => 2 edges
        assert_eq!(visualizer.debug_graph_stats(), (3, 2, 2));
    }
}
