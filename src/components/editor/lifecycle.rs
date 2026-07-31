use iced::event::Event;
use iced::keyboard::Key;
use iced::task::Task;
use iced::{Element, Subscription, window};

use super::*;

impl Editor {
    pub fn create(flags: Configuration) -> (Self, Task<Message>) {
        if let Err(error) = crate::notebook::configure_backend(&flags) {
            eprintln!("[cognate] storage backend startup failed: {error}");
        }
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

        let offline_replay_command = if notebook::is_api_backend() {
            Task::perform(notebook::replay_offline_queue(), |result| {
                Message::OfflineReplayCompleted(result)
            })
        } else {
            Task::none()
        };

        (
            editor_instance,
            Task::batch(vec![
                initial_command,
                offline_replay_command,
                metadata_debounce_task,
            ]),
        )
    }

    // Update method delegates to focused reducers by message domain.
    pub fn update(state: &mut Self, message: Message) -> Task<Message> {
        reducer::route_message(state, message)
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
}
