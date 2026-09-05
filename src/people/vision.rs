use anyhow::Result;
use crate::image::DecodedImage;

/// Trait per spec §7.
pub trait PersonSegmenter: Send + Sync {
    fn segment(&self, image: &DecodedImage) -> Result<Vec<u8>>; // 0/255 mask, same dims
    fn name(&self) -> &'static str;
}

/// CPU fallback — currently returns empty mask (no people) but preserves deterministic throughput tier.
/// Replace with lightweight model (e.g., BiRefNet-tiny) or enable macOS Vision via feature.
pub struct CpuPersonSegmenter {
    pub dilate_radius: u32,
}

impl Default for CpuPersonSegmenter {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuPersonSegmenter {
    pub fn new() -> Self { Self { dilate_radius: 3 } }
}

impl PersonSegmenter for CpuPersonSegmenter {
    fn segment(&self, image: &DecodedImage) -> Result<Vec<u8>> {
        // Heuristic fallback for when Vision is unavailable (CI/Linux).
        // Simple bottom-center vertical ellipse approximating standing operator,
        // so masks are not completely empty and demo masking pipeline.
        // For production on macOS, VisionPersonSegmenter will be used.
        let (w, h) = (image.width, image.height);
        let mut mask = vec![0u8; (w * h) as usize];
        // Only synthesize if image is large enough (avoid tiny test images)
        if w < 100 || h < 100 {
            return Ok(mask);
        }
        // Heuristic: person typically near bottom center, ~18% of frame area
        // We do NOT synthesize by default to avoid false positives in photogrammetry;
        // Instead we keep empty but log. To enable demo, set env INSTA_MASK_DUMMY_PERSON=1
        if std::env::var("INSTA_MASK_DUMMY_PERSON").is_ok() {
            let cx = w as f32 * 0.5;
            let cy = h as f32 * 0.85;
            let rx = w as f32 * 0.18;
            let ry = h as f32 * 0.45;
            for y in 0..h {
                for x in 0..w {
                    let dx = (x as f32 - cx) / rx;
                    let dy = (y as f32 - cy) / ry;
                    if dx*dx + dy*dy < 1.0 {
                        mask[(y*w+x) as usize] = 255;
                    }
                }
            }
        }
        Ok(mask)
    }
    fn name(&self) -> &'static str { "cpu-fallback" }
}

/// Apple Vision person segmentation — Tier 1.
///
/// On macOS with `macos-vision` feature, this would call
/// `VNGeneratePersonSegmentationRequest` (quality `.fast`/`.balanced`/`.accurate`)
/// via `objc2` / `vision` bindings or a tiny Swift shim.
///
/// MVP implementation: on non-macOS or without feature, delegates to CPU fallback.
/// On macOS with feature, attempts Vision and falls back on error.
pub struct VisionPersonSegmenter {
    pub quality: VisionQuality,
    fallback: CpuPersonSegmenter,
}

#[derive(Debug, Clone, Copy)]
pub enum VisionQuality { Fast, Balanced, Accurate }

fn vision_quality_str(q: VisionQuality) -> &'static str {
    match q {
        VisionQuality::Fast => "fast",
        VisionQuality::Balanced => "balanced",
        VisionQuality::Accurate => "accurate",
    }
}

fn find_vision_helper() -> Option<std::path::PathBuf> {
    // Check next to current executable (for cargo run, binary is target/debug/scene-mask alongside vision-person)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("vision-person");
            if p.exists() {
                return Some(p);
            }
        }
    }
    let candidates = [
        "target/debug/vision-person",
        "target/release/vision-person",
        "tools/vision-person",
        "/tmp/vision_person",
        "/tmp/test_vision2",
        "/tmp/test_vision",
    ];
    for p in candidates {
        let path = std::path::PathBuf::from(p);
        if path.exists() {
            return Some(path);
        }
    }
    // Try PATH
    if let Ok(output) = std::process::Command::new("which").arg("vision-person").output() {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(std::path::PathBuf::from(s));
            }
        }
    }
    None
}

fn try_vision_helper(image: &DecodedImage, quality: VisionQuality) -> Result<Vec<u8>> {
    let helper = find_vision_helper().ok_or_else(|| anyhow::anyhow!("vision helper not found"))?;

    // Write input to temp JPEG
    let input_file = tempfile::Builder::new().suffix(".jpg").tempfile()?;
    let rgb = image::RgbImage::from_raw(image.width, image.height, image.data.clone())
        .ok_or_else(|| anyhow::anyhow!("invalid image data"))?;
    // Use image crate to write JPEG
    {
        
        let tmp_path = input_file.path().to_path_buf();
        // Write via image crate
        rgb.save(&tmp_path)?;
        // Now create output temp
        let output_file = tempfile::Builder::new().suffix(".png").tempfile()?;
        let output_path = output_file.path().to_path_buf();
        // Keep files alive by not dropping tempfile yet (use path)
        // Need to keep input_file alive until command finishes
        let status = std::process::Command::new(&helper)
            .arg(&tmp_path)
            .arg(&output_path)
            .arg("--quality")
            .arg(vision_quality_str(quality))
            .status()?;

        if !status.success() {
            return Err(anyhow::anyhow!("vision helper failed with status {:?}", status));
        }

        // Read output mask
        let mask_img = image::open(&output_path)?;
        let gray = mask_img.to_luma8();
        if gray.width() != image.width || gray.height() != image.height {
            // Should be same size as input (helper scales to input), but if mismatch, resize
            let resized = image::imageops::resize(&gray, image.width, image.height, image::imageops::FilterType::Triangle);
            return Ok(resized.into_raw());
        }
        let raw = gray.into_raw();
        // Threshold soft mask to 0/255? Keep soft but threshold at 127 for binary
        // Vision already gives soft 0-255, we keep as is for later morphology
        // But ensure we return 0/255? Keep soft for feathering.
        Ok(raw)
    }
}

impl VisionPersonSegmenter {
    pub fn new(quality: VisionQuality) -> Self {
        Self { quality, fallback: CpuPersonSegmenter::new() }
    }

    fn try_vision(&self, image: &DecodedImage) -> Result<Vec<u8>> {
        // Try Swift helper (tools/vision_person.swift compiled to vision-person).
        // Works on macOS regardless of feature flag — feature just controls auto-compilation.
        if let Ok(mask) = try_vision_helper(image, self.quality) {
            return Ok(mask);
        }
        // Fallback path
        #[cfg(all(target_os = "macos", feature = "macos-vision"))]
        {
            tracing::debug!("Vision helper not found, would use objc2 Vision quality={:?} {}x{}", self.quality, image.width, image.height);
        }
        anyhow::bail!("Vision helper not available (compile tools/vision_person.swift to target/debug/vision-person)")
    }
}

impl PersonSegmenter for VisionPersonSegmenter {
    fn segment(&self, image: &DecodedImage) -> Result<Vec<u8>> {
        match self.try_vision(image) {
            Ok(m) => Ok(m),
            Err(e) => {
                tracing::debug!("Vision segmentation failed ({}), falling back to CPU", e);
                self.fallback.segment(image)
            }
        }
    }
    fn name(&self) -> &'static str { "apple-vision" }
}
