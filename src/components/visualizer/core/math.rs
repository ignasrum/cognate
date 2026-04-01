use iced::Color;
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::f32::consts::PI;
use std::hash::{Hash, Hasher};

use super::{GraphCanvasState, GraphNode, MAX_LABEL_LENGTH};

pub(super) fn normalize_labels(raw_labels: &[String]) -> Vec<String> {
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

pub(super) fn truncate_label(value: &str) -> String {
    let mut truncated = value.chars().take(MAX_LABEL_LENGTH).collect::<String>();
    if value.chars().count() > MAX_LABEL_LENGTH {
        truncated.push_str("...");
    }
    truncated
}

pub(super) fn shared_label_count(left: &[String], right: &[String]) -> usize {
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

pub(super) fn fibonacci_sphere_point(index: usize, total: usize) -> [f32; 3] {
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

pub(super) fn rotate_3d(point: [f32; 3], yaw: f32, pitch: f32) -> [f32; 3] {
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

pub(super) fn wrap_angle(angle: f32) -> f32 {
    let two_pi = PI * 2.0;
    (angle + PI).rem_euclid(two_pi) - PI
}

pub(super) fn finalize_center_transition(state: &mut GraphCanvasState) {
    if state.center_transition_to_note.is_some() {
        state.center_note_path = state.center_transition_to_note.clone();
    }

    state.center_transition_from_note = None;
    state.center_transition_to_note = None;
    state.center_transition_blend = 1.0;
}

pub(super) fn lerp(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}

pub(super) fn lerp_3d(start: [f32; 3], end: [f32; 3], t: f32) -> [f32; 3] {
    [
        lerp(start[0], end[0], t),
        lerp(start[1], end[1], t),
        lerp(start[2], end[2], t),
    ]
}

pub(super) fn lerp_angle(start: f32, end: f32, t: f32) -> f32 {
    let delta = wrap_angle(end - start);
    wrap_angle(start + delta * t)
}

pub(super) fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let p = -2.0 * t + 2.0;
        1.0 - (p * p * p) / 2.0
    }
}

pub(super) fn normalize_3d(point: [f32; 3]) -> [f32; 3] {
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

pub(super) fn rotated_point_for_note_path(
    nodes: &[GraphNode],
    rotated_points: &[[f32; 3]],
    note_path: Option<&str>,
) -> Option<[f32; 3]> {
    let path = note_path?;
    nodes
        .iter()
        .position(|node| node.note_path == path)
        .and_then(|index| rotated_points.get(index).copied())
}

pub(super) fn add_3d(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

pub(super) fn scale_3d(point: [f32; 3], scalar: f32) -> [f32; 3] {
    [point[0] * scalar, point[1] * scalar, point[2] * scalar]
}

pub(super) fn hashed_direction(seed: &str) -> [f32; 3] {
    let theta = hash_to_unit_f32(seed, 3) * PI * 2.0;
    let y = hash_to_unit_f32(seed, 9) * 2.0 - 1.0;
    let radius = (1.0 - y * y).max(0.0).sqrt();
    [radius * theta.cos(), y, radius * theta.sin()]
}

pub(super) fn hash_to_unit_f32(seed: &str, salt: u64) -> f32 {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    salt.hash(&mut hasher);
    let value = hasher.finish();
    (value as f64 / u64::MAX as f64) as f32
}

pub(super) fn color_from_seed(seed: &str, saturation: f32, value: f32) -> Color {
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
