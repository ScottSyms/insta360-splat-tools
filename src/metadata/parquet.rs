use std::path::Path;
/// Parquet stub: for v1 write JSON sidecar to avoid heavy arrow dependency
pub fn write_keyframes_parquet(frames: &serde_json::Value, path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    // Write JSON for now, but with .parquet extension for compatibility
    std::fs::write(path, serde_json::to_string_pretty(frames)?)?;
    Ok(())
}
