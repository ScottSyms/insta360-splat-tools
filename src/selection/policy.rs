use crate::config::Config;

#[derive(Debug, Clone)]
pub struct SelectionPolicy {
    pub rotation_threshold_deg: f64,
    pub minimum_interval_us: i64,
    pub maximum_interval_us: i64,
    pub angular_velocity_threshold_deg_s: Option<f64>,
}

impl From<&Config> for SelectionPolicy {
    fn from(cfg: &Config) -> Self {
        Self {
            rotation_threshold_deg: cfg.selection.rotation_threshold_deg,
            minimum_interval_us: cfg.selection.minimum_interval_ms * 1000,
            maximum_interval_us: cfg.selection.maximum_interval_ms * 1000,
            angular_velocity_threshold_deg_s: cfg.selection.angular_velocity_threshold_deg_s,
        }
    }
}
