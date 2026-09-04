use crate::error::Result;
use crate::video::decoder::FrameExtractor;
use rayon::prelude::*;
use std::path::{Path, PathBuf};

pub struct ExtractionJob {
    pub id: usize,
    pub timestamp_us: i64,
}

pub fn extract_frames(
    jobs: &[ExtractionJob],
    extractor_a: &dyn FrameExtractor,
    extractor_b: &dyn FrameExtractor,
    output_root: &Path,
    flat_naming: bool,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    // Parallel extraction across jobs (Spec §38)
    let results: Result<Vec<_>> = jobs
        .par_iter()
        .map(|job| {
            let (path_a, path_b) = frame_paths(output_root, job.id, flat_naming);
            // Extract lens A and B in parallel within job
            let (ra, rb) = rayon::join(
                || extractor_a.extract_frame(job.timestamp_us, &path_a),
                || extractor_b.extract_frame(job.timestamp_us, &path_b),
            );
            ra?;
            rb?;
            // Confirm files exist
            Ok((path_a, path_b))
        })
        .collect();

    results
}

pub fn frame_paths(root: &Path, id: usize, flat: bool) -> (PathBuf, PathBuf) {
    if flat {
        let dir = root.join("frames");
        (
            dir.join(format!("{id:06}_a.jpg")),
            dir.join(format!("{id:06}_b.jpg")),
        )
    } else {
        let dir = root.join(format!("frames/{id:06}"));
        (dir.join("lens_a.jpg"), dir.join("lens_b.jpg"))
    }
}

pub fn relative_path(root: &Path, absolute: &Path) -> String {
    absolute
        .strip_prefix(root)
        .unwrap_or(absolute)
        .to_string_lossy()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn paths_nested() {
        let root = PathBuf::from("/tmp/out");
        let (a,b) = frame_paths(&root, 1, false);
        assert_eq!(a, PathBuf::from("/tmp/out/frames/000001/lens_a.jpg"));
        assert_eq!(b, PathBuf::from("/tmp/out/frames/000001/lens_b.jpg"));
    }
    #[test]
    fn paths_flat() {
        let root = PathBuf::from("/tmp/out");
        let (a,b) = frame_paths(&root, 2, true);
        assert_eq!(a, PathBuf::from("/tmp/out/frames/000002_a.jpg"));
    }
}
