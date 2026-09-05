use rusqlite::Connection;

pub fn insert_frame(conn: &Connection, rig_id: i64) -> anyhow::Result<i64> {
    conn.execute("INSERT INTO frames (rig_id) VALUES (?1)", rusqlite::params![rig_id])?;
    Ok(conn.last_insert_rowid())
}
