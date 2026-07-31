use super::NoteMetadata;

/// Metadata sent with a search request. The API owns the authoritative search
/// index; the UI only supplies the current note metadata snapshot.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SearchNote {
    pub rel_path: String,
    pub labels: Vec<String>,
    pub last_updated: Option<String>,
}

impl From<&NoteMetadata> for SearchNote {
    fn from(note: &NoteMetadata) -> Self {
        Self {
            rel_path: note.rel_path.clone(),
            labels: note.labels.clone(),
            last_updated: note.last_updated.clone(),
        }
    }
}
