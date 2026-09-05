use crate::pairs::graph::{CandidatePair, PairSource};
use crate::pairs::score::PairScores;
use crate::geometry::overlap::estimated_overlap;
use nalgebra::UnitQuaternion;
use std::collections::HashMap;

/// Generate geometry edges §10.5 + cross-lens §10.6
pub fn geometry_edges(
    images: &[(String, u64, u32)],
    world_from_rig: &HashMap<u64, UnitQuaternion<f64>>,
    cam_from_rig: &HashMap<u32, UnitQuaternion<f64>>,
    fov_half_deg: f64,
    threshold: f32,
) -> Vec<CandidatePair> {
    let mut out = Vec::new();
    // Build world_from_camera per image
    let mut wfc: HashMap<String, UnitQuaternion<f64>> = HashMap::new();
    for (path, fid, sensor) in images {
        if let (Some(wr), Some(cfr)) = (world_from_rig.get(fid), cam_from_rig.get(sensor)) {
            let wfc_q = crate::geometry::transforms::world_from_camera(wr, cfr);
            wfc.insert(path.clone(), wfc_q);
        }
    }

    for i in 0..images.len() {
        for j in (i+1)..images.len() {
            let (a_path, a_fid, a_sensor) = &images[i];
            let (b_path, b_fid, b_sensor) = &images[j];
            // Same-frame policy §10.3 auto: skip if same frame and fisheye non-overlapping
            if a_fid == b_fid && a_sensor != b_sensor {
                // Fisheye opposite → skip (auto)
                continue;
            }
            if let (Some(qa), Some(qb)) = (wfc.get(a_path), wfc.get(b_path)) {
                let overlap = estimated_overlap(qa, qb, fov_half_deg) as f32;
                if overlap < threshold { continue; }
                let angular = qa.angle_to(qb).to_degrees();
                let is_cross = a_sensor != b_sensor;
                let mut src = PairSource::GEOMETRY;
                if is_cross { src |= PairSource::CROSS_LENS; }
                let scores = PairScores::new(0.5, overlap, 1.0 - (angular as f32/180.0), if is_cross{0.7}else{0.0}, 0.0, 0.0);
                out.push(CandidatePair {
                    image_a: a_path.clone(),
                    image_b: b_path.clone(),
                    source: src,
                    score: scores.total,
                    temporal_score: scores.temporal,
                    overlap_score: overlap,
                    rotation_score: scores.rotation,
                    loop_score: 0.0,
                    frame_delta: (*b_fid as i32) - (*a_fid as i32),
                    time_delta_s: 0.0,
                    angular_delta_deg: angular,
                    estimated_overlap: overlap,
                    selected: true,
                });
            }
        }
    }
    out
}
