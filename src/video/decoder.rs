use crate::error::{AppError, Result};
use std::path::{Path, PathBuf};

pub trait FrameExtractor: Send + Sync {
    fn extract_frame(&self, timestamp_us: i64, output_path: &Path) -> Result<()>;
}

pub struct FfmpegExtractor {
    pub path: PathBuf,
}

impl FfmpegExtractor {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl FrameExtractor for FfmpegExtractor {
    fn extract_frame(&self, timestamp_us: i64, output_path: &Path) -> Result<()> {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(AppError::Io)?;
        }
        let sec = timestamp_us as f64 / 1_000_000.0;
        // ffmpeg CLI for robust exact seek: -ss <sec> -i <input> -vframes 1 <output>
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-ss",
                &format!("{:.6}", sec),
                "-i",
                &self.path.to_string_lossy(),
                "-vframes",
                "1",
                "-q:v",
                "2",
                output_path.to_string_lossy().as_ref(),
            ])
            .status()
            .map_err(|e| AppError::Decode(format!("failed to spawn ffmpeg: {e}")))?;

        if !status.success() {
            return Err(AppError::Seek {
                timestamp_us,
                msg: format!("ffmpeg extraction failed at {}s for {}", sec, self.path.display()),
            });
        }
        Ok(())
    }
}
