use std::path::PathBuf;

/// Minimal capture descriptor for project layout (§5)
#[derive(Debug, Clone)]
pub struct Insta360Capture {
    pub front: PathBuf,
    pub rear: PathBuf,
    pub project_dir: PathBuf,
}
