pub(super) fn round_scale_step(scale: f32) -> f32 {
    (scale * 100.0).round() / 100.0
}
