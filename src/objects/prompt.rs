use anyhow::Result;
use crate::image::DecodedImage;

/// Tier 3 promptable segmentation — text prompt → grounding → SAM-style segmentation.
/// MVP stub returns empty.
pub trait PromptableSegmenter: Send + Sync {
    fn segment_prompt(&self, image: &DecodedImage, prompt: &str) -> Result<Vec<u8>>;
}

pub struct DummyPromptSegmenter;

impl PromptableSegmenter for DummyPromptSegmenter {
    fn segment_prompt(&self, image: &DecodedImage, _prompt: &str) -> Result<Vec<u8>> {
        Ok(vec![0u8; (image.width * image.height) as usize])
    }
}
