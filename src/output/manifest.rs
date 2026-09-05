use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: String,
    pub source: Source,
    pub selection: SelectionMeta,
    pub frames: Vec<FrameEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub video_a: String,
    pub video_b: String,
    pub camera_model: Option<String>,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionMeta {
    pub rotation_threshold_deg: f64,
    pub minimum_interval_ms: i64,
    pub maximum_interval_ms: i64,
    pub optical_flow_validation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameEntry {
    pub id: usize,
    pub timestamp_us: i64,
    pub elapsed_ms: i64,
    pub lens_a: String,
    pub lens_b: String,
    pub motion: MotionMeta,
    pub visual: VisualMeta,
    /// Absolute IMU-integrated rig orientation at capture time, wxyz (§8.2 world_from_rig)
    #[serde(default = "default_orientation_wxyz")]
    pub world_from_rig_wxyz: [f64; 4],
}

fn default_orientation_wxyz() -> [f64; 4] { [1.0, 0.0, 0.0, 0.0] }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionMeta {
    pub rotation_delta_deg: f64,
    pub angular_velocity_deg_s: f64,
    pub acceleration_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualMeta {
    pub flow_score: f64,
    pub blur_score: f64,
    pub exposure_score: f64,
}

impl Manifest {
    pub fn new(source: Source, selection: SelectionMeta) -> Self {
        Self {
            schema_version: "1.0".to_string(),
            source,
            selection,
            frames: Vec::new(),
        }
    }
}
