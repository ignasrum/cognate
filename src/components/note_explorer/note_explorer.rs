use iced::widget::{Button, Column, Container, Row, Scrollable, Text};
use iced::{task::Task, Element, Length};
use std::collections::{HashMap, HashSet};
use std::path::Path;

// Import the correct styling types - button directly
use iced::widget::button;

use crate::notebook::{self, NoteMetadata};

#[derive(Debug, Clone)]
pub enum Message {
    NoteSelected(String),
    LoadNotes,
    NotesLoaded(Vec<NoteMetadata>),
    ToggleFolder(String),
    InitiateFolderRename(String),
    // Removed: ExpandToNote(String),
    CollapseAllAndExpandToNote(String),
}

#[derive(Debug, Clone)]
enum NodeOwned {
    Folder {
        name: String,
        children: Vec<NodeOwned>,
        is_expanded: bool,
        path: String,
    },
    NoteDir {
        name: String,
        #[allow(dead_code)]
        metadata: NoteMetadata,
        path: String,
    },
    Placeholder,
}

#[derive(Debug, Default)]
pub struct NoteExplorer {
    pub notes: Vec<NoteMetadata>,
    pub notebook_path: String,
    pub expanded_folders: HashMap<String, bool>,
}

impl NoteExplorer {
    pub fn new(notebook_path: String) -> Self {
        Self {
            notes: Vec::new(),
            notebook_path,
            expanded_folders: HashMap::new(),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::LoadNotes => {
                #[cfg(debug_assertions)]
                eprintln!(
                    "NoteExplorer: Received LoadNotes message. Loading from path: {}",
                    self.notebook_path
                );
                let notebook_path = self.notebook_path.clone();
                Task::perform(
                    notebook::load_notes_metadata(notebook_path),
                    Message::NotesLoaded,
                )
            }
            Message::NotesLoaded(notes) => {
                #[cfg(debug_assertions)]
                eprintln!(
                    "NoteExplorer: Received NotesLoaded message with {} notes.",
                    notes.len()
                );
                self.notes = notes;
                self.notes.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

                let mut all_folders: HashSet<String> = HashSet::new();
                for note in &self.notes {
                    let path = Path::new(&note.rel_path);
                    let mut current_folder_path = String::new();
                    if let Some(parent) = path.parent() {
                        for component in parent.iter() {
                            let component_name = component.to_string_lossy().into_owned();
                            if !current_folder_path.is_empty() {
                                current_folder_path.push('/');
                            }
                            current_folder_path.push_str(&component_name);
                            if !current_folder_path.is_empty() && current_folder_path != "." {
                                all_folders.insert(current_folder_path.clone());
                            }
                        }
                    } else {
                    }
                }

                let mut new_expanded_folders = HashMap::new();
                for folder_path in all_folders {
                    let is_expanded = *self.expanded_folders.get(&folder_path).unwrap_or(&false);
                    new_expanded_folders.insert(folder_path, is_expanded);
                }
                let is_root_expanded = *self.expanded_folders.get("").unwrap_or(&false);
                new_expanded_folders.insert("".to_string(), is_root_expanded);

                self.expanded_folders = new_expanded_folders;

                Task::none()
            }
            Message::NoteSelected(_path) => Task::none(),
            Message::ToggleFolder(folder_path) => {
                if let Some(is_expanded) = self.expanded_folders.get_mut(&folder_path) {
                    *is_expanded = !*is_expanded;
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "Toggled folder '{}' to expanded: {}",
                        folder_path, *is_expanded
                    );
                } else {
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "Attempted to toggle non-existent folder path: {}",
                        folder_path
                    );
                }
                Task::none()
            }
            Message::InitiateFolderRename(_folder_path) => Task::none(),
            Message::CollapseAllAndExpandToNote(note_path) => {
                #[cfg(debug_assertions)]
                eprintln!(
                    "NoteExplorer: Received CollapseAllAndExpandToNote message for path: {}",
                    note_path
                );
                // Collapse all folders first
                for (_, is_expanded) in self.expanded_folders.iter_mut() {
                    *is_expanded = false;
                }
                #[cfg(debug_assertions)]
                eprintln!("Collapsed all folders.");

                // Then expand to the specific note
                let mut current_path = Path::new(&note_path).parent().map(|p| p.to_path_buf());
                while let Some(path_buf) = current_path {
                    let folder_path_str = path_buf.to_string_lossy().into_owned();
                    if !folder_path_str.is_empty() && folder_path_str != "." {
                        self.expanded_folders.insert(folder_path_str.clone(), true);
                        #[cfg(debug_assertions)]
                        eprintln!("Expanded folder: {}", folder_path_str);
                        current_path = path_buf.parent().map(|p| p.to_path_buf());
                    } else {
                        break;
                    }
                }
                Task::none()
            }
        }
    }

    fn build_owned_tree(
        notes: &[NoteMetadata],
        expanded_folders: &HashMap<String, bool>,
    ) -> Vec<NodeOwned> {
        let mut root_children: Vec<NodeOwned> = Vec::new();
        let mut sorted_notes = notes.to_vec();
        sorted_notes.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

        for note in sorted_notes {
            let path = Path::new(&note.rel_path);
            let components: Vec<String> = path
                .iter()
                .map(|comp| comp.to_string_lossy().into_owned())
                .collect();

            let mut current_nodes_list = &mut root_children;
            let mut current_folder_path_components: Vec<String> = Vec::new();

            for (i, component) in components.iter().enumerate() {
                let component_name = component.clone();
                if component_name.is_empty() {
                    continue;
                }

                let is_last_component = i == components.len() - 1;

                if is_last_component {
                    let note_dir_node = NodeOwned::NoteDir {
                        name: component_name.clone(),
                        metadata: note.clone(),
                        path: note.rel_path.clone(),
                    };
                    current_nodes_list.push(note_dir_node);
                } else {
                    let existing_folder_index = current_nodes_list.iter().position(|node| {
                        if let NodeOwned::Folder { name, .. } = node {
                            name == &component_name
                        } else {
                            false
                        }
                    });

                    let next_folder_children = match existing_folder_index {
                        Some(index) => {
                            if let NodeOwned::Folder { children, .. } =
                                &mut current_nodes_list[index]
                            {
                                children
                            } else {
                                unreachable!("Should be a folder node at this index")
                            }
                        }
                        None => {
                            current_nodes_list.push(NodeOwned::Placeholder);
                            let new_folder_index = current_nodes_list.len() - 1;
                            let placeholder_node = &mut current_nodes_list[new_folder_index];

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

                            if let NodeOwned::Folder { children, .. } = placeholder_node {
                                children
                            } else {
                                unreachable!("Placeholder should now be a Folder node")
                            }
                        }
                    };
                    current_nodes_list = next_folder_children;
                    if existing_folder_index.is_some() {
                        current_folder_path_components.push(component_name.clone());
                    }
                }
            }
        }

        fn sort_owned_nodes(nodes: &mut Vec<NodeOwned>) {
            nodes.sort_by(|a, b| match (a, b) {
                (
                    NodeOwned::Folder { name: name_a, .. },
                    NodeOwned::Folder { name: name_b, .. },
                ) => name_a.cmp(name_b),
                (
                    NodeOwned::NoteDir { name: name_a, .. },
                    NodeOwned::NoteDir { name: name_b, .. },
                ) => name_a.cmp(name_b),
                (NodeOwned::Folder { .. }, NodeOwned::NoteDir { .. }) => std::cmp::Ordering::Less,
                (NodeOwned::NoteDir { .. }, NodeOwned::Folder { .. }) => {
                    std::cmp::Ordering::Greater
                }
                _ => std::cmp::Ordering::Equal,
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

    fn render_owned_nodes(
        &self,
        nodes: &[NodeOwned],
        selected_note_path: Option<&String>,
        indent_level: usize,
    ) -> Column<'_, Message> {
        let mut column = Column::new().spacing(3);
        let indent_space = "  ".repeat(indent_level);

        for node in nodes {
            match node {
                NodeOwned::Folder {
                    name,
                    children,
                    is_expanded,
                    path: folder_path,
                } => {
                    let folder_indicator = if *is_expanded { 'v' } else { '>' };
                    let indicator_text =
                        Text::new(format!("{} {}", indent_space, folder_indicator));
                    let folder_name_text = Text::new(name.clone()).size(16);

                    let folder_content_row = Row::new()
                        .push(indicator_text)
                        .push(folder_name_text)
                        .spacing(3) // Adjust spacing between indicator and name
                        .align_y(iced::Alignment::Center);

                    let folder_button = Button::new(folder_content_row)
                        .on_press(Message::ToggleFolder(folder_path.clone()))
                        .style(button::text) // Use button styling function
                        .width(Length::Fill);

                    let mut folder_row = Row::new().push(folder_button);

                    if !folder_path.is_empty() {
                        folder_row = folder_row.push(
                            Button::new(Text::new("Rename").size(14))
                                .on_press(Message::InitiateFolderRename(folder_path.clone()))
                                .style(button::secondary) // Use button styling function
                                .padding(3)
                                .width(Length::Shrink),
                        );
                    }

                    folder_row = folder_row
                        .spacing(5)
                        .align_y(iced::Alignment::Center)
                        .width(Length::Fill);

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
                    // Corrected the pattern here
                    ..
                } => {
                    let is_selected = Some(note_path) == selected_note_path;
                    let button_style = if is_selected {
                        button::primary // Use button styling function
                    } else {
                        button::text // Use button styling function
                    };

                    let note_button_text = format!("{}o {}", indent_space, name);

                    column = column.push(
                        Button::new(Text::new(note_button_text).size(16))
                            .on_press(Message::NoteSelected(note_path.clone()))
                            .style(button_style),
                    );
                }
                NodeOwned::Placeholder => {
                    #[cfg(debug_assertions)]
                    eprintln!("Warning: Encountered a Placeholder node during rendering.");
                }
            }
        }
        column
    }

    pub fn view(&self, selected_note_path: Option<&String>) -> Element<'_, Message> {
        let mut column = Column::new().spacing(5).width(Length::Fill);

        if self.notebook_path.is_empty() || self.notes.is_empty() {
            column = column.push(Text::new("No notes found."));
        } else {
            let root_tree = NoteExplorer::build_owned_tree(&self.notes, &self.expanded_folders);
            let tree_view = self.render_owned_nodes(&root_tree, selected_note_path, 0);
            column = column.push(tree_view);
        }

        // Wrap the column in a container with right padding to avoid scrollbar overlap
        Scrollable::new(
            Container::new(column)
                .padding([0.0, 15.0])
                .width(Length::Fill)
        )
        .into()
    }
}
