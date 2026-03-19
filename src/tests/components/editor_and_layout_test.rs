#[cfg(test)]
mod tests {
    use crate::components::editor::state::editor_state::EditorState;
    use crate::components::editor::ui::{dialogs, input_fields, layout};
    use crate::components::editor::{Editor, Message as EditorMessage};
    use crate::components::note_explorer;
    use crate::components::visualizer;
    use crate::configuration::Configuration;
    use crate::notebook::{MarkdownWithAttachmentsExportSummary, NoteMetadata};
    use iced::widget::text_editor::Content;

    #[test]
    fn editor_create_covers_known_and_unknown_themes() {
        let cfg_known = Configuration {
            theme: "Dark".to_string(),
            notebook_path: String::new(),
            scale: 1.0,
            config_path: "config.json".to_string(),
            version: "0.1.0".to_string(),
        };
        let _ = Editor::create(cfg_known);

        let cfg_unknown = Configuration {
            theme: "CustomThemeThatDoesNotExist".to_string(),
            notebook_path: String::new(),
            scale: 1.0,
            config_path: "config.json".to_string(),
            version: "0.1.0".to_string(),
        };
        let _ = Editor::create(cfg_unknown);
    }

    #[test]
    fn editor_update_view_and_subscription_are_callable() {
        let mut editor = Editor::default();

        let _ = Editor::view(&editor);
        let _ = Editor::subscription(&editor);

        let messages = vec![
            EditorMessage::AboutButtonClicked,
            EditorMessage::MarkdownLinkClicked("https://example.com".to_string()),
            EditorMessage::ToggleVisualizer,
            EditorMessage::NewNote,
            EditorMessage::NewNoteInputChanged("new/path".to_string()),
            EditorMessage::CreateNote,
            EditorMessage::CancelNewNote,
            EditorMessage::DeleteNote,
            EditorMessage::ConfirmDeleteNote(false),
            EditorMessage::ConfirmDeleteEmbeddedImages(false),
            EditorMessage::MoveNote,
            EditorMessage::MoveNoteInputChanged("moved/path".to_string()),
            EditorMessage::ConfirmMoveNote,
            EditorMessage::CancelMoveNote,
            EditorMessage::ExportMarkdownWithAttachments,
            EditorMessage::NoteMoved(Err("move failed".to_string()), "old/path".to_string()),
            EditorMessage::NoteDeleted(Err("delete failed".to_string()), "to/delete".to_string()),
            EditorMessage::MetadataSaved(Ok(())),
            EditorMessage::MetadataSaved(Err("meta failed".to_string())),
            EditorMessage::NoteContentSaved(Ok(())),
            EditorMessage::NoteContentSaved(Err("save failed".to_string())),
            EditorMessage::MarkdownWithAttachmentsExported(Err(
                "markdown export failed".to_string()
            )),
            EditorMessage::MarkdownWithAttachmentsExported(Ok(
                MarkdownWithAttachmentsExportSummary {
                    markdown_path: "/tmp/exported_markdown/note.md".to_string(),
                    attachments_dir: "/tmp/exported_markdown/images".to_string(),
                    exported_count: 1,
                    skipped_count: 0,
                    rewritten_reference_count: 1,
                },
            )),
            EditorMessage::LoadedNoteContent(
                "folder/note".to_string(),
                "body".to_string(),
                std::collections::HashMap::new(),
            ),
            EditorMessage::Undo,
            EditorMessage::Redo,
            EditorMessage::NoteExplorerMsg(note_explorer::Message::ToggleFolder(
                "folder".to_string(),
            )),
            EditorMessage::VisualizerMsg(visualizer::Message::FocusOnNote(None)),
            EditorMessage::InitiateFolderRename("folder".to_string()),
            EditorMessage::NoteCreated(Err("create failed".to_string())),
            EditorMessage::NoteMoved(Ok("new/path".to_string()), "old/path".to_string()),
            EditorMessage::NoteDeleted(Ok(()), "to/delete".to_string()),
        ];

        for message in messages {
            let _ = Editor::update(&mut editor, message);
        }

        let _ = Editor::update(
            &mut editor,
            EditorMessage::NoteExplorerMsg(note_explorer::Message::NotesLoaded(vec![
                NoteMetadata {
                    rel_path: "folder/note".to_string(),
                    labels: vec!["tag".to_string()],
                    last_updated: None,
                },
            ])),
        );
        let _ = Editor::update(
            &mut editor,
            EditorMessage::NoteSelected("folder/note".to_string()),
        );
    }

    #[test]
    fn editor_typing_updates_last_updated_in_memory() {
        let mut editor = Editor::default();

        let _ = Editor::update(
            &mut editor,
            EditorMessage::NoteExplorerMsg(note_explorer::Message::NotesLoaded(vec![
                NoteMetadata {
                    rel_path: "folder/note".to_string(),
                    labels: vec![],
                    last_updated: None,
                },
            ])),
        );
        let _ = Editor::update(
            &mut editor,
            EditorMessage::NoteSelected("folder/note".to_string()),
        );

        assert_eq!(editor.debug_last_updated_for("folder/note"), None);

        let _ = Editor::update(
            &mut editor,
            EditorMessage::EditorAction(iced::widget::text_editor::Action::Edit(
                iced::widget::text_editor::Edit::Insert('x'),
            )),
        );

        assert!(
            editor.debug_last_updated_for("folder/note").is_some(),
            "typing should update last_updated in memory so the UI refreshes immediately"
        );
    }

    #[test]
    fn layout_and_dialog_builders_cover_main_variants() {
        let mut state = EditorState::new();
        let content = Content::with_text("hello");
        let markdown_content = iced::widget::markdown::Content::parse("hello");
        let markdown_image_handles = std::collections::HashMap::new();
        let mut explorer = note_explorer::NoteExplorer::new("dummy".to_string());
        explorer.notes = vec![
            NoteMetadata {
                rel_path: "folder/note".to_string(),
                labels: vec!["tag".to_string()],
                last_updated: None,
            },
            NoteMetadata {
                rel_path: "single".to_string(),
                labels: vec![],
                last_updated: None,
            },
        ];
        let visualizer = visualizer::Visualizer::new();

        let _ = layout::generate_layout(
            &state,
            &content,
            &markdown_content,
            &markdown_image_handles,
            &explorer,
            &visualizer,
            None,
        );

        state.set_notebook_path("dummy".to_string());
        state.set_selected_note_path(Some("folder/note".to_string()));
        state.set_selected_note_labels(vec!["tag".to_string()]);
        let _ = layout::generate_layout(
            &state,
            &content,
            &markdown_content,
            &markdown_image_handles,
            &explorer,
            &visualizer,
            Some((0, 1)),
        );

        state.set_show_about_info(true);
        let _ = layout::generate_layout(
            &state,
            &content,
            &markdown_content,
            &markdown_image_handles,
            &explorer,
            &visualizer,
            None,
        );
        state.set_show_about_info(false);

        state.set_show_visualizer(true);
        let _ = layout::generate_layout(
            &state,
            &content,
            &markdown_content,
            &markdown_image_handles,
            &explorer,
            &visualizer,
            None,
        );
        state.set_show_visualizer(false);

        state.show_new_note_dialog();
        let _ = layout::generate_layout(
            &state,
            &content,
            &markdown_content,
            &markdown_image_handles,
            &explorer,
            &visualizer,
            None,
        );
        state.hide_new_note_dialog();

        state.show_move_note_dialog("folder".to_string());
        let _ = layout::generate_layout(
            &state,
            &content,
            &markdown_content,
            &markdown_image_handles,
            &explorer,
            &visualizer,
            None,
        );
        state.hide_move_note_dialog();

        state.show_embedded_image_delete_dialog(2);
        let _ = layout::generate_layout(
            &state,
            &content,
            &markdown_content,
            &markdown_image_handles,
            &explorer,
            &visualizer,
            None,
        );
        state.hide_embedded_image_delete_dialog();

        let _ = dialogs::about_dialog("0.2.0");
        let _ = dialogs::new_note_dialog("new/path");
        let _ = dialogs::move_note_dialog("folder/note", "other/note", false);
        let _ = dialogs::move_note_dialog("folder", "renamed", true);
        let _ = dialogs::confirm_embedded_image_delete_dialog(2);

        let _ = input_fields::create_labels_section(
            Some(&"folder/note".to_string()),
            &["tag".to_string()],
            "new",
        );
        let _ = input_fields::create_labels_section(None, &[], "");
    }
}
