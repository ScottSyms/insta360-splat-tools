use rusqlite::Connection;

pub fn insert_image(conn: &Connection, name: &str, camera_id: i64) -> anyhow::Result<i64> {
    conn.execute(
        "INSERT INTO images (name, camera_id) VALUES (?1,?2)",
        rusqlite::params![name, camera_id],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn image_id_map(conn: &Connection) -> anyhow::Result<std::collections::HashMap<String,i64>> {
    let mut stmt = conn.prepare("SELECT name, image_id FROM images")?;
    let mut map = std::collections::HashMap::new();
    for row in stmt.query_map([], |r| Ok((r.get::<_,String>(0)?, r.get::<_,i64>(1)?)))? {
        let (n, id) = row?;
        map.insert(n, id);
    }
    Ok(map)
}
