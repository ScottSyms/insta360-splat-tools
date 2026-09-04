use anyhow::Result;

pub trait RoiSegmenter: Send + Sync {
    fn segment_roi(&self, image: &crate::image::DecodedImage, bbox: [u32; 4]) -> Result<Vec<u8>>;
}

/// Tier 2 placeholder — segments only ROI via stub (returns bbox-filled mask).
pub struct DummyRoiSegmenter;

impl RoiSegmenter for DummyRoiSegmenter {
    fn segment_roi(&self, image: &crate::image::DecodedImage, bbox: [u32; 4]) -> Result<Vec<u8>> {
        let (w, h) = (image.width, image.height);
        let mut mask = vec![0u8; (w * h) as usize];
        let (x, y, bw, bh) = (bbox[0], bbox[1], bbox[2], bbox[3]);
        let pad = 2;
        let x0 = x.saturating_sub(pad);
        let y0 = y.saturating_sub(pad);
        let x1 = (x + bw + pad).min(w);
        let y1 = (y + bh + pad).min(h);
        for yy in y0..y1 {
            for xx in x0..x1 {
                mask[(yy * w + xx) as usize] = 255;
            }
        }
        Ok(mask)
    }
}
