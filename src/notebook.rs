use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const STAGED_DELETE_PREFIX: &str = ".cognate_txn_delete_";
const STAGED_DELETE_CLEANUP_GRACE_NANOS: u128 = 5 * 60 * 1_000_000_000;
#[cfg(test)]
const SEARCH_INDEX_EXTERNAL_REFRESH_INTERVAL: Duration = Duration::from_millis(150);
#[cfg(not(test))]
const SEARCH_INDEX_EXTERNAL_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

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

#[derive(Debug, Clone, Default)]
struct IndexedNoteContent {
    content: Arc<str>,
    content_lower: Arc<str>,
    modified_time: Option<SystemTime>,
}

#[derive(Debug, Default)]
struct NotebookSearchIndex {
    notes_by_path: HashMap<String, IndexedNoteContent>,
    last_external_refresh: Option<Instant>,
}

static SEARCH_INDEXES_BY_NOTEBOOK: OnceLock<Mutex<HashMap<String, NotebookSearchIndex>>> =
    OnceLock::new();

fn search_indexes() -> &'static Mutex<HashMap<String, NotebookSearchIndex>> {
    SEARCH_INDEXES_BY_NOTEBOOK.get_or_init(|| Mutex::new(HashMap::new()))
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

fn note_file_modified_time(note_file_path: &Path) -> Option<SystemTime> {
    fs::metadata(note_file_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
}

fn read_note_content_for_index(notebook_path: &str, rel_path: &str) -> IndexedNoteContent {
    let note_file_path = Path::new(notebook_path).join(rel_path).join("note.md");
    let content = fs::read_to_string(&note_file_path).unwrap_or_default();
    let content_lower = content.to_lowercase();
    let modified_time = note_file_modified_time(&note_file_path);

    IndexedNoteContent {
        content_lower: Arc::from(content_lower),
        content: Arc::from(content),
        modified_time,
    }
}

fn cache_upsert_search_index_note_content(
    notebook_path: &str,
    rel_path: &str,
    content: &str,
    modified_time: Option<SystemTime>,
) {
    let mut search_indexes = search_indexes()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let index = search_indexes.entry(notebook_path.to_string()).or_default();

    index.notes_by_path.insert(
        rel_path.to_string(),
        IndexedNoteContent {
            content: Arc::from(content.to_string()),
            content_lower: Arc::from(content.to_lowercase()),
            modified_time,
        },
    );
}

fn cache_remove_search_index_entries(notebook_path: &str, rel_path: &str) {
    let mut search_indexes = search_indexes()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(index) = search_indexes.get_mut(notebook_path) else {
        return;
    };

    let prefix = format!("{rel_path}/");
    index
        .notes_by_path
        .retain(|path, _| path != rel_path && !path.starts_with(&prefix));
}

fn cache_rename_search_index_entries(notebook_path: &str, from_rel_path: &str, to_rel_path: &str) {
    let mut search_indexes = search_indexes()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(index) = search_indexes.get_mut(notebook_path) else {
        return;
    };

    let from_prefix = format!("{from_rel_path}/");
    let to_prefix = format!("{to_rel_path}/");
    let existing_paths: Vec<String> = index.notes_by_path.keys().cloned().collect();
    let mut remapped_entries = Vec::new();

    for existing_path in existing_paths {
        if existing_path == from_rel_path {
            remapped_entries.push((existing_path, to_rel_path.to_string()));
        } else if existing_path.starts_with(&from_prefix) {
            let suffix = existing_path[from_prefix.len()..].to_string();
            remapped_entries.push((existing_path, format!("{to_prefix}{suffix}")));
        }
    }

    for (old_path, new_path) in remapped_entries {
        if let Some(entry) = index.notes_by_path.remove(&old_path) {
            index.notes_by_path.insert(new_path, entry);
        }
    }
}

fn should_refresh_search_index_from_filesystem(last_refresh: Option<Instant>) -> bool {
    match last_refresh {
        Some(last) => last.elapsed() >= SEARCH_INDEX_EXTERNAL_REFRESH_INTERVAL,
        None => true,
    }
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

    let note_paths: HashSet<&str> = notes.iter().map(|note| note.rel_path.as_str()).collect();
    let (missing_paths, refresh_candidates, should_refresh) = {
        let mut search_indexes = search_indexes()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let index = search_indexes.entry(notebook_path.clone()).or_default();
        index
            .notes_by_path
            .retain(|rel_path, _| note_paths.contains(rel_path.as_str()));

        let should_refresh =
            should_refresh_search_index_from_filesystem(index.last_external_refresh);
        let mut missing_paths = Vec::new();
        let mut refresh_candidates = Vec::new();

        for note in &notes {
            if let Some(indexed) = index.notes_by_path.get(&note.rel_path) {
                if should_refresh {
                    refresh_candidates.push((note.rel_path.clone(), indexed.modified_time));
                }
            } else {
                missing_paths.push(note.rel_path.clone());
            }
        }

        (missing_paths, refresh_candidates, should_refresh)
    };

    let mut missing_entries = Vec::with_capacity(missing_paths.len());
    for rel_path in missing_paths {
        missing_entries.push((
            rel_path.clone(),
            read_note_content_for_index(&notebook_path, &rel_path),
        ));
    }

    let mut refreshed_entries = Vec::new();
    if should_refresh {
        for (rel_path, previous_modified_time) in refresh_candidates {
            let note_file_path = Path::new(&notebook_path).join(&rel_path).join("note.md");
            let modified_time = note_file_modified_time(&note_file_path);

            if previous_modified_time != modified_time {
                refreshed_entries.push((
                    rel_path.clone(),
                    previous_modified_time,
                    read_note_content_for_index(&notebook_path, &rel_path),
                ));
            }
        }
    }

    let content_snapshot_by_path = {
        let mut search_indexes = search_indexes()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let index = search_indexes.entry(notebook_path.clone()).or_default();
        index
            .notes_by_path
            .retain(|rel_path, _| note_paths.contains(rel_path.as_str()));

        for (rel_path, indexed_note) in missing_entries {
            index.notes_by_path.entry(rel_path).or_insert(indexed_note);
        }

        for (rel_path, expected_previous_modified_time, indexed_note) in refreshed_entries {
            let should_apply = index.notes_by_path.get(&rel_path).map_or(true, |existing| {
                existing.modified_time == expected_previous_modified_time
            });

            if should_apply {
                index.notes_by_path.insert(rel_path, indexed_note);
            }
        }

        if should_refresh {
            index.last_external_refresh = Some(Instant::now());
        }

        let mut snapshot = HashMap::with_capacity(notes.len());
        for note in &notes {
            if let Some(indexed) = index.notes_by_path.get(&note.rel_path) {
                snapshot.insert(note.rel_path.clone(), indexed.clone());
            }
        }
        snapshot
    };

    let mut results = Vec::new();

    for note in &notes {
        let rel_path_match = note.rel_path.to_lowercase().contains(&normalized_query);
        let label_match = note
            .labels
            .iter()
            .find(|label| label.to_lowercase().contains(&normalized_query))
            .cloned();

        let content_match = content_snapshot_by_path
            .get(&note.rel_path)
            .and_then(|indexed| {
                if !indexed.content_lower.contains(&normalized_query) {
                    return None;
                }
                find_matching_content_snippet(indexed.content.as_ref(), &normalized_query)
            });

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
                rel_path: note.rel_path.clone(),
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
    cache_upsert_search_index_note_content(
        notebook_path,
        rel_path,
        "",
        note_file_modified_time(&note_file_path),
    );
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
    cache_remove_search_index_entries(notebook_path, rel_path);

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

    cache_rename_search_index_entries(notebook_path, current_rel_path, new_rel_path);

    #[cfg(debug_assertions)]
    eprintln!("Move/Rename process completed. New path: {}", new_rel_path);
    // Return the new relative path of the item that was moved/renamed
    Ok(new_rel_path.to_string())
}
