use crate::notebook::{self, NoteMetadata};

pub(super) fn flush_editor_state_for_shutdown(
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

pub(super) fn round_scale_step(scale: f32) -> f32 {
    (scale * 100.0).round() / 100.0
}
