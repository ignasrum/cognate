use iced::{Point, Rectangle};

use super::super::math::{lerp_3d, rotate_3d, rotated_point_for_note_path};
use super::super::{GraphCanvasState, GraphProgram, ProjectedNode};

impl GraphProgram {
    pub(super) fn project_nodes(
        &self,
        state: &GraphCanvasState,
        bounds: Rectangle,
    ) -> Vec<ProjectedNode> {
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

    pub(super) fn hit_test(
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

#[cfg(test)]
mod tests {
    use iced::{Point, Rectangle, Size};

    use super::super::super::{GraphNode, GraphProgram};
    use super::super::GraphCanvasState;

    fn program(nodes: Vec<GraphNode>) -> GraphProgram {
        GraphProgram {
            nodes,
            edges: Vec::new(),
            max_shared_labels_per_edge: 1,
            selected_note_path: None,
            focus_yaw: 0.0,
            focus_pitch: 0.0,
            focus_zoom: 1.0,
            focus_version: 0,
        }
    }

    fn node(note_path: &str, position: [f32; 3]) -> GraphNode {
        GraphNode {
            note_path: note_path.to_string(),
            labels: Vec::new(),
            position,
            degree: 0,
        }
    }

    #[test]
    fn projection_rejects_empty_and_tiny_bounds() {
        let graph = program(vec![node("note", [0.0, 0.0, 0.0])]);
        let state = GraphCanvasState::default();

        assert!(
            graph
                .project_nodes(&state, Rectangle::new(Point::ORIGIN, Size::ZERO))
                .is_empty()
        );
        assert!(
            graph
                .project_nodes(&state, Rectangle::new(Point::ORIGIN, Size::new(1.0, 2.0)))
                .is_empty()
        );
    }

    #[test]
    fn focused_node_is_projected_at_the_canvas_center() {
        let mut graph = program(vec![
            node("focused", [1.0, 0.0, 0.0]),
            node("other", [-1.0, 0.0, 0.0]),
        ]);
        graph.selected_note_path = Some("focused".to_string());
        let state = GraphCanvasState {
            center_note_path: Some("focused".to_string()),
            ..GraphCanvasState::default()
        };

        let projected = graph.project_nodes(
            &state,
            Rectangle::new(Point::ORIGIN, Size::new(200.0, 200.0)),
        );
        let focused = projected.iter().find(|node| node.index == 0).unwrap();

        assert!((focused.point.x - 100.0).abs() < 0.001);
        assert!((focused.point.y - 104.0).abs() < 0.001);
    }

    #[test]
    fn hit_testing_returns_only_nodes_under_the_cursor() {
        let graph = program(vec![node("note", [0.0, 0.0, 0.0])]);
        let state = GraphCanvasState::default();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(200.0, 200.0));
        let projected = graph.project_nodes(&state, bounds);

        assert_eq!(graph.hit_test(&state, bounds, projected[0].point), Some(0));
        assert_eq!(graph.hit_test(&state, bounds, Point::new(0.0, 0.0)), None);
    }
}
