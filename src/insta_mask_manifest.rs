use serde::{Deserialize, Serialize};

/// Input manifest from imu-keyframes (subset we need).

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyframesManifest {
    pub schema_version: String,
    pub source: KeyframesSource,
    pub selection: KeyframesSelection,
    pub frames: Vec<KeyframeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyframesSource {
    pub video_a: String,
    pub video_b: String,
    pub camera_model: Option<String>,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyframesSelection {
    pub rotation_threshold_deg: f64,
    pub minimum_interval_ms: i64,
    pub maximum_interval_ms: i64,
    pub optical_flow_validation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyframeEntry {
    pub id: usize,
    pub timestamp_us: i64,
    pub elapsed_ms: i64,
    pub lens_a: String,
    pub lens_b: String,
    pub motion: KeyframeMotion,
    pub visual: KeyframeVisual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyframeMotion {
    pub rotation_delta_deg: f64,
    pub angular_velocity_deg_s: f64,
    pub acceleration_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyframeVisual {
    pub flow_score: f64,
    pub blur_score: f64,
    pub exposure_score: f64,
}

/// Output manifest for scene-mask per spec §20.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskManifest {
    pub schema_version: String,
    pub source_manifest: String,
    pub processing: ProcessingMeta,
    pub frames: Vec<MaskFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingMeta {
    pub people_backend: String,
    pub people_quality: String,
    pub shadow_mode: String,
    pub temporal_propagation: bool,
    pub preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colmap_masks: Option<String>,
    #[serde(default)]
    pub colmap_invert: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskFrame {
    pub id: usize,
    pub lens_a: LensMask,
    pub lens_b: LensMask,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LensMask {
    pub source: String,
    pub mask: String,
    pub removed_fraction: f64,
}
