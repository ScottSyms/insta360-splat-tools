use image::GrayImage;
use imageproc::distance_transform::Norm;
use imageproc::morphology::{dilate as ip_dilate, erode as ip_erode};

/// Morphology on 8-bit masks (0=retain, 255=exclude) using `imageproc` for O(n) speed.
/// Slightly conservative dilation is preferred for reconstruction (§15).

pub fn dilate(mask: &[u8], w: u32, h: u32, radius: u32) -> Vec<u8> {
    if radius == 0 {
        return mask.to_vec();
    }
    let img = GrayImage::from_raw(w, h, mask.to_vec()).unwrap();
    let k = radius.min(255) as u8;
    ip_dilate(&img, Norm::LInf, k).into_raw()
}

pub fn erode(mask: &[u8], w: u32, h: u32, radius: u32) -> Vec<u8> {
    if radius == 0 {
        return mask.to_vec();
    }
    let img = GrayImage::from_raw(w, h, mask.to_vec()).unwrap();
    let k = radius.min(255) as u8;
    ip_erode(&img, Norm::LInf, k).into_raw()
}

pub fn close(mask: &[u8], w: u32, h: u32, radius: u32) -> Vec<u8> {
    let d = dilate(mask, w, h, radius);
    erode(&d, w, h, radius)
}
