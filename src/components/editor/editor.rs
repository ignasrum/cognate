use iced::event::Event;
use iced::keyboard::Key;
use iced::task::Task;
use iced::widget::text_editor::{Action, Edit};
use iced::{Element, Subscription, window};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

#[path = "core/clipboard.rs"]
mod clipboard;
#[path = "core/embedded_image_service.rs"]
mod embedded_image_service;
#[path = "core/embedded_images.rs"]
mod embedded_images;
#[cfg(test)]
#[path = "core/image_tag_tests.rs"]
mod image_tag_tests;
#[path = "core/persistence.rs"]
mod persistence;
#[path = "core/preview.rs"]
mod preview;
#[path = "reducer.rs"]
mod reducer;
#[path = "update_handlers.rs"]
mod update_handlers;

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
use self::embedded_image_service::EmbeddedImageWorkflow;
use self::embedded_images::save_base64_image_for_note;
use self::persistence::round_scale_step;
use self::preview::{
    build_markdown_preview_content, cursor_preview_character_index, cursor_preview_character_range,
    preview_rendered_char_count,
};

// Import required types and modules
use crate::components::editor::actions::{label_actions, note_actions};
use crate::components::editor::note_coordinator;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::editor::text_management::content_handler;
use crate::components::editor::text_management::undo_manager;
use crate::components::editor::text_management::undo_manager::UndoManager;
use crate::components::editor::ui::layout;
use crate::configuration::{Configuration, save_scale_to_config};
use crate::notebook::{self, NoteMetadata, NotebookError};

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
    MetadataSaved(Result<(), NotebookError>),

    // Search
    SearchQueryChanged(String),
    RunSearch,
    SearchCompleted(Vec<notebook::NoteSearchResult>),
    ClearSearch,

    // Content management
    NoteContentSaved(Result<(), NotebookError>),
    DebouncedMetadataSaveElapsed(u64),
    DebouncedMetadataSaveCompleted(u64, Result<(), NotebookError>),
    WindowCloseRequested(window::Id),
    ShutdownFlushCompleted(window::Id, Result<(), NotebookError>),

    // Visualizer
    ToggleVisualizer,
    VisualizerMsg(visualizer::Message),

    // Note operations
    NewNote,
    NewNoteInputChanged(String),
    CreateNote,
    NoteCreated(Result<NoteMetadata, NotebookError>),
    CancelNewNote,
    DeleteNote,
    ConfirmDeleteNote(bool),
    ConfirmDeleteEmbeddedImages(bool),
    NoteDeleted(Result<(), NotebookError>, String),
    MoveNote,
    MoveNoteInputChanged(String),
    ConfirmMoveNote,
    CancelMoveNote,
    NoteMoved(Result<String, NotebookError>, String),

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
    embedded_image_workflow: EmbeddedImageWorkflow,
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
            embedded_image_workflow: EmbeddedImageWorkflow::default(),
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
        reducer::route_message(state, message)
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

    fn sync_markdown_preview(&mut self) {
        self.embedded_image_workflow.sync_preview_assets(
            self.state.notebook_path(),
            self.state.selected_note_path(),
            &self.markdown_text,
        );
        let preview_markdown = build_markdown_preview_content(
            &self.markdown_text,
            self.embedded_image_workflow.images(),
        );
        self.markdown_preview = iced::widget::markdown::Content::parse(&preview_markdown);
    }

    fn prune_embedded_images_for_current_markdown(&mut self) {
        let pending_state_cleared = self.embedded_image_workflow.prune_for_current_markdown(
            self.state.notebook_path(),
            self.state.selected_note_path(),
            &self.markdown_text,
        );
        if pending_state_cleared {
            self.state.hide_embedded_image_delete_dialog();
        }
    }

    fn handle_confirm_delete_embedded_images(&mut self, confirmed: bool) -> Task<Message> {
        if !self.embedded_image_workflow.has_pending_deletion() {
            self.embedded_image_workflow.clear_pending_deletion();
            self.state.hide_embedded_image_delete_dialog();
            return Task::none();
        }

        if confirmed {
            let Some(action) = self.embedded_image_workflow.take_pending_delete_action() else {
                self.embedded_image_workflow.clear_pending_deletion();
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

            for image_id in self.embedded_image_workflow.take_pending_deletion_ids() {
                if let Some(image_path) = self
                    .embedded_image_workflow
                    .remove_image_path_for_id(&image_id)
                    && let Err(_err) = std::fs::remove_file(&image_path)
                    && _err.kind() != std::io::ErrorKind::NotFound
                {
                    #[cfg(debug_assertions)]
                    eprintln!("Failed to delete image file '{}': {}", image_path, _err);
                }
            }

            self.sync_markdown_preview();
            return Task::batch(vec![
                self.with_preview_scroll_task(save_task),
                metadata_save_task,
            ]);
        }

        self.embedded_image_workflow.clear_pending_deletion();
        self.state.hide_embedded_image_delete_dialog();
        Task::none()
    }

    fn dereferenced_embedded_images_for_action(&self, action: &Action) -> HashSet<String> {
        self.embedded_image_workflow.dereferenced_for_action(
            &self.markdown_text,
            self.content.cursor(),
            action,
        )
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
                let result = note_coordinator::save_metadata_snapshot(&notebook_path, &notes);
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
            self.embedded_image_workflow.images(),
        ) else {
            return Task::none();
        };

        let rendered_preview_markdown = build_markdown_preview_content(
            &self.markdown_text,
            self.embedded_image_workflow.images(),
        );
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
                state.embedded_image_workflow.images(),
            )
        } else {
            None
        };

        layout::generate_layout(
            &state.state,
            &state.content,
            &state.markdown_preview,
            state.embedded_image_workflow.image_handles(),
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
            embedded_image_workflow: EmbeddedImageWorkflow::default(),
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
