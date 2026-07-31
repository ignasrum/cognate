use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub(super) struct CachedNote {
    pub(super) content: Arc<str>,
    pub(super) content_lower: Arc<str>,
    pub(super) modified_time: Option<SystemTime>,
}

#[derive(Debug, Default)]
pub(super) struct NoteCache {
    pub(super) notes: HashMap<String, CachedNote>,
}

impl NoteCache {
    pub(super) fn upsert(
        &mut self,
        rel_path: &str,
        content: &str,
        modified_time: Option<SystemTime>,
    ) {
        self.notes.insert(
            rel_path.to_string(),
            CachedNote {
                content: Arc::from(content.to_string()),
                content_lower: Arc::from(content.to_lowercase()),
                modified_time,
            },
        );
    }

    pub(super) fn remove(&mut self, rel_path: &str) {
        let prefix = format!("{rel_path}/");
        self.notes
            .retain(|path, _| path != rel_path && !path.starts_with(&prefix));
    }

    pub(super) fn rename(&mut self, from_rel: &str, to_rel: &str) {
        let from_prefix = format!("{from_rel}/");
        let to_prefix = format!("{to_rel}/");
        let existing_paths: Vec<String> = self.notes.keys().cloned().collect();
        let mut remapped = Vec::new();

        for path in existing_paths {
            if path == from_rel {
                remapped.push((path, to_rel.to_string()));
            } else if path.starts_with(&from_prefix) {
                let suffix = path[from_prefix.len()..].to_string();
                remapped.push((path, format!("{to_prefix}{suffix}")));
            }
        }

        for (old_path, new_path) in remapped {
            if let Some(entry) = self.notes.remove(&old_path) {
                self.notes.insert(new_path, entry);
            }
        }
    }

    pub(super) fn clear(&mut self) {
        self.notes.clear();
    }
}
