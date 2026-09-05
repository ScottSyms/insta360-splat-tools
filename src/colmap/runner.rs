use std::path::Path;

pub fn run_feature_extractor(database: &Path, image_path: &Path, mask_path: Option<&Path>) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new("colmap");
    cmd.arg("feature_extractor")
        .arg("--database_path").arg(database)
        .arg("--image_path").arg(image_path);
    if let Some(masks) = mask_path {
        cmd.arg("--ImageReader.mask_path").arg(masks);
    }
    let out = cmd.output()?;
    if !out.status.success() {
        anyhow::bail!("feature_extractor failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
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
    let out = cmd.output()?;
    if !out.status.success() {
        anyhow::bail!("matches_importer failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
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
    let out = cmd.output()?;
    if !out.status.success() {
        anyhow::bail!("mapper failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}
