use crate::selection::candidate::Candidate;

/// Improved candidate search within temporal window (Spec §26)
/// Scores candidates by sharpness + exposure etc. For MVP this is a placeholder.
/// Returns best timestamp among window.

pub fn score_candidate(candidate: &Candidate, blur_score: f64, exposure_score: f64) -> f64 {
    // Simple weighted score
    // rotation normalized 0-1 (capped at 15deg), blur 0-1, exposure 0-1
    let rot_norm = (candidate.rotation_delta_deg / 15.0).clamp(0.0, 1.0);
    0.4 * rot_norm + 0.4 * blur_score + 0.2 * exposure_score
}
