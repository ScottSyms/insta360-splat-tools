/// FOV cone approximation (fallback method 1 §10.5)
#[derive(Debug, Clone)]
pub struct FovCone {
    pub half_angle_deg: f64,
}

impl FovCone {
    pub fn fisheye_default() -> Self { Self { half_angle_deg: 100.0 } } // ~200° fisheye
    pub fn pinhole_default() -> Self { Self { half_angle_deg: 45.0 } }
}
