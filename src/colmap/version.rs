pub fn colmap_version() -> Option<String> {
    std::process::Command::new("colmap").arg("--version").output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
}

pub fn check_version() -> anyhow::Result<()> {
    if let Some(v) = colmap_version() {
        tracing::info!("COLMAP version: {}", v.trim());
    } else {
        tracing::warn!("colmap not found in PATH, DB creation will use rusqlite fallback");
    }
    Ok(())
}
