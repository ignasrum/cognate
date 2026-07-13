use std::path::{Path, PathBuf};

use super::EMBEDDED_IMAGE_DIR;

pub(super) fn save_base64_image_for_note(
    notebook_path: &str,
    rel_note_path: &str,
    base64_image: &str,
) -> Result<String, String> {
    cognate_engine::storage::AttachmentManager::save_image_from_base64(
        Path::new(notebook_path),
        rel_note_path,
        base64_image,
    )
    .map_err(|err| err.to_string())
}

pub(super) fn resolve_embedded_image_reference(
    note_dir: &Path,
    image_ref: &str,
) -> Option<PathBuf> {
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
