use std::collections::HashSet;
use std::path::Path;

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
    
    // Dialog states
    show_visualizer: bool,
    show_new_note_input: bool,
    new_note_path_input: String,
    show_move_note_input: bool,
    move_note_current_path: Option<String>,
    move_note_new_path_input: String,
    show_about_info: bool,
    
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
            show_visualizer: false,
            show_new_note_input: false,
            new_note_path_input: String::new(),
            show_move_note_input: false,
            move_note_current_path: None,
            move_note_new_path_input: String::new(),
            show_about_info: false,
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
        self.show_visualizer
    }
    
    pub fn show_new_note_input(&self) -> bool {
        self.show_new_note_input
    }
    
    pub fn new_note_path_input(&self) -> &str {
        &self.new_note_path_input
    }
    
    pub fn show_move_note_input(&self) -> bool {
        self.show_move_note_input
    }
    
    pub fn move_note_current_path(&self) -> Option<&String> {
        self.move_note_current_path.as_ref()
    }
    
    pub fn move_note_new_path_input(&self) -> &str {
        &self.move_note_new_path_input
    }
    
    pub fn show_about_info(&self) -> bool {
        self.show_about_info
    }
    
    pub fn is_loading_note(&self) -> bool {
        self.loading_note
    }
    
    // Dialog state management
    pub fn is_any_dialog_open(&self) -> bool {
        self.show_new_note_input || self.show_move_note_input || self.show_about_info
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
        self.show_visualizer = !self.show_visualizer;
        
        if self.show_visualizer {
            self.show_new_note_input = false;
            self.show_move_note_input = false;
            self.show_about_info = false;
        }
    }
    
    pub fn toggle_about_info(&mut self) {
        self.show_about_info = !self.show_about_info;
        
        if self.show_about_info {
            self.show_visualizer = false;
            self.show_new_note_input = false;
            self.show_move_note_input = false;
        }
    }
    
    pub fn show_new_note_dialog(&mut self) {
        if !self.notebook_path.is_empty() {
            self.show_new_note_input = true;
            self.new_note_path_input = String::new();
            self.show_visualizer = false;
            self.show_move_note_input = false;
            self.show_about_info = false;
        }
    }
    
    pub fn hide_new_note_dialog(&mut self) {
        self.show_new_note_input = false;
        self.new_note_path_input = String::new();
    }
    
    pub fn update_new_note_path(&mut self, path: String) {
        if self.show_new_note_input {
            self.new_note_path_input = path;
        }
    }
    
    pub fn show_move_note_dialog(&mut self, current_path: String) {
        self.show_new_note_input = false;
        self.show_visualizer = false;
        self.show_about_info = false;
        self.show_move_note_input = true;
        self.move_note_current_path = Some(current_path.clone());
        self.move_note_new_path_input = current_path;
    }
    
    pub fn show_rename_folder_dialog(&mut self, folder_path: String) {
        if !self.notebook_path.is_empty() {
            self.show_new_note_input = false;
            self.show_visualizer = false;
            self.show_about_info = false;
            self.show_move_note_input = true;
            self.move_note_current_path = Some(folder_path.clone());
            self.move_note_new_path_input = folder_path;
            self.selected_note_path = None;
        }
    }
    
    pub fn hide_move_note_dialog(&mut self) {
        self.show_move_note_input = false;
        self.move_note_current_path = None;
        self.move_note_new_path_input = String::new();
    }
    
    pub fn update_move_note_path(&mut self, path: String) {
        if self.show_move_note_input {
            self.move_note_new_path_input = path;
        }
    }
    
    pub fn take_move_note_current_path(&mut self) -> Option<String> {
        self.move_note_current_path.take()
    }
    
    pub fn take_selected_note_path(&mut self) -> Option<String> {
        self.selected_note_path.take()
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
        self.show_about_info = show;
    }
    
    pub fn set_show_new_note_input(&mut self, show: bool) {
        self.show_new_note_input = show;
    }
    
    pub fn set_show_visualizer(&mut self, show: bool) {
        self.show_visualizer = show;
    }
}

impl Default for EditorState {
    fn default() -> Self {
        Self::new()
    }
}
