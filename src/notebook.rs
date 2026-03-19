use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const STAGED_DELETE_PREFIX: &str = ".cognate_txn_delete_";
const STAGED_DELETE_CLEANUP_GRACE_NANOS: u128 = 5 * 60 * 1_000_000_000;
const EMBEDDED_IMAGES_FILE: &str = "embedded_images.json";
const EXPORTED_MARKDOWN_DIR: &str = "exported_markdown";
const EXPORTED_MARKDOWN_ATTACHMENTS_DIR: &str = "images";

// These structs are now defined once in this common module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMetadata {
    pub rel_path: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookMetadata {
    pub notes: Vec<NoteMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSearchResult {
    pub rel_path: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct EmbeddedImagesStore {
    #[serde(default)]
    images: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownWithAttachmentsExportSummary {
    pub markdown_path: String,
    pub attachments_dir: String,
    pub exported_count: usize,
    pub skipped_count: usize,
    pub rewritten_reference_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExportedEmbeddedImagesResult {
    exported_count: usize,
    skipped_count: usize,
    exported_file_names_by_id: HashMap<String, String>,
}

pub fn current_timestamp_rfc3339() -> String {
    OffsetDateTime::from_unix_timestamp(OffsetDateTime::now_utc().unix_timestamp())
        .ok()
        .and_then(|timestamp| timestamp.format(&Rfc3339).ok())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

fn format_system_time_rfc3339(timestamp: SystemTime) -> Option<String> {
    OffsetDateTime::from_unix_timestamp(OffsetDateTime::from(timestamp).unix_timestamp())
        .ok()
        .and_then(|dt| dt.format(&Rfc3339).ok())
}

fn normalize_rfc3339_to_seconds(timestamp: &str) -> String {
    if let Some(dot_index) = timestamp.find('.') {
        let base = &timestamp[..dot_index];
        let remainder = &timestamp[dot_index + 1..];
        if let Some(tz_index) = remainder.find(['Z', '+', '-']) {
            return format!("{}{}", base, &remainder[tz_index..]);
        }
        return base.to_string();
    }
    timestamp.to_string()
}

fn truncate_search_snippet(input: &str, max_chars: usize) -> String {
    let char_count = input.chars().count();
    if char_count <= max_chars {
        input.to_string()
    } else {
        let mut truncated: String = input.chars().take(max_chars).collect();
        truncated.push_str("...");
        truncated
    }
}

fn find_matching_content_snippet(content: &str, normalized_query: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.to_lowercase().contains(normalized_query) {
            return Some(truncate_search_snippet(trimmed, 120));
        }
    }

    None
}

pub async fn search_notes(
    notebook_path: String,
    notes: Vec<NoteMetadata>,
    query: String,
) -> Vec<NoteSearchResult> {
    let normalized_query = query.trim().to_lowercase();
    if normalized_query.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();

    for note in notes {
        let rel_path_match = note.rel_path.to_lowercase().contains(&normalized_query);
        let label_match = note
            .labels
            .iter()
            .find(|label| label.to_lowercase().contains(&normalized_query))
            .cloned();

        let note_file_path = Path::new(&notebook_path)
            .join(&note.rel_path)
            .join("note.md");
        let content_match = fs::read_to_string(note_file_path)
            .ok()
            .and_then(|content| find_matching_content_snippet(&content, &normalized_query));

        if rel_path_match || label_match.is_some() || content_match.is_some() {
            let snippet = if let Some(content_snippet) = content_match {
                content_snippet
            } else if let Some(matching_label) = label_match {
                format!(
                    "Label match: {}",
                    truncate_search_snippet(matching_label.as_str(), 100)
                )
            } else {
                "Path match".to_string()
            };

            results.push(NoteSearchResult {
                rel_path: note.rel_path,
                snippet,
            });
        }
    }

    results.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    results
}

fn validate_notebook_relative_path(rel_path: &str, path_kind: &str) -> Result<(), String> {
    if rel_path.is_empty()
        || rel_path == "."
        || rel_path == ".."
        || rel_path.starts_with('/')
        || rel_path.contains("..")
    {
        return Err(format!(
            "Invalid {} '{}'. Paths cannot be empty, '.', '..', start with '/', or contain '..'.",
            path_kind, rel_path
        ));
    }
    Ok(())
}

fn ensure_path_within_notebook_if_canonicalizable(
    notebook_path: &Path,
    target_path: &Path,
    rel_path: &str,
    outside_error_prefix: &str,
    _target_canonicalize_warning: &str,
) -> Result<(), String> {
    if let Ok(canonical_notebook_path) = notebook_path.canonicalize() {
        if let Ok(canonical_target_path) = target_path.canonicalize() {
            if !canonical_target_path.starts_with(&canonical_notebook_path) {
                return Err(format!("{} '{}'", outside_error_prefix, rel_path));
            }
        } else {
            #[cfg(debug_assertions)]
            eprintln!("{}", _target_canonicalize_warning);
        }
    } else {
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: Could not canonicalize notebook path '{}'. Skipping thorough path validation.",
            notebook_path.display()
        );
    }

    Ok(())
}

fn remove_note_from_metadata(notes: &mut Vec<NoteMetadata>, rel_path: &str) -> bool {
    if let Some(index) = notes.iter().position(|note| note.rel_path == rel_path) {
        notes.remove(index);
        true
    } else {
        false
    }
}

fn update_metadata_paths_for_move(
    notes: &mut [NoteMetadata],
    current_rel_path: &str,
    new_rel_path: &str,
    is_moving_note_dir: bool,
) -> bool {
    let mut updated_metadata = false;

    if is_moving_note_dir {
        if let Some(note) = notes
            .iter_mut()
            .find(|note| note.rel_path == current_rel_path)
        {
            note.rel_path = new_rel_path.to_string();
            updated_metadata = true;
        }
    } else {
        let old_prefix = if current_rel_path.is_empty() {
            String::new()
        } else {
            format!("{}/", current_rel_path)
        };

        let new_prefix = if new_rel_path.is_empty() {
            String::new()
        } else {
            format!("{}/", new_rel_path)
        };

        for note in notes.iter_mut() {
            if note.rel_path.starts_with(&old_prefix) {
                let suffix = note.rel_path.trim_start_matches(&old_prefix);
                note.rel_path = format!("{}{}", new_prefix, suffix);
                updated_metadata = true;
            } else if note.rel_path == current_rel_path && !current_rel_path.is_empty() {
                note.rel_path = new_rel_path.to_string();
                updated_metadata = true;
            }
        }
    }

    updated_metadata
}

fn persist_metadata_if_changed(
    notebook_path: &str,
    notes: &[NoteMetadata],
    metadata_changed: bool,
    operation_description: &str,
    _rel_path: &str,
) -> Result<(), String> {
    if metadata_changed {
        if let Err(e) = save_metadata(notebook_path, notes) {
            #[cfg(debug_assertions)]
            eprintln!(
                "Critical Error: Failed to save metadata after {}: {}",
                operation_description, e
            );
            return Err(format!(
                "Failed to save metadata after {}: {}",
                operation_description, e
            ));
        }
        #[cfg(debug_assertions)]
        eprintln!(
            "Metadata saved successfully after {}.",
            operation_description
        );
    } else {
        #[cfg(debug_assertions)]
        eprintln!(
            "No relevant metadata found or updated for '{}', skipping metadata save.",
            _rel_path
        );
    }

    Ok(())
}

fn build_transaction_staging_path(
    notebook_path: &Path,
    rel_path: &str,
    operation: &str,
) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sanitized_rel_path = rel_path.replace('/', "__");

    notebook_path.join(format!(
        ".cognate_txn_{}_{}_{}",
        operation, sanitized_rel_path, timestamp
    ))
}

fn cleanup_stale_staged_delete_entries(notebook_path: &Path) {
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    let read_dir = match fs::read_dir(notebook_path) {
        Ok(entries) => entries,
        Err(_e) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "Warning: Failed to scan notebook directory '{}' for stale staged deletes: {}",
                notebook_path.display(),
                _e
            );
            return;
        }
    };

    for entry_result in read_dir {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(_e) => {
                #[cfg(debug_assertions)]
                eprintln!("Warning: Failed to read notebook directory entry: {}", _e);
                continue;
            }
        };

        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();

        if !file_name.starts_with(STAGED_DELETE_PREFIX) {
            continue;
        }

        let timestamp_nanos = match file_name.rsplit('_').next() {
            Some(ts) => match ts.parse::<u128>() {
                Ok(parsed) => parsed,
                Err(_) => {
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "Warning: Could not parse staged-delete timestamp from '{}'; skipping cleanup for this entry.",
                        file_name
                    );
                    continue;
                }
            },
            None => continue,
        };

        let age_nanos = now_nanos.saturating_sub(timestamp_nanos);
        if age_nanos < STAGED_DELETE_CLEANUP_GRACE_NANOS {
            continue;
        }

        let staged_path = entry.path();
        let removal_result = if staged_path.is_dir() {
            fs::remove_dir_all(&staged_path)
        } else {
            fs::remove_file(&staged_path)
        };

        if let Err(_e) = removal_result {
            #[cfg(debug_assertions)]
            eprintln!(
                "Warning: Failed to remove stale staged delete '{}': {}",
                staged_path.display(),
                _e
            );
        } else {
            #[cfg(debug_assertions)]
            eprintln!(
                "Cleaned up stale staged delete '{}'.",
                staged_path.display()
            );
        }
    }
}

fn remove_empty_parent_directories(notebook_path: &Path, deleted_note_dir_path: &Path) {
    let canonical_notebook_path = notebook_path.canonicalize().ok();
    let mut current_parent = deleted_note_dir_path.parent().map(Path::to_path_buf);

    while let Some(parent_path) = current_parent {
        if parent_path == notebook_path {
            break;
        }

        if let Some(canonical_root) = canonical_notebook_path.as_ref() {
            if let Ok(canonical_parent) = parent_path.canonicalize() {
                if canonical_parent == *canonical_root
                    || !canonical_parent.starts_with(canonical_root)
                {
                    break;
                }
            } else {
                break;
            }
        } else if parent_path == notebook_path || !parent_path.starts_with(notebook_path) {
            break;
        }

        match fs::remove_dir(&parent_path) {
            Ok(()) => {
                #[cfg(debug_assertions)]
                eprintln!(
                    "Removed empty parent directory after delete: {}",
                    parent_path.display()
                );
                current_parent = parent_path.parent().map(Path::to_path_buf);
            }
            Err(_e) if _e.kind() == ErrorKind::NotFound => {
                current_parent = parent_path.parent().map(Path::to_path_buf);
            }
            Err(_e) if _e.kind() == ErrorKind::DirectoryNotEmpty => {
                break;
            }
            Err(_e) => {
                #[cfg(debug_assertions)]
                eprintln!(
                    "Warning: Failed to remove parent directory '{}' after delete: {}",
                    parent_path.display(),
                    _e
                );
                break;
            }
        }
    }
}

// The save_metadata function also lives here
pub fn save_metadata(
    notebook_path: &str,
    notes: &[NoteMetadata],
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    #[cfg(debug_assertions)]
    eprintln!(
        "Saving metadata to: {}",
        Path::new(notebook_path).join("metadata.json").display()
    );

    let metadata_path = Path::new(notebook_path).join("metadata.json");

    // Ensure the notebook directory exists before saving metadata
    if let Some(parent) = metadata_path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        #[cfg(debug_assertions)]
        eprintln!("Failed to create parent directory for metadata file: {}", e);
        return Err(Box::new(e));
    }

    let notebook_metadata = NotebookMetadata {
        notes: notes.to_vec(),
    };

    let json_string = serde_json::to_string_pretty(&notebook_metadata)?;

    fs::write(&metadata_path, json_string)?;

    #[cfg(debug_assertions)]
    eprintln!("Metadata saved successfully.");
    Ok(())
}

// The load_notes_metadata function from note_explorer.rs should also probably live here
// to keep metadata logic together. I'll move it and adjust note_explorer.rs accordingly.
pub async fn load_notes_metadata(notebook_path: String) -> Vec<NoteMetadata> {
    let file_path = Path::new(&notebook_path).join("metadata.json");
    cleanup_stale_staged_delete_entries(Path::new(&notebook_path));
    #[cfg(debug_assertions)]
    eprintln!(
        "load_notes_metadata: Attempting to read file: {}",
        file_path.display()
    );

    let contents = match fs::read_to_string(&file_path) {
        Ok(c) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "load_notes_metadata: Successfully read file: {}",
                file_path.display()
            );
            c
        }
        Err(_err) => {
            // Added underscore here
            #[cfg(debug_assertions)]
            eprintln!(
                "load_notes_metadata: Error reading metadata file {}: {}",
                file_path.display(),
                _err // Use the underscore version here too
            );
            // If the file doesn't exist, assume it's a new notebook and return empty notes
            if _err.kind() == ErrorKind::NotFound {
                // Use the underscore version here too
                #[cfg(debug_assertions)]
                eprintln!("Metadata file not found, assuming new notebook.");
                return Vec::new();
            }
            return Vec::new(); // Return empty vector on other errors
        }
    };

    let metadata: NotebookMetadata = match serde_json::from_str(&contents) {
        Ok(m) => {
            #[cfg(debug_assertions)]
            eprintln!("load_notes_metadata: Successfully parsed metadata.");
            m
        }
        Err(_err) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "load_notes_metadata: Error parsing metadata from {}: {}",
                file_path.display(),
                _err
            );
            return Vec::new();
        }
    };

    let mut notes = metadata.notes;
    let mut metadata_changed = false;

    for note in &mut notes {
        if let Some(existing_timestamp) = note.last_updated.clone() {
            let normalized = normalize_rfc3339_to_seconds(&existing_timestamp);
            if normalized != existing_timestamp {
                note.last_updated = Some(normalized);
                metadata_changed = true;
            }
        } else {
            let note_file_path = Path::new(&notebook_path)
                .join(&note.rel_path)
                .join("note.md");

            if let Ok(file_metadata) = fs::metadata(note_file_path)
                && let Ok(modified_time) = file_metadata.modified()
                && let Some(formatted_time) = format_system_time_rfc3339(modified_time)
            {
                note.last_updated = Some(formatted_time);
                metadata_changed = true;
            }
        }
    }

    if metadata_changed && let Err(_error) = save_metadata(&notebook_path, &notes) {
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: failed to persist backfilled last_updated metadata: {}",
            _error
        );
    }

    notes
}

// Function to save note content
pub async fn save_note_content(
    notebook_path: String,
    rel_note_path: String,
    content: String,
) -> Result<(), String> {
    let full_note_path = Path::new(&notebook_path)
        .join(&rel_note_path)
        .join("note.md");
    #[cfg(debug_assertions)]
    eprintln!("Attempting to save note to: {}", full_note_path.display());

    // Ensure the directory exists before writing the file
    if let Some(parent) = full_note_path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        return Err(format!("Failed to create directory for note: {}", e));
    }

    let existing_content = match fs::read_to_string(&full_note_path) {
        Ok(existing) => Some(existing),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "Failed to read existing note before save: {}",
                error
            ));
        }
    };

    if existing_content.as_deref() == Some(content.as_str()) {
        return Ok(());
    }

    fs::write(&full_note_path, content).map_err(|e| format!("Failed to save note: {}", e))?;

    let mut notes = load_notes_metadata(notebook_path.clone()).await;
    let mut metadata_changed = false;
    let updated_at = current_timestamp_rfc3339();

    if let Some(note) = notes.iter_mut().find(|n| n.rel_path == rel_note_path)
        && note.last_updated.as_deref() != Some(updated_at.as_str())
    {
        note.last_updated = Some(updated_at);
        metadata_changed = true;
    }

    if metadata_changed {
        save_metadata(&notebook_path, &notes)
            .map_err(|e| format!("Failed to save metadata after content update: {}", e))?;
    }

    Ok(())
}

fn embedded_images_path(notebook_path: &str, rel_note_path: &str) -> PathBuf {
    Path::new(notebook_path)
        .join(rel_note_path)
        .join(EMBEDDED_IMAGES_FILE)
}

pub fn load_note_embedded_images(
    notebook_path: &str,
    rel_note_path: &str,
) -> HashMap<String, String> {
    let images_path = embedded_images_path(notebook_path, rel_note_path);

    let contents = match fs::read_to_string(&images_path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == ErrorKind::NotFound => return HashMap::new(),
        Err(_err) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "Failed to read embedded images store '{}': {}",
                images_path.display(),
                _err
            );
            return HashMap::new();
        }
    };

    match serde_json::from_str::<EmbeddedImagesStore>(&contents) {
        Ok(store) => store.images,
        Err(_err) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "Failed to parse embedded images store '{}': {}",
                images_path.display(),
                _err
            );
            HashMap::new()
        }
    }
}

pub async fn save_note_embedded_images(
    notebook_path: String,
    rel_note_path: String,
    images: HashMap<String, String>,
) -> Result<(), String> {
    let images_path = embedded_images_path(&notebook_path, &rel_note_path);

    if images.is_empty() {
        match fs::remove_file(&images_path) {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                return Err(format!(
                    "Failed to remove embedded images store '{}': {}",
                    images_path.display(),
                    err
                ));
            }
        }
        return Ok(());
    }

    if let Some(parent) = images_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create embedded image directory: {}", err))?;
    }

    let store = EmbeddedImagesStore { images };
    let serialized = serde_json::to_string_pretty(&store)
        .map_err(|err| format!("Failed to serialize embedded images store: {}", err))?;
    fs::write(&images_path, serialized).map_err(|err| {
        format!(
            "Failed to write embedded images store '{}': {}",
            images_path.display(),
            err
        )
    })?;

    Ok(())
}

pub async fn export_note_markdown_with_attachments(
    notebook_path: String,
    rel_note_path: String,
    markdown: String,
    images: HashMap<String, String>,
) -> Result<MarkdownWithAttachmentsExportSummary, String> {
    validate_notebook_relative_path(&rel_note_path, "relative path")?;

    let note_dir = Path::new(&notebook_path).join(&rel_note_path);
    if !note_dir.exists() {
        return Err(format!(
            "Cannot export markdown because note '{}' does not exist on disk.",
            rel_note_path
        ));
    }

    let export_root_dir = note_dir.join(EXPORTED_MARKDOWN_DIR);
    fs::create_dir_all(&export_root_dir).map_err(|err| {
        format!(
            "Failed to create markdown export directory '{}': {}",
            export_root_dir.display(),
            err
        )
    })?;

    let markdown_stem = Path::new(&rel_note_path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(sanitize_export_file_stem)
        .unwrap_or_else(|| "note".to_string());

    let markdown_path = unique_export_path(&export_root_dir, &markdown_stem, "md");
    let attachments_dir = export_root_dir.join(EXPORTED_MARKDOWN_ATTACHMENTS_DIR);

    let export_result = if images.is_empty() {
        ExportedEmbeddedImagesResult {
            exported_count: 0,
            skipped_count: 0,
            exported_file_names_by_id: HashMap::new(),
        }
    } else {
        fs::create_dir_all(&attachments_dir).map_err(|err| {
            format!(
                "Failed to create markdown attachment directory '{}': {}",
                attachments_dir.display(),
                err
            )
        })?;
        export_embedded_images_to_directory(&attachments_dir, &images)?
    };

    let (rewritten_markdown, rewritten_reference_count) = rewrite_embedded_image_tags_for_export(
        &markdown,
        &export_result.exported_file_names_by_id,
        EXPORTED_MARKDOWN_ATTACHMENTS_DIR,
    );

    fs::write(&markdown_path, rewritten_markdown).map_err(|err| {
        format!(
            "Failed to write exported markdown file '{}': {}",
            markdown_path.display(),
            err
        )
    })?;

    Ok(MarkdownWithAttachmentsExportSummary {
        markdown_path: markdown_path.to_string_lossy().to_string(),
        attachments_dir: attachments_dir.to_string_lossy().to_string(),
        exported_count: export_result.exported_count,
        skipped_count: export_result.skipped_count,
        rewritten_reference_count,
    })
}

fn export_embedded_images_to_directory(
    export_dir: &Path,
    images: &HashMap<String, String>,
) -> Result<ExportedEmbeddedImagesResult, String> {
    use base64::Engine;

    let mut sorted_images: Vec<(&String, &String)> = images.iter().collect();
    sorted_images.sort_by(|left, right| left.0.cmp(right.0));

    let mut exported_count = 0usize;
    let mut skipped_count = 0usize;
    let mut exported_file_names_by_id = HashMap::new();

    for (raw_image_id, encoded_image) in sorted_images {
        let image_bytes = match base64::engine::general_purpose::STANDARD.decode(encoded_image) {
            Ok(bytes) => bytes,
            Err(_err) => {
                skipped_count += 1;
                #[cfg(debug_assertions)]
                eprintln!(
                    "Skipping embedded image '{}' due to base64 decode error: {}",
                    raw_image_id, _err
                );
                continue;
            }
        };

        let extension = image_extension_from_bytes(&image_bytes).unwrap_or("img");
        let file_stem = sanitize_export_file_stem(raw_image_id);
        let file_path = unique_export_path(export_dir, &file_stem, extension);

        fs::write(&file_path, image_bytes).map_err(|err| {
            format!(
                "Failed to write exported image '{}': {}",
                file_path.display(),
                err
            )
        })?;

        let exported_file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}.{}", file_stem, extension));

        exported_file_names_by_id.insert(raw_image_id.clone(), exported_file_name);
        exported_count += 1;
    }

    Ok(ExportedEmbeddedImagesResult {
        exported_count,
        skipped_count,
        exported_file_names_by_id,
    })
}

fn rewrite_embedded_image_tags_for_export(
    markdown: &str,
    exported_file_names_by_id: &HashMap<String, String>,
    attachments_directory_name: &str,
) -> (String, usize) {
    let marker = "![image:";
    let mut cursor = 0usize;
    let mut rewritten_reference_count = 0usize;
    let mut rewritten = String::with_capacity(markdown.len());

    while let Some(relative_start) = markdown[cursor..].find(marker) {
        let start = cursor + relative_start;
        rewritten.push_str(&markdown[cursor..start]);

        let image_id_start = start + marker.len();
        let Some(relative_end) = markdown[image_id_start..].find(']') else {
            rewritten.push_str(&markdown[start..]);
            cursor = markdown.len();
            break;
        };

        let image_id_end = image_id_start + relative_end;
        let image_id = markdown[image_id_start..image_id_end].trim();
        let is_already_standard_markdown_image = markdown[image_id_end + 1..].starts_with('(');

        if !is_already_standard_markdown_image
            && let Some(exported_file_name) = exported_file_names_by_id.get(image_id)
        {
            rewritten.push_str("![image:");
            rewritten.push_str(image_id);
            rewritten.push_str("](");
            rewritten.push_str(attachments_directory_name);
            rewritten.push('/');
            rewritten.push_str(exported_file_name);
            rewritten.push(')');
            rewritten_reference_count += 1;
        } else {
            rewritten.push_str(&markdown[start..=image_id_end]);
        }

        cursor = image_id_end + 1;
    }

    if cursor < markdown.len() {
        rewritten.push_str(&markdown[cursor..]);
    }

    (rewritten, rewritten_reference_count)
}

fn sanitize_export_file_stem(raw: &str) -> String {
    let mut sanitized = String::with_capacity(raw.len());

    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }

    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "image".to_string()
    } else {
        trimmed.chars().take(64).collect()
    }
}

fn unique_export_path(directory: &Path, stem: &str, extension: &str) -> PathBuf {
    let mut candidate = directory.join(format!("{}.{}", stem, extension));
    let mut suffix = 1usize;

    while candidate.exists() {
        candidate = directory.join(format!("{}_{}.{}", stem, suffix, extension));
        suffix += 1;
    }

    candidate
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

    if bytes.starts_with(b"BM") {
        return Some("bmp");
    }

    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }

    if bytes.starts_with(&[b'I', b'I', 0x2A, 0x00]) || bytes.starts_with(&[b'M', b'M', 0x00, 0x2A])
    {
        return Some("tiff");
    }

    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Some("ico");
    }

    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        let brand = &bytes[8..12];

        if matches!(brand, b"avif" | b"avis") {
            return Some("avif");
        }

        if matches!(brand, b"heic" | b"heix" | b"heif" | b"hevx") {
            return Some("heic");
        }
    }

    None
}

// Function to create a new note
pub async fn create_new_note(
    notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>, // Pass the notes vector to update
) -> Result<NoteMetadata, String> {
    #[cfg(debug_assertions)]
    eprintln!("Attempting to create new note with rel_path: {}", rel_path);
    let full_notebook_path = Path::new(notebook_path);
    let note_dir_path = full_notebook_path.join(rel_path);
    let note_file_path = note_dir_path.join("note.md");

    validate_notebook_relative_path(rel_path, "relative path")?;
    ensure_path_within_notebook_if_canonicalizable(
        full_notebook_path,
        &note_dir_path,
        rel_path,
        "Cannot create note outside the notebook directory:",
        &format!(
            "Warning: Could not canonicalize new note path '{}'. This is expected if the parent directory doesn't exist yet.",
            rel_path
        ),
    )?;

    // Check if a note with the same relative path already exists in metadata
    if notes.iter().any(|note| note.rel_path == rel_path) {
        return Err(format!(
            "A note with the path '{}' already exists in metadata.",
            rel_path
        ));
    }

    // Check if the directory or file already exists on the filesystem within the notebook path
    if note_dir_path.exists() || note_file_path.exists() {
        // We've already done canonicalize check above, but repeating the exists check is fine.
        if note_dir_path.exists() {
            return Err(format!(
                "A directory or file already exists at '{}'.",
                rel_path
            ));
        }
    }

    // Create the note directory and the note.md file
    if let Err(e) = fs::create_dir_all(&note_dir_path) {
        return Err(format!("Failed to create directory for new note: {}", e));
    }

    if let Err(e) = fs::write(&note_file_path, "") {
        // Clean up the created directory if file creation fails
        let _ = fs::remove_dir_all(&note_dir_path);
        return Err(format!("Failed to create note file: {}", e));
    }

    // Create metadata for the new note
    let new_note_metadata = NoteMetadata {
        rel_path: rel_path.to_string(),
        labels: Vec::new(),
        last_updated: Some(current_timestamp_rfc3339()),
    };

    let previous_notes = notes.clone();

    // Add the new note metadata to the in-memory notes vector
    notes.push(new_note_metadata.clone());

    // Save the updated metadata file
    if let Err(e) = save_metadata(notebook_path, notes) {
        #[cfg(debug_assertions)]
        eprintln!(
            "Critical Error: Failed to save metadata after creating note: {}",
            e
        );
        // Roll back in-memory metadata and filesystem changes to avoid inconsistency.
        *notes = previous_notes;
        let cleanup_result = fs::remove_dir_all(&note_dir_path);
        if let Err(cleanup_error) = cleanup_result {
            return Err(format!(
                "Failed to save metadata after creating note: {}. Rollback cleanup failed: {}",
                e, cleanup_error
            ));
        }
        return Err(format!(
            "Failed to save metadata after creating note: {}",
            e
        ));
    }

    #[cfg(debug_assertions)]
    eprintln!("New note created successfully: {}", rel_path);
    Ok(new_note_metadata)
}

// Function to delete a note
pub async fn delete_note(
    notebook_path: &str,
    rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<(), String> {
    #[cfg(debug_assertions)]
    eprintln!("Attempting to delete note with rel_path: {}", rel_path);

    validate_notebook_relative_path(rel_path, "relative path")?;

    let note_dir_path = Path::new(notebook_path).join(rel_path);
    let full_notebook_path = Path::new(notebook_path);

    // Ensure the path is within the notebook directory
    if let Ok(canonical_notebook_path) = full_notebook_path.canonicalize() {
        if let Ok(canonical_note_dir_path) = note_dir_path.canonicalize() {
            if !canonical_note_dir_path.starts_with(&canonical_notebook_path) {
                return Err(format!(
                    "Cannot delete path outside the notebook directory: '{}'",
                    rel_path
                ));
            }
        } else {
            // If canonicalize fails, check if the path exists relative to the notebook root.
            // This is less safe but better than nothing if canonicalize fails.
            if !Path::new(notebook_path).join(rel_path).exists() {
                return Err(format!(
                    "Path '{}' does not exist within the notebook.",
                    rel_path
                ));
            }
            #[cfg(debug_assertions)]
            eprintln!(
                "Warning: Could not canonicalize path '{}'. Proceeding with deletion attempt based on relative path.",
                rel_path
            );
        }
    } else {
        // If canonicalize fails for notebook path, just rely on relative path existence check.
        if !Path::new(notebook_path).join(rel_path).exists() {
            return Err(format!(
                "Path '{}' does not exist within the notebook.",
                rel_path
            ));
        }
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: Could not canonicalize notebook path '{}'. Skipping thorough path validation.",
            notebook_path
        );
    }

    let previous_notes = notes.clone();
    let metadata_changed = remove_note_from_metadata(notes, rel_path);

    if !metadata_changed {
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: Note with rel_path '{}' not found in metadata. Proceeding with filesystem deletion only.",
            rel_path
        );
        // If not found in metadata, we still attempt to delete the directory on disk
    }

    let mut staged_delete_path: Option<PathBuf> = None;

    // Stage deletion by renaming first so we can roll back if metadata persistence fails.
    if note_dir_path.exists() {
        let transaction_path =
            build_transaction_staging_path(full_notebook_path, rel_path, "delete");

        if let Err(e) = fs::rename(&note_dir_path, &transaction_path) {
            #[cfg(debug_assertions)]
            eprintln!(
                "Error staging directory {} for deletion: {}",
                note_dir_path.display(),
                e
            );
            return Err(format!(
                "Failed to stage item for deletion on filesystem: {}",
                e
            ));
        }

        staged_delete_path = Some(transaction_path);
        #[cfg(debug_assertions)]
        eprintln!(
            "Item staged successfully for deletion on filesystem: {}",
            note_dir_path.display()
        );
    } else {
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: Item not found on filesystem for rel_path '{}'. Metadata (if it existed) was removed.",
            rel_path
        );
    }

    if let Err(metadata_error) = persist_metadata_if_changed(
        notebook_path,
        notes,
        metadata_changed,
        "deleting item",
        rel_path,
    ) {
        *notes = previous_notes;

        if let Some(staged_path) = staged_delete_path
            && let Err(rollback_error) = fs::rename(&staged_path, &note_dir_path)
        {
            return Err(format!(
                "{} Rollback failed while restoring filesystem state: {}",
                metadata_error, rollback_error
            ));
        }

        return Err(metadata_error);
    }

    if let Some(staged_path) = staged_delete_path
        && let Err(_e) = fs::remove_dir_all(&staged_path)
    {
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: Metadata commit succeeded, but failed to finalize staged deletion '{}': {}",
            staged_path.display(),
            _e
        );
    }

    remove_empty_parent_directories(full_notebook_path, &note_dir_path);

    #[cfg(debug_assertions)]
    eprintln!("Deletion process completed for: {}", rel_path);
    Ok(())
}

// Function to move/rename a note or folder
pub async fn move_note(
    notebook_path: &str,
    current_rel_path: &str,
    new_rel_path: &str,
    notes: &mut Vec<NoteMetadata>,
) -> Result<String, String> {
    #[cfg(debug_assertions)]
    eprintln!(
        "Attempting to move/rename item from '{}' to '{}'",
        current_rel_path, new_rel_path
    );

    let current_fs_path = Path::new(notebook_path).join(current_rel_path);
    let new_fs_path = Path::new(notebook_path).join(new_rel_path);
    let full_notebook_path = Path::new(notebook_path);

    // --- Validation ---

    validate_notebook_relative_path(current_rel_path, "current relative path")?;
    validate_notebook_relative_path(new_rel_path, "new relative path")?;

    // Ensure the current path exists on the filesystem
    if !current_fs_path.exists() {
        return Err(format!(
            "Item at path '{}' not found on the filesystem.",
            current_rel_path
        ));
    }

    // Ensure both current and new paths are within the notebook directory after canonicalization
    if let Ok(canonical_notebook_path) = full_notebook_path.canonicalize() {
        if let Ok(canonical_current_path) = current_fs_path.canonicalize() {
            if !canonical_current_path.starts_with(&canonical_notebook_path) {
                return Err(format!(
                    "Cannot move/rename item from path outside the notebook directory: '{}'",
                    current_rel_path
                ));
            }
        } else {
            return Err(format!(
                "Failed to canonicalize current item path: '{}'",
                current_rel_path
            ));
        }

        ensure_path_within_notebook_if_canonicalizable(
            full_notebook_path,
            &new_fs_path,
            new_rel_path,
            "Cannot move/rename item to path outside the notebook directory:",
            &format!(
                "Warning: Could not canonicalize new item path '{}'. Proceeding with move attempt, but this might indicate a path issue.",
                new_rel_path
            ),
        )?;
    } else {
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: Could not canonicalize notebook path '{}'. Skipping thorough path validation.",
            notebook_path
        );
    }

    // Check if the target path already exists on the filesystem within the notebook path
    if new_fs_path.exists() {
        // Re-check canonicalization safety if exists() is true
        if let Ok(canonical_notebook_path) = full_notebook_path.canonicalize() {
            if let Ok(canonical_new_fs_path) = new_fs_path.canonicalize()
                && canonical_new_fs_path.starts_with(&canonical_notebook_path)
            {
                return Err(format!(
                    "An item already exists at the target path '{}'.",
                    new_rel_path
                ));
            }
        } else {
            // If canonicalize fails for notebook path, just rely on exists()
            return Err(format!(
                "An item already exists at the target path '{}'.",
                new_rel_path
            ));
        }
    }

    // --- File System Operation ---

    // Create parent directories for the new path if they don't exist
    if let Some(parent) = new_fs_path.parent() {
        if !parent.exists() {
            #[cfg(debug_assertions)]
            eprintln!(
                "Creating parent directories for new path: {}",
                parent.display()
            );
            if let Err(e) = fs::create_dir_all(parent) {
                return Err(format!(
                    "Failed to create parent directories for new path: {}",
                    e
                ));
            }
        }
    } else {
        // This case means new_rel_path is just a name (e.g., "new_folder" or "new_note")
        // and new_fs_path is directly inside notebook_path. No parent directory creation needed beyond the notebook root.
        #[cfg(debug_assertions)]
        eprintln!("New path has no parent, attempting rename directly inside notebook root.");
    }

    // Perform the actual move/rename
    let previous_notes = notes.clone();

    #[cfg(debug_assertions)]
    eprintln!(
        "Attempting filesystem rename from '{}' to '{}'",
        current_fs_path.display(),
        new_fs_path.display()
    );
    if let Err(e) = fs::rename(&current_fs_path, &new_fs_path) {
        return Err(format!(
            "Failed to move/rename item from '{}' to '{}': {}",
            current_rel_path, new_rel_path, e
        ));
    }
    #[cfg(debug_assertions)]
    eprintln!("Filesystem move/rename successful.");

    // --- Metadata Update ---

    // Check if the current path corresponds to a note directory (contains note.md)
    let is_moving_note_dir = Path::new(notebook_path)
        .join(current_rel_path)
        .join("note.md")
        .exists();

    let updated_metadata =
        update_metadata_paths_for_move(notes, current_rel_path, new_rel_path, is_moving_note_dir);

    if is_moving_note_dir {
        if updated_metadata {
            #[cfg(debug_assertions)]
            eprintln!("Updated metadata for the moved note.");
        } else {
            #[cfg(debug_assertions)]
            eprintln!(
                "Warning: Moved note directory '{}' not found in metadata. Metadata was not updated for this item.",
                current_rel_path
            );
        }
    } else if updated_metadata {
        #[cfg(debug_assertions)]
        eprintln!("Updated metadata for notes within the moved/renamed folder.");
    }

    if let Err(metadata_error) = persist_metadata_if_changed(
        notebook_path,
        notes,
        updated_metadata,
        "moving/renaming",
        current_rel_path,
    ) {
        *notes = previous_notes;
        if let Err(rollback_error) = fs::rename(&new_fs_path, &current_fs_path) {
            return Err(format!(
                "{} Rollback failed while restoring filesystem state: {}",
                metadata_error, rollback_error
            ));
        }
        return Err(metadata_error);
    }

    #[cfg(debug_assertions)]
    eprintln!("Move/Rename process completed. New path: {}", new_rel_path);
    // Return the new relative path of the item that was moved/renamed
    Ok(new_rel_path.to_string())
}
