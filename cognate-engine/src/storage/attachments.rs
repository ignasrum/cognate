use base64::Engine;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::concurrency::ConcurrencyManager;
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AttachmentMetadata {
    pub rel_path: String,
    pub media_type: String,
    pub size: u64,
    pub revision: String,
}

pub fn attachment_revision(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn media_type_for_extension(extension: &str) -> &'static str {
    match extension {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
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

        let extension = image_extension_from_bytes(&image_bytes).ok_or_else(|| {
            EngineError::validation("save_image", "Unsupported or invalid image signature.")
        })?;
        Self::save_image_bytes_with_extension(notebook_path, rel_note_path, &image_bytes, extension)
            .await
    }

    pub async fn save_image_bytes(
        notebook_path: &Path,
        rel_note_path: &str,
        image_bytes: &[u8],
    ) -> Result<String, EngineError> {
        let extension = image_extension_from_bytes(image_bytes).ok_or_else(|| {
            EngineError::validation("save_image", "Unsupported or invalid image signature.")
        })?;
        Self::save_image_bytes_with_extension(notebook_path, rel_note_path, image_bytes, extension)
            .await
    }

    async fn save_image_bytes_with_extension(
        notebook_path: &Path,
        rel_note_path: &str,
        image_bytes: &[u8],
        extension: &str,
    ) -> Result<String, EngineError> {
        let image_id = generate_embedded_image_id();
        let file_name = format!("{image_id}.{extension}");
        let concurrency = ConcurrencyManager::new(notebook_path);
        let _notebook_lock = concurrency.acquire_notebook().await?;
        let _note_lock = concurrency.acquire_note(rel_note_path).await?;

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
        super::fs_utils::write_bytes_file_atomically(&image_path, image_bytes)
            .await
            .map_err(|error| {
                EngineError::storage("save_image", format!("Failed to write image file: {error}"))
            })?;

        Ok(format!("images/{}", file_name))
    }
    pub async fn list_attachments(
        notebook_path: &Path,
        rel_note_path: &str,
    ) -> Result<Vec<AttachmentMetadata>, EngineError> {
        let note_path = super::fs_utils::validate_relative_path("note path", rel_note_path)?;
        let images_dir = notebook_path.join(&note_path).join("images");
        super::fs_utils::ensure_path_within_notebook_if_canonicalizable(
            notebook_path,
            &images_dir,
            rel_note_path,
            "Attachment directory escapes notebook boundaries",
        )
        .await?;
        let mut entries = Vec::new();
        let mut directory = match tokio::fs::read_dir(&images_dir).await {
            Ok(directory) => directory,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(entries),
            Err(error) => return Err(EngineError::storage("list attachments", error.to_string())),
        };
        while let Some(entry) = directory
            .next_entry()
            .await
            .map_err(|error| EngineError::storage("list attachments", error.to_string()))?
        {
            let file_type = entry
                .file_type()
                .await
                .map_err(|error| EngineError::storage("list attachments", error.to_string()))?;
            if !file_type.is_file() {
                continue;
            }
            let entry_path = entry.path();
            super::fs_utils::ensure_path_within_notebook_if_canonicalizable(
                notebook_path,
                &entry_path,
                rel_note_path,
                "Attachment path escapes notebook boundaries",
            )
            .await?;
            let bytes = tokio::fs::read(&entry_path)
                .await
                .map_err(|error| EngineError::storage("list attachments", error.to_string()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let extension = name.rsplit('.').next().unwrap_or_default();
            entries.push(AttachmentMetadata {
                rel_path: format!("images/{name}"),
                media_type: media_type_for_extension(extension).to_string(),
                size: bytes.len() as u64,
                revision: attachment_revision(&bytes),
            });
        }
        entries.sort_by(|left, right| left.rel_path.cmp(&right.rel_path));
        Ok(entries)
    }

    pub async fn read_attachment_bytes(
        notebook_path: &Path,
        rel_note_path: &str,
        attachment_path: &str,
    ) -> Result<Vec<u8>, EngineError> {
        let note_path = super::fs_utils::validate_relative_path("note path", rel_note_path)?;
        let attachment =
            super::fs_utils::validate_relative_path("attachment path", attachment_path)?;
        if !attachment.starts_with("images/") {
            return Err(EngineError::validation(
                "attachment path",
                "attachment must be under images/",
            ));
        }
        let full_relative_path = note_path.join(attachment);
        Self::read_image_bytes(notebook_path, full_relative_path.to_string_lossy().as_ref()).await
    }

    pub async fn replace_attachment_bytes(
        notebook_path: &Path,
        rel_note_path: &str,
        attachment_path: &str,
        expected_revision: &str,
        bytes: &[u8],
    ) -> Result<String, EngineError> {
        let note_path = super::fs_utils::validate_relative_path("note path", rel_note_path)?;
        let attachment =
            super::fs_utils::validate_relative_path("attachment path", attachment_path)?;
        if !attachment.starts_with("images/") {
            return Err(EngineError::validation(
                "attachment path",
                "attachment must be under images/",
            ));
        }
        let concurrency = ConcurrencyManager::new(notebook_path);
        let _notebook_lock = concurrency.acquire_notebook().await?;
        let _note_lock = concurrency.acquire_note(rel_note_path).await?;
        let full_path = notebook_path.join(&note_path).join(&attachment);
        super::fs_utils::ensure_path_within_notebook_if_canonicalizable(
            notebook_path,
            &full_path,
            rel_note_path,
            "Attachment path escapes notebook boundaries",
        )
        .await?;
        let current = tokio::fs::read(&full_path)
            .await
            .map_err(|error| EngineError::storage("replace attachment", error.to_string()))?;
        let current_revision = attachment_revision(&current);
        if expected_revision != current_revision {
            return Err(EngineError::conflict(
                "replace attachment",
                format!(
                    "expected revision '{}' but found '{}'",
                    expected_revision, current_revision
                ),
            ));
        }
        super::fs_utils::write_bytes_file_atomically(&full_path, bytes).await?;
        Ok(attachment_revision(bytes))
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
        let concurrency = ConcurrencyManager::new(notebook_path);
        let _lock = concurrency.acquire_notebook().await?;
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

    pub async fn delete_attachment_if_match(
        notebook_path: &Path,
        rel_note_path: &str,
        attachment_path: &str,
        expected_revision: &str,
    ) -> Result<(), EngineError> {
        let note_path = super::fs_utils::validate_relative_path("note path", rel_note_path)?;
        let attachment =
            super::fs_utils::validate_relative_path("attachment path", attachment_path)?;
        if !attachment.starts_with("images/") {
            return Err(EngineError::validation(
                "attachment path",
                "attachment must be under images/",
            ));
        }
        let concurrency = ConcurrencyManager::new(notebook_path);
        let _notebook_lock = concurrency.acquire_notebook().await?;
        let _note_lock = concurrency.acquire_note(rel_note_path).await?;
        let full_path = notebook_path.join(&note_path).join(&attachment);
        super::fs_utils::ensure_path_within_notebook_if_canonicalizable(
            notebook_path,
            &full_path,
            rel_note_path,
            "Attachment path escapes notebook boundaries",
        )
        .await?;
        let current = tokio::fs::read(&full_path)
            .await
            .map_err(|error| EngineError::storage("delete attachment", error.to_string()))?;
        let current_revision = attachment_revision(&current);
        if expected_revision != "*" && expected_revision != current_revision {
            return Err(EngineError::conflict(
                "delete attachment",
                format!(
                    "expected revision '{}' but found '{}', attachment was not deleted",
                    expected_revision, current_revision
                ),
            ));
        }
        tokio::fs::remove_file(&full_path)
            .await
            .map_err(|error| EngineError::storage("delete attachment", error.to_string()))
    }
}
