use anyhow::{Context, Result};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGB8
}

impl DecodedImage {
    pub fn from_path(path: &Path) -> Result<Self> {
        let img = image::open(path)
            .with_context(|| format!("failed to open image {}", path.display()))?;
        let rgb = img.to_rgb8();
        let (w, h) = (rgb.width(), rgb.height());
        Ok(Self {
            width: w,
            height: h,
            data: rgb.into_raw(),
        })
    }

    pub fn to_gray(&self) -> Vec<u8> {
        let mut gray = Vec::with_capacity((self.width * self.height) as usize);
        for chunk in self.data.chunks_exact(3) {
            let r = chunk[0] as f32;
            let g = chunk[1] as f32;
            let b = chunk[2] as f32;
            let lum = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
            gray.push(lum);
        }
        gray
    }

    pub fn to_luma_image(&self) -> image::GrayImage {
        let gray = self.to_gray();
        image::GrayImage::from_raw(self.width, self.height, gray).unwrap()
    }
}
