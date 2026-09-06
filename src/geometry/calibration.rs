use serde::{Deserialize, Serialize};
use nalgebra::{UnitQuaternion, Vector3};

/// Calibration file per §8.1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calibration {
    pub version: u32,
    pub rig: String,
    #[serde(default)]
    pub coordinate_convention: String,
    pub imu_from_rig: Transform,
    pub cameras: Vec<CameraCalibration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transform {
    pub rotation_wxyz: [f64; 4],
    pub translation_m: [f64; 3],
}

impl Transform {
    pub fn rotation(&self) -> UnitQuaternion<f64> {
        // wxyz -> nalgebra expects w,x,y,z
        let q = nalgebra::Quaternion::new(self.rotation_wxyz[0], self.rotation_wxyz[1], self.rotation_wxyz[2], self.rotation_wxyz[3]);
        UnitQuaternion::from_quaternion(q)
    }
    pub fn translation(&self) -> Vector3<f64> {
        Vector3::new(self.translation_m[0], self.translation_m[1], self.translation_m[2])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraCalibration {
    pub sensor_id: u32,
    pub name: String,
    pub model: String,
    pub width: u32,
    pub height: u32,
    pub params: Vec<f64>,
    pub camera_from_rig: Transform,
}

pub type RigCalibration = Calibration;

impl Calibration {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let s = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&s)?)
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Default Insta360 X3 dual fisheye: cam0 forward, cam1 backward (180° yaw)
    ///
    /// Uses OPENCV_FISHEYE (equidistant-style, 8 params: fx,fy,cx,cy,k1,k2,k3,k4). Each
    /// lens is a genuine ~200° FOV fisheye; OPENCV (rectilinear + low-order distortion) is
    /// undefined beyond 180° and severely wrong well before that, which corrupts epipolar
    /// geometry and feature reprojection near the frame edges — this was traced (via a
    /// direct .ply inspection) to a majority of gaussians ending up NaN when training a
    /// splat from an OPENCV-model reconstruction: gradient-based optimization diverging on
    /// degenerate depth/reprojection geometry, not a trainer-quality issue.
    ///
    /// Initial focal length is a physically-motivated equidistant-fisheye estimate —
    /// image_radius / half_fov_rad, assuming the ~200° FOV circle roughly fills the square
    /// frame and half_fov ~= 100° — rather than the previous width/2 guess (which had no
    /// justification for *any* lens model and was closer to a ~90°-half-angle pinhole
    /// assumption). It's a starting point for bundle adjustment to refine, not a
    /// calibrated value; k1..k4 start at 0 for the same reason.
    ///
    /// OPENCV_FISHEYE isn't supported by opensplat 1.2 or msplat 1.1.4's COLMAP loaders
    /// (both error on camera model id 5) — colmap-map's output must be run through `colmap
    /// image_undistorter` first to get a PINHOLE reconstruction + undistorted images
    /// before either trainer can consume it.
    pub fn default_x3(width: u32, height: u32) -> Self {
        let half_fov_rad: f64 = 100.0_f64.to_radians();
        let focal = (width.max(height) as f64 / 2.0) / half_fov_rad;
        let cx = width as f64 / 2.0;
        let cy = height as f64 / 2.0;
        Self {
            version: 1,
            rig: "insta360-x3".to_string(),
            coordinate_convention: "wxyz, right-handed, world_up=+Z, camera_forward=+Z".to_string(),
            imu_from_rig: Transform { rotation_wxyz: [1.0,0.0,0.0,0.0], translation_m: [0.0,0.0,0.0] },
            cameras: vec![
                CameraCalibration {
                    sensor_id: 0,
                    name: "cam0".to_string(),
                    model: "OPENCV_FISHEYE".to_string(),
                    width, height,
                    params: vec![focal, focal, cx, cy, 0.0,0.0,0.0,0.0],
                    camera_from_rig: Transform { rotation_wxyz: [1.0,0.0,0.0,0.0], translation_m: [0.0,0.0,0.0] },
                },
                CameraCalibration {
                    sensor_id: 1,
                    name: "cam1".to_string(),
                    model: "OPENCV_FISHEYE".to_string(),
                    width, height,
                    params: vec![focal, focal, cx, cy, 0.0,0.0,0.0,0.0],
                    camera_from_rig: Transform { rotation_wxyz: [0.0,0.0,1.0,0.0], translation_m: [0.0,0.0,0.0] }, // 180° yaw
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_x3_loads() {
        let cal = Calibration::default_x3(2880,2880);
        assert_eq!(cal.cameras.len(), 2);
        assert_eq!(cal.cameras[0].sensor_id, 0);
    }
}
