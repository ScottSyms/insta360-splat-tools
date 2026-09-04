/// Simple feather via box blur (separable). For MVP, approximate with dilate difference.
/// Full Gaussian feather can be added later with `imageproc` or Accelerate.

pub fn feather(mask: &[u8], w: u32, h: u32, radius: u32) -> Vec<u8> {
    if radius == 0 {
        return mask.to_vec();
    }
    // For now, just dilate and blend edge — keep binary for photogrammetry masks.
    // Future: produce soft 0-255 gradient.
    crate::mask::morphology::dilate(mask, w, h, radius / 2)
}
