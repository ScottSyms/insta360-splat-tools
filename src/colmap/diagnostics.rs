use rusqlite::Connection;

pub fn diagnostics(conn: &Connection) -> anyhow::Result<String> {
    let cameras: i64 = conn.query_row("SELECT COUNT(*) FROM cameras", [], |r| r.get(0))?;
    let images: i64 = conn.query_row("SELECT COUNT(*) FROM images", [], |r| r.get(0))?;
    let rigs: i64 = conn.query_row("SELECT COUNT(*) FROM rigs", [], |r| r.get(0)).unwrap_or(0);
    let frames: i64 = conn.query_row("SELECT COUNT(*) FROM frames", [], |r| r.get(0)).unwrap_or(0);
    Ok(format!("cameras={} images={} rigs={} frames={}", cameras, images, rigs, frames))
}
