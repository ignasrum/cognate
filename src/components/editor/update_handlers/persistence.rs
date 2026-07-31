#[cfg(not(test))]
use native_dialog::{DialogBuilder, MessageLevel};

use super::*;
use crate::components::editor::LabelMutationRollback;
use crate::components::editor::state::editor_state::NoteConflict;
use crate::notebook::NotebookError;
use std::time::Duration;

pub(super) fn handle_debounced_metadata(state: &mut Editor, message: Message) -> Task<Message> {
    match message {
        Message::DebouncedMetadataSaveElapsed(generation) => {
            if generation != state.metadata_save_generation {
                return Task::none();
            }

            if state.metadata_save_in_flight {
                state.metadata_save_reschedule_after_in_flight = true;
                return Task::none();
            }

            state.metadata_save_in_flight = true;
            state.metadata_save_reschedule_after_in_flight = false;
            state.persist_metadata_snapshot_task(generation)
        }
        Message::DebouncedMetadataSaveCompleted(saved_generation, result) => {
            state.metadata_save_in_flight = false;

            if let Err(error) = &result {
                report_persistence_error(
                    "Failed to Save Notebook Metadata",
                    &format!(
                        "Cognate could not save notebook metadata for your latest changes:\n\n{}",
                        error.ui_message()
                    ),
                );
            } else {
                state.persisted_metadata = state.note_explorer.notes.clone();
                state.metadata_persisted_generation = saved_generation;
                #[cfg(debug_assertions)]
                eprintln!("Debounced metadata saved successfully.");
            }

            let should_save_latest = state.metadata_save_reschedule_after_in_flight
                || saved_generation < state.metadata_save_generation;
            state.metadata_save_reschedule_after_in_flight = false;

            if should_save_latest {
                state.metadata_save_in_flight = true;
                return state.persist_metadata_snapshot_task(state.metadata_save_generation);
            }

            Task::none()
        }
        _ => unreachable!("debounced-metadata handler received invalid message"),
    }
}

pub(super) fn handle_shutdown(state: &mut Editor, message: Message) -> Task<Message> {
    match message {
        Message::WindowCloseRequested(window_id) => {
            if state.shutdown_in_progress {
                return Task::none();
            }

            if state.state.connection_error().is_some() {
                return window::close(window_id);
            }

            state.shutdown_in_progress = true;

            let notebook_path = state.state.notebook_path().to_string();
            let content_note_path = state.content_note_path.clone();
            let markdown_text = state.markdown_text.clone();
            let content_dirty = state.content_dirty();
            let notes = state.note_explorer.notes.clone();
            let metadata_dirty = state.metadata_dirty();

            Task::perform(
                async move {
                    let result = match tokio::time::timeout(
                        Duration::from_secs(5),
                        note_coordinator::flush_for_shutdown(
                            &notebook_path,
                            content_note_path,
                            &markdown_text,
                            content_dirty,
                            &notes,
                            metadata_dirty,
                        ),
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => Err(NotebookError::api(
                            "shutdown flush",
                            None,
                            None,
                            "request timed out after 5 seconds",
                            true,
                        )),
                    };
                    (window_id, result)
                },
                |(window_id, result)| Message::ShutdownFlushCompleted(window_id, result),
            )
        }
        Message::ShutdownFlushCompleted(window_id, result) => {
            state.shutdown_in_progress = false;

            match result {
                Ok(()) => window::close(window_id),
                Err(_error) => {
                    eprintln!(
                        "[cognate] shutdown flush failed; keeping window open: {}",
                        _error.ui_message()
                    );
                    #[cfg(not(test))]
                    {
                        let _ = DialogBuilder::message()
                            .set_level(MessageLevel::Error)
                            .set_title("Failed to Save Before Exit")
                            .set_text(format!(
                                "Cognate could not safely save your latest changes before exit:\n\n{}",
                                _error.ui_message()
                            ))
                            .alert()
                            .show();
                    }
                    Task::none()
                }
            }
        }
        _ => unreachable!("shutdown handler received invalid message"),
    }
}

pub(super) fn handle_save_feedback(state: &mut Editor, message: Message) -> Task<Message> {
    match message {
        Message::MetadataSaved(result, rollback) => {
            if let Err(error) = result {
                if let Some(rollback) = rollback {
                    restore_label_mutation(state, rollback);
                }
                report_persistence_error(
                    "Failed to Save Notebook Metadata",
                    &format!(
                        "Cognate could not save notebook metadata:\n\n{}",
                        error.ui_message()
                    ),
                );
            } else {
                #[cfg(debug_assertions)]
                eprintln!("Metadata saved successfully.");
            }
            Task::none()
        }
        Message::NoteContentSaved(result) => {
            if let Err(error) = result {
                if open_conflict_dialog(state, error.clone()) {
                    return Task::none();
                }
                report_persistence_error(
                    "Failed to Save Note Content",
                    &format!(
                        "Cognate could not save note content to disk:\n\n{}",
                        error.ui_message()
                    ),
                );
            } else {
                // API note writes update the metadata timestamp as part of the
                // same conditional write. Keep shutdown's metadata snapshot in
                // sync so it does not issue a second, stale metadata request.
                state.persisted_metadata = state.note_explorer.notes.clone();
                #[cfg(debug_assertions)]
                eprintln!("Note content saved successfully.");
            }
            Task::none()
        }
        Message::OfflineReplayCompleted(result) => {
            if let Err(error) = result
                && !open_conflict_dialog(state, error.clone())
            {
                eprintln!("[cognate] startup_offline_replay_failed: {error}");
            }
            state
                .note_explorer
                .update(note_explorer::Message::LoadNotes)
                .map(Message::NoteExplorerMsg)
        }
        Message::ConflictCopySaved(result) => match result {
            Ok(path) => {
                state.state.hide_conflict_dialog();
                eprintln!("Saved conflict copy as {path}");
                state
                    .note_explorer
                    .update(note_explorer::Message::LoadNotes)
                    .map(Message::NoteExplorerMsg)
            }
            Err(error) => {
                report_persistence_error("Failed to Save Conflict Copy", &error.ui_message());
                Task::none()
            }
        },
        Message::ScaleSaved(result) => {
            if let Err(error) = result {
                report_persistence_error(
                    "Failed to Save UI Scale",
                    &format!(
                        "Cognate could not save the updated UI scale to config:\n\n{}",
                        error
                    ),
                );
            }
            Task::none()
        }
        _ => unreachable!("save-feedback handler received invalid message"),
    }
}

fn open_conflict_dialog(state: &mut Editor, error: NotebookError) -> bool {
    let NotebookError::Conflict {
        context: _,
        note_path,
        local_content,
        server_content,
        server_revision,
    } = error
    else {
        return false;
    };

    let Some(note_path) = note_path.or_else(|| state.state.selected_note_path().cloned()) else {
        return false;
    };
    let duplicate = state.state.conflict().is_some_and(|existing| {
        existing.note_path == note_path && existing.server_revision == server_revision
    });
    if !duplicate {
        eprintln!(
            "[cognate] conflict_dialog_open note={} server_revision={}",
            note_path,
            server_revision.chars().take(12).collect::<String>()
        );
        state.state.show_conflict_dialog(NoteConflict {
            note_path,
            local_content,
            server_content,
            server_revision,
        });
    }
    true
}

fn restore_label_mutation(state: &mut Editor, rollback: LabelMutationRollback) {
    state
        .state
        .set_selected_note_labels(rollback.selected_labels);
    state.state.set_new_label_text(rollback.input_text);

    if let Some(note) = state
        .note_explorer
        .notes
        .iter_mut()
        .find(|note| note.rel_path == rollback.note_path)
    {
        note.labels = rollback.note_labels;
    }

    state.visualizer.sync_notes(&state.note_explorer.notes);
}

fn report_persistence_error(title: &str, detail: &str) {
    eprintln!("{}: {}", title, detail);

    #[cfg(not(test))]
    {
        let _ = DialogBuilder::message()
            .set_level(MessageLevel::Error)
            .set_title(title)
            .set_text(detail)
            .alert()
            .show();
    }
}
