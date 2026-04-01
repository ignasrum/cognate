use std::collections::{HashMap, HashSet};

use crate::notebook::NoteMetadata;

use super::math::{
    add_3d, fibonacci_sphere_point, hash_to_unit_f32, hashed_direction, normalize_3d,
    normalize_labels, scale_3d, shared_label_count, wrap_angle,
};
use super::{
    DEFAULT_CAMERA_PITCH, DEFAULT_CAMERA_YAW, DEFAULT_CAMERA_ZOOM, GraphCache, GraphEdge,
    GraphNode, Visualizer,
};

impl Visualizer {
    pub(super) fn build_graph_cache(notes: &[NoteMetadata]) -> GraphCache {
        let mut normalized_notes: Vec<(String, Vec<String>)> = notes
            .iter()
            .map(|note| (note.rel_path.clone(), normalize_labels(&note.labels)))
            .collect();
        normalized_notes.sort_by(|left, right| left.0.cmp(&right.0));

        let mut distinct_labels: HashSet<String> = HashSet::new();

        for (_, labels) in &normalized_notes {
            for label in labels {
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

        GraphCache {
            nodes,
            edges,
            max_shared_labels_per_edge,
        }
    }

    pub(super) fn rebuild_graph_cache(&mut self) {
        self.graph_cache = Self::build_graph_cache(&self.notes);
    }

    pub(super) fn calculate_focus_camera(&self, target_note: Option<&str>) -> (f32, f32, f32) {
        let Some(note_path) = target_note else {
            return (
                DEFAULT_CAMERA_YAW,
                DEFAULT_CAMERA_PITCH,
                DEFAULT_CAMERA_ZOOM,
            );
        };

        let Some(node) = self
            .graph_cache
            .nodes
            .iter()
            .find(|node| node.note_path == note_path)
        else {
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

    pub(super) fn apply_focus_target(&mut self, target_note: Option<String>) {
        self.focus_target_note = target_note;

        let (yaw, pitch, zoom) = self.calculate_focus_camera(self.focus_target_note.as_deref());
        self.focus_yaw = yaw;
        self.focus_pitch = pitch;
        self.focus_zoom = zoom;
        self.focus_version = self.focus_version.wrapping_add(1);
    }
}
