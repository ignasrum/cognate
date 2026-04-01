use std::error::Error;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::search::{cache_upsert_search_index_note_content, note_file_modified_time};
use super::{
    NoteMetadata, NotebookMetadata, STAGED_DELETE_CLEANUP_GRACE_NANOS, STAGED_DELETE_PREFIX,
};

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
            #[cfg(debug_assertions)]
            eprintln!(
                "load_notes_metadata: Error reading metadata file {}: {}",
                file_path.display(),
                _err
            );
            if _err.kind() == ErrorKind::NotFound {
                #[cfg(debug_assertions)]
                eprintln!("Metadata file not found, assuming new notebook.");
                return Vec::new();
            }
            return Vec::new();
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

pub async fn save_note_content(
    notebook_path: String,
    rel_note_path: String,
    content: String,
) -> Result<(), String> {
    save_note_content_sync(&notebook_path, &rel_note_path, &content)
}

pub fn save_note_content_sync(
    notebook_path: &str,
    rel_note_path: &str,
    content: &str,
) -> Result<(), String> {
    let full_note_path = Path::new(&notebook_path)
        .join(rel_note_path)
        .join("note.md");
    #[cfg(debug_assertions)]
    eprintln!("Attempting to save note to: {}", full_note_path.display());

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

    if existing_content.as_deref() == Some(content) {
        cache_upsert_search_index_note_content(
            notebook_path,
            rel_note_path,
            content,
            note_file_modified_time(&full_note_path),
        );
        return Ok(());
    }

    fs::write(&full_note_path, content).map_err(|e| format!("Failed to save note: {}", e))?;
    cache_upsert_search_index_note_content(
        notebook_path,
        rel_note_path,
        content,
        note_file_modified_time(&full_note_path),
    );

    Ok(())
}
