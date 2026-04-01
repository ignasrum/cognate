use iced::event::Event;
use iced::keyboard::Key;
use iced::task::Task;
use iced::widget::text_editor::{Action, Edit};
use iced::{Element, Subscription, window};
use native_dialog::{DialogBuilder, MessageLevel};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

#[path = "core/clipboard.rs"]
mod clipboard;
#[path = "core/embedded_images.rs"]
mod embedded_images;
#[cfg(test)]
#[path = "core/image_tag_tests.rs"]
mod image_tag_tests;
#[path = "core/persistence.rs"]
mod persistence;
#[path = "core/preview.rs"]
mod preview;

pub(crate) const HTML_BR_SENTINEL: &str = "\u{E000}";
const EMBEDDED_IMAGE_DIR: &str = "images";
#[cfg(test)]
const METADATA_SAVE_DEBOUNCE_WINDOW: Duration = Duration::from_millis(20);
#[cfg(not(test))]
const METADATA_SAVE_DEBOUNCE_WINDOW: Duration = Duration::from_millis(1200);

use self::clipboard::{
    ClipboardPastePayload, paste_text_from_action, read_clipboard_image_file_as_base64_from_text,
    read_clipboard_paste_payload,
};
use self::embedded_images::{resolve_embedded_image_reference, save_base64_image_for_note};
use self::persistence::{flush_editor_state_for_shutdown, round_scale_step};
use self::preview::{
    build_markdown_preview_content, cursor_preview_character_index, cursor_preview_character_range,
    extract_embedded_image_ids, preview_markdown_after_action, preview_rendered_char_count,
};

// Import required types and modules
use crate::components::editor::actions::{label_actions, note_actions};
use crate::components::editor::state::editor_state::EditorState;
use crate::components::editor::text_management::content_handler;
use crate::components::editor::text_management::undo_manager;
use crate::components::editor::text_management::undo_manager::UndoManager;
use crate::components::editor::ui::layout;
use crate::configuration::{Configuration, save_scale_to_config};
use crate::notebook::{self, NoteMetadata};

// Import re-exported components
use crate::components::note_explorer;
use crate::components::note_explorer::NoteExplorer;
use crate::components::visualizer;
use crate::components::visualizer::Visualizer;

// Define the Message enum in this module
#[derive(Debug, Clone)]
pub enum Message {
    // Text editing operations
    EditorAction(iced::widget::text_editor::Action),
    PasteFromClipboard,
    LoadedNoteContent(String, String, HashMap<String, String>),
    HandleTabKey,
    SelectAll,
    Undo,
    Redo,

    // Note explorer interaction
    NoteExplorerMsg(note_explorer::Message),
    NoteSelected(String),

    // Label management
    NewLabelInputChanged(String),
    AddLabel,
    RemoveLabel(String),
    MetadataSaved(Result<(), String>),

    // Search
    SearchQueryChanged(String),
    RunSearch,
    SearchCompleted(Vec<notebook::NoteSearchResult>),
    ClearSearch,

    // Content management
    NoteContentSaved(Result<(), String>),
    DebouncedMetadataSaveElapsed(u64),
    DebouncedMetadataSaveCompleted(u64, Result<(), String>),
    WindowCloseRequested(window::Id),
    ShutdownFlushCompleted(window::Id, Result<(), String>),

    // Visualizer
    ToggleVisualizer,
    VisualizerMsg(visualizer::Message),

    // Note operations
    NewNote,
    NewNoteInputChanged(String),
    CreateNote,
    NoteCreated(Result<NoteMetadata, String>),
    CancelNewNote,
    DeleteNote,
    ConfirmDeleteNote(bool),
    ConfirmDeleteEmbeddedImages(bool),
    NoteDeleted(Result<(), String>, String),
    MoveNote,
    MoveNoteInputChanged(String),
    ConfirmMoveNote,
    CancelMoveNote,
    NoteMoved(Result<String, String>, String),

    // Folder operations
    InitiateFolderRename(String),

    // UI interactions
    AboutButtonClicked,
    IncreaseScale,
    DecreaseScale,
    MarkdownLinkClicked(String),
    ScaleSaved(Result<(), String>),
}

// Define the Editor struct
pub struct Editor {
    // Core state management
    state: EditorState,

    // Text management
    content: iced::widget::text_editor::Content,
    markdown_text: String,
    markdown_preview: iced::widget::markdown::Content,
    embedded_images: HashMap<String, String>,
    embedded_image_handles: HashMap<String, iced::widget::image::Handle>,
    pending_embedded_image_deletion_ids: HashSet<String>,
    pending_embedded_image_delete_action: Option<Action>,
    embedded_image_prompt_note_path: Option<String>,
    content_note_path: Option<String>,
    metadata_save_generation: u64,
    metadata_save_in_flight: bool,
    metadata_save_reschedule_after_in_flight: bool,
    shutdown_in_progress: bool,

    // Undo/redo management
    undo_manager: UndoManager,

    // UI components and state
    note_explorer: NoteExplorer,
    visualizer: Visualizer,
}

// Implement static methods for Editor to work with iced::application
impl Editor {
    // Keep create method for internal use
    pub fn create(flags: Configuration) -> (Self, Task<Message>) {
        let notebook_path_clone = flags.notebook_path.clone();

        let mut editor_instance = Editor {
            content: iced::widget::text_editor::Content::with_text(""),
            markdown_text: String::new(),
            markdown_preview: iced::widget::markdown::Content::parse(""),
            embedded_images: HashMap::new(),
            embedded_image_handles: HashMap::new(),
            pending_embedded_image_deletion_ids: HashSet::new(),
            pending_embedded_image_delete_action: None,
            embedded_image_prompt_note_path: None,
            content_note_path: None,
            metadata_save_generation: 0,
            metadata_save_in_flight: false,
            metadata_save_reschedule_after_in_flight: false,
            shutdown_in_progress: false,
            undo_manager: UndoManager::new(),
            state: EditorState::new(),
            note_explorer: note_explorer::NoteExplorer::new(notebook_path_clone.clone()),
            visualizer: visualizer::Visualizer::new(),
        };

        editor_instance.state.set_notebook_path(notebook_path_clone);
        editor_instance.state.set_config_path(flags.config_path);
        editor_instance.state.set_ui_scale(flags.scale);
        editor_instance.state.set_app_version(flags.version);

        let initial_command = if !editor_instance.state.notebook_path().is_empty() {
            editor_instance
                .note_explorer
                .update(note_explorer::Message::LoadNotes)
                .map(Message::NoteExplorerMsg)
        } else {
            Task::none()
        };

        (editor_instance, initial_command)
    }

    // Update method delegates to focused reducers by message domain.
    pub fn update(state: &mut Self, message: Message) -> Task<Message> {
        match message {
            Message::HandleTabKey
            | Message::SelectAll
            | Message::Undo
            | Message::Redo
            | Message::PasteFromClipboard
            | Message::EditorAction(_)
            | Message::LoadedNoteContent(_, _, _) => Self::handle_text_messages(state, message),

            Message::NoteExplorerMsg(_) | Message::NoteSelected(_) => {
                Self::handle_selection_messages(state, message)
            }

            Message::NewLabelInputChanged(_) | Message::AddLabel | Message::RemoveLabel(_) => {
                Self::handle_label_messages(state, message)
            }

            Message::SearchQueryChanged(_)
            | Message::RunSearch
            | Message::SearchCompleted(_)
            | Message::ClearSearch => Self::handle_search_messages(state, message),

            Message::DebouncedMetadataSaveElapsed(_)
            | Message::DebouncedMetadataSaveCompleted(_, _) => {
                Self::handle_debounced_metadata_messages(state, message)
            }

            Message::WindowCloseRequested(_) | Message::ShutdownFlushCompleted(_, _) => {
                Self::handle_shutdown_messages(state, message)
            }

            Message::MetadataSaved(_) | Message::NoteContentSaved(_) | Message::ScaleSaved(_) => {
                Self::handle_save_feedback_messages(message)
            }

            Message::ToggleVisualizer | Message::VisualizerMsg(_) => {
                Self::handle_visualizer_messages(state, message)
            }

            Message::NewNote
            | Message::NewNoteInputChanged(_)
            | Message::CreateNote
            | Message::CancelNewNote
            | Message::NoteCreated(_)
            | Message::DeleteNote
            | Message::ConfirmDeleteNote(_)
            | Message::ConfirmDeleteEmbeddedImages(_)
            | Message::NoteDeleted(_, _)
            | Message::MoveNote
            | Message::MoveNoteInputChanged(_)
            | Message::ConfirmMoveNote
            | Message::CancelMoveNote
            | Message::NoteMoved(_, _) => Self::handle_note_lifecycle_messages(state, message),

            Message::InitiateFolderRename(_)
            | Message::AboutButtonClicked
            | Message::IncreaseScale
            | Message::DecreaseScale
            | Message::MarkdownLinkClicked(_) => Self::handle_ui_messages(state, message),
        }
    }

    fn handle_text_messages(state: &mut Self, message: Message) -> Task<Message> {
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
                    state.pending_embedded_image_deletion_ids = dereferenced_image_ids;
                    state.pending_embedded_image_delete_action = Some(action);
                    state.state.show_embedded_image_delete_dialog(
                        state.pending_embedded_image_deletion_ids.len(),
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
                state.embedded_images = images;
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

    fn handle_selection_messages(state: &mut Self, message: Message) -> Task<Message> {
        let previous_markdown = state.markdown_text.clone();
        let task = match message {
            Message::NoteExplorerMsg(note_explorer_message) => {
                note_actions::handle_note_explorer_message(
                    &mut state.note_explorer,
                    &mut state.visualizer,
                    &mut state.state,
                    &mut state.content,
                    &mut state.markdown_text,
                    note_explorer_message,
                )
            }
            Message::NoteSelected(note_path) => note_actions::handle_note_selected(
                &mut state.note_explorer,
                &mut state.undo_manager,
                &mut state.state,
                note_path,
            ),
            _ => unreachable!("selection handler received invalid message"),
        };

        if state.markdown_text != previous_markdown {
            if state.state.selected_note_path().is_none() {
                state.content_note_path = None;
            }
            state.prune_embedded_images_for_current_markdown();
            state.sync_markdown_preview();
            return Task::batch(vec![task, state.scroll_preview_to_cursor_task()]);
        }

        task
    }

    fn handle_label_messages(state: &mut Self, message: Message) -> Task<Message> {
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

    fn handle_search_messages(state: &mut Self, message: Message) -> Task<Message> {
        match message {
            Message::SearchQueryChanged(query) => {
                state.state.set_search_query(query.clone());
                if query.trim().is_empty() {
                    state.state.set_search_results(Vec::new());
                    return Task::none();
                }

                let notebook_path = state.state.notebook_path().to_string();
                let notes = state.note_explorer.notes.clone();
                Task::perform(
                    async move { notebook::search_notes(notebook_path, notes, query).await },
                    Message::SearchCompleted,
                )
            }
            Message::RunSearch => {
                let query = state.state.search_query().trim().to_string();
                if query.is_empty() || state.state.notebook_path().is_empty() {
                    state.state.set_search_results(Vec::new());
                    return Task::none();
                }

                let notebook_path = state.state.notebook_path().to_string();
                let notes = state.note_explorer.notes.clone();
                Task::perform(
                    async move { notebook::search_notes(notebook_path, notes, query).await },
                    Message::SearchCompleted,
                )
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

    fn handle_debounced_metadata_messages(state: &mut Self, message: Message) -> Task<Message> {
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
                            error
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

    fn handle_shutdown_messages(state: &mut Self, message: Message) -> Task<Message> {
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
                        let result = flush_editor_state_for_shutdown(
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
                                error
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

    fn handle_save_feedback_messages(message: Message) -> Task<Message> {
        match message {
            Message::MetadataSaved(result) => {
                if let Err(error) = result {
                    Self::report_persistence_error(
                        "Failed to Save Notebook Metadata",
                        &format!("Cognate could not save notebook metadata:\n\n{}", error),
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
                        &format!("Cognate could not save note content to disk:\n\n{}", error),
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

    fn handle_visualizer_messages(state: &mut Self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleVisualizer => {
                state.state.toggle_visualizer();

                if state.state.show_visualizer() && !state.state.notebook_path().is_empty() {
                    let _ = state.visualizer.update(visualizer::Message::UpdateNotes(
                        state.note_explorer.notes.clone(),
                    ));
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

    fn handle_note_lifecycle_messages(state: &mut Self, message: Message) -> Task<Message> {
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
                state.embedded_images.clear();
                state.embedded_image_handles.clear();
                state.pending_embedded_image_deletion_ids.clear();
                state.pending_embedded_image_delete_action = None;
                state.embedded_image_prompt_note_path = None;
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

    fn handle_ui_messages(state: &mut Self, message: Message) -> Task<Message> {
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

    fn sync_markdown_preview(&mut self) {
        self.refresh_embedded_images_for_current_markdown();
        self.sync_embedded_image_handles();
        let preview_markdown =
            build_markdown_preview_content(&self.markdown_text, &self.embedded_images);
        self.markdown_preview = iced::widget::markdown::Content::parse(&preview_markdown);
    }

    fn prune_embedded_images_for_current_markdown(&mut self) {
        let current_note_path = self.state.selected_note_path().cloned();
        if self.embedded_image_prompt_note_path != current_note_path {
            self.pending_embedded_image_deletion_ids.clear();
            self.pending_embedded_image_delete_action = None;
            self.state.hide_embedded_image_delete_dialog();
            self.embedded_image_prompt_note_path = current_note_path;
        }

        self.refresh_embedded_images_for_current_markdown();
    }

    fn handle_confirm_delete_embedded_images(&mut self, confirmed: bool) -> Task<Message> {
        if self.pending_embedded_image_deletion_ids.is_empty() {
            self.pending_embedded_image_delete_action = None;
            self.state.hide_embedded_image_delete_dialog();
            return Task::none();
        }

        if confirmed {
            let Some(action) = self.pending_embedded_image_delete_action.take() else {
                self.pending_embedded_image_deletion_ids.clear();
                self.state.hide_embedded_image_delete_dialog();
                return Task::none();
            };

            self.state.hide_embedded_image_delete_dialog();

            let previous_markdown = self.markdown_text.clone();
            let save_task = content_handler::handle_editor_action(
                &mut self.content,
                &mut self.markdown_text,
                &mut self.undo_manager,
                action,
                self.state.selected_note_path(),
                self.state.notebook_path(),
                &self.state,
            );

            let mut metadata_save_task = Task::none();
            if self.markdown_text != previous_markdown {
                metadata_save_task = self.touch_selected_note_last_updated_and_schedule_save_task();
            }

            for image_id in std::mem::take(&mut self.pending_embedded_image_deletion_ids) {
                if let Some(image_path) = self.embedded_images.remove(&image_id)
                    && let Err(_err) = std::fs::remove_file(&image_path)
                    && _err.kind() != std::io::ErrorKind::NotFound
                {
                    #[cfg(debug_assertions)]
                    eprintln!("Failed to delete image file '{}': {}", image_path, _err);
                }
                self.embedded_image_handles.remove(&image_id);
            }

            self.sync_markdown_preview();
            return Task::batch(vec![
                self.with_preview_scroll_task(save_task),
                metadata_save_task,
            ]);
        }

        self.pending_embedded_image_deletion_ids.clear();
        self.pending_embedded_image_delete_action = None;
        self.state.hide_embedded_image_delete_dialog();
        Task::none()
    }

    fn dereferenced_embedded_images_for_action(&self, action: &Action) -> HashSet<String> {
        if self.embedded_images.is_empty() {
            return HashSet::new();
        }

        let Some(after_markdown) =
            preview_markdown_after_action(&self.markdown_text, self.content.cursor(), action)
        else {
            return HashSet::new();
        };

        if after_markdown == self.markdown_text {
            return HashSet::new();
        }

        let before = extract_embedded_image_ids(&self.markdown_text);
        let after = extract_embedded_image_ids(&after_markdown);

        before
            .into_iter()
            .filter(|image_id| {
                self.embedded_images.contains_key(image_id) && !after.contains(image_id)
            })
            .collect()
    }

    fn schedule_debounced_metadata_save_task(&mut self) -> Task<Message> {
        self.metadata_save_generation = self.metadata_save_generation.wrapping_add(1);
        let generation = self.metadata_save_generation;

        Task::perform(
            async move {
                let (sender, receiver) = iced::futures::channel::oneshot::channel();

                // Sleep on a dedicated OS thread and wake the async task via oneshot.
                // This avoids blocking the Iced executor worker thread.
                let _ = std::thread::Builder::new()
                    .name("cognate-metadata-debounce".to_string())
                    .spawn(move || {
                        std::thread::sleep(METADATA_SAVE_DEBOUNCE_WINDOW);
                        let _ = sender.send(generation);
                    });

                receiver.await.unwrap_or(generation)
            },
            Message::DebouncedMetadataSaveElapsed,
        )
    }

    fn persist_metadata_snapshot_task(&self, generation: u64) -> Task<Message> {
        if self.state.notebook_path().trim().is_empty() {
            return Task::none();
        }

        let notebook_path = self.state.notebook_path().to_string();
        let notes = self.note_explorer.notes.clone();

        Task::perform(
            async move {
                let result =
                    notebook::save_metadata(&notebook_path, &notes).map_err(|e| e.to_string());
                (generation, result)
            },
            |(generation, result)| Message::DebouncedMetadataSaveCompleted(generation, result),
        )
    }

    fn touch_selected_note_last_updated_and_schedule_save_task(&mut self) -> Task<Message> {
        if self.touch_selected_note_last_updated() {
            self.schedule_debounced_metadata_save_task()
        } else {
            Task::none()
        }
    }

    fn persist_scale_task(&self) -> Task<Message> {
        let config_path = self.state.config_path().to_string();
        let scale = self.state.ui_scale();

        if config_path.trim().is_empty() {
            return Task::none();
        }

        Task::perform(
            async move { save_scale_to_config(&config_path, scale) },
            Message::ScaleSaved,
        )
    }

    fn with_preview_scroll_task(&self, task: Task<Message>) -> Task<Message> {
        Task::batch(vec![task, self.scroll_preview_to_cursor_task()])
    }

    fn scroll_preview_to_cursor_task(&self) -> Task<Message> {
        if self.state.selected_note_path().is_none()
            || self.state.show_visualizer()
            || self.state.show_move_note_input()
            || self.state.show_new_note_input()
            || self.state.show_embedded_image_delete_confirmation()
            || self.state.show_about_info()
        {
            return Task::none();
        }

        let Some(cursor_char_index) = cursor_preview_character_index(
            &self.markdown_text,
            self.content.cursor(),
            &self.embedded_images,
        ) else {
            return Task::none();
        };

        let rendered_preview_markdown =
            build_markdown_preview_content(&self.markdown_text, &self.embedded_images);
        let total_rendered_chars = preview_rendered_char_count(&rendered_preview_markdown);

        let y = if total_rendered_chars == 0 {
            0.0
        } else {
            (cursor_char_index as f32 / total_rendered_chars as f32).clamp(0.0, 1.0)
        };

        iced::widget::operation::snap_to(
            layout::MARKDOWN_PREVIEW_SCROLLABLE_ID,
            iced::widget::operation::RelativeOffset::<Option<f32>> {
                x: None,
                y: Some(y),
            },
        )
    }

    fn sync_embedded_image_handles(&mut self) {
        self.embedded_image_handles
            .retain(|image_id, _| self.embedded_images.contains_key(image_id));

        for (image_id, image_path) in &self.embedded_images {
            if self.embedded_image_handles.contains_key(image_id) {
                continue;
            }

            if let Ok(image_bytes) = std::fs::read(image_path) {
                self.embedded_image_handles.insert(
                    image_id.clone(),
                    iced::widget::image::Handle::from_bytes(image_bytes),
                );
            }
        }
    }

    fn refresh_embedded_images_for_current_markdown(&mut self) {
        self.embedded_images.clear();

        let Some(selected_note_path) = self.state.selected_note_path() else {
            return;
        };

        if self.state.notebook_path().is_empty() {
            return;
        }

        let note_dir = Path::new(self.state.notebook_path()).join(selected_note_path);

        for image_ref in extract_embedded_image_ids(&self.markdown_text) {
            if let Some(image_path) = resolve_embedded_image_reference(&note_dir, &image_ref) {
                self.embedded_images
                    .insert(image_ref, image_path.to_string_lossy().into_owned());
            }
        }
    }

    fn touch_selected_note_last_updated(&mut self) -> bool {
        if let Some(selected_path) = self.state.selected_note_path().cloned()
            && let Some(note) = self
                .note_explorer
                .notes
                .iter_mut()
                .find(|note| note.rel_path == selected_path)
        {
            note.last_updated = Some(notebook::current_timestamp_rfc3339());
            return true;
        }

        false
    }

    // Keep view method as is, but fix the state reference
    pub fn view(state: &Self) -> Element<'_, Message> {
        let selected_text = state.content.selection();
        let preview_indicator_char_range = if state.state.selected_note_path().is_some() {
            cursor_preview_character_range(
                &state.markdown_text,
                state.content.cursor(),
                selected_text.as_deref(),
                &state.embedded_images,
            )
        } else {
            None
        };

        layout::generate_layout(
            &state.state,
            &state.content,
            &state.markdown_preview,
            &state.embedded_image_handles,
            &state.note_explorer,
            &state.visualizer,
            preview_indicator_char_range,
        )
    }

    pub fn scale_factor(state: &Self) -> f32 {
        state.state.ui_scale()
    }

    // Keep subscription method as is
    pub fn subscription(_state: &Self) -> Subscription<Message> {
        let keyboard_subscription =
            iced::event::listen_with(|event, _status, _shell| match event {
                Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    // Handle primary command shortcuts:
                    // - macOS: Cmd
                    // - other platforms: Ctrl
                    if modifiers.command()
                        && let Key::Character(c) = &key
                    {
                        if c == "a" || c == "A" {
                            return Some(Message::SelectAll);
                        }
                        if c == "z" || c == "Z" {
                            if modifiers.shift() {
                                return Some(Message::Redo);
                            }
                            return Some(Message::Undo);
                        }
                    }

                    // Handle Tab key press (no modifiers)
                    if key == Key::Named(iced::keyboard::key::Named::Tab) && modifiers.is_empty() {
                        return Some(Message::HandleTabKey);
                    }

                    None
                }
                _ => None,
            });

        let close_request_subscription =
            window::close_requests().map(Message::WindowCloseRequested);

        Subscription::batch(vec![keyboard_subscription, close_request_subscription])
    }

    #[cfg(test)]
    pub(crate) fn debug_last_updated_for(&self, rel_path: &str) -> Option<String> {
        self.note_explorer
            .notes
            .iter()
            .find(|note| note.rel_path == rel_path)
            .and_then(|note| note.last_updated.clone())
    }
}

// Keep Default impl for Editor
impl Default for Editor {
    fn default() -> Self {
        Self {
            content: iced::widget::text_editor::Content::with_text(""),
            markdown_text: String::new(),
            markdown_preview: iced::widget::markdown::Content::parse(""),
            embedded_images: HashMap::new(),
            embedded_image_handles: HashMap::new(),
            pending_embedded_image_deletion_ids: HashSet::new(),
            pending_embedded_image_delete_action: None,
            embedded_image_prompt_note_path: None,
            content_note_path: None,
            metadata_save_generation: 0,
            metadata_save_in_flight: false,
            metadata_save_reschedule_after_in_flight: false,
            shutdown_in_progress: false,
            undo_manager: UndoManager::new(),
            state: EditorState::new(),
            note_explorer: note_explorer::NoteExplorer::new(String::new()),
            visualizer: visualizer::Visualizer::new(),
        }
    }
}
