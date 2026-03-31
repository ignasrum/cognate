use iced::event::Event;
use iced::keyboard::Key;
use iced::task::Task;
use iced::widget::text_editor::{Action, Cursor as EditorCursor, Edit, Position as EditorPosition};
use iced::{Element, Subscription, window};
use native_dialog::{DialogBuilder, MessageLevel};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) const HTML_BR_SENTINEL: &str = "\u{E000}";
const EMBEDDED_IMAGE_DIR: &str = "images";
#[cfg(test)]
const METADATA_SAVE_DEBOUNCE_WINDOW: Duration = Duration::from_millis(20);
#[cfg(not(test))]
const METADATA_SAVE_DEBOUNCE_WINDOW: Duration = Duration::from_millis(1200);
#[cfg(test)]
const HTML_BR_SENTINEL_CHAR: char = '\u{E000}';

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
                        state.persist_embedded_images_task(),
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
                        state.persist_embedded_images_task(),
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
                        state.persist_embedded_images_task(),
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
                        state.persist_embedded_images_task(),
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
                        state.persist_embedded_images_task(),
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
            return Task::batch(vec![
                task,
                state.persist_embedded_images_task(),
                state.scroll_preview_to_cursor_task(),
            ]);
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

                if let Err(_err) = &result {
                    #[cfg(debug_assertions)]
                    eprintln!("Error saving debounced metadata: {}", _err);
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
                    Ok(()) => window::close(window_id),
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

    fn handle_save_feedback_messages(message: Message) -> Task<Message> {
        match message {
            Message::MetadataSaved(result) => {
                if let Err(_err) = result {
                    #[cfg(debug_assertions)]
                    eprintln!("Error saving metadata: {}", _err);
                } else {
                    #[cfg(debug_assertions)]
                    eprintln!("Metadata saved successfully.");
                }
                Task::none()
            }
            Message::NoteContentSaved(result) => {
                if let Err(_err) = result {
                    #[cfg(debug_assertions)]
                    eprintln!("Error saving note content: {}", _err);
                } else {
                    #[cfg(debug_assertions)]
                    eprintln!("Note content saved successfully.");
                }
                Task::none()
            }
            Message::ScaleSaved(result) => {
                if let Err(_err) = result {
                    #[cfg(debug_assertions)]
                    eprintln!("Error saving scale to config: {}", _err);
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
            return Task::batch(vec![
                task,
                state.persist_embedded_images_task(),
                state.scroll_preview_to_cursor_task(),
            ]);
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

    fn persist_embedded_images_task(&self) -> Task<Message> {
        Task::none()
    }

    fn schedule_debounced_metadata_save_task(&mut self) -> Task<Message> {
        self.metadata_save_generation = self.metadata_save_generation.wrapping_add(1);
        let generation = self.metadata_save_generation;

        Task::perform(
            async move {
                std::thread::sleep(METADATA_SAVE_DEBOUNCE_WINDOW);
                generation
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

fn flush_editor_state_for_shutdown(
    notebook_path: &str,
    content_note_path: Option<String>,
    markdown_text: &str,
    notes: &[NoteMetadata],
) -> Result<(), String> {
    if notebook_path.trim().is_empty() {
        return Ok(());
    }

    if let Some(note_path) = content_note_path {
        notebook::save_note_content_sync(notebook_path, &note_path, markdown_text)?;
    }

    notebook::save_metadata(notebook_path, notes).map_err(|error| error.to_string())
}

fn generate_embedded_image_id() -> String {
    let timestamp_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("img_{timestamp_nanos:x}")
}

fn save_base64_image_for_note(
    notebook_path: &str,
    rel_note_path: &str,
    base64_image: &str,
) -> Result<String, String> {
    let image_bytes = decode_base64_image_to_bytes(base64_image)
        .ok_or_else(|| "Failed to decode image data from clipboard.".to_string())?;
    let extension = image_extension_from_bytes(&image_bytes).unwrap_or("png");
    let image_id = generate_embedded_image_id();
    let file_name = format!("{image_id}.{extension}");

    let note_dir = Path::new(notebook_path).join(rel_note_path);
    let images_dir = note_dir.join(EMBEDDED_IMAGE_DIR);
    std::fs::create_dir_all(&images_dir).map_err(|err| {
        format!(
            "Failed to create image directory '{}': {}",
            images_dir.display(),
            err
        )
    })?;

    let image_path = images_dir.join(&file_name);
    std::fs::write(&image_path, image_bytes).map_err(|err| {
        format!(
            "Failed to write image file '{}': {}",
            image_path.display(),
            err
        )
    })?;

    Ok(format!("{}/{}", EMBEDDED_IMAGE_DIR, file_name))
}

fn image_extension_from_bytes(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("png");
    }

    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }

    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("gif");
    }

    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }

    None
}

fn extract_embedded_image_ids(markdown: &str) -> HashSet<String> {
    let mut referenced = HashSet::new();

    for event in pulldown_cmark::Parser::new_ext(markdown, markdown_parser_options()) {
        if let pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { dest_url, .. }) = event {
            let image_ref = dest_url.as_ref().trim();
            if !image_ref.is_empty() {
                referenced.insert(image_ref.to_string());
            }
        }
    }

    referenced
}

fn preview_markdown_after_action(
    markdown: &str,
    cursor: EditorCursor,
    action: &Action,
) -> Option<String> {
    match action {
        Action::Edit(edit) => preview_markdown_after_edit(markdown, cursor, edit),
        _ => None,
    }
}

fn preview_markdown_after_edit(
    markdown: &str,
    cursor: EditorCursor,
    edit: &Edit,
) -> Option<String> {
    let (selection_start, selection_end) =
        selection_byte_range(markdown, cursor.position, cursor.selection)?;

    let mut preview = markdown.to_string();
    let has_selection = selection_start != selection_end;

    match edit {
        Edit::Insert(ch) => {
            preview.replace_range(selection_start..selection_end, &ch.to_string());
        }
        Edit::Paste(text) => {
            preview.replace_range(selection_start..selection_end, text.as_str());
        }
        Edit::Enter => {
            preview.replace_range(selection_start..selection_end, "\n");
        }
        Edit::Backspace => {
            if has_selection {
                preview.replace_range(selection_start..selection_end, "");
            } else {
                let backspace_start = previous_char_boundary(markdown, selection_start)?;
                preview.replace_range(backspace_start..selection_start, "");
            }
        }
        Edit::Delete => {
            if has_selection {
                preview.replace_range(selection_start..selection_end, "");
            } else {
                let delete_end = next_char_boundary(markdown, selection_end)?;
                preview.replace_range(selection_end..delete_end, "");
            }
        }
        Edit::Indent | Edit::Unindent => return None,
    }

    Some(preview)
}

fn selection_byte_range(
    markdown: &str,
    position: EditorPosition,
    selection: Option<EditorPosition>,
) -> Option<(usize, usize)> {
    let position_index = position_to_byte_index(markdown, position)?;

    if let Some(selection_position) = selection {
        let selection_index = position_to_byte_index(markdown, selection_position)?;
        if position_index <= selection_index {
            Some((position_index, selection_index))
        } else {
            Some((selection_index, position_index))
        }
    } else {
        Some((position_index, position_index))
    }
}

fn position_to_byte_index(markdown: &str, position: EditorPosition) -> Option<usize> {
    let mut line_start = 0usize;
    let mut current_line = 0usize;

    while current_line < position.line {
        let next_newline = markdown[line_start..].find('\n')?;
        line_start += next_newline + 1;
        current_line += 1;
    }

    let line_end = markdown[line_start..]
        .find('\n')
        .map(|offset| line_start + offset)
        .unwrap_or(markdown.len());
    let line_text = &markdown[line_start..line_end];
    let column_offset = column_byte_offset(line_text, position.column)?;

    Some(line_start + column_offset)
}

fn position_to_byte_index_clamped_for_preview(markdown: &str, position: EditorPosition) -> usize {
    let mut line_start = 0usize;
    let mut current_line = 0usize;

    while current_line < position.line {
        let Some(next_newline) = markdown[line_start..].find('\n') else {
            return markdown.len();
        };

        line_start += next_newline + 1;
        current_line += 1;
    }

    let line_end = markdown[line_start..]
        .find('\n')
        .map(|offset| line_start + offset)
        .unwrap_or(markdown.len());

    let mut clamped_index = line_start.saturating_add(position.column).min(line_end);

    while clamped_index > line_start && !markdown.is_char_boundary(clamped_index) {
        clamped_index -= 1;
    }

    clamped_index
}

fn cursor_preview_character_index(
    markdown: &str,
    cursor: EditorCursor,
    images: &HashMap<String, String>,
) -> Option<usize> {
    let cursor_byte_index = position_to_byte_index_clamped_for_preview(markdown, cursor.position);
    Some(adjusted_preview_character_index_at_byte(
        markdown,
        cursor_byte_index,
        images,
    ))
}

fn selection_byte_range_from_selected_text(
    markdown: &str,
    selected_text: Option<&str>,
    cursor_byte_index: usize,
) -> Option<(usize, usize)> {
    let selected_text = selected_text?;
    if selected_text.is_empty() || selected_text.len() > markdown.len() {
        return None;
    }

    if selected_text == markdown {
        return Some((0, markdown.len()));
    }

    let mut best_match: Option<(usize, usize, usize, bool)> = None;
    let mut search_start = 0usize;

    while let Some(relative_match) = markdown[search_start..].find(selected_text) {
        let match_start = search_start + relative_match;
        let match_end = match_start + selected_text.len();
        let contains_cursor = cursor_byte_index >= match_start && cursor_byte_index <= match_end;
        let distance = if contains_cursor {
            0
        } else if cursor_byte_index < match_start {
            match_start - cursor_byte_index
        } else {
            cursor_byte_index.saturating_sub(match_end)
        };

        let replace = match best_match {
            None => true,
            Some((_, _, best_distance, best_contains_cursor)) => {
                distance < best_distance
                    || (distance == best_distance && contains_cursor && !best_contains_cursor)
            }
        };

        if replace {
            best_match = Some((match_start, match_end, distance, contains_cursor));
        }

        search_start = match_start + 1;
        if search_start > markdown.len() {
            break;
        }
    }

    best_match.map(|(start, end, _, _)| (start, end))
}

fn cursor_preview_character_range(
    markdown: &str,
    cursor: EditorCursor,
    selected_text: Option<&str>,
    images: &HashMap<String, String>,
) -> Option<(usize, usize)> {
    let cursor_byte_index = position_to_byte_index_clamped_for_preview(markdown, cursor.position);
    let cursor_char_index =
        adjusted_preview_character_index_at_byte(markdown, cursor_byte_index, images);

    let cursor_range = cursor.selection.and_then(|selection_position| {
        let selection_byte_index =
            position_to_byte_index_clamped_for_preview(markdown, selection_position);

        if selection_byte_index == cursor_byte_index {
            None
        } else if cursor_byte_index <= selection_byte_index {
            Some((cursor_byte_index, selection_byte_index))
        } else {
            Some((selection_byte_index, cursor_byte_index))
        }
    });

    let (selection_start, selection_end) = cursor_range
        .or_else(|| {
            selection_byte_range_from_selected_text(markdown, selected_text, cursor_byte_index)
        })
        .unwrap_or((cursor_byte_index, cursor_byte_index));

    if selection_start == selection_end {
        return Some((cursor_char_index, 1));
    };

    let preview_start = adjusted_preview_character_index_at_byte(markdown, selection_start, images);
    let preview_end = adjusted_preview_character_index_at_byte(markdown, selection_end, images);
    let preview_length = preview_end.saturating_sub(preview_start);

    if preview_length == 0 {
        Some((cursor_char_index, 1))
    } else {
        Some((preview_start, preview_length))
    }
}

fn preview_character_index_at_byte(
    markdown: &str,
    byte_index: usize,
    images: &HashMap<String, String>,
) -> usize {
    let clamped_index = byte_index.min(markdown.len());
    let preview_full = build_markdown_preview_content(markdown, images);
    let preview_prefix = build_markdown_preview_content(&markdown[..clamped_index], images);

    let boundary = if preview_full.starts_with(&preview_prefix) {
        preview_prefix.len()
    } else {
        common_prefix_byte_len(&preview_full, &preview_prefix)
    };

    preview_rendered_char_count_until_byte(&preview_full, boundary)
}

fn preview_rendered_char_count_until_byte(preview_markdown: &str, byte_boundary: usize) -> usize {
    let clamped_boundary = byte_boundary.min(preview_markdown.len());

    pulldown_cmark::Parser::new_ext(preview_markdown, markdown_parser_options())
        .into_offset_iter()
        .fold(0usize, |count, (event, range)| {
            if range.start >= clamped_boundary {
                return count;
            }

            match event {
                pulldown_cmark::Event::Text(text) => {
                    if range.end <= clamped_boundary {
                        count + text.chars().count()
                    } else {
                        count
                            + preview_markdown[range.start..clamped_boundary]
                                .chars()
                                .count()
                    }
                }
                pulldown_cmark::Event::Code(code) => {
                    if range.end <= clamped_boundary {
                        count + code.chars().count()
                    } else {
                        count
                            + preview_markdown[range.start..clamped_boundary]
                                .chars()
                                .count()
                    }
                }
                pulldown_cmark::Event::SoftBreak | pulldown_cmark::Event::HardBreak => {
                    if range.end <= clamped_boundary {
                        count + 1
                    } else {
                        count
                    }
                }
                _ => count,
            }
        })
}

fn common_prefix_byte_len(left: &str, right: &str) -> usize {
    let mut total = 0usize;

    for (left_char, right_char) in left.chars().zip(right.chars()) {
        if left_char != right_char {
            break;
        }

        total += left_char.len_utf8();
    }

    total
}

fn adjusted_preview_character_index_at_byte(
    markdown: &str,
    byte_index: usize,
    images: &HashMap<String, String>,
) -> usize {
    let clamped_index = byte_index.min(markdown.len());
    let mut rendered_char_count = preview_character_index_at_byte(markdown, clamped_index, images);

    let previous_char = markdown[..clamped_index].chars().next_back();
    let next_char = markdown[clamped_index..].chars().next();

    if matches!(previous_char, Some(' ' | '\t' | '\n' | '\r'))
        && matches!(next_char, Some(ch) if ch != '\n' && ch != '\r')
        && let Some(next_boundary) = next_char_boundary(markdown, clamped_index)
    {
        let rendered_with_next = preview_character_index_at_byte(markdown, next_boundary, images);

        if rendered_with_next > rendered_char_count {
            rendered_char_count = rendered_with_next.saturating_sub(1);
        }
    }

    rendered_char_count
}

fn preview_rendered_char_count(preview_markdown: &str) -> usize {
    pulldown_cmark::Parser::new_ext(preview_markdown, markdown_parser_options()).fold(
        0usize,
        |count, event| match event {
            pulldown_cmark::Event::Text(text) => count + text.chars().count(),
            pulldown_cmark::Event::Code(code) => count + code.chars().count(),
            pulldown_cmark::Event::SoftBreak | pulldown_cmark::Event::HardBreak => count + 1,
            _ => count,
        },
    )
}

#[cfg(test)]
fn preview_line_from_cursor_byte(markdown: &str, cursor_byte_index: usize) -> usize {
    let clamped_index = cursor_byte_index.min(markdown.len());
    let prefix = &markdown[..clamped_index];
    let normalized_prefix = normalize_html_line_break_tags(prefix);

    normalized_prefix
        .chars()
        .filter(|ch| *ch == '\n' || *ch == HTML_BR_SENTINEL_CHAR)
        .count()
}

fn column_byte_offset(text: &str, column: usize) -> Option<usize> {
    if column <= text.len() && text.is_char_boundary(column) {
        Some(column)
    } else {
        None
    }
}

fn previous_char_boundary(text: &str, index: usize) -> Option<usize> {
    if index == 0 || index > text.len() {
        return None;
    }

    text[..index]
        .char_indices()
        .next_back()
        .map(|(offset, _)| offset)
}

fn next_char_boundary(text: &str, index: usize) -> Option<usize> {
    if index >= text.len() {
        return None;
    }

    let ch = text[index..].chars().next()?;
    Some(index + ch.len_utf8())
}

fn resolve_embedded_image_reference(note_dir: &Path, image_ref: &str) -> Option<PathBuf> {
    let normalized_ref = image_ref.trim().replace('\\', "/");
    if normalized_ref.is_empty() || normalized_ref.contains("://") || normalized_ref.contains("..")
    {
        return None;
    }

    if !normalized_ref.starts_with(&format!("{}/", EMBEDDED_IMAGE_DIR)) {
        return None;
    }

    Some(note_dir.join(normalized_ref))
}

fn build_markdown_preview_content(markdown: &str, images: &HashMap<String, String>) -> String {
    let _ = images;
    normalize_html_line_break_tags(markdown)
}

fn normalize_html_line_break_tags(markdown: &str) -> String {
    let mut normalized = String::with_capacity(markdown.len());
    let mut cursor = 0usize;

    for (event, range) in
        pulldown_cmark::Parser::new_ext(markdown, markdown_parser_options()).into_offset_iter()
    {
        if let pulldown_cmark::Event::Html(html) | pulldown_cmark::Event::InlineHtml(html) = event
            && let Some(line_breaks) = html_line_breaks_replacement(html.as_ref())
        {
            normalized.push_str(&markdown[cursor..range.start]);
            normalized.push_str(&line_breaks);
            let mut next_cursor = range.end;

            if markdown[next_cursor..].starts_with("\r\n") {
                next_cursor += 2;
            } else if markdown[next_cursor..].starts_with('\n') {
                next_cursor += 1;
            }

            cursor = next_cursor;
        }
    }

    if cursor == 0 {
        return markdown.to_string();
    }

    if cursor < markdown.len() {
        normalized.push_str(&markdown[cursor..]);
    }

    normalized
}

fn html_line_breaks_replacement(html: &str) -> Option<String> {
    let mut cursor = 0usize;
    let mut count = 0usize;

    while cursor < html.len() {
        let remaining = &html[cursor..];
        let leading_ws = remaining.len() - remaining.trim_start().len();
        cursor += leading_ws;

        if cursor >= html.len() {
            break;
        }

        if !html[cursor..].starts_with('<') {
            return None;
        }

        let Some(tag_end) = html[cursor..].find('>') else {
            return None;
        };

        let tag = &html[cursor..cursor + tag_end + 1];
        if !is_html_line_break_tag(tag) {
            return None;
        }

        count += 1;
        cursor += tag_end + 1;
    }

    if count == 0 {
        None
    } else {
        Some(HTML_BR_SENTINEL.repeat(count))
    }
}

fn is_html_line_break_tag(html: &str) -> bool {
    let trimmed = html.trim();
    if !(trimmed.starts_with('<') && trimmed.ends_with('>')) {
        return false;
    }

    let mut inner = trimmed[1..trimmed.len() - 1].trim();
    if inner.starts_with('/') {
        return false;
    }

    if let Some(without_self_close) = inner.strip_suffix('/') {
        inner = without_self_close.trim();
    }

    let mut parts = inner.split_whitespace();
    let Some(tag_name) = parts.next() else {
        return false;
    };

    tag_name.eq_ignore_ascii_case("br")
}

fn markdown_parser_options() -> pulldown_cmark::Options {
    pulldown_cmark::Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | pulldown_cmark::Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | pulldown_cmark::Options::ENABLE_TABLES
        | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
        | pulldown_cmark::Options::ENABLE_TASKLISTS
}

fn decode_base64_image_to_bytes(base64_data: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .ok()
}

enum ClipboardPastePayload {
    Text(String),
    ImageBase64(String),
}

fn read_clipboard_paste_payload() -> Result<Option<ClipboardPastePayload>, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|err| format!("Failed to open clipboard: {}", err))?;

    if let Ok(text) = clipboard.get_text() {
        if !text.is_empty() {
            if let Some(image_base64) = read_clipboard_image_file_as_base64_from_text(&text) {
                return Ok(Some(ClipboardPastePayload::ImageBase64(image_base64)));
            }

            return Ok(Some(ClipboardPastePayload::Text(text)));
        }
    }

    Ok(read_clipboard_image_as_base64_png_from(&mut clipboard)?
        .map(ClipboardPastePayload::ImageBase64))
}

fn read_clipboard_image_as_base64_png_from(
    clipboard: &mut arboard::Clipboard,
) -> Result<Option<String>, String> {
    use base64::Engine;

    let attempt_count = clipboard_image_retry_attempts();
    let mut image = None;

    for attempt in 0..attempt_count {
        match clipboard.get_image() {
            Ok(value) => {
                image = Some(value);
                break;
            }
            Err(_) if attempt + 1 < attempt_count => {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(_) => break,
        }
    }

    let Some(image) = image else {
        return Ok(None);
    };

    let mut encoded_png = Vec::new();
    {
        let mut encoder =
            png::Encoder::new(&mut encoded_png, image.width as u32, image.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|err| format!("Failed to write image header: {}", err))?;
        writer
            .write_image_data(image.bytes.as_ref())
            .map_err(|err| format!("Failed to encode clipboard image as PNG: {}", err))?;
    }

    Ok(Some(
        base64::engine::general_purpose::STANDARD.encode(encoded_png),
    ))
}

fn clipboard_image_retry_attempts() -> usize {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return 3;
        }
    }

    1
}

fn read_clipboard_image_file_as_base64_from_text(text: &str) -> Option<String> {
    parse_clipboard_image_file_paths(text)
        .into_iter()
        .find_map(|path| read_image_file_as_base64(&path))
}

fn paste_text_from_action(action: &Action) -> Option<String> {
    let Action::Edit(Edit::Paste(text)) = action else {
        return None;
    };

    Some((**text).clone())
}

fn parse_clipboard_image_file_paths(text: &str) -> Vec<PathBuf> {
    let mut lines: Vec<&str> = text.lines().collect();
    if let Some(first) = lines.first().copied()
        && matches!(first.trim().to_ascii_lowercase().as_str(), "copy" | "cut")
    {
        lines.remove(0);
    }

    lines
        .into_iter()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(parse_clipboard_file_line)
        .filter(|path| path.is_file() && is_probably_image_file(path))
        .collect()
}

fn parse_clipboard_file_line(line: &str) -> Option<PathBuf> {
    let trimmed = line.trim();
    let unquoted = if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    if unquoted.starts_with("file://") {
        parse_file_uri_to_path(unquoted)
    } else {
        Some(PathBuf::from(unquoted))
    }
}

fn parse_file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.trim().strip_prefix("file://")?;
    let path_part = if rest.starts_with('/') {
        rest.to_string()
    } else {
        let (host, path) = rest.split_once('/')?;
        if !(host.is_empty() || host.eq_ignore_ascii_case("localhost")) {
            return None;
        }
        format!("/{}", path)
    };

    percent_decode(&path_part).map(PathBuf::from)
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut cursor = 0usize;

    while cursor < bytes.len() {
        if bytes[cursor] == b'%' {
            if cursor + 2 >= bytes.len() {
                return None;
            }

            let hi = (bytes[cursor + 1] as char).to_digit(16)?;
            let lo = (bytes[cursor + 2] as char).to_digit(16)?;
            decoded.push(((hi << 4) + lo) as u8);
            cursor += 3;
        } else {
            decoded.push(bytes[cursor]);
            cursor += 1;
        }
    }

    String::from_utf8(decoded).ok()
}

fn is_probably_image_file(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };

    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "bmp"
            | "webp"
            | "tif"
            | "tiff"
            | "ico"
            | "avif"
            | "heic"
            | "heif"
    )
}

fn read_image_file_as_base64(path: &Path) -> Option<String> {
    use base64::Engine;

    if !is_probably_image_file(path) {
        return None;
    }

    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() == 0 || metadata.len() > 50 * 1024 * 1024 {
        return None;
    }

    let bytes = std::fs::read(path).ok()?;
    Some(base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn round_scale_step(scale: f32) -> f32 {
    (scale * 100.0).round() / 100.0
}

#[cfg(test)]
mod image_tag_tests {
    use super::{
        HTML_BR_SENTINEL, build_markdown_preview_content, column_byte_offset,
        cursor_preview_character_index, cursor_preview_character_range, extract_embedded_image_ids,
        html_line_breaks_replacement, is_probably_image_file, normalize_html_line_break_tags,
        parse_clipboard_image_file_paths, parse_file_uri_to_path, paste_text_from_action,
        percent_decode, preview_line_from_cursor_byte,
        read_clipboard_image_file_as_base64_from_text, read_image_file_as_base64,
    };
    use base64::Engine;
    use iced::widget::text_editor::{
        Action, Cursor as EditorCursor, Edit, Position as EditorPosition,
    };
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn extract_embedded_image_ids_parses_all_tags() {
        let markdown = "A ![first](images/img_one.png) and ![second](images/img_two.jpg).";
        let ids = extract_embedded_image_ids(markdown);

        let expected: HashSet<String> = [
            "images/img_one.png".to_string(),
            "images/img_two.jpg".to_string(),
        ]
        .into_iter()
        .collect();
        assert_eq!(ids, expected);
    }

    #[test]
    fn build_markdown_preview_content_keeps_standard_markdown_images() {
        let markdown = "before ![image](images/img_one.png) after";
        let images = HashMap::new();

        let rendered = build_markdown_preview_content(markdown, &images);
        assert_eq!(rendered, markdown);
    }

    #[test]
    fn normalize_html_line_break_tags_converts_br_variants() {
        let markdown = "one<br>two<br/>three<BR />four";
        let normalized = normalize_html_line_break_tags(markdown);

        let expected = format!("one{0}two{0}three{0}four", HTML_BR_SENTINEL);
        assert_eq!(normalized, expected);
    }

    #[test]
    fn normalize_html_line_break_tags_keeps_non_break_html() {
        let markdown = "before <span>inline</span> after";
        let normalized = normalize_html_line_break_tags(markdown);

        assert_eq!(normalized, markdown);
    }

    #[test]
    fn normalize_html_line_break_tags_does_not_touch_code_blocks() {
        let markdown = "```html\n<br>\n```";
        let normalized = normalize_html_line_break_tags(markdown);

        assert_eq!(normalized, markdown);
    }

    #[test]
    fn normalize_html_line_break_tags_converts_multiple_breaks_in_one_html_fragment() {
        let markdown = "one<br><br>two";
        let normalized = normalize_html_line_break_tags(markdown);

        let expected = format!("one{0}{0}two", HTML_BR_SENTINEL);
        assert_eq!(normalized, expected);
    }

    #[test]
    fn html_line_breaks_replacement_rejects_mixed_html() {
        assert_eq!(html_line_breaks_replacement("<br><span>"), None);
    }

    #[test]
    fn normalize_html_line_break_tags_keeps_multiple_break_events_with_spaces_between_tags() {
        let markdown = "line 1<br> <br> <br> <br>line 2";
        let normalized = normalize_html_line_break_tags(markdown);

        assert_eq!(normalized.matches(HTML_BR_SENTINEL).count(), 4);
    }

    #[test]
    fn normalize_html_line_break_tags_consumes_newline_immediately_after_break_tag() {
        let markdown = "line 1<br>\nline 2";
        let normalized = normalize_html_line_break_tags(markdown);

        let expected = format!("line 1{}line 2", HTML_BR_SENTINEL);
        assert_eq!(normalized, expected);
    }

    #[test]
    fn preview_line_from_cursor_byte_tracks_newlines() {
        let markdown = "first line\nsecond line\nthird line";
        let third_line_start = markdown
            .find("third")
            .expect("test fixture should contain third line");

        assert_eq!(preview_line_from_cursor_byte(markdown, third_line_start), 2);
    }

    #[test]
    fn preview_line_from_cursor_byte_counts_html_break_tags() {
        let markdown = "one<br>two<br/>three";
        let third_segment_start = markdown
            .find("three")
            .expect("test fixture should contain third segment");

        assert_eq!(
            preview_line_from_cursor_byte(markdown, third_segment_start),
            2
        );
    }

    #[test]
    fn cursor_preview_character_index_handles_space_before_following_text() {
        let markdown = "hello sample";
        let cursor = EditorCursor {
            position: EditorPosition { line: 0, column: 6 },
            selection: None,
        };

        let index = cursor_preview_character_index(markdown, cursor, &HashMap::new());
        assert_eq!(
            index,
            Some(6),
            "cursor after a space before text should still point to the next visible character"
        );
    }

    #[test]
    fn cursor_preview_character_index_handles_newline_before_following_text() {
        let markdown = "hello\nsample";
        let cursor = EditorCursor {
            position: EditorPosition { line: 1, column: 0 },
            selection: None,
        };

        let index = cursor_preview_character_index(markdown, cursor, &HashMap::new());
        assert_eq!(
            index,
            Some(6),
            "cursor after a newline before text should still point to the next visible character"
        );
    }

    #[test]
    fn cursor_preview_character_range_defaults_to_single_character_without_selection() {
        let markdown = "hello";
        let cursor = EditorCursor {
            position: EditorPosition { line: 0, column: 2 },
            selection: None,
        };

        let range = cursor_preview_character_range(markdown, cursor, None, &HashMap::new());
        assert_eq!(range, Some((2, 1)));
    }

    #[test]
    fn cursor_preview_character_range_matches_selected_text_length() {
        let markdown = "hello world";
        let cursor = EditorCursor {
            position: EditorPosition {
                line: 0,
                column: 11,
            },
            selection: Some(EditorPosition { line: 0, column: 6 }),
        };

        let range = cursor_preview_character_range(markdown, cursor, None, &HashMap::new());
        assert_eq!(range, Some((6, 5)));
    }

    #[test]
    fn cursor_preview_character_range_uses_selected_text_for_word_selection() {
        let markdown = "hello world";
        let cursor = EditorCursor {
            position: EditorPosition { line: 0, column: 7 },
            selection: Some(EditorPosition { line: 0, column: 7 }),
        };

        let range =
            cursor_preview_character_range(markdown, cursor, Some("world"), &HashMap::new());
        assert_eq!(range, Some((6, 5)));
    }

    #[test]
    fn cursor_preview_character_range_clamps_virtual_end_position_for_select_all() {
        let markdown = "hello";
        let cursor = EditorCursor {
            position: EditorPosition { line: 1, column: 0 },
            selection: Some(EditorPosition { line: 0, column: 0 }),
        };

        let range =
            cursor_preview_character_range(markdown, cursor, Some("hello"), &HashMap::new());
        assert_eq!(range, Some((0, 5)));
    }

    #[test]
    fn cursor_preview_character_index_stays_stable_inside_ordered_list_marker() {
        let markdown = "1. first";

        for column in 0..=3 {
            let cursor = EditorCursor {
                position: EditorPosition { line: 0, column },
                selection: None,
            };

            let index = cursor_preview_character_index(markdown, cursor, &HashMap::new());
            assert_eq!(
                index,
                Some(0),
                "ordered-list marker should not shift rendered index at column {}",
                column
            );
        }

        let after_first_character = EditorCursor {
            position: EditorPosition { line: 0, column: 4 },
            selection: None,
        };
        assert_eq!(
            cursor_preview_character_index(markdown, after_first_character, &HashMap::new()),
            Some(1)
        );
    }

    #[test]
    fn column_byte_offset_handles_multibyte_unicode() {
        let text = "båd";

        assert_eq!(column_byte_offset(text, 0), Some(0));
        assert_eq!(column_byte_offset(text, 1), Some(1));
        assert_eq!(column_byte_offset(text, 2), None);
        assert_eq!(column_byte_offset(text, 3), Some(3));
        assert_eq!(column_byte_offset(text, 4), Some(4));
    }

    #[test]
    fn cursor_preview_character_index_tracks_multibyte_unicode_plain_text() {
        let markdown = "båd";

        let before_unicode = EditorCursor {
            position: EditorPosition { line: 0, column: 1 },
            selection: None,
        };
        let after_unicode = EditorCursor {
            position: EditorPosition { line: 0, column: 3 },
            selection: None,
        };

        assert_eq!(
            cursor_preview_character_index(markdown, before_unicode, &HashMap::new()),
            Some(1)
        );
        assert_eq!(
            cursor_preview_character_index(markdown, after_unicode, &HashMap::new()),
            Some(2)
        );
    }

    #[test]
    fn cursor_preview_character_index_tracks_multibyte_unicode_inside_ordered_list() {
        let markdown = "1. båd";

        let before_unicode = EditorCursor {
            position: EditorPosition { line: 0, column: 4 },
            selection: None,
        };
        let after_unicode = EditorCursor {
            position: EditorPosition { line: 0, column: 6 },
            selection: None,
        };

        assert_eq!(
            cursor_preview_character_index(markdown, before_unicode, &HashMap::new()),
            Some(1)
        );
        assert_eq!(
            cursor_preview_character_index(markdown, after_unicode, &HashMap::new()),
            Some(2)
        );
    }

    #[test]
    fn percent_decode_decodes_percent_encoded_text() {
        assert_eq!(
            percent_decode("/tmp/a%20b.png"),
            Some("/tmp/a b.png".to_string())
        );
    }

    #[test]
    fn parse_file_uri_to_path_parses_localhost_uri() {
        let parsed = parse_file_uri_to_path("file://localhost/tmp/image.png");
        assert_eq!(parsed, Some(PathBuf::from("/tmp/image.png")));
    }

    #[test]
    fn is_probably_image_file_matches_common_extensions() {
        assert!(is_probably_image_file(Path::new("/tmp/img.PNG")));
        assert!(!is_probably_image_file(Path::new("/tmp/file.txt")));
    }

    #[test]
    fn parse_clipboard_image_file_paths_supports_gnome_copied_files_format() {
        let path = write_temp_test_file("png", b"\x89PNG\r\n\x1a\n");
        let escaped = path.to_string_lossy().replace(' ', "%20");
        let clipboard_text = format!("copy\nfile://{escaped}");

        let parsed = parse_clipboard_image_file_paths(&clipboard_text);
        assert_eq!(parsed, vec![path.clone()]);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn paste_text_from_action_extracts_pasted_text() {
        let action = Action::Edit(Edit::Paste(Arc::new("hello".to_string())));
        assert_eq!(paste_text_from_action(&action), Some("hello".to_string()));

        let non_paste_action = Action::Edit(Edit::Insert('a'));
        assert_eq!(paste_text_from_action(&non_paste_action), None);
    }

    #[test]
    fn read_clipboard_image_file_as_base64_from_text_reads_image_path() {
        let path = write_temp_test_file("png", b"\x89PNG\r\n\x1a\npayload");
        let expected = read_image_file_as_base64(&path).expect("image should be readable");
        let clipboard_text = path.to_string_lossy().to_string();

        let actual = read_clipboard_image_file_as_base64_from_text(&clipboard_text)
            .expect("clipboard path should resolve to image bytes");
        assert_eq!(actual, expected);

        let decoded = base64::engine::general_purpose::STANDARD
            .decode(actual)
            .expect("base64 should decode");
        assert_eq!(decoded, b"\x89PNG\r\n\x1a\npayload");

        let _ = std::fs::remove_file(path);
    }

    fn write_temp_test_file(ext: &str, bytes: &[u8]) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("cognate_test_{nanos}.{ext}"));
        std::fs::write(&path, bytes).expect("failed to write temp file");
        path
    }
}
