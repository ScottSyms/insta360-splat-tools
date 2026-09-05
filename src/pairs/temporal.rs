use crate::pairs::graph::{CandidatePair, PairSource};
use crate::pairs::score::PairScores;

/// Generate temporal edges §10.2
pub fn temporal_edges(
    images: &[(String, u64, u32)], // (relative_path, frame_id, sensor_id)
    before: usize,
    after: usize,
) -> Vec<CandidatePair> {
    let mut out = Vec::new();
    // Group by sensor
    for sensor in [0,1] {
        let mut seq: Vec<_> = images.iter().filter(|(_,_,s)| *s==sensor).collect();
        seq.sort_by_key(|(_, fid, _)| *fid);
        for (idx, (path, fid, _)) in seq.iter().enumerate() {
            for delta in 1..=before {
                if idx >= delta {
                    let (prev_path, prev_fid, _) = seq[idx - delta];
                    let frame_delta = (*fid as i32) - (*prev_fid as i32);
                    let scores = PairScores::new(1.0 - delta as f32/10.0, 0.5, 0.8, 0.0, 0.0, 0.0);
                    out.push(CandidatePair {
                        image_a: (*path).clone(),
                        image_b: (*prev_path).clone(),
                        source: PairSource::TEMPORAL,
                        score: scores.total,
                        temporal_score: scores.temporal,
                        overlap_score: scores.overlap,
                        rotation_score: scores.rotation,
                        loop_score: 0.0,
                        frame_delta,
                        time_delta_s: frame_delta as f64 * 0.1,
                        angular_delta_deg: 0.0,
                        estimated_overlap: 0.5,
                        selected: true,
                    });
                }
            }
            for delta in 1..=after {
                if idx + delta < seq.len() {
                    let (next_path, next_fid, _) = seq[idx + delta];
                    let frame_delta = (*next_fid as i32) - (*fid as i32);
                    let scores = PairScores::new(1.0 - delta as f32/10.0, 0.5, 0.8, 0.0, 0.0, 0.0);
                    out.push(CandidatePair {
                        image_a: (*path).clone(),
                        image_b: (*next_path).clone(),
                        source: PairSource::TEMPORAL,
                        score: scores.total,
                        temporal_score: scores.temporal,
                        overlap_score: scores.overlap,
                        rotation_score: scores.rotation,
                        loop_score: 0.0,
                        frame_delta,
                        time_delta_s: frame_delta as f64 * 0.1,
                        angular_delta_deg: 0.0,
                        estimated_overlap: 0.5,
                        selected: true,
                    });
                }
            }
        }
    }
    out
}
