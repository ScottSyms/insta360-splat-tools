use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub selection: SelectionConfig,
    pub visual: VisualConfig,
    pub quality: QualityConfig,
    pub output: OutputConfig,
    #[serde(default)]
    pub sync: SyncConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionConfig {
    pub rotation_threshold_deg: f64,
    pub minimum_interval_ms: i64,
    pub maximum_interval_ms: i64,
    pub angular_velocity_threshold_deg_s: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualConfig {
    pub enabled: bool,
    pub minimum_flow_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityConfig {
    pub blur_filter: bool,
    pub exposure_filter: bool,
    pub candidate_search_window_ms: i64,
    pub blur_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub format: String,
    pub jpeg_quality: u8,
    pub flat_naming: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub max_lens_skew_ms: i64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self { max_lens_skew_ms: 50 }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            selection: SelectionConfig {
                rotation_threshold_deg: 5.0,
                minimum_interval_ms: 300,
                maximum_interval_ms: 2500,
                angular_velocity_threshold_deg_s: Some(150.0),
            },
            visual: VisualConfig {
                enabled: false,
                minimum_flow_score: 0.10,
            },
            quality: QualityConfig {
                blur_filter: true,
                exposure_filter: true,
                candidate_search_window_ms: 100,
                blur_threshold: 80.0,
            },
            output: OutputConfig {
                format: "jpeg".to_string(),
                jpeg_quality: 95,
                flat_naming: false,
            },
            sync: SyncConfig { max_lens_skew_ms: 50 },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Preset {
    IndoorWalk,
    OutdoorWalk,
    Vehicle,
    TripodPan,
    SlowSurvey,
    DenseReconstruction,
}

impl Preset {
    pub fn apply(&self, cfg: &mut Config) {
        match self {
            Preset::IndoorWalk => {
                cfg.selection.rotation_threshold_deg = 5.0;
                cfg.selection.minimum_interval_ms = 300;
                cfg.selection.maximum_interval_ms = 2500;
            }
            Preset::OutdoorWalk => {
                cfg.selection.rotation_threshold_deg = 7.0;
                cfg.selection.minimum_interval_ms = 400;
                cfg.selection.maximum_interval_ms = 3000;
            }
            Preset::Vehicle => {
                cfg.selection.rotation_threshold_deg = 8.0;
                cfg.selection.minimum_interval_ms = 200;
                cfg.selection.maximum_interval_ms = 1500;
                cfg.visual.enabled = true;
            }
            Preset::TripodPan => {
                cfg.selection.rotation_threshold_deg = 3.0;
                cfg.selection.minimum_interval_ms = 200;
                cfg.selection.maximum_interval_ms = 2000;
            }
            Preset::SlowSurvey => {
                cfg.selection.rotation_threshold_deg = 3.0;
                cfg.selection.minimum_interval_ms = 150;
                cfg.selection.maximum_interval_ms = 1500;
            }
            Preset::DenseReconstruction => {
                cfg.selection.rotation_threshold_deg = 2.0;
                cfg.selection.minimum_interval_ms = 100;
                cfg.selection.maximum_interval_ms = 1000;
            }
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "indoor-walk" | "indoor_walk" => Some(Self::IndoorWalk),
            "outdoor-walk" | "outdoor_walk" => Some(Self::OutdoorWalk),
            "vehicle" => Some(Self::Vehicle),
            "tripod-pan" | "tripod_pan" => Some(Self::TripodPan),
            "slow-survey" | "slow_survey" => Some(Self::SlowSurvey),
            "dense-reconstruction" | "dense_reconstruction" => Some(Self::DenseReconstruction),
            _ => None,
        }
    }
}
