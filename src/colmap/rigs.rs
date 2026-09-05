use rusqlite::Connection;

pub fn ensure_rig(conn: &Connection, ref_camera_id: i64) -> anyhow::Result<i64> {
    // Try COLMAP 4.x schema: rigs(ref_sensor_id, ref_sensor_type)
    // sensor_type 0 = camera (from COLMAP docs)
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(rigs)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if cols.contains(&"ref_sensor_id".to_string()) {
        conn.execute(
            "INSERT INTO rigs (ref_sensor_id, ref_sensor_type) VALUES (?1, 0)",
            rusqlite::params![ref_camera_id],
        )?;
        let rig_id = conn.last_insert_rowid();
        // Also need rig_sensors for each camera in rig
        // For now, caller will insert rig_sensors; we insert ref camera as first sensor
        conn.execute(
            "INSERT INTO rig_sensors (rig_id, sensor_id, sensor_type, sensor_from_rig) VALUES (?1, ?2, 0, ?3)",
            rusqlite::params![rig_id, ref_camera_id, Vec::<u8>::new()],
        )?;
        Ok(rig_id)
    } else if cols.contains(&"ref_camera_id".to_string()) {
        conn.execute("INSERT INTO rigs (ref_camera_id) VALUES (?1)", rusqlite::params![ref_camera_id])?;
        Ok(conn.last_insert_rowid())
    } else {
        anyhow::bail!("unsupported rigs schema: {:?}", cols);
    }
}

pub fn ensure_rig_with_sensors(conn: &Connection, camera_ids: &[i64]) -> anyhow::Result<i64> {
    if camera_ids.is_empty() {
        anyhow::bail!("no camera ids for rig");
    }
    let rig_id = ensure_rig(conn, camera_ids[0])?;
    // Insert remaining sensors
    for &cam_id in &camera_ids[1..] {
        // Try new schema
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(rig_sensors)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if cols.contains(&"sensor_id".to_string()) {
            conn.execute(
                "INSERT INTO rig_sensors (rig_id, sensor_id, sensor_type, sensor_from_rig) VALUES (?1, ?2, 0, ?3)",
                rusqlite::params![rig_id, cam_id, Vec::<u8>::new()],
            )?;
        }
    }
    Ok(rig_id)
}
