use std::path::{Path, PathBuf};

use super::EMBEDDED_IMAGE_DIR;

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
