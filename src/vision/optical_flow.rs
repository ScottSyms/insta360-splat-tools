use crate::video::frame::DecodedFrame;

#[derive(Debug, Clone)]
pub struct VisualMotionScore {
    pub flow_score: f64,
    pub median_displacement: f64,
    pub tracked_ratio: f64,
}

pub trait VisualMotionBackend: Send + Sync {
    fn compare(&self, previous: &DecodedFrame, current: &DecodedFrame) -> anyhow::Result<VisualMotionScore>;
}

/// Simple histogram-difference backend (portable, no OpenCV)
pub struct HistogramBackend;

impl VisualMotionBackend for HistogramBackend {
    fn compare(&self, previous: &DecodedFrame, current: &DecodedFrame) -> anyhow::Result<VisualMotionScore> {
        let g1 = previous.to_gray();
        let g2 = current.to_gray();
        if g1.len() != g2.len() {
            // different sizes: resize logic would go here; for now compute normalized hist diff
            // Approximate by histogram distance
        }
        let h1 = crate::vision::exposure::histogram(&g1);
        let h2 = crate::vision::exposure::histogram(&g2);
        let total = g1.len() as f64;
        // L1 distance normalized
        let diff: f64 = h1.iter().zip(h2.iter()).map(|(a,b)| (*a as f64 - *b as f64).abs()).sum::<f64>() / (2.0 * total);
        // diff 0 => identical, 1 => completely different
        Ok(VisualMotionScore {
            flow_score: diff.clamp(0.0, 1.0),
            median_displacement: diff * 100.0,
            tracked_ratio: 1.0 - diff,
        })
    }
}

/// Placeholder for sparse optical flow (future: optical-flow-lk or opencv)
pub struct SparseFlowBackend;

impl VisualMotionBackend for SparseFlowBackend {
    fn compare(&self, previous: &DecodedFrame, current: &DecodedFrame) -> anyhow::Result<VisualMotionScore> {
        // For MVP, delegate to histogram
        HistogramBackend.compare(previous, current)
    }
}
