use native_dialog::{DialogBuilder, MessageLevel};

use super::*;

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

            state.shutdown_in_progress = true;

            let notebook_path = state.state.notebook_path().to_string();
            let content_note_path = state.content_note_path.clone();
            let markdown_text = state.markdown_text.clone();
            let notes = state.note_explorer.notes.clone();

            Task::perform(
                async move {
                    let result = note_coordinator::flush_for_shutdown(
                        &notebook_path,
                        content_note_path,
                        &markdown_text,
                        &notes,
                    );
                    (window_id, result)
                },
                |(window_id, result)| Message::ShutdownFlushCompleted(window_id, result),
            )
        }
        Message::ShutdownFlushCompleted(window_id, result) => {
            state.shutdown_in_progress = false;

            match result {
                Ok(()) => {
                    notebook::clear_search_index_for_notebook(state.state.notebook_path());
                    window::close(window_id)
                }
                Err(error) => {
                    let _ = DialogBuilder::message()
                        .set_level(MessageLevel::Error)
                        .set_title("Failed to Save Before Exit")
                        .set_text(format!(
                            "Cognate could not safely save your latest changes before exit:\n\n{}",
                            error.ui_message()
                        ))
                        .alert()
                        .show();
                    Task::none()
                }
            }
        }
        _ => unreachable!("shutdown handler received invalid message"),
    }
}

pub(super) fn handle_save_feedback(message: Message) -> Task<Message> {
    match message {
        Message::MetadataSaved(result) => {
            if let Err(error) = result {
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
                report_persistence_error(
                    "Failed to Save Note Content",
                    &format!(
                        "Cognate could not save note content to disk:\n\n{}",
                        error.ui_message()
                    ),
                );
            } else {
                #[cfg(debug_assertions)]
                eprintln!("Note content saved successfully.");
            }
            Task::none()
        }
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
