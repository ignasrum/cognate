use crate::notebook::NoteMetadata;
use iced::widget::{Column, Container, Text, canvas, container};
use iced::{Color, Element, Length, Point, Theme, task::Task};
use std::cell::{Cell, RefCell};
#[cfg(test)]
use std::collections::HashSet;
use std::time::Instant;

#[path = "core/canvas_impl.rs"]
mod canvas_impl;
#[path = "core/graph_cache.rs"]
mod graph_cache;
#[path = "core/math.rs"]
mod math;

const DEFAULT_CAMERA_YAW: f32 = 0.0;
const DEFAULT_CAMERA_PITCH: f32 = -0.15;
const DEFAULT_CAMERA_ZOOM: f32 = 1.0;
const MIN_CAMERA_ZOOM: f32 = 0.55;
const MAX_CAMERA_ZOOM: f32 = 4.0;
const MAX_LABEL_LENGTH: usize = 32;
const MAX_DOUBLE_CLICK_INTERVAL_MS: u128 = 300;
const MAX_DOUBLE_CLICK_DISTANCE: f32 = 6.0;
const CAMERA_TRANSITION_DURATION_MS: f32 = 320.0;

#[derive(Debug, Clone)]
pub enum Message {
    UpdateNotes(Vec<NoteMetadata>),
    FocusOnNote(Option<String>),
    NoteSelectedInVisualizer(String),
}

#[derive(Debug, Clone)]
struct GraphNode {
    note_path: String,
    labels: Vec<String>,
    position: [f32; 3],
    degree: usize,
}

#[derive(Debug, Clone)]
struct GraphEdge {
    start: usize,
    end: usize,
    shared_labels: usize,
}

#[derive(Debug, Clone, Default)]
struct GraphCache {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    max_shared_labels_per_edge: usize,
}

#[derive(Debug, Clone, Copy)]
struct ProjectedNode {
    index: usize,
    point: Point,
    radius: f32,
    depth: f32,
}

#[derive(Debug, Clone)]
struct GraphProgram {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    max_shared_labels_per_edge: usize,
    selected_note_path: Option<String>,
    focus_yaw: f32,
    focus_pitch: f32,
    focus_zoom: f32,
    focus_version: u64,
}

#[derive(Debug, Clone, Copy)]
struct CameraTransition {
    from_yaw: f32,
    from_pitch: f32,
    from_zoom: f32,
    to_yaw: f32,
    to_pitch: f32,
    to_zoom: f32,
    started_at: Instant,
}

#[derive(Debug)]
struct GraphCanvasState {
    yaw: f32,
    pitch: f32,
    zoom: f32,
    drag_anchor: Option<Point>,
    drag_origin_yaw: f32,
    drag_origin_pitch: f32,
    hovered_node_index: Option<usize>,
    last_node_click: Option<(usize, Point, Instant)>,
    camera_transition: Option<CameraTransition>,
    center_note_path: Option<String>,
    center_transition_from_note: Option<String>,
    center_transition_to_note: Option<String>,
    center_transition_blend: f32,
    applied_focus_version: u64,
}

#[derive(Debug, Default)]
pub struct Visualizer {
    pub notes: Vec<NoteMetadata>,
    graph_cache: RefCell<GraphCache>,
    graph_cache_key: Cell<Option<u64>>,
    focus_target_note: Option<String>,
    focus_yaw: f32,
    focus_pitch: f32,
    focus_zoom: f32,
    focus_version: u64,
}

impl Visualizer {
    pub fn new() -> Self {
        Self {
            notes: Vec::new(),
            graph_cache: RefCell::new(GraphCache::default()),
            graph_cache_key: Cell::new(None),
            focus_target_note: None,
            focus_yaw: DEFAULT_CAMERA_YAW,
            focus_pitch: DEFAULT_CAMERA_PITCH,
            focus_zoom: DEFAULT_CAMERA_ZOOM,
            focus_version: 1,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::UpdateNotes(notes) => {
                self.notes = notes;
                self.refresh_graph_cache_if_needed();
                Task::none()
            }
            Message::FocusOnNote(note_path) => {
                self.apply_focus_target(note_path);
                Task::none()
            }
            Message::NoteSelectedInVisualizer(_) => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message, Theme> {
        self.refresh_graph_cache_if_needed();
        let graph_cache = self.graph_cache.borrow();

        let mut content = Column::new()
            .spacing(12)
            .padding(16)
            .width(Length::Fill)
            .height(Length::Fill);

        if self.notes.is_empty() {
            content = content.push(Text::new(
                "No notes available for visualization. Open a notebook first.",
            ));
            return Container::new(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }

        let isolated_note_count = graph_cache
            .nodes
            .iter()
            .filter(|node| node.degree == 0)
            .count();

        content = content
            .push(
                Text::new(format!(
                    "{} notes | {} label links | {} isolated notes",
                    graph_cache.nodes.len(),
                    graph_cache.edges.len(),
                    isolated_note_count
                ))
                .size(15),
            )
            .push(
                Text::new(
                    "Click a node to center it. Double-click to open it. Drag to orbit. Scroll to zoom. Edges mean shared labels.",
                )
                .size(14)
                .style(|_theme: &Theme| iced::widget::text::Style {
                    color: Some(Color::from_rgba(0.88, 0.92, 0.99, 0.88)),
                }),
            );

        let graph_program = GraphProgram {
            nodes: graph_cache.nodes.clone(),
            edges: graph_cache.edges.clone(),
            max_shared_labels_per_edge: graph_cache.max_shared_labels_per_edge,
            selected_note_path: self.focus_target_note.clone(),
            focus_yaw: self.focus_yaw,
            focus_pitch: self.focus_pitch,
            focus_zoom: self.focus_zoom,
            focus_version: self.focus_version,
        };

        let graph_canvas = canvas::Canvas::new(graph_program)
            .width(Length::Fill)
            .height(Length::Fill);

        content = content.push(
            Container::new(graph_canvas)
                .padding(4)
                .style(|_theme: &Theme| container::Style {
                    background: Some(iced::Background::Color(Color::from_rgb(0.04, 0.06, 0.10))),
                    text_color: None,
                    border: iced::Border {
                        radius: 8.0.into(),
                        width: 1.0,
                        color: Color::from_rgba(0.55, 0.72, 0.94, 0.35),
                    },
                    shadow: iced::Shadow::default(),
                    snap: false,
                })
                .width(Length::Fill)
                .height(Length::Fill),
        );

        Container::new(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    #[cfg(test)]
    pub(crate) fn debug_graph_stats(&self) -> (usize, usize, usize) {
        self.refresh_graph_cache_if_needed();
        let cache = self.graph_cache.borrow();
        let unique_label_count = cache
            .nodes
            .iter()
            .flat_map(|node| node.labels.iter().cloned())
            .collect::<HashSet<_>>()
            .len();
        (cache.nodes.len(), cache.edges.len(), unique_label_count)
    }
}
