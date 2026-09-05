use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    pub project: String,
    pub calibration: String,
    pub frames: usize,
    pub pairs: usize,
    pub config_hash: String,
}

impl RunManifest {
    pub fn write(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
