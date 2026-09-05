use nalgebra::{UnitQuaternion, Vector3};

/// world_from_camera = world_from_rig * rig_from_camera  (§8.2)
/// Our calibration stores camera_from_rig, so rig_from_camera = camera_from_rig.inverse()
pub fn world_from_camera(
    world_from_rig: &UnitQuaternion<f64>,
    camera_from_rig: &UnitQuaternion<f64>,
) -> UnitQuaternion<f64> {
    world_from_rig * camera_from_rig.inverse()
}

/// Camera forward in world: rotate [0,0,1] by world_from_camera
pub fn camera_forward_world(world_from_camera: &UnitQuaternion<f64>) -> [f64; 3] {
    let v = world_from_camera * Vector3::new(0.0, 0.0, 1.0);
    [v.x, v.y, v.z]
}

/// Angular separation θ = angle(R_i⁻¹ * R_j) (§10.4)
pub fn angular_separation_deg(a: &UnitQuaternion<f64>, b: &UnitQuaternion<f64>) -> f64 {
    a.angle_to(b).to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;
    use nalgebra::UnitQuaternion;
    use std::f64::consts::FRAC_PI_2;

    fn q_yaw_deg(deg: f64) -> UnitQuaternion<f64> {
        UnitQuaternion::from_axis_angle(&nalgebra::Vector3::z_axis(), deg.to_radians())
    }

    #[test]
    fn identity() {
        let q = UnitQuaternion::identity();
        let f = camera_forward_world(&q);
        assert_abs_diff_eq!(f[2], 1.0, epsilon=1e-9);
    }

    #[test]
    fn ninety_yaw() {
        let q = q_yaw_deg(90.0);
        // Forward [0,0,1] rotated around Z stays [0,0,1]
        let f = camera_forward_world(&q);
        assert_abs_diff_eq!(f[2], 1.0, epsilon=1e-9);
        // But angular separation
        let id = UnitQuaternion::identity();
        assert_abs_diff_eq!(angular_separation_deg(&id, &q), 90.0, epsilon=1e-5);
    }

    #[test]
    fn composition_order() {
        let r10 = q_yaw_deg(10.0);
        let r20 = q_yaw_deg(20.0);
        let world_from_rig = r10;
        let camera_from_rig = UnitQuaternion::identity();
        let wfc = world_from_camera(&world_from_rig, &camera_from_rig);
        assert_abs_diff_eq!(wfc.angle_to(&r10), 0.0, epsilon=1e-9);
        let _ = r20;
    }

    #[test]
    fn inverse() {
        let q = q_yaw_deg(45.0);
        let inv = q.inverse();
        assert_abs_diff_eq!(q.angle_to(&inv).to_degrees(), 90.0, epsilon=1e-5);
    }

    #[test]
    fn camera_from_rig_vs_rig_from_camera() {
        let world_from_rig = q_yaw_deg(30.0);
        let camera_from_rig = q_yaw_deg(180.0); // rear lens
        let wfc = world_from_camera(&world_from_rig, &camera_from_rig);
        // world_from_rig 30° + rig_from_camera 180° (inverse of 180° = 180° again for yaw)
        // 30 * 180 = 210° total
        let expected = q_yaw_deg(210.0);
        let angle_deg = wfc.angle_to(&expected).to_degrees();
        assert!(angle_deg < 1e-4 || (360.0 - angle_deg) < 1e-4);
    }
}
