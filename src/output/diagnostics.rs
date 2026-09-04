use crate::insta360::telemetry::ImuSample;
use crate::selection::candidate::Candidate;
use std::path::Path;

pub fn write_diagnostics(
    output_root: &Path,
    samples: &[ImuSample],
    candidates: &[Candidate],
) -> anyhow::Result<()> {
    let candidate_set: std::collections::HashSet<i64> = candidates.iter().map(|c| c.timestamp_us).collect();

    // CSV
    let csv_path = output_root.join("analysis.csv");
    write_csv_manual(&csv_path, samples, &candidate_set)?;

    // JSON analysis
    let analysis = serde_json::json!({
        "sample_count": samples.len(),
        "candidate_count": candidates.len(),
        "duration_us": samples.last().map(|s| s.timestamp_us).unwrap_or(0),
        "candidates": candidates.iter().map(|c| serde_json::json!({
            "timestamp_us": c.timestamp_us,
            "elapsed_ms": c.elapsed_us / 1000,
            "rotation_delta_deg": c.rotation_delta_deg,
            "angular_velocity_deg_s": c.angular_velocity_deg_s,
            "reason": c.reason,
        })).collect::<Vec<_>>(),
    });
    let json_path = output_root.join("analysis.json");
    std::fs::create_dir_all(output_root)?;
    std::fs::write(&json_path, serde_json::to_string_pretty(&analysis)?)?;

    Ok(())
}

fn write_csv_manual(path: &Path, samples: &[ImuSample], candidate_set: &std::collections::HashSet<i64>) -> anyhow::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::File::create(path)?;
    writeln!(f, "timestamp_us,gyro_x,gyro_y,gyro_z,accel_x,accel_y,accel_z,selected")?;
    for s in samples {
        let selected = if candidate_set.contains(&s.timestamp_us) { "1" } else { "0" };
        writeln!(
            f,
            "{},{:.6},{:.6},{:.6},{:.4},{:.4},{:.4},{}",
            s.timestamp_us, s.gyro_rad_s[0], s.gyro_rad_s[1], s.gyro_rad_s[2],
            s.accel_m_s2[0], s.accel_m_s2[1], s.accel_m_s2[2], selected
        )?;
    }
    Ok(())
}
