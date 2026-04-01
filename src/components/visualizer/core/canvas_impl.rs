use iced::widget::canvas;
use iced::{Color, Font, Pixels, Point, Rectangle, Theme, mouse};
use std::cmp::Ordering;
use std::time::Instant;

use super::math::{
    color_from_seed, ease_in_out_cubic, finalize_center_transition, hash_to_unit_f32, lerp,
    lerp_3d, lerp_angle, rotate_3d, rotated_point_for_note_path, truncate_label, wrap_angle,
};
use super::{
    CAMERA_TRANSITION_DURATION_MS, CameraTransition, GraphCanvasState, GraphProgram,
    MAX_CAMERA_ZOOM, MAX_DOUBLE_CLICK_DISTANCE, MAX_DOUBLE_CLICK_INTERVAL_MS, MIN_CAMERA_ZOOM,
    Message, ProjectedNode,
};

impl Default for GraphCanvasState {
    fn default() -> Self {
        Self {
            yaw: super::DEFAULT_CAMERA_YAW,
            pitch: super::DEFAULT_CAMERA_PITCH,
            zoom: super::DEFAULT_CAMERA_ZOOM,
            drag_anchor: None,
            drag_origin_yaw: super::DEFAULT_CAMERA_YAW,
            drag_origin_pitch: super::DEFAULT_CAMERA_PITCH,
            hovered_node_index: None,
            last_node_click: None,
            camera_transition: None,
            center_note_path: None,
            center_transition_from_note: None,
            center_transition_to_note: None,
            center_transition_blend: 1.0,
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

        let mut rotated_points: Vec<[f32; 3]> = self
            .nodes
            .iter()
            .map(|node| rotate_3d(node.position, state.yaw, state.pitch))
            .collect();

        let fallback_center_path = state
            .center_note_path
            .as_deref()
            .or(self.selected_note_path.as_deref());
        let from_center_path = state
            .center_transition_from_note
            .as_deref()
            .or(fallback_center_path);
        let to_center_path = state
            .center_transition_to_note
            .as_deref()
            .or(fallback_center_path);
        let blend = state.center_transition_blend.clamp(0.0, 1.0);
        let transition_is_active =
            state.center_transition_from_note != state.center_transition_to_note;

        let center_offset = match (
            rotated_point_for_note_path(&self.nodes, &rotated_points, from_center_path),
            rotated_point_for_note_path(&self.nodes, &rotated_points, to_center_path),
        ) {
            (Some(from), Some(to)) if transition_is_active => lerp_3d(from, to, blend),
            (Some(_), Some(to)) => to,
            (Some(from), None) => from,
            (None, Some(to)) => to,
            (None, None) => [0.0, 0.0, 0.0],
        };

        for point in &mut rotated_points {
            point[0] -= center_offset[0];
            point[1] -= center_offset[1];
            point[2] -= center_offset[2];
        }

        let focus_point = match (
            rotated_point_for_note_path(&self.nodes, &rotated_points, from_center_path),
            rotated_point_for_note_path(&self.nodes, &rotated_points, to_center_path),
        ) {
            (Some(from), Some(to)) if transition_is_active => Some(lerp_3d(from, to, blend)),
            (Some(_), Some(to)) => Some(to),
            (Some(from), None) => Some(from),
            (None, Some(to)) => Some(to),
            (None, None) => None,
        };

        if let Some(focus_point) = focus_point {
            let max_z = rotated_points
                .iter()
                .map(|point| point[2])
                .fold(f32::NEG_INFINITY, f32::max);

            let preferred_focus_z: f32 = camera_distance - 0.95;
            let target_focus_z = if max_z.is_finite() {
                preferred_focus_z
                    .max(max_z + 0.2)
                    .min(camera_distance - 0.65)
            } else {
                preferred_focus_z
            };

            let z_shift = target_focus_z - focus_point[2];
            for point in &mut rotated_points {
                point[2] += z_shift;
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
            canvas::Event::Window(iced::window::Event::RedrawRequested(now)) => {
                if state.applied_focus_version != self.focus_version {
                    state.drag_anchor = None;
                    state.drag_origin_yaw = state.yaw;
                    state.drag_origin_pitch = state.pitch;
                    state.hovered_node_index = None;
                    state.applied_focus_version = self.focus_version;

                    if state.center_note_path.is_none() {
                        state.center_note_path = self.selected_note_path.clone();
                    }
                    state.center_transition_from_note = state.center_note_path.clone();
                    state.center_transition_to_note = self.selected_note_path.clone();
                    state.center_transition_blend =
                        if state.center_transition_from_note == state.center_transition_to_note {
                            1.0
                        } else {
                            0.0
                        };

                    state.camera_transition = Some(CameraTransition {
                        from_yaw: state.yaw,
                        from_pitch: state.pitch,
                        from_zoom: state.zoom,
                        to_yaw: self.focus_yaw,
                        to_pitch: self.focus_pitch,
                        to_zoom: self.focus_zoom,
                        started_at: *now,
                    });
                }

                if let Some(transition) = state.camera_transition {
                    let elapsed_ms =
                        now.duration_since(transition.started_at).as_secs_f32() * 1000.0;
                    let progress = (elapsed_ms / CAMERA_TRANSITION_DURATION_MS).clamp(0.0, 1.0);
                    let eased = ease_in_out_cubic(progress);

                    state.yaw = lerp_angle(transition.from_yaw, transition.to_yaw, eased);
                    state.pitch = lerp_angle(transition.from_pitch, transition.to_pitch, eased);
                    state.zoom = lerp(transition.from_zoom, transition.to_zoom, eased);
                    state.center_transition_blend = eased;

                    if progress < 1.0 {
                        return Some(iced::widget::Action::request_redraw());
                    }

                    state.yaw = transition.to_yaw;
                    state.pitch = transition.to_pitch;
                    state.zoom = transition.to_zoom;
                    state.camera_transition = None;
                    finalize_center_transition(state);
                }

                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let cursor_position = cursor.position_in(bounds)?;

                if let Some(hit_index) = self.hit_test(state, bounds, cursor_position)
                    && let Some(node) = self.nodes.get(hit_index)
                {
                    let now = Instant::now();
                    let is_double_click = state.last_node_click.as_ref().is_some_and(
                        |(last_index, last_position, last_clicked_at)| {
                            *last_index == hit_index
                                && last_position.distance(cursor_position)
                                    <= MAX_DOUBLE_CLICK_DISTANCE
                                && now.duration_since(*last_clicked_at).as_millis()
                                    <= MAX_DOUBLE_CLICK_INTERVAL_MS
                        },
                    );
                    state.last_node_click = Some((hit_index, cursor_position, now));

                    let message = if is_double_click {
                        Message::NoteSelectedInVisualizer(node.note_path.clone())
                    } else {
                        Message::FocusOnNote(Some(node.note_path.clone()))
                    };

                    return Some(iced::widget::Action::publish(message).and_capture());
                }

                state.last_node_click = None;
                state.camera_transition = None;
                finalize_center_transition(state);
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
                state.camera_transition = None;
                finalize_center_transition(state);
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
