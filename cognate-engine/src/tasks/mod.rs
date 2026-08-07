use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskItem {
    pub id: String,
    pub note_path: String,
    pub text: String,
    pub line_number: usize,
    pub is_completed: bool,
    pub due_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TaskRegister {
    pub tasks_by_note: HashMap<String, Vec<TaskItem>>,
}

impl TaskRegister {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_tasks_for_note(&mut self, note_path: &str, content: &str) {
        let mut tasks = Vec::new();

        for (zero_idx, line) in content.lines().enumerate() {
            let line_number = zero_idx + 1;
            let trimmed = line.trim_start();

            // Check list item marker (- or *)
            if !trimmed.starts_with("- ") && !trimmed.starts_with("* ") {
                continue;
            }

            let list_content = &trimmed[2..];
            let is_task = list_content.starts_with("[ ]")
                || list_content.starts_with("[x]")
                || list_content.starts_with("[X]");

            if !is_task {
                continue;
            }

            let is_completed = list_content.starts_with("[x]") || list_content.starts_with("[X]");
            let task_text = list_content[3..].trim();

            if task_text.is_empty() {
                continue;
            }

            // Extract due date if any
            let due_date = extract_due_date(task_text);

            // Clean task text (remove due date indicator from display text if present)
            let cleaned_text = clean_task_text(task_text);

            let unique_id = format!("{}:{}:{}", note_path, line_number, cleaned_text);

            tasks.push(TaskItem {
                id: unique_id,
                note_path: note_path.to_string(),
                text: cleaned_text,
                line_number,
                is_completed,
                due_date,
            });
        }

        if tasks.is_empty() {
            self.tasks_by_note.remove(note_path);
        } else {
            self.tasks_by_note.insert(note_path.to_string(), tasks);
        }
    }

    pub fn remove_note(&mut self, note_path: &str) {
        self.tasks_by_note.remove(note_path);
    }

    pub fn rename_notes(&mut self, from_rel: &str, to_rel: &str) {
        let from_prefix = format!("{from_rel}/");
        let to_prefix = format!("{to_rel}/");
        let paths = self.tasks_by_note.keys().cloned().collect::<Vec<_>>();

        for path in paths {
            let Some(new_path) = (if path == from_rel {
                Some(to_rel.to_string())
            } else if path.starts_with(&from_prefix) {
                Some(format!("{to_prefix}{}", &path[from_prefix.len()..]))
            } else {
                None
            }) else {
                continue;
            };

            if let Some(mut tasks) = self.tasks_by_note.remove(&path) {
                for task in &mut tasks {
                    task.note_path = new_path.clone();
                    task.id = task
                        .id
                        .strip_prefix(&format!("{path}:"))
                        .map(|suffix| format!("{new_path}:{suffix}"))
                        .unwrap_or_else(|| task.id.clone());
                }
                self.tasks_by_note.insert(new_path, tasks);
            }
        }
    }

    pub fn get_pending_tasks(&self) -> Vec<&TaskItem> {
        let mut pending = Vec::new();
        for tasks in self.tasks_by_note.values() {
            for task in tasks {
                if !task.is_completed {
                    pending.push(task);
                }
            }
        }
        pending
    }
}

fn extract_due_date(text: &str) -> Option<String> {
    // 1. Search for due: YYYY-MM-DD
    if let Some(idx) = text.find("due:") {
        let after_due = &text[idx + 4..].trim_start();
        if after_due.len() >= 10 {
            let potential_date = &after_due[..10];
            if is_valid_date_format(potential_date) {
                return Some(potential_date.to_string());
            }
        }
    }

    // 2. Search for @due(YYYY-MM-DD)
    if let Some(idx) = text.find("@due(") {
        let after_open = &text[idx + 5..];
        if let Some(close_idx) = after_open.find(')') {
            let potential_date = &after_open[..close_idx];
            if is_valid_date_format(potential_date) {
                return Some(potential_date.to_string());
            }
        }
    }

    None
}

fn clean_task_text(text: &str) -> String {
    let mut cleaned = text.to_string();

    // Remove due: YYYY-MM-DD
    if let Some(idx) = cleaned.find("due:") {
        let after_due = &cleaned[idx + 4..];
        let has_space = after_due.starts_with(' ');
        let date_part = if has_space {
            after_due.get(1..11).unwrap_or("")
        } else {
            after_due.get(0..10).unwrap_or("")
        };

        if is_valid_date_format(date_part) {
            let replace_len = 4 + if has_space { 11 } else { 10 };
            cleaned.replace_range(idx..idx + replace_len, "");
        }
    }

    // Remove @due(YYYY-MM-DD)
    if let Some(idx) = cleaned.find("@due(") {
        let rest = &cleaned[idx..];
        if let Some(close_idx) = rest.find(')') {
            let potential_date = &rest[5..close_idx];
            if is_valid_date_format(potential_date) {
                cleaned.replace_range(idx..idx + close_idx + 1, "");
            }
        }
    }

    cleaned.trim().to_string()
}

fn is_valid_date_format(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 10 {
        return false;
    }
    // Check YYYY-MM-DD pattern
    bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[5].is_ascii_digit()
        && bytes[6].is_ascii_digit()
        && bytes[8].is_ascii_digit()
        && bytes[9].is_ascii_digit()
}
