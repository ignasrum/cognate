use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::fs_utils::validate_relative_path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
pub struct MetadataLoadResult {
    pub notes: Vec<NoteMetadata>,
    pub warning: Option<String>,
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

fn parse_rfc3339_timestamp(timestamp: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(timestamp, &Rfc3339).ok()
}

pub(super) fn reconcile_last_updated_timestamp(
    existing_timestamp: Option<&str>,
    note_file_modified_time: Option<SystemTime>,
) -> Option<String> {
    let file_timestamp = note_file_modified_time.and_then(|modified_time| {
        format_system_time_rfc3339(modified_time).and_then(|formatted| {
            parse_rfc3339_timestamp(&formatted).map(|parsed| (parsed, formatted))
        })
    });

    match existing_timestamp {
        Some(existing_timestamp) => {
            let normalized = normalize_rfc3339_to_seconds(existing_timestamp);
            let existing_parsed = parse_rfc3339_timestamp(&normalized);

            if let Some((file_parsed, file_formatted)) = file_timestamp
                && existing_parsed.is_none_or(|existing| file_parsed > existing)
            {
                return Some(file_formatted);
            }

            Some(normalized)
        }
        None => file_timestamp.map(|(_, formatted)| formatted),
    }
}

pub(super) fn sanitize_loaded_notes(notes: Vec<NoteMetadata>) -> (Vec<NoteMetadata>, Vec<String>) {
    let mut sanitized = Vec::with_capacity(notes.len());
    let mut warnings = Vec::new();

    for note in notes {
        match validate_relative_path("metadata note path", &note.rel_path) {
            Ok(_) => sanitized.push(note),
            Err(error) => warnings.push(format!(
                "Skipped invalid metadata entry '{}': {}",
                note.rel_path, error
            )),
        }
    }

    (sanitized, warnings)
}
