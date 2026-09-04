use crate::image::DecodedImage;

/// Fast heuristic per spec §5 (people shadows).
/// Start from person mask, search darker connected regions, luminance/chroma, spatial connection, dilate/feather.
/// MVP: expand person mask downward/south as shadow (common for overhead sun) with configurable radius/intensity check.

#[derive(Debug, Clone)]
pub struct ShadowConfig {
    pub enabled: bool,
    pub expand_px: u32,
    pub darken_threshold: u8, // luminance delta to consider shadow
    pub dilate: u32,
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self { enabled: false, expand_px: 12, darken_threshold: 20, dilate: 4 }
    }
}

pub fn detect_shadows(person_mask: &[u8], image: &DecodedImage, cfg: &ShadowConfig) -> Vec<u8> {
    if !cfg.enabled || person_mask.iter().all(|&v| v < 128) {
        return vec![0u8; (image.width * image.height) as usize];
    }
    let (w, h) = (image.width, image.height);
    let gray = image.to_gray();

    // Estimate shadow region: for each person pixel, look downward up to expand_px, include if darker than local median.
    let mut shadow = vec![0u8; (w * h) as usize];
    let wh = w as usize;

    // Precompute row median approximation via mean for speed? Simplify: global dark check.
    let mean_lum: f32 = gray.iter().map(|&v| v as f32).sum::<f32>() / gray.len() as f32;

    for y in 0..h {
        for x in 0..w {
            if person_mask[(y * w + x) as usize] < 128 { continue; }
            // search south
            for dy in 1..=cfg.expand_px {
                let ny = y + dy;
                if ny >= h { break; }
                let idx = (ny * w + x) as usize;
                if person_mask[idx] > 127 { continue; } // already person
                let lum = gray[idx] as i16;
                // darker than mean minus threshold and darker than person pixel's neighborhood
                if (mean_lum - lum as f32) > cfg.darken_threshold as f32 {
                    shadow[idx] = 255;
                }
            }
        }
    }

    if cfg.dilate > 0 {
        shadow = crate::mask::morphology::dilate(&shadow, w, h, cfg.dilate);
    }
    shadow
}
