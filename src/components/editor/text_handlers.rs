use iced::task::Task;
use iced::widget::text_editor::{Action, Edit};
use std::sync::Arc;

use super::clipboard::{
    ClipboardPastePayload, paste_text_from_action, read_clipboard_image_file_as_base64_from_text,
    read_clipboard_paste_payload,
};
use super::embedded_images::save_base64_image_for_note;
use super::*;
use crate::components::editor::text_management::undo_manager;

impl Editor {
    pub(super) fn handle_text_messages(state: &mut Self, message: Message) -> Task<Message> {
        match message {
            Message::HandleTabKey => {
                let previous_markdown = state.markdown_text.clone();
                let save_task = content_handler::handle_tab_key(
                    &mut state.content,
                    &mut state.markdown_text,
                    state.state.selected_note_path(),
                    state.state.notebook_path(),
                    &state.state,
                );

                if state.markdown_text != previous_markdown {
                    let metadata_save_task =
                        state.touch_selected_note_last_updated_and_schedule_save_task();
                    state.prune_embedded_images_for_current_markdown();
                    state.sync_markdown_preview();
                    return Task::batch(vec![
                        save_task,
                        metadata_save_task,
                        state.scroll_preview_to_cursor_task(),
                    ]);
                }

                state.with_preview_scroll_task(save_task)
            }
            Message::SelectAll => {
                let task = content_handler::handle_select_all(&mut state.content, &state.state);
                state.with_preview_scroll_task(task)
            }
            Message::Undo => {
                let previous_markdown = state.markdown_text.clone();
                let task = undo_manager::handle_undo(
                    &mut state.undo_manager,
                    &mut state.content,
                    &mut state.markdown_text,
                    state.state.selected_note_path(),
                    state.state.notebook_path(),
                    &state.state,
                );
                if state.markdown_text != previous_markdown {
                    let metadata_save_task =
                        state.touch_selected_note_last_updated_and_schedule_save_task();
                    state.prune_embedded_images_for_current_markdown();
                    state.sync_markdown_preview();
                    return Task::batch(vec![
                        task,
                        metadata_save_task,
                        state.scroll_preview_to_cursor_task(),
                    ]);
                }
                state.with_preview_scroll_task(task)
            }
            Message::Redo => {
                let previous_markdown = state.markdown_text.clone();
                let task = undo_manager::handle_redo(
                    &mut state.undo_manager,
                    &mut state.content,
                    &mut state.markdown_text,
                    state.state.selected_note_path(),
                    state.state.notebook_path(),
                    &state.state,
                );
                if state.markdown_text != previous_markdown {
                    let metadata_save_task =
                        state.touch_selected_note_last_updated_and_schedule_save_task();
                    state.prune_embedded_images_for_current_markdown();
                    state.sync_markdown_preview();
                    return Task::batch(vec![
                        task,
                        metadata_save_task,
                        state.scroll_preview_to_cursor_task(),
                    ]);
                }
                state.with_preview_scroll_task(task)
            }
            Message::PasteFromClipboard => Self::handle_paste_from_clipboard_shortcut(state),
            Message::EditorAction(action) => {
                if matches!(action, Action::Edit(Edit::Paste(_))) {
                    return Self::handle_paste_action(state, action);
                }

                let dereferenced_image_ids = state.dereferenced_embedded_images_for_action(&action);
                if !dereferenced_image_ids.is_empty() {
                    state
                        .embedded_image_workflow
                        .stage_pending_deletion(dereferenced_image_ids, action);
                    state.state.show_embedded_image_delete_dialog(
                        state.embedded_image_workflow.pending_deletion_count(),
                    );
                    return Task::none();
                }

                let previous_markdown = state.markdown_text.clone();
                let save_task = content_handler::handle_editor_action(
                    &mut state.content,
                    &mut state.markdown_text,
                    &mut state.undo_manager,
                    action,
                    state.state.selected_note_path(),
                    state.state.notebook_path(),
                    &state.state,
                );

                if state.markdown_text != previous_markdown {
                    let metadata_save_task =
                        state.touch_selected_note_last_updated_and_schedule_save_task();
                    state.prune_embedded_images_for_current_markdown();
                    state.sync_markdown_preview();
                    return Task::batch(vec![
                        save_task,
                        metadata_save_task,
                        state.scroll_preview_to_cursor_task(),
                    ]);
                }

                state.with_preview_scroll_task(save_task)
            }
            Message::LoadedNoteContent(note_path, new_content, images) => {
                if state.state.selected_note_path() != Some(&note_path) {
                    return Task::none();
                }
                state.content_note_path = Some(note_path.clone());
                state.embedded_image_workflow.set_loaded_images(images);
                let previous_markdown = state.markdown_text.clone();
                let task = content_handler::handle_loaded_note_content(
                    &mut state.content,
                    &mut state.markdown_text,
                    &mut state.undo_manager,
                    &mut state.state,
                    note_path,
                    new_content,
                );
                if state.markdown_text != previous_markdown {
                    state.prune_embedded_images_for_current_markdown();
                    state.sync_markdown_preview();
                }
                state.with_preview_scroll_task(task)
            }
            _ => unreachable!("text handler received non-text message"),
        }
    }

    fn handle_paste_from_clipboard_shortcut(state: &mut Self) -> Task<Message> {
        if state.state.selected_note_path().is_none()
            || state.state.show_visualizer()
            || state.state.show_move_note_input()
            || state.state.show_new_note_input()
            || state.state.show_embedded_image_delete_confirmation()
            || state.state.show_about_info()
        {
            return Task::none();
        }

        let clipboard_payload = match read_clipboard_paste_payload() {
            Ok(payload) => payload,
            Err(_err) => {
                #[cfg(debug_assertions)]
                eprintln!("Failed to read clipboard data: {}", _err);
                None
            }
        };

        match clipboard_payload {
            Some(ClipboardPastePayload::ImageBase64(image_base64)) => {
                let Some(selected_note_path) = state.state.selected_note_path().cloned() else {
                    return Task::none();
                };

                state.undo_manager.add_to_history(
                    &selected_note_path,
                    state.markdown_text.clone(),
                    state.content.cursor(),
                );

                let image_tag = match save_base64_image_for_note(
                    state.state.notebook_path(),
                    &selected_note_path,
                    &image_base64,
                ) {
                    Ok(relative_path) => format!("![image]({relative_path})"),
                    Err(_err) => {
                        #[cfg(debug_assertions)]
                        eprintln!("Failed to persist pasted image: {}", _err);
                        return Task::none();
                    }
                };

                state
                    .content
                    .perform(Action::Edit(Edit::Paste(Arc::new(image_tag))));
                state.markdown_text = state.content.text();
                state.prune_embedded_images_for_current_markdown();
                let metadata_save_task =
                    state.touch_selected_note_last_updated_and_schedule_save_task();
                state.sync_markdown_preview();

                let notebook_path = state.state.notebook_path().to_string();
                let note_path = selected_note_path;
                let content_text = state.markdown_text.clone();
                let save_content_task = Task::perform(
                    async move {
                        notebook::save_note_content(notebook_path, note_path, content_text).await
                    },
                    Message::NoteContentSaved,
                );

                Task::batch(vec![
                    save_content_task,
                    metadata_save_task,
                    state.scroll_preview_to_cursor_task(),
                ])
            }
            Some(ClipboardPastePayload::Text(text_to_paste)) => {
                let previous_markdown = state.markdown_text.clone();
                let save_task = content_handler::handle_editor_action(
                    &mut state.content,
                    &mut state.markdown_text,
                    &mut state.undo_manager,
                    Action::Edit(Edit::Paste(Arc::new(text_to_paste))),
                    state.state.selected_note_path(),
                    state.state.notebook_path(),
                    &state.state,
                );

                if state.markdown_text != previous_markdown {
                    let metadata_save_task =
                        state.touch_selected_note_last_updated_and_schedule_save_task();
                    state.prune_embedded_images_for_current_markdown();
                    state.sync_markdown_preview();
                    return Task::batch(vec![
                        save_task,
                        metadata_save_task,
                        state.scroll_preview_to_cursor_task(),
                    ]);
                }

                state.with_preview_scroll_task(save_task)
            }
            None => Task::none(),
        }
    }

    fn handle_paste_action(state: &mut Self, fallback_action: Action) -> Task<Message> {
        if state.state.selected_note_path().is_none()
            || state.state.show_visualizer()
            || state.state.show_move_note_input()
            || state.state.show_new_note_input()
            || state.state.show_embedded_image_delete_confirmation()
            || state.state.show_about_info()
        {
            let task = content_handler::handle_editor_action(
                &mut state.content,
                &mut state.markdown_text,
                &mut state.undo_manager,
                fallback_action,
                state.state.selected_note_path(),
                state.state.notebook_path(),
                &state.state,
            );
            return state.with_preview_scroll_task(task);
        }

        let fallback_text = paste_text_from_action(&fallback_action);
        let image_base64 = fallback_text
            .as_deref()
            .filter(|text| !text.is_empty())
            .and_then(read_clipboard_image_file_as_base64_from_text);

        let Some(image_base64) = image_base64 else {
            let task = content_handler::handle_editor_action(
                &mut state.content,
                &mut state.markdown_text,
                &mut state.undo_manager,
                fallback_action,
                state.state.selected_note_path(),
                state.state.notebook_path(),
                &state.state,
            );
            return state.with_preview_scroll_task(task);
        };

        let Some(selected_note_path) = state.state.selected_note_path().cloned() else {
            return Task::none();
        };

        state.undo_manager.add_to_history(
            &selected_note_path,
            state.markdown_text.clone(),
            state.content.cursor(),
        );

        let image_tag = match save_base64_image_for_note(
            state.state.notebook_path(),
            &selected_note_path,
            &image_base64,
        ) {
            Ok(relative_path) => format!("![image]({relative_path})"),
            Err(_err) => {
                #[cfg(debug_assertions)]
                eprintln!("Failed to persist pasted image: {}", _err);
                return Task::none();
            }
        };

        state
            .content
            .perform(Action::Edit(Edit::Paste(Arc::new(image_tag))));
        state.markdown_text = state.content.text();
        state.prune_embedded_images_for_current_markdown();
        let metadata_save_task = state.touch_selected_note_last_updated_and_schedule_save_task();
        state.sync_markdown_preview();

        let notebook_path = state.state.notebook_path().to_string();
        let note_path = selected_note_path;
        let content_text = state.markdown_text.clone();
        let save_content_task = Task::perform(
            async move { notebook::save_note_content(notebook_path, note_path, content_text).await },
            Message::NoteContentSaved,
        );

        Task::batch(vec![
            save_content_task,
            metadata_save_task,
            state.scroll_preview_to_cursor_task(),
        ])
    }
}
