use super::*;

pub(super) fn handle(state: &mut Editor, message: Message) -> Task<Message> {
    let previous_markdown = state.markdown_text.clone();
    let task = match message {
        Message::NewNote => {
            state.state.show_new_note_dialog();
            Task::none()
        }
        Message::NewNoteInputChanged(text) => {
            state.state.update_new_note_path(text);
            Task::none()
        }
        Message::CreateNote => {
            note_actions::handle_create_note(&mut state.state, state.note_explorer.notes.clone())
        }
        Message::CancelNewNote => {
            state.state.hide_new_note_dialog();
            Task::none()
        }
        Message::NoteCreated(result) => {
            note_actions::handle_note_created(result, &mut state.note_explorer)
        }
        Message::DeleteNote => note_actions::handle_delete_note(&mut state.state),
        Message::ConfirmDeleteNote(confirmed) => note_actions::handle_confirm_delete_note(
            confirmed,
            &mut state.state,
            state.note_explorer.notes.clone(),
        ),
        Message::ConfirmDeleteEmbeddedImages(confirmed) => {
            state.handle_confirm_delete_embedded_images(confirmed)
        }
        Message::NoteDeleted(result, deleted_path) => note_actions::handle_note_deleted(
            result,
            deleted_path,
            &mut state.state,
            &mut state.content,
            &mut state.markdown_text,
            &mut state.undo_manager,
            &mut state.note_explorer,
        ),
        Message::MoveNote => {
            if let Some(current_path) = state.state.selected_note_path() {
                state.state.show_move_note_dialog(current_path.clone());
            }
            Task::none()
        }
        Message::MoveNoteInputChanged(text) => {
            state.state.update_move_note_path(text);
            Task::none()
        }
        Message::ConfirmMoveNote => note_actions::handle_confirm_move_note(
            &mut state.state,
            state.note_explorer.notes.clone(),
        ),
        Message::CancelMoveNote => {
            state.state.hide_move_note_dialog();
            note_actions::get_select_note_command(
                state.state.selected_note_path(),
                &state.note_explorer.notes,
            )
        }
        Message::NoteMoved(result, old_path) => note_actions::handle_note_moved(
            result,
            old_path,
            &mut state.state,
            &mut state.undo_manager,
            &mut state.note_explorer,
        ),
        _ => unreachable!("note-lifecycle handler received invalid message"),
    };

    if state.state.selected_note_path().is_none() {
        state.content_note_path = None;
    }

    if state.markdown_text != previous_markdown {
        if state.state.selected_note_path().is_none() {
            state.embedded_image_workflow.clear_all();
            state.content_note_path = None;
            state.state.hide_embedded_image_delete_dialog();
        } else {
            state.prune_embedded_images_for_current_markdown();
        }
        state.sync_markdown_preview();
        return Task::batch(vec![task, state.scroll_preview_to_cursor_task()]);
    }

    task
}
