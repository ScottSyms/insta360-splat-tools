/// Validate propagated mask — if viewpoint change is large, invalidate and force re-segment.
/// For MVP, check rotation_delta from manifest; if > threshold, reject propagation.

pub fn validate_propagation(rotation_delta_deg: f64, cfg: &crate::temporal::PropagationConfig) -> bool {
    if !cfg.enabled {
        return false;
    }
    rotation_delta_deg <= cfg.max_rotation_deg
}
