use super::*;
use native_dialog::{DialogBuilder, MessageLevel};

impl Editor {
    pub(super) fn handle_label_messages(state: &mut Self, message: Message) -> Task<Message> {
        match message {
            Message::NewLabelInputChanged(text) => {
                label_actions::handle_label_input_changed(&mut state.state, text);
                Task::none()
            }
            Message::AddLabel => label_actions::handle_add_label(
                &mut state.state,
                &mut state.note_explorer,
                &mut state.visualizer,
            ),
            Message::RemoveLabel(label) => label_actions::handle_remove_label(
                &mut state.state,
                &mut state.note_explorer,
                &mut state.visualizer,
                label,
            ),
            _ => unreachable!("label handler received invalid message"),
        }
    }

    pub(super) fn handle_search_messages(state: &mut Self, message: Message) -> Task<Message> {
        match message {
            Message::SearchQueryChanged(query) => {
                state.state.set_search_query(query);
                let query = state.state.search_query().trim().to_string();
                if query.trim().is_empty() {
                    state.state.set_search_results(Vec::new());
                    return Task::none();
                }

                Self::spawn_search_task(state, query)
            }
            Message::RunSearch => {
                let query = state.state.search_query().trim().to_string();
                if query.is_empty() || state.state.notebook_path().is_empty() {
                    state.state.set_search_results(Vec::new());
                    return Task::none();
                }

                Self::spawn_search_task(state, query)
            }
            Message::SearchCompleted(results) => {
                state.state.set_search_results(results);
                Task::none()
            }
            Message::ClearSearch => {
                state.state.clear_search();
                Task::none()
            }
            _ => unreachable!("search handler received invalid message"),
        }
    }

    fn spawn_search_task(state: &Self, query: String) -> Task<Message> {
        let notebook_path = state.state.notebook_path().to_string();
        let notes = state
            .note_explorer
            .notes
            .iter()
            .map(notebook::SearchNote::from)
            .collect::<Vec<notebook::SearchNote>>();
        Task::perform(
            async move { notebook::search_notes_with_snapshot(notebook_path, notes, query).await },
            Message::SearchCompleted,
        )
    }

    pub(super) fn handle_debounced_metadata_messages(
        state: &mut Self,
        message: Message,
    ) -> Task<Message> {
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
                    Self::report_persistence_error(
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

    pub(super) fn handle_shutdown_messages(state: &mut Self, message: Message) -> Task<Message> {
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

    pub(super) fn handle_save_feedback_messages(message: Message) -> Task<Message> {
        match message {
            Message::MetadataSaved(result) => {
                if let Err(error) = result {
                    Self::report_persistence_error(
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
                    Self::report_persistence_error(
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
                    Self::report_persistence_error(
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

    pub(super) fn handle_visualizer_messages(state: &mut Self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleVisualizer => {
                state.state.toggle_visualizer();

                if state.state.show_visualizer() && !state.state.notebook_path().is_empty() {
                    state.visualizer.sync_notes(&state.note_explorer.notes);
                    let _ = state.visualizer.update(visualizer::Message::FocusOnNote(
                        state.state.selected_note_path().cloned(),
                    ));
                    Task::none()
                } else {
                    note_actions::get_select_note_command(
                        state.state.selected_note_path(),
                        &state.note_explorer.notes,
                    )
                }
            }
            Message::VisualizerMsg(visualizer_message) => note_actions::handle_visualizer_message(
                &mut state.visualizer,
                &mut state.note_explorer,
                &mut state.state,
                &mut state.undo_manager,
                visualizer_message,
            ),
            _ => unreachable!("visualizer handler received invalid message"),
        }
    }

    pub(super) fn handle_note_lifecycle_messages(
        state: &mut Self,
        message: Message,
    ) -> Task<Message> {
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
            Message::CreateNote => note_actions::handle_create_note(
                &mut state.state,
                state.note_explorer.notes.clone(),
            ),
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

    pub(super) fn handle_ui_messages(state: &mut Self, message: Message) -> Task<Message> {
        match message {
            Message::InitiateFolderRename(folder_path) => {
                state.state.show_rename_folder_dialog(folder_path);
                Task::none()
            }
            Message::AboutButtonClicked => {
                state.state.toggle_about_info();
                Task::none()
            }
            Message::IncreaseScale => {
                let new_scale = round_scale_step((state.state.ui_scale() + 0.1).min(4.0));
                state.state.set_ui_scale(new_scale);
                state.persist_scale_task()
            }
            Message::DecreaseScale => {
                let new_scale = round_scale_step((state.state.ui_scale() - 0.1).max(0.5));
                state.state.set_ui_scale(new_scale);
                state.persist_scale_task()
            }
            Message::MarkdownLinkClicked(_uri) => {
                #[cfg(debug_assertions)]
                eprintln!("Markdown link clicked: {}", _uri);
                Task::none()
            }
            _ => unreachable!("ui handler received invalid message"),
        }
    }
}
