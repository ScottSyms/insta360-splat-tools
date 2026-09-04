use anyhow::{Context, Result};
use std::path::Path;

pub fn write_mask_png(mask: &[u8], width: u32, height: u32, out: &Path) -> Result<()> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create dir {}", parent.display()))?;
    }
    let img = image::GrayImage::from_raw(width, height, mask.to_vec())
        .ok_or_else(|| anyhow::anyhow!("invalid mask dimensions {width}x{height} len {}", mask.len()))?;
    img.save(out).with_context(|| format!("save mask {}", out.display()))?;
    Ok(())
}

pub fn write_clean_image(image: &crate::image::DecodedImage, mask: Option<&[u8]>, out: &Path, mode: &str) -> Result<()> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut buf = image.data.clone();
    match mode {
        "transparent" => {
            // Not applicable for JPEG; save as PNG with alpha
            let mut rgba = Vec::with_capacity((image.width * image.height * 4) as usize);
            for (i, px) in image.data.chunks_exact(3).enumerate() {
                let alpha = if let Some(m) = mask { if m[i] > 127 { 0 } else { 255 } } else { 255 };
                rgba.extend_from_slice(&[px[0], px[1], px[2], alpha]);
            }
            let img = image::RgbaImage::from_raw(image.width, image.height, rgba).unwrap();
            img.save(out)?;
        }
        "solid" => {
            // e.g. magenta for masked
            if let Some(m) = mask {
                for (i, px) in buf.chunks_exact_mut(3).enumerate() {
                    if m[i] > 127 {
                        px[0] = 255; px[1] = 0; px[2] = 255;
                    }
                }
            }
            let img = image::RgbImage::from_raw(image.width, image.height, buf).unwrap();
            img.save(out)?;
        }
        "blur" | "inpaint" | _ => {
            // For MVP, solid is fallback
            if let Some(m) = mask {
                for (i, px) in buf.chunks_exact_mut(3).enumerate() {
                    if m[i] > 127 {
                        px[0] = 128; px[1] = 128; px[2] = 128;
                    }
                }
            }
            let img = image::RgbImage::from_raw(image.width, image.height, buf).unwrap();
            img.save(out)?;
        }
    }
    Ok(())
}
