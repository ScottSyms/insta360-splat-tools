use image::imageops::FilterType;

pub fn resize_image(data: &[u8], w: u32, h: u32, max_dim: u32) -> (Vec<u8>, u32, u32, f32) {
    if w <= max_dim && h <= max_dim {
        return (data.to_vec(), w, h, 1.0);
    }
    let scale = max_dim as f32 / w.max(h) as f32;
    let nw = (w as f32 * scale).round() as u32;
    let nh = (h as f32 * scale).round() as u32;
    let img = image::RgbImage::from_raw(w, h, data.to_vec()).unwrap();
    let resized = image::imageops::resize(&img, nw, nh, FilterType::Triangle);
    (resized.into_raw(), nw, nh, scale)
}

pub fn resize_mask(mask: &[u8], w: u32, h: u32, target_w: u32, target_h: u32) -> Vec<u8> {
    if w == target_w && h == target_h {
        return mask.to_vec();
    }
    let img = image::GrayImage::from_raw(w, h, mask.to_vec()).unwrap();
    let resized = image::imageops::resize(&img, target_w, target_h, FilterType::Nearest);
    resized.into_raw()
}
