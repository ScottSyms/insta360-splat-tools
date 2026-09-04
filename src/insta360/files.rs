use crate::error::{AppError, Result};
use crate::insta360::metadata::StreamInfo;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ValidatedPair {
    pub video_a: PathBuf,
    pub video_b: PathBuf,
    pub info_a: StreamInfo,
    pub info_b: StreamInfo,
    pub camera_model: Option<String>,
    /// which file holds telemetry (usually _00), None if neither (error else)
    pub telemetry_file: Option<PathBuf>,
}

pub fn validate_pair(a: &Path, b: &Path, max_skew_ms: i64) -> Result<ValidatedPair> {
    if !a.exists() {
        return Err(AppError::InvalidInput(format!("video-a not found: {}", a.display())));
    }
    if !b.exists() {
        return Err(AppError::MissingLens(format!("video-b not found: {}", b.display())));
    }
    if a == b {
        return Err(AppError::InvalidInput("video-a and video-b are the same file".to_string()));
    }
    let info_a = StreamInfo::from_file(a)?;
    let info_b = StreamInfo::from_file(b)?;

    // durations compatible within 10% or 5 seconds
    if info_a.duration_ms > 0 && info_b.duration_ms > 0 {
        let diff = (info_a.duration_ms - info_b.duration_ms).abs();
        let max_dur = info_a.duration_ms.max(info_b.duration_ms);
        let pct = diff as f64 / max_dur as f64;
        if pct > 0.10 && diff > 5000 {
            return Err(AppError::MismatchedRecording(format!(
                "durations differ too much: a={}ms b={}ms diff={}ms",
                info_a.duration_ms, info_b.duration_ms, diff
            )));
        }
        // also check sync skew per spec §11 default 50ms for X3, not 2ms
        if diff > max_skew_ms && diff > 100 {
            tracing::warn!(
                "lens duration skew {}ms exceeds max_lens_skew_ms {}ms (a={} b={})",
                diff,
                max_skew_ms,
                a.display(),
                b.display()
            );
        }
    }

    let has_a = has_telemetry(a);
    let has_b = has_telemetry(b);
    let telemetry_file = if has_a {
        Some(a.to_path_buf())
    } else if has_b {
        Some(b.to_path_buf())
    } else {
        None
    };

    if telemetry_file.is_none() {
        tracing::warn!("neither file has INSV telemetry trailer; proceeding without IMU");
    }

    // Try to read camera model if telemetry present
    let camera_model = telemetry_file
        .as_ref()
        .and_then(|p| detect_camera_model(p));

    Ok(ValidatedPair {
        video_a: a.to_path_buf(),
        video_b: b.to_path_buf(),
        info_a,
        info_b,
        camera_model,
        telemetry_file,
    })
}

pub fn discover_pair(dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let mut insv: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(dir).max_depth(1).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path().to_path_buf();
        if p.is_file() {
            if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                if ext.eq_ignore_ascii_case("insv") || ext.eq_ignore_ascii_case("mp4") || ext.eq_ignore_ascii_case("mov") {
                    insv.push(p);
                }
            }
        }
    }
    insv.sort();
    if insv.len() < 2 {
        return Err(AppError::MissingLens(format!(
            "input directory {} contains fewer than 2 video files (found {})",
            dir.display(),
            insv.len()
        )));
    }
    // Heuristic: look for _00 and _10 pair with same prefix
    // e.g. VID_20250731_222121_00_019.insv and VID_20250731_222121_10_019.insv
    // Pair by prefix before _00/_10
    for a in &insv {
        let a_name = a.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if a_name.contains("_00_") {
            let prefix = a_name.split("_00_").next().unwrap_or(a_name);
            for b in &insv {
                let b_name = b.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if b_name.contains("_10_") && b_name.starts_with(prefix) {
                    return Ok((a.clone(), b.clone()));
                }
            }
        }
    }
    // fallback: first two by name
    tracing::warn!("no _00/_10 pair detected, using first two files by name");
    Ok((insv[0].clone(), insv[1].clone()))
}

fn has_telemetry(path: &Path) -> bool {
    // Check for INSV magic 32 bytes at EOF: 8db42d694ccc418790edff439fe026bf
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if meta.len() < 32 {
        return false;
    }
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    use std::io::{Seek, SeekFrom, Read};
    if f.seek(SeekFrom::End(-32)).is_err() {
        return false;
    }
    let mut buf = [0u8; 32];
    if f.read_exact(&mut buf).is_err() {
        return false;
    }
    buf == *b"8db42d694ccc418790edff439fe026bf"
}

fn detect_camera_model(path: &Path) -> Option<String> {
    // Use telemetry-parser to extract metadata without full parse overhead if possible
    // Quick attempt: parse via telemetry-parser and read camera_type
    // Fallback: unknown
    match try_detect_with_parser(path) {
        Ok(m) => m,
        Err(e) => {
            tracing::debug!("camera model detection failed for {}: {e}", path.display());
            None
        }
    }
}

fn try_detect_with_parser(_path: &Path) -> Result<Option<String>> {
    // Cheap: skip heavy parse; model will be detected during full telemetry parse later.
    Ok(None)
}
