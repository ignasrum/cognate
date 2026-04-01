use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::EMBEDDED_IMAGE_DIR;

fn generate_embedded_image_id() -> String {
    let timestamp_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("img_{timestamp_nanos:x}")
}

pub(super) fn save_base64_image_for_note(
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

fn decode_base64_image_to_bytes(base64_data: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .ok()
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
