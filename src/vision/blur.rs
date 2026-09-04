/// Laplacian variance sharpness metric (§18.1)
/// Higher variance => sharper image.

pub fn laplacian_variance(gray: &[u8], width: u32, height: u32) -> f64 {
    if gray.len() != (width * height) as usize || width < 3 || height < 3 {
        return 0.0;
    }
    let w = width as usize;
    let h = height as usize;

    // Compute Laplacian per pixel (skip border)
    // Kernel [[0,1,0],[1,-4,1],[0,1,0]]
    let mut laplacians: Vec<f64> = Vec::with_capacity((w-2)*(h-2));
    for y in 1..h-1 {
        for x in 1..w-1 {
            let idx = y * w + x;
            let v = gray[idx] as i32;
            let up = gray[(y-1)*w + x] as i32;
            let down = gray[(y+1)*w + x] as i32;
            let left = gray[y*w + (x-1)] as i32;
            let right = gray[y*w + (x+1)] as i32;
            let lap = up + down + left + right - 4 * v;
            laplacians.push(lap as f64);
        }
    }

    if laplacians.is_empty() {
        return 0.0;
    }
    let mean = laplacians.iter().sum::<f64>() / laplacians.len() as f64;
    let var = laplacians.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / laplacians.len() as f64;
    var
}

/// Normalize variance to 0-1 blur score (1 = sharp)
pub fn blur_score_from_variance(var: f64, threshold: f64) -> f64 {
    // Threshold is variance that maps to ~0.5 score
    // Use sigmoid-ish: score = clamp(var / (var + threshold), 0,1) or simple linear
    // For now, linear normalized against threshold
    if var <= 0.0 {
        return 0.0;
    }
    // Map: var = threshold => 0.5, var = 2*threshold => 0.66, etc via var/(var+threshold)
    (var / (var + threshold)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharp_vs_blur() {
        // Create 16x16 checkerboard (sharp) vs uniform (blur)
        let w = 16; let h = 16;
        let mut sharp = vec![0u8; w*h];
        for y in 0..h { for x in 0..w { sharp[y*w+x] = if (x+y)%2==0 {0} else {255}; } }
        let blur = vec![128u8; w*h];
        let v_sharp = laplacian_variance(&sharp, w as u32, h as u32);
        let v_blur = laplacian_variance(&blur, w as u32, h as u32);
        assert!(v_sharp > v_blur * 10.0, "sharp {} blur {}", v_sharp, v_blur);
    }
}
