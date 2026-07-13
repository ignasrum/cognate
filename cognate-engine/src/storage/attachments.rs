use base64::Engine;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::EngineError;

fn generate_embedded_image_id() -> String {
    let timestamp_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("img_{timestamp_nanos:x}")
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
    base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .ok()
}

pub struct AttachmentManager;

impl AttachmentManager {
    /// Decodes base64 payload and writes it to target note's subfolder, returning the Markdown relative path.
    /// Performs format signature checks (magic bytes) to resolve file extension (.png, .jpg, etc.).
    pub async fn save_image_from_base64(
        notebook_path: &Path,
        rel_note_path: &str,
        base64_data: &str,
    ) -> Result<String, EngineError> {
        let image_bytes = decode_base64_image_to_bytes(base64_data).ok_or_else(|| {
            EngineError::validation("save_image", "Failed to decode image data from base64.")
        })?;

        let extension = image_extension_from_bytes(&image_bytes).unwrap_or("png");
        let image_id = generate_embedded_image_id();
        let file_name = format!("{image_id}.{extension}");

        let note_dir = notebook_path.join(rel_note_path);
        let images_dir = note_dir.join("images");

        // Safety check to ensure images_dir is under notebook_path
        super::fs_utils::ensure_path_within_notebook_if_canonicalizable(
            notebook_path,
            &images_dir,
            rel_note_path,
            "Image directory escapes notebook boundaries",
        )
        .await?;

        tokio::fs::create_dir_all(&images_dir)
            .await
            .map_err(|err| {
                EngineError::storage(
                    "save_image",
                    format!("Failed to create image directory: {}", err),
                )
            })?;

        let image_path = images_dir.join(&file_name);
        tokio::fs::write(&image_path, image_bytes)
            .await
            .map_err(|err| {
                EngineError::storage("save_image", format!("Failed to write image file: {}", err))
            })?;

        Ok(format!("images/{}", file_name))
    }

    /// Reads raw image file bytes for UI rendering
    pub async fn read_image_bytes(
        notebook_path: &Path,
        rel_path: &str,
    ) -> Result<Vec<u8>, EngineError> {
        let full_path = notebook_path.join(rel_path);

        // Safety check to ensure we don't escape notebook_path
        super::fs_utils::ensure_path_within_notebook_if_canonicalizable(
            notebook_path,
            &full_path,
            rel_path,
            "Attachment path escapes notebook boundaries",
        )
        .await?;

        tokio::fs::read(&full_path).await.map_err(|err| {
            EngineError::storage(
                "read_image_bytes",
                format!("Failed to read attachment file: {}", err),
            )
        })
    }

    /// Deletes specific attachment file
    pub async fn delete_attachment(
        notebook_path: &Path,
        rel_path: &str,
    ) -> Result<(), EngineError> {
        let full_path = notebook_path.join(rel_path);

        // Safety check to ensure we don't escape notebook_path
        super::fs_utils::ensure_path_within_notebook_if_canonicalizable(
            notebook_path,
            &full_path,
            rel_path,
            "Attachment path escapes notebook boundaries",
        )
        .await?;

        if tokio::fs::try_exists(&full_path).await.unwrap_or(false) {
            tokio::fs::remove_file(&full_path).await.map_err(|err| {
                EngineError::storage(
                    "delete_attachment",
                    format!("Failed to delete attachment: {}", err),
                )
            })?;
        }

        Ok(())
    }
}
