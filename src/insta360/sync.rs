use crate::error::Result;
use crate::insta360::files::ValidatedPair;

#[derive(Debug, Clone)]
pub struct SyncResult {
    pub duration_us: i64,
}

pub fn synchronize(pair: &ValidatedPair, max_skew_ms: i64) -> Result<SyncResult> {
    let dur_a = pair.info_a.duration_us;
    let dur_b = pair.info_b.duration_us;
    let end_us = dur_a.min(dur_b).max(0);
    // if duration unknown (0), fallback to large value; selection will be driven by IMU timestamps instead
    let effective_end = if end_us == 0 { 200_000_000 } else { end_us }; // 200s guess
    let lens_skew_us = (dur_a - dur_b).abs();
    let max_skew_us = max_skew_ms * 1000;

    // Log skew; don't fail hard per sample finding (42ms diff is normal)
    if lens_skew_us > max_skew_us {
        tracing::warn!(
            "lens skew {}us ({}ms) exceeds max {}ms; durations a={} b={}",
            lens_skew_us,
            lens_skew_us / 1000,
            max_skew_ms,
            dur_a,
            dur_b
        );
    } else {
        tracing::info!("synchronized lens skew {}us within tolerance", lens_skew_us);
    }

    Ok(SyncResult {
        duration_us: effective_end,
    })
}
