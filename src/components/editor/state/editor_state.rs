use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiMode {
    Editor,
    Visualizer,
    NewNoteDialog,
    MoveNoteDialog,
    About,
}

#[derive(Debug)]
pub struct EditorState {
    // Core state
    notebook_path: String,
    app_version: String,
    
    // Note selection and metadata
    selected_note_path: Option<String>,
    selected_note_labels: Vec<String>,
    
    // Text input states
    new_label_text: String,
    
    // UI mode and dialog-specific state
    ui_mode: UiMode,
    new_note_path_input: String,
    move_note_current_path: Option<String>,
    move_note_new_path_input: String,
    
    // Flag indicating if we're loading a new note
    loading_note: bool,
}

impl EditorState {
    pub fn new() -> Self {
        Self {
            notebook_path: String::new(),
            app_version: String::new(),
            selected_note_path: None,
            selected_note_labels: Vec::new(),
            new_label_text: String::new(),
            ui_mode: UiMode::Editor,
            new_note_path_input: String::new(),
            move_note_current_path: None,
            move_note_new_path_input: String::new(),
            loading_note: false,
        }
    }
    
    // Accessor methods
    pub fn notebook_path(&self) -> &str {
        &self.notebook_path
    }
    
    pub fn app_version(&self) -> &str {
        &self.app_version
    }
    
    pub fn selected_note_path(&self) -> Option<&String> {
        self.selected_note_path.as_ref()
    }
    
    pub fn selected_note_labels(&self) -> &[String] {
        &self.selected_note_labels
    }
    
    pub fn new_label_text(&self) -> &str {
        &self.new_label_text
    }
    
    pub fn show_visualizer(&self) -> bool {
        self.ui_mode == UiMode::Visualizer
    }
    
    pub fn show_new_note_input(&self) -> bool {
        self.ui_mode == UiMode::NewNoteDialog
    }
    
    pub fn new_note_path_input(&self) -> &str {
        &self.new_note_path_input
    }
    
    pub fn show_move_note_input(&self) -> bool {
        self.ui_mode == UiMode::MoveNoteDialog
    }
    
    pub fn move_note_current_path(&self) -> Option<&String> {
        self.move_note_current_path.as_ref()
    }
    
    pub fn move_note_new_path_input(&self) -> &str {
        &self.move_note_new_path_input
    }
    
    pub fn show_about_info(&self) -> bool {
        self.ui_mode == UiMode::About
    }
    
    pub fn is_loading_note(&self) -> bool {
        self.loading_note
    }
    
    // Dialog state management
    pub fn is_any_dialog_open(&self) -> bool {
        matches!(
            self.ui_mode,
            UiMode::NewNoteDialog | UiMode::MoveNoteDialog | UiMode::About
        )
    }
    
    // Mutator methods
    pub fn set_notebook_path(&mut self, path: String) {
        self.notebook_path = path;
    }
    
    pub fn set_app_version(&mut self, version: String) {
        self.app_version = version;
    }
    
    pub fn set_selected_note_path(&mut self, path: Option<String>) {
        self.selected_note_path = path;
    }
    
    pub fn set_selected_note_labels(&mut self, labels: Vec<String>) {
        self.selected_note_labels = labels;
    }
    
    pub fn set_new_label_text(&mut self, text: String) {
        self.new_label_text = text;
    }
    
    pub fn clear_new_label_text(&mut self) {
        self.new_label_text = String::new();
    }
    
    pub fn set_loading_note(&mut self, loading: bool) {
        self.loading_note = loading;
    }
    
    // Dialog management
    pub fn toggle_visualizer(&mut self) {
        self.ui_mode = if self.ui_mode == UiMode::Visualizer {
            UiMode::Editor
        } else {
            UiMode::Visualizer
        };
    }
    
    pub fn toggle_about_info(&mut self) {
        self.ui_mode = if self.ui_mode == UiMode::About {
            UiMode::Editor
        } else {
            UiMode::About
        };
    }
    
    pub fn show_new_note_dialog(&mut self) {
        if !self.notebook_path.is_empty() {
            self.ui_mode = UiMode::NewNoteDialog;
            self.new_note_path_input = String::new();
        }
    }
    
    pub fn hide_new_note_dialog(&mut self) {
        if self.ui_mode == UiMode::NewNoteDialog {
            self.ui_mode = UiMode::Editor;
        }
        self.new_note_path_input = String::new();
    }
    
    pub fn update_new_note_path(&mut self, path: String) {
        if self.show_new_note_input() {
            self.new_note_path_input = path;
        }
    }
    
    pub fn show_move_note_dialog(&mut self, current_path: String) {
        self.ui_mode = UiMode::MoveNoteDialog;
        self.move_note_current_path = Some(current_path.clone());
        self.move_note_new_path_input = current_path;
    }
    
    pub fn show_rename_folder_dialog(&mut self, folder_path: String) {
        if !self.notebook_path.is_empty() {
            self.ui_mode = UiMode::MoveNoteDialog;
            self.move_note_current_path = Some(folder_path.clone());
            self.move_note_new_path_input = folder_path;
        }
    }
    
    pub fn hide_move_note_dialog(&mut self) {
        if self.ui_mode == UiMode::MoveNoteDialog {
            self.ui_mode = UiMode::Editor;
        }
        self.move_note_current_path = None;
        self.move_note_new_path_input = String::new();
    }
    
    pub fn update_move_note_path(&mut self, path: String) {
        if self.show_move_note_input() {
            self.move_note_new_path_input = path;
        }
    }
    
    // Note-related utilities
    pub fn is_folder_path(&self, path: &str, all_notes: &[crate::notebook::NoteMetadata]) -> bool {
        let mut all_folders: HashSet<String> = HashSet::new();
        for note in all_notes {
            if let Some(parent) = Path::new(&note.rel_path).parent() {
                let folder_path = parent.to_string_lossy().into_owned();
                if !folder_path.is_empty() && folder_path != "." {
                    all_folders.insert(folder_path);
                }
            }
        }
        
        all_folders.contains(path)
    }
    
    // New mutator methods for private fields
    pub fn set_show_about_info(&mut self, show: bool) {
        if show {
            self.ui_mode = UiMode::About;
        } else if self.ui_mode == UiMode::About {
            self.ui_mode = UiMode::Editor;
        }
    }
    
    pub fn set_show_new_note_input(&mut self, show: bool) {
        if show {
            self.ui_mode = UiMode::NewNoteDialog;
        } else if self.ui_mode == UiMode::NewNoteDialog {
            self.ui_mode = UiMode::Editor;
        }
    }
    
    pub fn set_show_visualizer(&mut self, show: bool) {
        if show {
            self.ui_mode = UiMode::Visualizer;
        } else if self.ui_mode == UiMode::Visualizer {
            self.ui_mode = UiMode::Editor;
        }
    }
}

impl Default for EditorState {
    fn default() -> Self {
        Self::new()
    }
}
