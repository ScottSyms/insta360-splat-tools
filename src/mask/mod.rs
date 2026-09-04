pub mod combine;
pub mod feather;
pub mod morphology;
pub mod static_mask;

pub use combine::combine_masks;
pub use morphology::{dilate, erode, close};
pub use static_mask::{StaticMask, StaticMaskConfig};

/// Invert mask: 0↔255 (for COLMAP where 0=masked vs our 255=exclude)
pub fn invert(mask: &[u8]) -> Vec<u8> {
    mask.iter().map(|&v| 255 - v).collect()
}
