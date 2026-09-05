use rusqlite::Connection;
use crate::geometry::calibration::CameraCalibration;

/// COLMAP camera models: map string to enum id for COLMAP 4.1
/// 0 SIMPLE_PINHOLE (3), 1 PINHOLE (4), 2 SIMPLE_RADIAL (4), 3 RADIAL (5),
/// 4 OPENCV (8), 5 OPENCV_FISHEYE (8), 6 FULL_OPENCV (12), 7 FOV (5),
/// 8 SIMPLE_RADIAL_FISHEYE (4), 9 RADIAL_FISHEYE (5), 10 THIN_PRISM_FISHEYE (12)
fn model_id(name: &str) -> i32 {
    match name {
        "SIMPLE_PINHOLE" => 0,
        "PINHOLE" => 1,
        "SIMPLE_RADIAL" => 2,
        "RADIAL" => 3,
        "OPENCV" => 4,
        "OPENCV_FISHEYE" => 5,
        "FULL_OPENCV" => 6,
        "FOV" => 7,
        "SIMPLE_RADIAL_FISHEYE" => 8,
        "RADIAL_FISHEYE" => 9,
        "THIN_PRISM_FISHEYE" => 10,
        _ => 0,
    }
}

fn num_params_for_model(model_id: i32) -> usize {
    match model_id {
        0 => 3,
        1 => 4,
        2 => 4,
        3 => 5,
        4 => 8,
        5 => 8,
        6 => 12,
        7 => 5,
        8 => 4,
        9 => 5,
        10 => 12,
        _ => 4,
    }
}

pub fn insert_camera(conn: &Connection, cam: &CameraCalibration) -> anyhow::Result<i64> {
    let mid = model_id(&cam.model);
    let expected = num_params_for_model(mid);
    if cam.params.len() != expected {
        anyhow::bail!(
            "camera {} model {} (id {}) expects {} params but got {}: {:?}",
            cam.name, cam.model, mid, expected, cam.params.len(), cam.params
        );
    }
    let params_blob: Vec<u8> = cam.params.iter().flat_map(|f| f.to_le_bytes()).collect();
    conn.execute(
        "INSERT INTO cameras (model, width, height, params, prior_focal_length) VALUES (?1,?2,?3,?4,1)",
        rusqlite::params![mid, cam.width as i32, cam.height as i32, params_blob],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn ensure_cameras(conn: &Connection, cams: &[CameraCalibration]) -> anyhow::Result<Vec<i64>> {
    // Clear existing for project DB? For now insert
    let mut ids = Vec::new();
    for cam in cams {
        ids.push(insert_camera(conn, cam)?);
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opencv_fisheye_has_8_params() {
        assert_eq!(model_id("OPENCV_FISHEYE"), 5);
        assert_eq!(num_params_for_model(5), 8);
    }
    #[test]
    fn full_opencv_has_12_params() {
        assert_eq!(model_id("FULL_OPENCV"), 6);
        assert_eq!(num_params_for_model(6), 12);
    }
}
