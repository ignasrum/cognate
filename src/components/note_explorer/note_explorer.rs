use iced::widget::{Button, Column, Row, Scrollable, Text};
use iced::{Command, Element, Length}; // Import Length
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::notebook::{self, NoteMetadata};

#[derive(Debug, Clone)]
pub enum Message {
    NoteSelected(String),
    LoadNotes,
    NotesLoaded(Vec<NoteMetadata>),
    ToggleFolder(String), // Message for toggling folder visibility
    InitiateFolderRename(String),
}

// Define the tree node structure (owned data)
#[derive(Debug, Clone)]
enum NodeOwned {
    Folder {
        name: String,
        children: Vec<NodeOwned>,
        is_expanded: bool,
        path: String, // Store the full relative path of the folder
    },
    NoteDir {
        name: String, // The name of the note directory
        #[allow(dead_code)] // Allow dead code because this field is used by the Editor component
        metadata: NoteMetadata, // Owned copy
        path: String, // Store the full relative path of the note directory
    },
    // Add a placeholder variant for temporary use during tree construction
    Placeholder,
}

#[derive(Debug, Default)]
pub struct NoteExplorer {
    pub notes: Vec<NoteMetadata>,
    pub notebook_path: String,
    expanded_folders: HashMap<String, bool>, // Keep track of expanded folders
}

impl NoteExplorer {
    pub fn new(notebook_path: String) -> Self {
        Self {
            notes: Vec::new(),
            notebook_path,
            expanded_folders: HashMap::new(),
        }
    }

    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::LoadNotes => {
                eprintln!(
                    "NoteExplorer: Received LoadNotes message. Loading from path: {}",
                    self.notebook_path
                );
                let notebook_path = self.notebook_path.clone();
                Command::perform(
                    notebook::load_notes_metadata(notebook_path),
                    Message::NotesLoaded,
                )
            }
            Message::NotesLoaded(notes) => {
                eprintln!(
                    "NoteExplorer: Received NotesLoaded message with {} notes.",
                    notes.len()
                );
                self.notes = notes;
                self.notes.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

                // Identify all unique folder paths from the loaded notes
                let mut all_folders: HashSet<String> = HashSet::new();
                for note in &self.notes {
                    let path = Path::new(&note.rel_path);
                    let mut current_folder_path = String::new();
                    // Iterate through ancestors (excluding the last component which is the note directory)
                    if let Some(parent) = path.parent() {
                        for component in parent.iter() {
                            let component_name = component.to_string_lossy().into_owned();
                            if !current_folder_path.is_empty() {
                                current_folder_path.push('/');
                            }
                            current_folder_path.push_str(&component_name);
                            // Add the folder path to the set, unless it's the root "."
                            if !current_folder_path.is_empty() && current_folder_path != "." {
                                all_folders.insert(current_folder_path.clone());
                            }
                        }
                    } else {
                        // Notes directly in the root have no parent() that is not "."
                        // We handle root notes directly in build_owned_tree, no folder state needed for root.
                    }
                }

                // Add new folders to expanded_folders with default state (collapsed)
                // Keep existing expanded states for folders that are still present
                let mut new_expanded_folders = HashMap::new();
                for folder_path in all_folders {
                    let is_expanded = *self.expanded_folders.get(&folder_path).unwrap_or(&false);
                    new_expanded_folders.insert(folder_path, is_expanded);
                }
                // Ensure the root folder state is preserved/initialized
                let is_root_expanded = *self.expanded_folders.get("").unwrap_or(&false);
                new_expanded_folders.insert("".to_string(), is_root_expanded);

                self.expanded_folders = new_expanded_folders;

                Command::none()
            }
            Message::NoteSelected(_path) => Command::none(),
            Message::ToggleFolder(folder_path) => {
                // Check if the folder exists in the expanded_folders map
                if let Some(is_expanded) = self.expanded_folders.get_mut(&folder_path) {
                    *is_expanded = !*is_expanded;
                    eprintln!(
                        "Toggled folder '{}' to expanded: {}",
                        folder_path, *is_expanded
                    );
                } else {
                    eprintln!(
                        "Attempted to toggle non-existent folder path: {}",
                        folder_path
                    );
                }
                Command::none()
            }
            Message::InitiateFolderRename(_folder_path) => {
                // This message is handled by the Editor to manage UI state.
                // We just need to pass it up.
                Command::none()
            }
        }
    }

    // Helper function to build the hierarchical tree structure for display
    fn build_owned_tree(
        notes: &[NoteMetadata],
        expanded_folders: &HashMap<String, bool>,
    ) -> Vec<NodeOwned> {
        let mut root_children: Vec<NodeOwned> = Vec::new();

        // Sort notes by path to ensure parents are processed before children
        let mut sorted_notes = notes.to_vec(); // Clone to sort
        sorted_notes.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

        for note in sorted_notes {
            let path = Path::new(&note.rel_path);
            // Collect components into an owned vector to avoid borrowing issues
            let components: Vec<String> = path
                .iter()
                .map(|comp| comp.to_string_lossy().into_owned())
                .collect();

            let mut current_nodes_list = &mut root_children;
            let mut current_folder_path_components: Vec<String> = Vec::new();

            // Traverse components, creating folders if they don't exist
            for (i, component) in components.iter().enumerate() {
                let component_name = component.clone();
                if component_name.is_empty() {
                    continue;
                }

                let is_last_component = i == components.len() - 1;

                if is_last_component {
                    // Add the note directory node to the current folder's children
                    let note_dir_node = NodeOwned::NoteDir {
                        name: component_name.clone(),
                        metadata: note.clone(), // Clone metadata
                        path: note.rel_path.clone(),
                    };
                    current_nodes_list.push(note_dir_node);
                } else {
                    // This component is a folder name
                    // Check if the folder already exists at this level
                    let existing_folder_index = current_nodes_list.iter().position(|node| {
                        if let NodeOwned::Folder { name, .. } = node {
                            name == &component_name
                        } else {
                            false
                        }
                    });

                    let next_folder_children = match existing_folder_index {
                        Some(index) => {
                            // Folder exists, get mutable reference to its children
                            if let NodeOwned::Folder { children, .. } =
                                &mut current_nodes_list[index]
                            {
                                children
                            } else {
                                unreachable!("Should be a folder node at this index")
                            }
                        }
                        None => {
                            // Folder doesn't exist, create it and add it
                            current_nodes_list.push(NodeOwned::Placeholder);
                            let new_folder_index = current_nodes_list.len() - 1;

                            // Get mutable reference to the placeholder node
                            let placeholder_node = &mut current_nodes_list[new_folder_index];

                            // Populate the placeholder with the actual Folder data
                            current_folder_path_components.push(component_name.clone());
                            let folder_path_str = current_folder_path_components.join("/");
                            let is_expanded =
                                *expanded_folders.get(&folder_path_str).unwrap_or(&false);

                            *placeholder_node = NodeOwned::Folder {
                                name: component_name.clone(),
                                children: Vec::new(),
                                is_expanded,
                                path: folder_path_str.clone(),
                            };

                            // Now get a mutable reference to the children of the newly populated folder
                            if let NodeOwned::Folder { children, .. } = placeholder_node {
                                children
                            } else {
                                unreachable!("Placeholder should now be a Folder node")
                            }
                        }
                    };
                    // Update current_nodes_list for the next iteration
                    current_nodes_list = next_folder_children;
                    if existing_folder_index.is_some() {
                        current_folder_path_components.push(component_name.clone());
                    }
                }
            }
        }

        // Sort the children within each folder node (folders before notes, then alphabetically)
        fn sort_owned_nodes(nodes: &mut Vec<NodeOwned>) {
            nodes.sort_by(|a, b| {
                match (a, b) {
                    (
                        NodeOwned::Folder { name: name_a, .. },
                        NodeOwned::Folder { name: name_b, .. },
                    ) => name_a.cmp(name_b),
                    (
                        NodeOwned::NoteDir { name: name_a, .. },
                        NodeOwned::NoteDir { name: name_b, .. },
                    ) => name_a.cmp(name_b),
                    (NodeOwned::Folder { .. }, NodeOwned::NoteDir { .. }) => {
                        std::cmp::Ordering::Less
                    } // Folders before notes
                    (NodeOwned::NoteDir { .. }, NodeOwned::Folder { .. }) => {
                        std::cmp::Ordering::Greater
                    } // Notes after folders
                    _ => std::cmp::Ordering::Equal, // Should not happen with valid nodes
                }
            });
            for node in nodes.iter_mut() {
                if let NodeOwned::Folder { children, .. } = node {
                    sort_owned_nodes(children);
                }
            }
        }

        sort_owned_nodes(&mut root_children);

        root_children
    }

    // Recursive rendering helper for owned nodes
    fn render_owned_nodes(
        &self,
        nodes: &[NodeOwned],
        selected_note_path: Option<&String>,
        indent_level: usize,
    ) -> Column<'_, Message> {
        let mut column = Column::new().spacing(3);
        let indent_space = "  ".repeat(indent_level); // Simple indentation for visual hierarchy

        for node in nodes {
            match node {
                NodeOwned::Folder {
                    name,
                    children,
                    is_expanded,
                    path: folder_path,
                } => {
                    let folder_indicator = if *is_expanded { 'v' } else { '>' };
                    // Added a space after the folder_indicator
                    let folder_button_text =
                        format!("{} {} {}", indent_space, folder_indicator, name);

                    // Create the button for the folder name and toggle
                    let folder_name_button = Button::new(Text::new(folder_button_text).size(16))
                        .on_press(Message::ToggleFolder(folder_path.clone()))
                        .style(iced::theme::Button::Text)
                        .width(Length::Fill); // Make the folder name button take up available space

                    let mut folder_row = Row::new().push(folder_name_button);

                    // Only show rename button if not the root folder (empty path)
                    if !folder_path.is_empty() {
                        folder_row = folder_row.push(
                            Button::new(Text::new("Rename").size(14))
                                .on_press(Message::InitiateFolderRename(folder_path.clone()))
                                .style(iced::theme::Button::Secondary)
                                .padding(3)
                                .width(Length::Shrink), // Ensure rename button takes minimal space
                        );
                    }

                    folder_row = folder_row.spacing(5).align_items(iced::Alignment::Center);

                    column = column.push(folder_row);

                    if *is_expanded {
                        column = column.push(self.render_owned_nodes(
                            children,
                            selected_note_path,
                            indent_level + 1,
                        ));
                    }
                }
                NodeOwned::NoteDir {
                    name,
                    path: note_path,
                    ..
                } => {
                    // No need for metadata here in rendering
                    let is_selected = Some(note_path) == selected_note_path;
                    let button_style = if is_selected {
                        iced::theme::Button::Primary
                    } else {
                        iced::theme::Button::Text
                    };

                    let note_button_text = format!("{}- {}", indent_space, name);

                    column = column.push(
                        Button::new(Text::new(note_button_text).size(16))
                            .on_press(Message::NoteSelected(note_path.clone()))
                            .style(button_style),
                    );
                }
                NodeOwned::Placeholder => {
                    // This should not be rendered
                    eprintln!("Warning: Encountered a Placeholder node during rendering.");
                }
            }
        }
        column
    }

    pub fn view(&self, selected_note_path: Option<&String>) -> Element<'_, Message> {
        let mut column = Column::new().spacing(5);

        if self.notebook_path.is_empty() || self.notes.is_empty() {
            column = column.push(Text::new("No notes found."));
        } else {
            // Build the owned tree structure for ALL notes.
            // build_owned_tree handles the hierarchy internally.
            let root_tree = NoteExplorer::build_owned_tree(&self.notes, &self.expanded_folders);

            // Render the tree starting from indent level 0
            let tree_view = self.render_owned_nodes(&root_tree, selected_note_path, 0);
            column = column.push(tree_view);
        }

        Scrollable::new(column).into()
    }
}
