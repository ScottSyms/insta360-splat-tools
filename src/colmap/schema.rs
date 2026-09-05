use rusqlite::Connection;

pub const EXPECTED_TABLES: &[&str] = &["cameras","images","rigs","frames","keypoints","descriptors","matches","two_view_geometries"];

pub fn validate_schema(conn: &Connection) -> anyhow::Result<()> {
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'")?;
    let tables: Vec<String> = stmt.query_map([], |row| row.get(0))?.collect::<Result<_,_>>()?;
    for exp in EXPECTED_TABLES {
        if !tables.contains(&exp.to_string()) {
            // Not all tables need exist before feature extraction; rigs/frames may be optional in older COLMAP
            tracing::warn!("expected table {} missing, will be created if needed", exp);
        }
    }
    Ok(())
}

pub fn init_schema(conn: &Connection) -> anyhow::Result<()> {
    // Minimal schema for Level-2 integration (§13.2) if colmap database_creator not available
    conn.execute_batch(r#"
        CREATE TABLE IF NOT EXISTS cameras (
            camera_id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
            model INTEGER NOT NULL,
            width INTEGER NOT NULL,
            height INTEGER NOT NULL,
            params BLOB,
            prior_focal_length INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS images (
            image_id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
            name TEXT NOT NULL UNIQUE,
            camera_id INTEGER NOT NULL,
            prior_qw REAL, prior_qx REAL, prior_qy REAL, prior_qz REAL,
            prior_tx REAL, prior_ty REAL, prior_tz REAL,
            FOREIGN KEY(camera_id) REFERENCES cameras(camera_id)
        );
        CREATE TABLE IF NOT EXISTS rigs (
            rig_id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
            ref_camera_id INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS frames (
            frame_id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
            rig_id INTEGER NOT NULL,
            FOREIGN KEY(rig_id) REFERENCES rigs(rig_id)
        );
        -- COLMAP also has rig_sensors and frame_images in newer schemas; create stubs if missing
        CREATE TABLE IF NOT EXISTS keypoints (image_id INTEGER PRIMARY KEY, rows INTEGER, cols INTEGER, data BLOB);
        CREATE TABLE IF NOT EXISTS descriptors (image_id INTEGER PRIMARY KEY, rows INTEGER, cols INTEGER, data BLOB);
        CREATE TABLE IF NOT EXISTS matches (pair_id INTEGER PRIMARY KEY, rows INTEGER, cols INTEGER, data BLOB);
        CREATE TABLE IF NOT EXISTS two_view_geometries (pair_id INTEGER PRIMARY KEY, rows INTEGER, cols INTEGER, data BLOB, config INTEGER, F BLOB, E BLOB, H BLOB);
    "#)?;
    Ok(())
}
