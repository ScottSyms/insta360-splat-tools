/// Combine masks by union (OR). 0=retain, 255=exclude.
/// `masks` is slice of (mask, w, h) — all must share dimensions or will be resized to first.
/// `keep` masks are subtracted.
///
pub fn combine_masks(
    base_masks: &[Vec<u8>],
    keep_masks: &[Vec<u8>],
    width: u32,
    height: u32,
) -> Vec<u8> {
    let n = (width * height) as usize;
    let mut out = vec![0u8; n];
    for mask in base_masks {
        assert_eq!(mask.len(), n, "combine mask size mismatch");
        for i in 0..n {
            if mask[i] > 127 {
                out[i] = 255;
            }
        }
    }
    for keep in keep_masks {
        assert_eq!(keep.len(), n);
        for i in 0..n {
            if keep[i] > 127 {
                out[i] = 0;
            }
        }
    }
    out
}
