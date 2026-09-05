use rusqlite::Connection;
use std::path::Path;
use crate::colmap::schema;

pub struct ColmapDatabase {
    pub path: std::path::PathBuf,
    pub conn: Connection,
}

impl ColmapDatabase {
    pub fn create_or_open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        // Try COLMAP creator first if binary exists
        if !path.exists() {
            if try_colmap_creator(path).is_err() {
                tracing::warn!("colmap database_creator not available, creating via rusqlite");
            }
        }
        let conn = Connection::open(path).map_err(|e| anyhow::anyhow!("open {}: {}", path.display(), e))?;
        conn.pragma_update(None, "foreign_keys", true).map_err(|e| anyhow::anyhow!("pragma foreign_keys: {}", e))?;
        conn.busy_timeout(std::time::Duration::from_millis(5000)).map_err(|e| anyhow::anyhow!("busy_timeout: {}", e))?;
        // Only init our minimal schema if colmap creator didn't already create it
        // Check if cameras table exists; if not, create minimal schema
        let has_cameras: bool = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='cameras'").map_err(|e| anyhow::anyhow!("prepare has_cameras: {}", e))?
            .query_map([], |_| Ok(())).map_err(|e| anyhow::anyhow!("query_map has_cameras: {}", e))?
            .next()
            .is_some();
        if !has_cameras {
            schema::init_schema(&conn).map_err(|e| anyhow::anyhow!("init_schema: {}", e))?;
        }
        schema::validate_schema(&conn).map_err(|e| anyhow::anyhow!("validate_schema: {}", e))?;
        Ok(Self { path: path.to_path_buf(), conn })
    }
}

fn try_colmap_creator(path: &Path) -> anyhow::Result<()> {
    let out = std::process::Command::new("colmap")
        .arg("database_creator")
        .arg("--database_path").arg(path)
        .output()?;
    if !out.status.success() {
        anyhow::bail!("colmap database_creator failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}
