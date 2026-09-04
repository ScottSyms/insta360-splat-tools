/// Exposure quality 0-1 (1 = good exposure)
pub fn exposure_score(gray: &[u8]) -> f64 {
    if gray.is_empty() {
        return 0.0;
    }
    let total = gray.len() as f64;
    let under = gray.iter().filter(|&&v| v < 16).count() as f64 / total;
    let over = gray.iter().filter(|&&v| v > 235).count() as f64 / total;
    let clipped = under + over;
    // 0 clipped => 1.0, 10% clipped => 0.9, 50% clipped => 0.5
    (1.0 - clipped).clamp(0.0, 1.0)
}

pub fn histogram(gray: &[u8]) -> [u32; 256] {
    let mut hist = [0u32; 256];
    for &v in gray {
        hist[v as usize] += 1;
    }
    hist
}
