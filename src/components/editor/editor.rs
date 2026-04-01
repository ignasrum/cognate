use iced::event::Event;
use iced::keyboard::Key;
use iced::task::Task;
use iced::widget::text_editor::Action;
use iced::{Element, Subscription, window};
use std::collections::HashSet;
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
#[path = "message.rs"]
mod message;
#[path = "metadata_debounce.rs"]
mod metadata_debounce;
#[path = "core/persistence.rs"]
mod persistence;
#[path = "core/preview.rs"]
mod preview;
#[path = "reducer.rs"]
mod reducer;
#[path = "selection_handlers.rs"]
mod selection_handlers;
#[path = "text_handlers.rs"]
mod text_handlers;
#[path = "update_handlers/mod.rs"]
mod update_handlers;

pub(crate) const HTML_BR_SENTINEL: &str = "\u{E000}";
const EMBEDDED_IMAGE_DIR: &str = "images";
#[cfg(test)]
const METADATA_SAVE_DEBOUNCE_WINDOW: Duration = Duration::from_millis(20);
#[cfg(not(test))]
const METADATA_SAVE_DEBOUNCE_WINDOW: Duration = Duration::from_millis(1200);

use self::embedded_image_service::EmbeddedImageWorkflow;
pub use self::message::Message;
use self::metadata_debounce::MetadataDebounceScheduler;
use self::persistence::round_scale_step;
use self::preview::{
    build_markdown_preview_content, cursor_preview_character_index, cursor_preview_character_range,
    preview_rendered_char_count,
};

// Import required types and modules
use crate::components::editor::actions::note_actions;
use crate::components::editor::note_coordinator;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::editor::text_management::content_handler;
use crate::components::editor::text_management::undo_manager::UndoManager;
use crate::components::editor::ui::layout;
use crate::configuration::{Configuration, save_scale_to_config};
use crate::notebook;

// Import re-exported components
use crate::components::note_explorer;
use crate::components::note_explorer::NoteExplorer;
use crate::components::visualizer;
use crate::components::visualizer::Visualizer;

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
    metadata_debounce_scheduler: MetadataDebounceScheduler,
    shutdown_in_progress: bool,
    search_generation: u64,

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
        let (metadata_debounce_scheduler, metadata_debounce_events) =
            MetadataDebounceScheduler::new(METADATA_SAVE_DEBOUNCE_WINDOW);
        let metadata_debounce_task = Task::run(
            metadata_debounce_events,
            Message::DebouncedMetadataSaveElapsed,
        );

        let mut editor_instance = Editor {
            content: iced::widget::text_editor::Content::with_text(""),
            markdown_text: String::new(),
            markdown_preview: iced::widget::markdown::Content::parse(""),
            embedded_image_workflow: EmbeddedImageWorkflow::default(),
            content_note_path: None,
            metadata_save_generation: 0,
            metadata_save_in_flight: false,
            metadata_save_reschedule_after_in_flight: false,
            metadata_debounce_scheduler,
            shutdown_in_progress: false,
            search_generation: 0,
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

        (
            editor_instance,
            Task::batch(vec![initial_command, metadata_debounce_task]),
        )
    }

    // Update method delegates to focused reducers by message domain.
    pub fn update(state: &mut Self, message: Message) -> Task<Message> {
        reducer::route_message(state, message)
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
        self.metadata_debounce_scheduler
            .schedule(self.metadata_save_generation);
        Task::none()
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

    fn next_search_generation(&mut self) -> u64 {
        self.search_generation = self.search_generation.wrapping_add(1);
        self.search_generation
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

    #[cfg(test)]
    pub(crate) fn debug_selected_note_path(&self) -> Option<String> {
        self.state.selected_note_path().cloned()
    }

    #[cfg(test)]
    pub(crate) fn debug_markdown_text(&self) -> String {
        self.markdown_text.clone()
    }

    #[cfg(test)]
    pub(crate) fn debug_metadata_state(&self) -> (u64, bool, bool) {
        (
            self.metadata_save_generation,
            self.metadata_save_in_flight,
            self.metadata_save_reschedule_after_in_flight,
        )
    }

    #[cfg(test)]
    pub(crate) fn debug_search_state(&self) -> (u64, String, Vec<notebook::NoteSearchResult>) {
        (
            self.search_generation,
            self.state.search_query().to_string(),
            self.state.search_results().to_vec(),
        )
    }

    #[cfg(test)]
    pub(crate) fn debug_shutdown_in_progress(&self) -> bool {
        self.shutdown_in_progress
    }

    #[cfg(test)]
    pub(crate) fn debug_shutdown_payload(
        &self,
    ) -> (String, Option<String>, String, Vec<notebook::NoteMetadata>) {
        (
            self.state.notebook_path().to_string(),
            self.content_note_path.clone(),
            self.markdown_text.clone(),
            self.note_explorer.notes.clone(),
        )
    }
}

// Keep Default impl for Editor
impl Default for Editor {
    fn default() -> Self {
        let (metadata_debounce_scheduler, _metadata_debounce_events) =
            MetadataDebounceScheduler::new(METADATA_SAVE_DEBOUNCE_WINDOW);

        Self {
            content: iced::widget::text_editor::Content::with_text(""),
            markdown_text: String::new(),
            markdown_preview: iced::widget::markdown::Content::parse(""),
            embedded_image_workflow: EmbeddedImageWorkflow::default(),
            content_note_path: None,
            metadata_save_generation: 0,
            metadata_save_in_flight: false,
            metadata_save_reschedule_after_in_flight: false,
            metadata_debounce_scheduler,
            shutdown_in_progress: false,
            search_generation: 0,
            undo_manager: UndoManager::new(),
            state: EditorState::new(),
            note_explorer: note_explorer::NoteExplorer::new(String::new()),
            visualizer: visualizer::Visualizer::new(),
        }
    }
}
