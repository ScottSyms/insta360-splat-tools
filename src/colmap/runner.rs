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

pub fn run_matcher(database: &Path, pairs: &Path) -> anyhow::Result<()> {
    // Use sequential matcher with custom pairs via matches_importer or exhaustive with pair list
    // For v1, use exhaustive_matcher if pairs not supported, else try sequential
    let mut cmd = std::process::Command::new("colmap");
    // Try vocab tree with pair list
    cmd.arg("sequential_matcher")
        .arg("--database_path").arg(database)
        .arg("--SequentialMatching.quadratic_overlap").arg("0")
        .arg("--SequentialMatching.overlap").arg("10");
    // If pairs file exists, we can use matches_importer alternative; for now just run sequential
    let _ = pairs;
    let out = cmd.output()?;
    if !out.status.success() {
        tracing::warn!("sequential_matcher fallback to exhaustive");
        let mut cmd2 = std::process::Command::new("colmap");
        cmd2.arg("exhaustive_matcher").arg("--database_path").arg(database);
        let out2 = cmd2.output()?;
        if !out2.status.success() {
            anyhow::bail!("matcher failed: {}", String::from_utf8_lossy(&out2.stderr));
        }
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
