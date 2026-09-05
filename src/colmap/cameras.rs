use rusqlite::Connection;
use crate::geometry::calibration::CameraCalibration;

/// COLMAP camera models: map string to enum id (subset)
fn model_id(name: &str) -> i32 {
    match name {
        "OPENCV" => 4,
        "OPENCV_FISHEYE" => 6,
        "FULL_OPENCV" => 12,
        _ => 0, // PINHOLE
    }
}

pub fn insert_camera(conn: &Connection, cam: &CameraCalibration) -> anyhow::Result<i64> {
    let params_blob: Vec<u8> = cam.params.iter().flat_map(|f| f.to_le_bytes()).collect();
    conn.execute(
        "INSERT INTO cameras (model, width, height, params, prior_focal_length) VALUES (?1,?2,?3,?4,1)",
        rusqlite::params![model_id(&cam.model), cam.width as i32, cam.height as i32, params_blob],
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
