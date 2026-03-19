use crate::notebook::NoteMetadata;
use iced::widget::{Column, Container, Text, canvas, container};
use iced::{Color, Element, Font, Length, Pixels, Point, Rectangle, Theme, mouse, task::Task};
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;
use std::hash::{Hash, Hasher};

const DEFAULT_CAMERA_YAW: f32 = 0.0;
const DEFAULT_CAMERA_PITCH: f32 = -0.15;
const DEFAULT_CAMERA_ZOOM: f32 = 1.0;
const MIN_CAMERA_ZOOM: f32 = 0.55;
const MAX_CAMERA_ZOOM: f32 = 4.0;
const MAX_LABEL_LENGTH: usize = 32;

#[derive(Debug, Clone)]
pub enum Message {
    UpdateNotes(Vec<NoteMetadata>),
    FocusOnNote(Option<String>),
    NoteSelectedInVisualizer(String),
    ToggleLabel(String),
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
    label_counts: Vec<(String, usize)>,
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

#[derive(Debug)]
struct GraphCanvasState {
    yaw: f32,
    pitch: f32,
    zoom: f32,
    drag_anchor: Option<Point>,
    drag_origin_yaw: f32,
    drag_origin_pitch: f32,
    hovered_node_index: Option<usize>,
    applied_focus_version: u64,
}

impl Default for GraphCanvasState {
    fn default() -> Self {
        Self {
            yaw: DEFAULT_CAMERA_YAW,
            pitch: DEFAULT_CAMERA_PITCH,
            zoom: DEFAULT_CAMERA_ZOOM,
            drag_anchor: None,
            drag_origin_yaw: DEFAULT_CAMERA_YAW,
            drag_origin_pitch: DEFAULT_CAMERA_PITCH,
            hovered_node_index: None,
            applied_focus_version: 0,
        }
    }
}

impl GraphProgram {
    fn project_nodes(&self, state: &GraphCanvasState, bounds: Rectangle) -> Vec<ProjectedNode> {
        if self.nodes.is_empty() || bounds.width <= 1.0 || bounds.height <= 1.0 {
            return Vec::new();
        }

        let mut projected = Vec::with_capacity(self.nodes.len());
        let center = Point::new(bounds.width * 0.5, bounds.height * 0.52);
        let orbit_scale = bounds.width.min(bounds.height) * 0.34 * state.zoom;
        let camera_distance = 2.8;
        let selected_index = self.selected_note_path.as_ref().and_then(|selected_path| {
            self.nodes
                .iter()
                .position(|node| &node.note_path == selected_path)
        });
        let selected_rotated_offset = selected_index
            .and_then(|index| self.nodes.get(index))
            .map(|node| rotate_3d(node.position, state.yaw, state.pitch))
            .unwrap_or([0.0, 0.0, 0.0]);

        let mut rotated_points: Vec<[f32; 3]> = self
            .nodes
            .iter()
            .map(|node| {
                let raw_rotated = rotate_3d(node.position, state.yaw, state.pitch);
                [
                    raw_rotated[0] - selected_rotated_offset[0],
                    raw_rotated[1] - selected_rotated_offset[1],
                    raw_rotated[2] - selected_rotated_offset[2],
                ]
            })
            .collect();

        if let Some(selected_index) = selected_index {
            let max_other_z = rotated_points
                .iter()
                .enumerate()
                .filter_map(|(index, point)| (index != selected_index).then_some(point[2]))
                .fold(f32::NEG_INFINITY, f32::max);

            let preferred_selected_z: f32 = camera_distance - 0.95;
            let target_selected_z = if max_other_z.is_finite() {
                preferred_selected_z
                    .max(max_other_z + 0.2)
                    .min(camera_distance - 0.65)
            } else {
                preferred_selected_z
            };

            if let Some(selected_point) = rotated_points.get(selected_index).copied() {
                let z_shift = target_selected_z - selected_point[2];
                for point in &mut rotated_points {
                    point[2] += z_shift;
                }
            }
        }

        for (index, node) in self.nodes.iter().enumerate() {
            let rotated = rotated_points[index];
            let safe_depth = (camera_distance - rotated[2]).max(0.6);
            let perspective = camera_distance / safe_depth;
            let point = Point::new(
                center.x + rotated[0] * orbit_scale * perspective,
                center.y + rotated[1] * orbit_scale * perspective,
            );

            let degree_size = (node.degree as f32).sqrt() * 0.35;
            let radius = (5.0 + degree_size) * perspective.clamp(0.62, 1.75);

            projected.push(ProjectedNode {
                index,
                point,
                radius,
                depth: rotated[2],
            });
        }

        projected
    }

    fn hit_test(
        &self,
        state: &GraphCanvasState,
        bounds: Rectangle,
        cursor_position: Point,
    ) -> Option<usize> {
        let projected = self.project_nodes(state, bounds);
        let mut best: Option<(usize, f32, f32)> = None;

        for node in projected {
            let dx = cursor_position.x - node.point.x;
            let dy = cursor_position.y - node.point.y;
            let distance_sq = dx * dx + dy * dy;
            let radius_sq = node.radius * node.radius;

            if distance_sq > radius_sq {
                continue;
            }

            match best {
                None => best = Some((node.index, distance_sq, node.depth)),
                Some((_, best_distance_sq, best_depth)) => {
                    if node.depth > best_depth
                        || (node.depth == best_depth && distance_sq < best_distance_sq)
                    {
                        best = Some((node.index, distance_sq, node.depth));
                    }
                }
            }
        }

        best.map(|(index, _, _)| index)
    }
}

impl canvas::Program<Message> for GraphProgram {
    type State = GraphCanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<iced::widget::Action<Message>> {
        match event {
            canvas::Event::Window(iced::window::Event::RedrawRequested(_)) => {
                if state.applied_focus_version != self.focus_version {
                    state.yaw = self.focus_yaw;
                    state.pitch = self.focus_pitch;
                    state.zoom = self.focus_zoom;
                    state.drag_anchor = None;
                    state.drag_origin_yaw = state.yaw;
                    state.drag_origin_pitch = state.pitch;
                    state.hovered_node_index = None;
                    state.applied_focus_version = self.focus_version;
                }

                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(cursor_position) = cursor.position_in(bounds) else {
                    return None;
                };

                if let Some(hit_index) = self.hit_test(state, bounds, cursor_position)
                    && let Some(node) = self.nodes.get(hit_index)
                {
                    return Some(
                        iced::widget::Action::publish(Message::NoteSelectedInVisualizer(
                            node.note_path.clone(),
                        ))
                        .and_capture(),
                    );
                }

                state.drag_anchor = Some(cursor_position);
                state.drag_origin_yaw = state.yaw;
                state.drag_origin_pitch = state.pitch;

                Some(iced::widget::Action::capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.drag_anchor.take().is_some() {
                    Some(iced::widget::Action::capture())
                } else {
                    None
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(anchor) = state.drag_anchor
                    && let Some(cursor_position) = cursor.position_in(bounds)
                {
                    let dx = cursor_position.x - anchor.x;
                    let dy = cursor_position.y - anchor.y;
                    state.yaw = wrap_angle(state.drag_origin_yaw + dx * 0.012);
                    state.pitch = wrap_angle(state.drag_origin_pitch + dy * 0.009);

                    let hovered = self.hit_test(state, bounds, cursor_position);
                    state.hovered_node_index = hovered;

                    return Some(iced::widget::Action::request_redraw().and_capture());
                }

                if let Some(cursor_position) = cursor.position_in(bounds) {
                    let hovered = self.hit_test(state, bounds, cursor_position);
                    if hovered != state.hovered_node_index {
                        state.hovered_node_index = hovered;
                        return Some(iced::widget::Action::request_redraw());
                    }
                } else if state.hovered_node_index.take().is_some() {
                    return Some(iced::widget::Action::request_redraw());
                }

                None
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if !cursor.is_over(bounds) {
                    return None;
                }

                let scroll_delta = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => *y / 60.0,
                };

                if scroll_delta.abs() <= f32::EPSILON {
                    return None;
                }

                let zoom_scale = 1.0 + scroll_delta * 0.12;
                let next_zoom = if zoom_scale > 0.0 {
                    state.zoom * zoom_scale
                } else {
                    state.zoom
                };
                state.zoom = next_zoom.clamp(MIN_CAMERA_ZOOM, MAX_CAMERA_ZOOM);
                Some(iced::widget::Action::request_redraw().and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let background = canvas::Path::rectangle(Point::ORIGIN, frame.size());
        frame.fill(&background, Color::from_rgb(0.05, 0.08, 0.12));

        for i in 0..40 {
            let px = hash_to_unit_f32("star-x", i as u64) * bounds.width;
            let py = hash_to_unit_f32("star-y", i as u64) * bounds.height;
            let twinkle = 0.16 + ((i as f32).sin() + 1.0) * 0.09;
            let radius = 0.9 + hash_to_unit_f32("star-r", i as u64) * 1.1;
            frame.fill(
                &canvas::Path::circle(Point::new(px, py), radius),
                Color::from_rgba(0.74, 0.88, 1.0, twinkle),
            );
        }

        let mut projected_nodes = self.project_nodes(state, bounds);
        projected_nodes.sort_by(|a, b| a.depth.partial_cmp(&b.depth).unwrap_or(Ordering::Equal));

        let mut indexed_projection: Vec<Option<ProjectedNode>> = vec![None; self.nodes.len()];
        for projection in &projected_nodes {
            indexed_projection[projection.index] = Some(*projection);
        }

        let edge_max = self.max_shared_labels_per_edge.max(1) as f32;
        for edge in &self.edges {
            let (Some(start), Some(end)) =
                (indexed_projection[edge.start], indexed_projection[edge.end])
            else {
                continue;
            };

            let depth_factor = ((start.depth + end.depth) * 0.5 + 1.8) / 3.6;
            let weight = edge.shared_labels as f32 / edge_max;
            let alpha = (0.06 + weight * 0.34) * depth_factor.clamp(0.3, 1.0);
            let width = 0.6 + 1.7 * weight;

            let edge_path = canvas::Path::line(start.point, end.point);
            frame.stroke(
                &edge_path,
                canvas::Stroke::default()
                    .with_color(Color::from_rgba(0.42, 0.64, 0.88, alpha))
                    .with_width(width),
            );
        }

        for projection in projected_nodes {
            let Some(node) = self.nodes.get(projection.index) else {
                continue;
            };

            let mut node_radius = projection.radius;
            let is_hovered = state.hovered_node_index == Some(projection.index);
            let is_selected = self
                .selected_note_path
                .as_ref()
                .is_some_and(|path| path == &node.note_path);
            if is_hovered {
                node_radius += 2.2;
            }

            let color_anchor = node
                .labels
                .first()
                .map(String::as_str)
                .unwrap_or("unlabeled");
            let mut node_color = color_from_seed(color_anchor, 0.63, 0.86);

            if node.labels.is_empty() {
                node_color = Color::from_rgb(0.68, 0.73, 0.82);
            }

            let depth_light = ((projection.depth + 1.1) / 2.1).clamp(0.4, 1.15);
            node_color = Color::from_rgba(
                (node_color.r * depth_light).min(1.0),
                (node_color.g * depth_light).min(1.0),
                (node_color.b * depth_light).min(1.0),
                0.94,
            );

            frame.fill(
                &canvas::Path::circle(projection.point, node_radius * 1.8),
                Color::from_rgba(node_color.r, node_color.g, node_color.b, 0.08),
            );
            frame.fill(
                &canvas::Path::circle(projection.point, node_radius),
                node_color,
            );
            frame.stroke(
                &canvas::Path::circle(projection.point, node_radius + 0.5),
                canvas::Stroke::default()
                    .with_color(Color::from_rgba(0.9, 0.95, 1.0, 0.18))
                    .with_width(1.0),
            );

            let show_label =
                is_selected || is_hovered || (projection.depth > 0.62 && node.degree >= 1);
            if show_label {
                let label = truncate_label(&node.note_path);
                let label_position = Point::new(
                    projection.point.x + node_radius + 5.0,
                    projection.point.y - node_radius - 4.0,
                );
                let label_size = if is_selected { 16.0 } else { 13.0 };

                if is_selected {
                    // Emphasize selected labels without relying on extra-bold font weights,
                    // which can miss glyph coverage on some systems.
                    for (offset_x, offset_y) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                        frame.fill_text(canvas::Text {
                            content: label.clone(),
                            position: Point::new(
                                label_position.x + offset_x,
                                label_position.y + offset_y,
                            ),
                            color: Color::from_rgba(0.0, 0.0, 0.0, 0.95),
                            size: Pixels(label_size),
                            font: Font::DEFAULT,
                            ..canvas::Text::default()
                        });
                    }
                }

                frame.fill_text(canvas::Text {
                    content: label,
                    position: label_position,
                    color: if is_selected {
                        Color::from_rgba(1.0, 1.0, 1.0, 1.0)
                    } else {
                        Color::from_rgba(0.93, 0.96, 1.0, 0.88)
                    },
                    size: Pixels(label_size),
                    font: Font::DEFAULT,
                    ..canvas::Text::default()
                });
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.drag_anchor.is_some() {
            return mouse::Interaction::Grabbing;
        }

        if state.hovered_node_index.is_some() {
            return mouse::Interaction::Pointer;
        }

        if cursor.is_over(bounds) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::None
        }
    }
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

    fn compute_notes_cache_key(&self) -> u64 {
        let mut normalized_notes: Vec<(String, Vec<String>)> = self
            .notes
            .iter()
            .map(|note| {
                let labels = normalize_labels(&note.labels);
                (note.rel_path.clone(), labels)
            })
            .collect();

        normalized_notes.sort_by(|left, right| left.0.cmp(&right.0));

        let mut hasher = DefaultHasher::new();
        for (path, labels) in normalized_notes {
            path.hash(&mut hasher);
            for label in labels {
                label.hash(&mut hasher);
            }
        }

        hasher.finish()
    }

    fn build_graph_cache(notes: &[NoteMetadata]) -> GraphCache {
        let mut normalized_notes: Vec<(String, Vec<String>)> = notes
            .iter()
            .map(|note| (note.rel_path.clone(), normalize_labels(&note.labels)))
            .collect();
        normalized_notes.sort_by(|left, right| left.0.cmp(&right.0));

        let mut label_counts_map: HashMap<String, usize> = HashMap::new();
        let mut distinct_labels: HashSet<String> = HashSet::new();

        for (_, labels) in &normalized_notes {
            for label in labels {
                *label_counts_map.entry(label.clone()).or_insert(0) += 1;
                distinct_labels.insert(label.clone());
            }
        }

        let mut sorted_labels: Vec<String> = distinct_labels.into_iter().collect();
        sorted_labels.sort();

        let mut label_anchor_by_name: HashMap<String, [f32; 3]> = HashMap::new();
        for (index, label) in sorted_labels.iter().enumerate() {
            label_anchor_by_name.insert(
                label.clone(),
                fibonacci_sphere_point(index, sorted_labels.len().max(1)),
            );
        }

        let mut nodes = Vec::with_capacity(normalized_notes.len());
        for (index, (note_path, labels)) in normalized_notes.iter().enumerate() {
            let position = if labels.is_empty() {
                let fallback = hashed_direction(note_path);
                let radius = 0.84 + hash_to_unit_f32(note_path, 31) * 0.16;
                scale_3d(fallback, radius)
            } else {
                let mut anchor_sum = [0.0, 0.0, 0.0];

                for label in labels {
                    if let Some(anchor) = label_anchor_by_name.get(label) {
                        anchor_sum = add_3d(anchor_sum, *anchor);
                    }
                }

                let blended_anchor = normalize_3d(anchor_sum);
                let jitter = hashed_direction(&format!("{note_path}::{index}::jitter"));
                let combined = normalize_3d(add_3d(
                    scale_3d(blended_anchor, 0.86),
                    scale_3d(jitter, 0.22),
                ));
                let radius = 0.56 + hash_to_unit_f32(note_path, 47) * 0.20;
                scale_3d(combined, radius)
            };

            nodes.push(GraphNode {
                note_path: note_path.clone(),
                labels: labels.clone(),
                position,
                degree: 0,
            });
        }

        let mut edges = Vec::new();
        let mut max_shared_labels_per_edge = 1usize;
        let mut degrees = vec![0usize; nodes.len()];

        for left in 0..nodes.len() {
            for right in (left + 1)..nodes.len() {
                let shared_labels = shared_label_count(&nodes[left].labels, &nodes[right].labels);
                if shared_labels == 0 {
                    continue;
                }

                max_shared_labels_per_edge = max_shared_labels_per_edge.max(shared_labels);
                degrees[left] += 1;
                degrees[right] += 1;
                edges.push(GraphEdge {
                    start: left,
                    end: right,
                    shared_labels,
                });
            }
        }

        for (node, degree) in nodes.iter_mut().zip(degrees) {
            node.degree = degree;
        }

        let mut label_counts: Vec<(String, usize)> = label_counts_map.into_iter().collect();
        label_counts.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then_with(|| left.0.to_lowercase().cmp(&right.0.to_lowercase()))
        });

        GraphCache {
            nodes,
            edges,
            label_counts,
            max_shared_labels_per_edge,
        }
    }

    fn refresh_graph_cache_if_needed(&self) {
        let cache_key = self.compute_notes_cache_key();
        if self.graph_cache_key.get() == Some(cache_key) {
            return;
        }

        let graph = Self::build_graph_cache(&self.notes);
        *self.graph_cache.borrow_mut() = graph;
        self.graph_cache_key.set(Some(cache_key));
    }

    fn calculate_focus_camera(&self, target_note: Option<&str>) -> (f32, f32, f32) {
        let Some(note_path) = target_note else {
            return (
                DEFAULT_CAMERA_YAW,
                DEFAULT_CAMERA_PITCH,
                DEFAULT_CAMERA_ZOOM,
            );
        };

        let cache = self.graph_cache.borrow();
        let Some(node) = cache.nodes.iter().find(|node| node.note_path == note_path) else {
            return (
                DEFAULT_CAMERA_YAW,
                DEFAULT_CAMERA_PITCH,
                DEFAULT_CAMERA_ZOOM,
            );
        };

        let point = normalize_3d(node.position);
        let yaw = point[0].atan2(point[2]);
        let horizontal = (point[0] * point[0] + point[2] * point[2]).sqrt();
        let pitch = wrap_angle(point[1].atan2(horizontal));

        (yaw, pitch, 1.20)
    }

    fn apply_focus_target(&mut self, target_note: Option<String>) {
        self.focus_target_note = target_note;
        self.refresh_graph_cache_if_needed();

        let (yaw, pitch, zoom) = self.calculate_focus_camera(self.focus_target_note.as_deref());
        self.focus_yaw = yaw;
        self.focus_pitch = pitch;
        self.focus_zoom = zoom;
        self.focus_version = self.focus_version.wrapping_add(1);
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
            Message::ToggleLabel(label) => {
                let _ = label;
                Task::none()
            }
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
                    "Click a node to open it. Drag to orbit. Scroll to zoom. Edges mean shared labels.",
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
        (
            cache.nodes.len(),
            cache.edges.len(),
            cache.label_counts.len(),
        )
    }
}

fn normalize_labels(raw_labels: &[String]) -> Vec<String> {
    let mut labels: Vec<String> = raw_labels
        .iter()
        .map(|label| label.trim())
        .filter(|label| !label.is_empty())
        .map(ToString::to_string)
        .collect();
    labels.sort();
    labels.dedup();
    labels
}

fn truncate_label(value: &str) -> String {
    let mut truncated = value.chars().take(MAX_LABEL_LENGTH).collect::<String>();
    if value.chars().count() > MAX_LABEL_LENGTH {
        truncated.push_str("...");
    }
    truncated
}

fn shared_label_count(left: &[String], right: &[String]) -> usize {
    let mut shared = 0usize;
    let mut left_index = 0usize;
    let mut right_index = 0usize;

    while left_index < left.len() && right_index < right.len() {
        match left[left_index].cmp(&right[right_index]) {
            Ordering::Less => left_index += 1,
            Ordering::Greater => right_index += 1,
            Ordering::Equal => {
                shared += 1;
                left_index += 1;
                right_index += 1;
            }
        }
    }

    shared
}

fn fibonacci_sphere_point(index: usize, total: usize) -> [f32; 3] {
    if total <= 1 {
        return [0.0, 0.0, 1.0];
    }

    let normalized = index as f32 / (total - 1) as f32;
    let y = 1.0 - normalized * 2.0;
    let radius = (1.0 - y * y).max(0.0).sqrt();
    let theta = PI * (3.0 - 5.0_f32.sqrt()) * index as f32;
    let x = radius * theta.cos();
    let z = radius * theta.sin();
    [x, y, z]
}

fn rotate_3d(point: [f32; 3], yaw: f32, pitch: f32) -> [f32; 3] {
    let yaw_sin = yaw.sin();
    let yaw_cos = yaw.cos();
    let pitch_sin = pitch.sin();
    let pitch_cos = pitch.cos();

    let xz_x = point[0] * yaw_cos - point[2] * yaw_sin;
    let xz_z = point[0] * yaw_sin + point[2] * yaw_cos;

    let yz_y = point[1] * pitch_cos - xz_z * pitch_sin;
    let yz_z = point[1] * pitch_sin + xz_z * pitch_cos;

    [xz_x, yz_y, yz_z]
}

fn wrap_angle(angle: f32) -> f32 {
    let two_pi = PI * 2.0;
    (angle + PI).rem_euclid(two_pi) - PI
}

fn normalize_3d(point: [f32; 3]) -> [f32; 3] {
    let magnitude = (point[0] * point[0] + point[1] * point[1] + point[2] * point[2]).sqrt();
    if magnitude <= f32::EPSILON {
        [0.0, 0.0, 0.0]
    } else {
        [
            point[0] / magnitude,
            point[1] / magnitude,
            point[2] / magnitude,
        ]
    }
}

fn add_3d(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn scale_3d(point: [f32; 3], scalar: f32) -> [f32; 3] {
    [point[0] * scalar, point[1] * scalar, point[2] * scalar]
}

fn hashed_direction(seed: &str) -> [f32; 3] {
    let theta = hash_to_unit_f32(seed, 3) * PI * 2.0;
    let y = hash_to_unit_f32(seed, 9) * 2.0 - 1.0;
    let radius = (1.0 - y * y).max(0.0).sqrt();
    [radius * theta.cos(), y, radius * theta.sin()]
}

fn hash_to_unit_f32(seed: &str, salt: u64) -> f32 {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    salt.hash(&mut hasher);
    let value = hasher.finish();
    (value as f64 / u64::MAX as f64) as f32
}

fn color_from_seed(seed: &str, saturation: f32, value: f32) -> Color {
    let hue = hash_to_unit_f32(seed, 17);
    hsv_to_rgb(hue, saturation, value)
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> Color {
    let sector = (hue * 6.0).floor();
    let fraction = hue * 6.0 - sector;

    let p = value * (1.0 - saturation);
    let q = value * (1.0 - fraction * saturation);
    let t = value * (1.0 - (1.0 - fraction) * saturation);

    match (sector as i32).rem_euclid(6) {
        0 => Color::from_rgb(value, t, p),
        1 => Color::from_rgb(q, value, p),
        2 => Color::from_rgb(p, value, t),
        3 => Color::from_rgb(p, q, value),
        4 => Color::from_rgb(t, p, value),
        _ => Color::from_rgb(value, p, q),
    }
}
