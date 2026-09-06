use crate::error::{AppError, Result};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub width: u32,
    pub height: u32,
    pub frame_rate: f64,
    pub duration_ms: i64,
    pub duration_us: i64,
}

impl StreamInfo {
    pub fn from_file(path: &Path) -> Result<Self> {
        probe_file(path)
    }
}

#[cfg(feature = "ffmpeg")]
fn probe_file(path: &Path) -> Result<StreamInfo> {
    ffmpeg_next::init().ok();
    let ictx = ffmpeg_next::format::input(path)
        .map_err(|e| AppError::Decode(format!("cannot open {}: {e}", path.display())))?;
    let stream = ictx
        .streams()
        .best(ffmpeg_next::media::Type::Video)
        .ok_or_else(|| AppError::Decode(format!("no video stream in {}", path.display())))?;
    let codec_ctx = ffmpeg_next::codec::context::Context::from_parameters(stream.parameters())
        .map_err(|e| AppError::Decode(e.to_string()))?;
    let decoder = codec_ctx
        .decoder()
        .video()
        .map_err(|e| AppError::Decode(e.to_string()))?;
    let width = decoder.width();
    let height = decoder.height();
    let avg_rate = stream.avg_frame_rate();
    let frame_rate = if avg_rate.denominator() == 0 {
        0.0
    } else {
        avg_rate.numerator() as f64 / avg_rate.denominator() as f64
    };
    let duration = ictx.duration();
    // ffmpeg duration is in AV_TIME_BASE (microseconds), -1 if unknown
    let duration_us = if duration == ffmpeg_next::ffi::AV_NOPTS_VALUE || duration < 0 {
        // fallback to stream duration
        let dur_ts = stream.duration();
        if dur_ts == ffmpeg_next::ffi::AV_NOPTS_VALUE || dur_ts < 0 {
            0
        } else {
            let tb = stream.time_base();
            // duration_us = dur_ts * tb.num / tb.den * 1e6  => dur_ts * 1e6 * num / den
            ((dur_ts as f64) * 1_000_000.0 * tb.numerator() as f64 / tb.denominator() as f64) as i64
        }
    } else {
        duration
    };
    let duration_ms = duration_us / 1000;

    Ok(StreamInfo {
        width,
        height,
        frame_rate,
        duration_ms,
        duration_us,
    })
}

#[cfg(not(feature = "ffmpeg"))]
fn probe_file(path: &Path) -> Result<StreamInfo> {
    std::fs::metadata(path).map_err(AppError::Io)?;
    Ok(StreamInfo {
        width: 0,
        height: 0,
        frame_rate: 0.0,
        duration_ms: 0,
        duration_us: 0,
    })
}
