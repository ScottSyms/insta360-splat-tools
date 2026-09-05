use crate::geometry::transforms::{angular_separation_deg, camera_forward_world};
use nalgebra::UnitQuaternion;

/// FOV overlap estimation §10.5
/// v1 implements method 2/3: sample spherical polygon vs cone, conservative.

pub fn estimated_overlap(
    world_from_cam_a: &UnitQuaternion<f64>,
    world_from_cam_b: &UnitQuaternion<f64>,
    fov_half_deg: f64,
) -> f64 {
    let fwd_a = camera_forward_world(world_from_cam_a);
    let fwd_b = camera_forward_world(world_from_cam_b);
    let dot = (fwd_a[0]*fwd_b[0] + fwd_a[1]*fwd_b[1] + fwd_a[2]*fwd_b[2]).clamp(-1.0, 1.0);
    let angle = dot.acos().to_degrees();
    // Overlap if angle < sum half angles (conservative)
    let sum = fov_half_deg * 2.0;
    if angle >= sum { 0.0 } else { 1.0 - (angle / sum) }
}

/// More precise: angular separation already computed (§10.4) can be used directly
pub fn overlap_from_separation(sep_deg: f64, fov_half_deg: f64) -> f64 {
    estimated_overlap(&UnitQuaternion::identity(), &UnitQuaternion::from_axis_angle(&nalgebra::Vector3::z_axis(), sep_deg.to_radians()), fov_half_deg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::UnitQuaternion;

    #[test]
    fn identical_high_overlap() {
        let q = UnitQuaternion::identity();
        assert!(estimated_overlap(&q,&q, 90.0) > 0.9);
    }
    #[test]
    fn opposite_zero() {
        let q1 = UnitQuaternion::identity();
        let q2 = UnitQuaternion::from_axis_angle(&nalgebra::Vector3::y_axis(), std::f64::consts::PI);
        assert!(estimated_overlap(&q1,&q2, 45.0) < 0.1);
    }
    #[test]
    fn cross_lens_after_rotation() {
        // cam0 forward, cam1 backward (180°), rig rotated 180° -> cam1 now looks where cam0 did
        let rig_rot = UnitQuaternion::from_axis_angle(&nalgebra::Vector3::y_axis(), std::f64::consts::PI);
        let cam0_from_rig = UnitQuaternion::identity();
        let cam1_from_rig = UnitQuaternion::from_axis_angle(&nalgebra::Vector3::y_axis(), std::f64::consts::PI);
        let wfc_a = crate::geometry::transforms::world_from_camera(&UnitQuaternion::identity(), &cam0_from_rig);
        let wfc_b = crate::geometry::transforms::world_from_camera(&rig_rot, &cam1_from_rig);
        let overlap = estimated_overlap(&wfc_a, &wfc_b, 100.0);
        assert!(overlap > 0.5, "cross-lens should overlap after 180° rig rotation, got {overlap}");
    }
}
