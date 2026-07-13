use iced::task::Task;
use iced::widget::text_editor::{Action, Cursor};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::components::editor::Message;

use super::embedded_images::resolve_embedded_image_reference;
use super::preview::{extract_embedded_image_ids, preview_markdown_after_action};

#[derive(Debug, Default)]
pub(super) struct EmbeddedImageWorkflow {
    images: HashMap<String, String>,
    image_handles: HashMap<String, iced::widget::image::Handle>,
    pending_deletion_ids: HashSet<String>,
    pending_delete_action: Option<Action>,
    prompt_note_path: Option<String>,
}

impl EmbeddedImageWorkflow {
    pub fn images(&self) -> &HashMap<String, String> {
        &self.images
    }

    pub fn image_handles(&self) -> &HashMap<String, iced::widget::image::Handle> {
        &self.image_handles
    }

    pub fn set_loaded_images(&mut self, images: HashMap<String, String>) {
        self.images = images;
    }

    pub fn clear_all(&mut self) {
        self.images.clear();
        self.image_handles.clear();
        self.pending_deletion_ids.clear();
        self.pending_delete_action = None;
        self.prompt_note_path = None;
    }

    pub fn prune_for_current_markdown(
        &mut self,
        notebook_path: &str,
        selected_note_path: Option<&String>,
        markdown_text: &str,
    ) -> bool {
        let current_note_path = selected_note_path.cloned();
        let pending_state_cleared = self.prompt_note_path != current_note_path;

        if pending_state_cleared {
            self.pending_deletion_ids.clear();
            self.pending_delete_action = None;
            self.prompt_note_path = current_note_path;
        }

        self.refresh_embedded_images_for_current_markdown(
            notebook_path,
            selected_note_path,
            markdown_text,
        );

        pending_state_cleared
    }

    pub fn sync_preview_assets(
        &mut self,
        notebook_path: &str,
        selected_note_path: Option<&String>,
        markdown_text: &str,
    ) -> Task<Message> {
        self.refresh_embedded_images_for_current_markdown(
            notebook_path,
            selected_note_path,
            markdown_text,
        );
        self.sync_embedded_image_handles(notebook_path)
    }

    fn refresh_embedded_images_for_current_markdown(
        &mut self,
        notebook_path: &str,
        selected_note_path: Option<&String>,
        markdown_text: &str,
    ) {
        self.images.clear();

        let Some(selected_note_path) = selected_note_path else {
            return;
        };

        if notebook_path.is_empty() {
            return;
        }

        let note_dir = Path::new(notebook_path).join(selected_note_path);

        for image_ref in extract_embedded_image_ids(markdown_text) {
            if resolve_embedded_image_reference(&note_dir, &image_ref).is_some() {
                let rel_path = format!("{}/{}", selected_note_path, image_ref);
                self.images.insert(image_ref, rel_path);
            }
        }
    }

    fn sync_embedded_image_handles(&mut self, notebook_path: &str) -> Task<Message> {
        self.image_handles
            .retain(|image_id, _| self.images.contains_key(image_id));

        let mut tasks = Vec::new();
        for (image_id, image_rel_path) in &self.images {
            if self.image_handles.contains_key(image_id) {
                continue;
            }

            let path = PathBuf::from(notebook_path);
            let rel_path = image_rel_path.clone();
            let img_id = image_id.clone();

            tasks.push(Task::perform(
                async move {
                    let result = cognate_engine::storage::AttachmentManager::read_image_bytes(
                        &path, &rel_path,
                    )
                    .await
                    .map_err(|err| err.to_string());
                    (img_id, result)
                },
                |(img_id, result)| Message::AttachmentLoaded(img_id, result),
            ));
        }

        Task::batch(tasks)
    }

    pub fn insert_image_handle(&mut self, image_id: String, bytes: Vec<u8>) {
        self.image_handles
            .insert(image_id, iced::widget::image::Handle::from_bytes(bytes));
    }

    pub fn dereferenced_for_action(
        &self,
        markdown_text: &str,
        cursor: Cursor,
        action: &Action,
    ) -> HashSet<String> {
        if self.images.is_empty() {
            return HashSet::new();
        }

        let Some(after_markdown) = preview_markdown_after_action(markdown_text, cursor, action)
        else {
            return HashSet::new();
        };

        if after_markdown == markdown_text {
            return HashSet::new();
        }

        let before = extract_embedded_image_ids(markdown_text);
        let after = extract_embedded_image_ids(&after_markdown);

        before
            .into_iter()
            .filter(|image_id| self.images.contains_key(image_id) && !after.contains(image_id))
            .collect()
    }

    pub fn stage_pending_deletion(&mut self, image_ids: HashSet<String>, action: Action) {
        self.pending_deletion_ids = image_ids;
        self.pending_delete_action = Some(action);
    }

    pub fn has_pending_deletion(&self) -> bool {
        !self.pending_deletion_ids.is_empty()
    }

    pub fn pending_deletion_count(&self) -> usize {
        self.pending_deletion_ids.len()
    }

    pub fn take_pending_delete_action(&mut self) -> Option<Action> {
        self.pending_delete_action.take()
    }

    pub fn clear_pending_deletion(&mut self) {
        self.pending_deletion_ids.clear();
        self.pending_delete_action = None;
    }

    pub fn take_pending_deletion_ids(&mut self) -> HashSet<String> {
        std::mem::take(&mut self.pending_deletion_ids)
    }

    pub fn remove_image_path_for_id(&mut self, image_id: &str) -> Option<String> {
        self.image_handles.remove(image_id);
        self.images.remove(image_id)
    }
}
