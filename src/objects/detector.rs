use anyhow::Result;
use crate::image::DecodedImage;

#[derive(Debug, Clone)]
pub struct Detection {
    pub label: String,
    pub bbox: [u32; 4], // x,y,w,h
    pub score: f32,
}

pub trait Detector: Send + Sync {
    fn detect(&self, image: &DecodedImage, classes: &[String]) -> Result<Vec<Detection>>;
}

/// Tier 2 placeholder — returns no detections. Replace with compact YOLO / RT-DETR Core ML model.
pub struct DummyDetector;

impl Detector for DummyDetector {
    fn detect(&self, _image: &DecodedImage, _classes: &[String]) -> Result<Vec<Detection>> {
        Ok(Vec::new())
    }
}
