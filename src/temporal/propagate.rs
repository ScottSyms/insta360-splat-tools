use crate::image::DecodedImage;

/// Tier optimization: propagate mask from frame N to N+1 without re-segmentation.
/// MVP: simple translate by estimated flow (or zero) and reuse if viewpoint change small.

#[derive(Debug, Clone)]
pub struct PropagationConfig {
    pub enabled: bool,
    pub max_rotation_deg: f64, // reuse threshold from first binary manifest if available
    pub max_translation_px: u32,
}

impl Default for PropagationConfig {
    fn default() -> Self {
        Self { enabled: true, max_rotation_deg: 8.0, max_translation_px: 24 }
    }
}

/// Naive propagation: shift mask by (dx,dy) or return cloned.
pub fn propagate_mask(prev_mask: &[u8], w: u32, h: u32, dx: i32, dy: i32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let sx = x as i32 - dx;
            let sy = y as i32 - dy;
            if sx >= 0 && sx < w as i32 && sy >= 0 && sy < h as i32 {
                out[(y * w + x) as usize] = prev_mask[(sy as u32 * w + sx as u32) as usize];
            }
        }
    }
    out
}

pub fn estimate_shift(_prev: &DecodedImage, _curr: &DecodedImage) -> (i32, i32) {
    // MVP: zero. Future: sparse optical flow (reuse vision::optical_flow).
    (0, 0)
}
