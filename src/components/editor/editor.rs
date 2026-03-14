use iced::event::Event;
use iced::keyboard::Key;
use iced::task::Task;
use iced::widget::text_editor::{Action, Edit};
use iced::{Element, Subscription};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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
    EmbeddedImagesSaved(Result<(), String>),

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

            Message::MetadataSaved(_)
            | Message::NoteContentSaved(_)
            | Message::EmbeddedImagesSaved(_)
            | Message::ScaleSaved(_) => Self::handle_save_feedback_messages(message),

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
                    state.touch_selected_note_last_updated();
                    state.prune_embedded_images_for_current_markdown();
                    state.sync_markdown_preview();
                    return Task::batch(vec![save_task, state.persist_embedded_images_task()]);
                }

                save_task
            }
            Message::SelectAll => {
                content_handler::handle_select_all(&mut state.content, &state.state)
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
                    state.prune_embedded_images_for_current_markdown();
                    state.sync_markdown_preview();
                    return Task::batch(vec![task, state.persist_embedded_images_task()]);
                }
                task
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
                    state.prune_embedded_images_for_current_markdown();
                    state.sync_markdown_preview();
                    return Task::batch(vec![task, state.persist_embedded_images_task()]);
                }
                task
            }
            Message::PasteFromClipboard => Self::handle_paste_from_clipboard_shortcut(state),
            Message::EditorAction(action) => {
                if matches!(action, Action::Edit(Edit::Paste(_))) {
                    return Self::handle_paste_action(state, action);
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
                    state.touch_selected_note_last_updated();
                    state.prune_embedded_images_for_current_markdown();
                    state.sync_markdown_preview();
                    return Task::batch(vec![save_task, state.persist_embedded_images_task()]);
                }

                save_task
            }
            Message::LoadedNoteContent(note_path, new_content, images) => {
                if state.state.selected_note_path() != Some(&note_path) {
                    return Task::none();
                }
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
                task
            }
            _ => unreachable!("text handler received non-text message"),
        }
    }

    fn handle_paste_from_clipboard_shortcut(state: &mut Self) -> Task<Message> {
        if state.state.selected_note_path().is_none()
            || state.state.show_visualizer()
            || state.state.show_move_note_input()
            || state.state.show_new_note_input()
            || state.state.show_about_info()
        {
            return Task::none();
        }

        let image_base64 = match read_clipboard_image_as_base64_png() {
            Ok(value) => value,
            Err(_err) => {
                #[cfg(debug_assertions)]
                eprintln!("Failed to read image clipboard data: {}", _err);
                None
            }
        };

        if let Some(image_base64) = image_base64 {
            let Some(selected_note_path) = state.state.selected_note_path().cloned() else {
                return Task::none();
            };

            state.undo_manager.add_to_history(
                &selected_note_path,
                state.markdown_text.clone(),
                state.content.cursor(),
            );

            let image_id = generate_embedded_image_id();
            let tag = format!("![image:{}]", image_id);
            state
                .content
                .perform(Action::Edit(Edit::Paste(Arc::new(tag))));
            state.markdown_text = state.content.text();
            state.embedded_images.insert(image_id, image_base64);
            state.prune_embedded_images_for_current_markdown();
            state.touch_selected_note_last_updated();
            state.sync_markdown_preview();

            let notebook_path = state.state.notebook_path().to_string();
            let note_path = selected_note_path;
            let content_text = state.markdown_text.clone();
            let save_content_task = Task::perform(
                async move { notebook::save_note_content(notebook_path, note_path, content_text).await },
                Message::NoteContentSaved,
            );

            return Task::batch(vec![
                save_content_task,
                state.persist_embedded_images_task(),
            ]);
        }

        let text_to_paste = match read_clipboard_text() {
            Ok(Some(text)) => text,
            Ok(None) => return Task::none(),
            Err(_err) => {
                #[cfg(debug_assertions)]
                eprintln!("Failed to read text clipboard data: {}", _err);
                return Task::none();
            }
        };

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
            state.touch_selected_note_last_updated();
            state.prune_embedded_images_for_current_markdown();
            state.sync_markdown_preview();
            return Task::batch(vec![save_task, state.persist_embedded_images_task()]);
        }

        save_task
    }

    fn handle_paste_action(state: &mut Self, fallback_action: Action) -> Task<Message> {
        if state.state.selected_note_path().is_none()
            || state.state.show_visualizer()
            || state.state.show_move_note_input()
            || state.state.show_new_note_input()
            || state.state.show_about_info()
        {
            return content_handler::handle_editor_action(
                &mut state.content,
                &mut state.markdown_text,
                &mut state.undo_manager,
                fallback_action,
                state.state.selected_note_path(),
                state.state.notebook_path(),
                &state.state,
            );
        }

        let image_base64 = match read_clipboard_image_as_base64_png() {
            Ok(Some(base64_png)) => base64_png,
            Ok(None) => {
                return content_handler::handle_editor_action(
                    &mut state.content,
                    &mut state.markdown_text,
                    &mut state.undo_manager,
                    fallback_action,
                    state.state.selected_note_path(),
                    state.state.notebook_path(),
                    &state.state,
                );
            }
            Err(_err) => {
                #[cfg(debug_assertions)]
                eprintln!("Failed to read image clipboard data: {}", _err);
                return content_handler::handle_editor_action(
                    &mut state.content,
                    &mut state.markdown_text,
                    &mut state.undo_manager,
                    fallback_action,
                    state.state.selected_note_path(),
                    state.state.notebook_path(),
                    &state.state,
                );
            }
        };

        let Some(selected_note_path) = state.state.selected_note_path().cloned() else {
            return Task::none();
        };

        state.undo_manager.add_to_history(
            &selected_note_path,
            state.markdown_text.clone(),
            state.content.cursor(),
        );

        let image_id = generate_embedded_image_id();
        let tag = format!("![image:{}]", image_id);
        state
            .content
            .perform(Action::Edit(Edit::Paste(Arc::new(tag))));
        state.markdown_text = state.content.text();
        state.embedded_images.insert(image_id, image_base64);
        state.prune_embedded_images_for_current_markdown();
        state.touch_selected_note_last_updated();
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
            state.persist_embedded_images_task(),
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
            state.prune_embedded_images_for_current_markdown();
            state.sync_markdown_preview();
            return Task::batch(vec![task, state.persist_embedded_images_task()]);
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
            Message::EmbeddedImagesSaved(result) => {
                if let Err(_err) = result {
                    #[cfg(debug_assertions)]
                    eprintln!("Error saving embedded images: {}", _err);
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
                }

                Task::none()
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

        if state.markdown_text != previous_markdown {
            if state.state.selected_note_path().is_none() {
                state.embedded_images.clear();
            } else {
                state.prune_embedded_images_for_current_markdown();
            }
            state.sync_markdown_preview();
            return Task::batch(vec![task, state.persist_embedded_images_task()]);
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
        self.sync_embedded_image_handles();
        let preview_markdown =
            build_markdown_preview_content(&self.markdown_text, &self.embedded_images);
        self.markdown_preview = iced::widget::markdown::Content::parse(&preview_markdown);
    }

    fn prune_embedded_images_for_current_markdown(&mut self) {
        let referenced = extract_embedded_image_ids(&self.markdown_text);
        self.embedded_images
            .retain(|image_id, _| referenced.contains(image_id));
    }

    fn persist_embedded_images_task(&self) -> Task<Message> {
        if let Some(selected_note_path) = self.state.selected_note_path() {
            let notebook_path = self.state.notebook_path().to_string();
            let note_path = selected_note_path.clone();
            let images = self.embedded_images.clone();
            Task::perform(
                async move {
                    notebook::save_note_embedded_images(notebook_path, note_path, images).await
                },
                Message::EmbeddedImagesSaved,
            )
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

    fn sync_embedded_image_handles(&mut self) {
        self.embedded_image_handles
            .retain(|image_id, _| self.embedded_images.contains_key(image_id));

        for (image_id, base64_png) in &self.embedded_images {
            if self.embedded_image_handles.contains_key(image_id) {
                continue;
            }

            if let Some(png_bytes) = decode_base64_png_to_bytes(base64_png) {
                self.embedded_image_handles.insert(
                    image_id.clone(),
                    iced::widget::image::Handle::from_bytes(png_bytes),
                );
            }
        }
    }

    fn touch_selected_note_last_updated(&mut self) {
        if let Some(selected_path) = self.state.selected_note_path().cloned()
            && let Some(note) = self
                .note_explorer
                .notes
                .iter_mut()
                .find(|note| note.rel_path == selected_path)
        {
            note.last_updated = Some(notebook::current_timestamp_rfc3339());
        }
    }

    // Keep view method as is, but fix the state reference
    pub fn view(state: &Self) -> Element<'_, Message> {
        layout::generate_layout(
            &state.state,
            &state.content,
            &state.markdown_preview,
            &state.embedded_image_handles,
            &state.note_explorer,
            &state.visualizer,
        )
    }

    pub fn scale_factor(state: &Self) -> f32 {
        state.state.ui_scale()
    }

    // Keep subscription method as is
    pub fn subscription(_state: &Self) -> Subscription<Message> {
        iced::event::listen_with(|event, _status, _shell| {
            match event {
                Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    // Handle Ctrl+A for Select All
                    if modifiers.control()
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
            }
        })
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
            undo_manager: UndoManager::new(),
            state: EditorState::new(),
            note_explorer: note_explorer::NoteExplorer::new(String::new()),
            visualizer: visualizer::Visualizer::new(),
        }
    }
}

fn generate_embedded_image_id() -> String {
    let timestamp_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("img_{timestamp_nanos:x}")
}

fn extract_embedded_image_ids(markdown: &str) -> HashSet<String> {
    let marker = "![image:";
    let mut referenced = HashSet::new();
    let mut cursor = 0usize;

    while let Some(relative_start) = markdown[cursor..].find(marker) {
        let start = cursor + relative_start + marker.len();
        let Some(relative_end) = markdown[start..].find(']') else {
            break;
        };

        let end = start + relative_end;
        let image_id = markdown[start..end].trim();
        if !image_id.is_empty() {
            referenced.insert(image_id.to_string());
        }

        cursor = end + 1;
    }

    referenced
}

fn build_markdown_preview_content(markdown: &str, images: &HashMap<String, String>) -> String {
    let marker = "![image:";
    let mut cursor = 0usize;
    let mut rendered = String::with_capacity(markdown.len());

    while let Some(relative_start) = markdown[cursor..].find(marker) {
        let start = cursor + relative_start;
        rendered.push_str(&markdown[cursor..start]);

        let image_id_start = start + marker.len();
        let Some(relative_end) = markdown[image_id_start..].find(']') else {
            rendered.push_str(&markdown[start..]);
            cursor = markdown.len();
            break;
        };

        let image_id_end = image_id_start + relative_end;
        let image_id = markdown[image_id_start..image_id_end].trim();

        if images.contains_key(image_id) {
            rendered.push_str("![image](cognate-image://");
            rendered.push_str(image_id);
            rendered.push(')');
        } else {
            rendered.push_str(&markdown[start..=image_id_end]);
        }

        cursor = image_id_end + 1;
    }

    if cursor < markdown.len() {
        rendered.push_str(&markdown[cursor..]);
    }

    rendered
}

fn decode_base64_png_to_bytes(base64_data: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .ok()
}

fn read_clipboard_image_as_base64_png() -> Result<Option<String>, String> {
    use base64::Engine;

    let mut clipboard =
        arboard::Clipboard::new().map_err(|err| format!("Failed to open clipboard: {}", err))?;
    let image = match clipboard.get_image() {
        Ok(image) => image,
        Err(_) => return Ok(None),
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

fn read_clipboard_text() -> Result<Option<String>, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|err| format!("Failed to open clipboard: {}", err))?;

    match clipboard.get_text() {
        Ok(text) => Ok(Some(text)),
        Err(_) => Ok(None),
    }
}

fn round_scale_step(scale: f32) -> f32 {
    (scale * 100.0).round() / 100.0
}

#[cfg(test)]
mod image_tag_tests {
    use super::{build_markdown_preview_content, extract_embedded_image_ids};
    use std::collections::{HashMap, HashSet};

    #[test]
    fn extract_embedded_image_ids_parses_all_tags() {
        let markdown = "A ![image:img_one] and ![image:img_two].";
        let ids = extract_embedded_image_ids(markdown);

        let expected: HashSet<String> = ["img_one".to_string(), "img_two".to_string()]
            .into_iter()
            .collect();
        assert_eq!(ids, expected);
    }

    #[test]
    fn build_markdown_preview_content_replaces_known_tags() {
        let markdown = "before ![image:img_one] after ![image:missing]";
        let mut images = HashMap::new();
        images.insert("img_one".to_string(), "AAAABBBB".to_string());

        let rendered = build_markdown_preview_content(markdown, &images);
        assert!(rendered.contains("cognate-image://img_one"));
        assert!(rendered.contains("![image:missing]"));
    }
}
