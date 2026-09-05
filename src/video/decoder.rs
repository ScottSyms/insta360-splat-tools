use crate::error::{AppError, Result};
use crate::video::frame::DecodedFrame;
use std::path::{Path, PathBuf};

pub trait FrameExtractor: Send + Sync {
    fn extract_frame(&self, timestamp_us: i64, output_path: &Path) -> Result<()>;
    fn decode_preview(&self, timestamp_us: i64) -> Result<DecodedFrame>;
}

pub struct FfmpegExtractor {
    pub path: PathBuf,
}

impl FfmpegExtractor {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn decode_at(&self, timestamp_us: i64, output_path: Option<&Path>, _preview: bool) -> Result<Option<DecodedFrame>> {
        let path = self.path.clone();
        // Run ffmpeg CLI for robust seek; fallback to ffmpeg-next if needed
        // Use CLI: ffmpeg -y -ss <sec> -i <input> -vframes 1 [-vf scale=...] <output>
        // For preview, decode to memory via ffmpeg + image? Simpler: use CLI to temp file then read
        // However for MVP we implement via ffmpeg CLI subprocess which is reliable and portable

        let sec = timestamp_us as f64 / 1_000_000.0;

        if let Some(out_path) = output_path {
            // Full-res extraction
            let status = std::process::Command::new("ffmpeg")
                .args([
                    "-y",
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-ss",
                    &format!("{:.6}", sec),
                    "-i",
                    &path.to_string_lossy(),
                    "-vframes",
                    "1",
                    "-q:v",
                    "2",
                    out_path.to_string_lossy().as_ref(),
                ])
                .status()
                .map_err(|e| AppError::Decode(format!("failed to spawn ffmpeg: {e}")))?;

            if !status.success() {
                return Err(AppError::Seek {
                    timestamp_us,
                    msg: format!("ffmpeg extraction failed at {}s for {}", sec, path.display()),
                });
            }
            Ok(None)
        } else {
            // Preview decode: extract to temp file scaled to 480 width
            let tmp = tempfile::Builder::new()
                .suffix(".jpg")
                .tempfile()
                .map_err(AppError::Io)?;
            let tmp_path = tmp.path().to_path_buf();

            // Use -vf scale=480:-1 for low-res preview
            let status = std::process::Command::new("ffmpeg")
                .args([
                    "-y",
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-ss",
                    &format!("{:.6}", sec),
                    "-i",
                    &path.to_string_lossy(),
                    "-vframes",
                    "1",
                    "-vf",
                    "scale=480:-1",
                    "-q:v",
                    "5",
                    tmp_path.to_string_lossy().as_ref(),
                ])
                .status()
                .map_err(|e| AppError::Decode(format!("failed to spawn ffmpeg: {e}")))?;

            if !status.success() {
                return Err(AppError::Seek {
                    timestamp_us,
                    msg: format!("ffmpeg preview failed at {}s", sec),
                });
            }

            // Load image via `image` crate
            let img = image::open(&tmp_path).map_err(|e| AppError::Decode(format!("failed to open preview: {e}")))?;
            let rgb = img.to_rgb8();
            let (w, h) = (rgb.width(), rgb.height());
            let data = rgb.into_raw();

            Ok(Some(DecodedFrame {
                width: w,
                height: h,
                data,
                timestamp_us,
            }))
        }
    }
}

impl FrameExtractor for FfmpegExtractor {
    fn extract_frame(&self, timestamp_us: i64, output_path: &Path) -> Result<()> {
        // ensure parent dir
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(AppError::Io)?;
        }
        self.decode_at(timestamp_us, Some(output_path), false)?;
        Ok(())
    }

    fn decode_preview(&self, timestamp_us: i64) -> Result<DecodedFrame> {
        let frame = self.decode_at(timestamp_us, None, true)?;
        frame.ok_or_else(|| AppError::Decode("preview decode returned None".to_string()))
    }
}

/// No-op extractor for dry-run without ffmpeg
pub struct NoopExtractor;

impl FrameExtractor for NoopExtractor {
    fn extract_frame(&self, _timestamp_us: i64, _output_path: &Path) -> Result<()> {
        Ok(())
    }
    fn decode_preview(&self, _timestamp_us: i64) -> Result<DecodedFrame> {
        Err(AppError::Decode("preview disabled (dry-run)".to_string()))
    }
}
