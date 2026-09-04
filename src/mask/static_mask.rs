use anyhow::Result;
use std::path::Path;

/// Static / deterministic masks: nadir cap, rect, polygon, precomputed PNG.

#[derive(Debug, Clone)]
pub enum StaticMask {
    None,
    Nadir { radius_ratio: f32 }, // circular mask at bottom center (operator/tripod)
    Rect { x: u32, y: u32, w: u32, h: u32 },
    Polygon { points: Vec<(f32, f32)> }, // normalized 0..1
    Image { mask: Vec<u8>, width: u32, height: u32 },
}

impl StaticMask {
    pub fn generate(&self, width: u32, height: u32) -> Vec<u8> {
        let mut out = vec![0u8; (width * height) as usize];
        match self {
            Self::None => {}
            Self::Nadir { radius_ratio } => {
                let cx = width as f32 / 2.0;
                let cy = height as f32; // bottom edge
                let r = (width as f32 * radius_ratio).max(height as f32 * radius_ratio);
                for y in 0..height {
                    for x in 0..width {
                        let dx = x as f32 - cx;
                        let dy = y as f32 - cy;
                        if dx * dx + dy * dy < r * r {
                            out[(y * width + x) as usize] = 255;
                        }
                    }
                }
            }
            Self::Rect { x, y, w, h } => {
                let x0 = (*x).min(width);
                let y0 = (*y).min(height);
                let x1 = (x + w).min(width);
                let y1 = (y + h).min(height);
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        out[(yy * width + xx) as usize] = 255;
                    }
                }
            }
            Self::Polygon { points } => {
                // Simple even-odd fill via scanline
                for y in 0..height {
                    for x in 0..width {
                        let fx = x as f32 / width as f32;
                        let fy = y as f32 / height as f32;
                        if point_in_polygon(fx, fy, points) {
                            out[(y * width + x) as usize] = 255;
                        }
                    }
                }
            }
            Self::Image { mask, width: mw, height: mh } => {
                // nearest-neighbor upscale/downscale to target
                let img = image::GrayImage::from_raw(*mw, *mh, mask.clone()).unwrap();
                let resized = image::imageops::resize(&img, width, height, image::imageops::FilterType::Nearest);
                return resized.into_raw();
            }
        }
        out
    }

    pub fn from_image_path(path: &Path) -> Result<Self> {
        let img = image::open(path)?.to_luma8();
        let (w, h) = (img.width(), img.height());
        Ok(Self::Image { mask: img.into_raw(), width: w, height: h })
    }
}

fn point_in_polygon(x: f32, y: f32, poly: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let n = poly.len();
    if n < 3 { return false; }
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        let intersect = ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi + 1e-6) + xi);
        if intersect { inside = !inside; }
        j = i;
    }
    inside
}

#[derive(Debug, Clone)]
pub struct StaticMaskConfig {
    pub enable_nadir: bool,
    pub nadir_radius_ratio: f32,
    pub rects: Vec<(u32,u32,u32,u32)>,
    pub polygon: Option<Vec<(f32,f32)>>,
    pub image_mask_path: Option<std::path::PathBuf>,
}

impl Default for StaticMaskConfig {
    fn default() -> Self {
        Self {
            enable_nadir: false,
            nadir_radius_ratio: 0.12,
            rects: Vec::new(),
            polygon: None,
            image_mask_path: None,
        }
    }
}
