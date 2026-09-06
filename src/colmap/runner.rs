use std::path::Path;

/// COLMAP prints its own per-image/per-block progress to stderr (glog-style). `Command`
/// inherits the parent's stdio by default, so as long as we don't redirect it (as `.output()`
/// does, to capture it into a buffer we then throw away on success), it streams straight
/// through to whoever is running us — visible live in workflow.py's terminal.
fn run_streaming(mut cmd: std::process::Command, label: &str) -> anyhow::Result<()> {
    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("{label} failed with {status} (see colmap output above for details)");
    }
    Ok(())
}

/// Configure rigs/frames/frame_data (and each sensor's calibrated `sensor_from_rig` pose)
/// via COLMAP's own `rig_configurator`, rather than hand-writing those tables ourselves.
/// Per its header doc (colmap/scene/rig.h), it clears any existing rigs/frames first, so
/// this is safe to call unconditionally.
pub fn run_rig_configurator(database: &Path, rig_config: &Path) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new("colmap");
    cmd.arg("rig_configurator")
        .arg("--database_path").arg(database)
        .arg("--rig_config_path").arg(rig_config);
    run_streaming(cmd, "rig_configurator")
}

pub fn run_feature_extractor(database: &Path, image_path: &Path, mask_path: Option<&Path>) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new("colmap");
    cmd.arg("feature_extractor")
        .arg("--database_path").arg(database)
        .arg("--image_path").arg(image_path);
    if let Some(masks) = mask_path {
        cmd.arg("--ImageReader.mask_path").arg(masks);
    }
    run_streaming(cmd, "feature_extractor")
}

pub fn run_matcher(database: &Path, pairs: &Path, rig_verification: bool) -> anyhow::Result<()> {
    // §15.4: use COLMAP's custom-pair-list importer against the IMU/geometry-derived
    // candidate graph, instead of a generic sequential/exhaustive matcher that would
    // silently ignore the planned pairs (and any cross-lens candidates in them).
    if !pairs.exists() {
        anyhow::bail!(
            "candidate pair list not found at {} — run colmap-pairs before colmap-match",
            pairs.display()
        );
    }
    let mut cmd = std::process::Command::new("colmap");
    cmd.arg("matches_importer")
        .arg("--database_path").arg(database)
        .arg("--match_list_path").arg(pairs)
        .arg("--match_type").arg("pairs")
        .arg("--FeatureMatching.rig_verification").arg(if rig_verification { "1" } else { "0" });
    run_streaming(cmd, "matches_importer")
}

pub fn run_mapper(database: &Path, image_path: &Path, output_path: &Path, fix_rig: bool) -> anyhow::Result<()> {
    std::fs::create_dir_all(output_path)?;
    let mut cmd = std::process::Command::new("colmap");
    cmd.arg("mapper")
        .arg("--database_path").arg(database)
        .arg("--image_path").arg(image_path)
        .arg("--output_path").arg(output_path);
    if fix_rig {
        cmd.arg("--Mapper.ba_refine_sensor_from_rig").arg("0");
    }
    run_streaming(cmd, "mapper")
}
