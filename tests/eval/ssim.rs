//! SSIM (Structural Similarity Index) math for visual comparison.
//!
//! Adapted from selkie (https://github.com/btucker/selkie), MIT license.
//! Originally `src/eval/ssim.rs`. Significant trimming and adaptation to
//! mermaid-little's fixture layout and feature set. SVG rasterisation and
//! end-to-end SSIM scoring of two raw SVGs is intentionally deferred: it
//! needs `resvg` + `image`, which are heavy dev-deps. When we're ready,
//! feature-gate it behind `eval-ssim` and call these helpers on the
//! decoded pixel buffers.
//!
//! Reference: Wang, Z., Bovik, A. C., Sheikh, H. R., & Simoncelli, E. P. (2004),
//! "Image quality assessment: from error visibility to structural similarity".

const K1: f64 = 0.01;
const K2: f64 = 0.03;
const L: f64 = 255.0;

/// Calculate SSIM between two grayscale images of equal length.
/// Returns a value in `[0, 1]` where 1 means identical.
pub fn calculate_ssim(img1: &[u8], img2: &[u8]) -> f64 {
    if img1.len() != img2.len() {
        return 0.0;
    }
    let n = img1.len() as f64;
    if n == 0.0 {
        return 1.0;
    }

    let mean1 = img1.iter().map(|&x| x as f64).sum::<f64>() / n;
    let mean2 = img2.iter().map(|&x| x as f64).sum::<f64>() / n;

    let mut var1 = 0.0;
    let mut var2 = 0.0;
    let mut covar = 0.0;
    for (&p1, &p2) in img1.iter().zip(img2.iter()) {
        let d1 = p1 as f64 - mean1;
        let d2 = p2 as f64 - mean2;
        var1 += d1 * d1;
        var2 += d2 * d2;
        covar += d1 * d2;
    }

    let denom_n = (n - 1.0).max(1.0);
    var1 /= denom_n;
    var2 /= denom_n;
    covar /= denom_n;

    let c1 = (K1 * L).powi(2);
    let c2 = (K2 * L).powi(2);

    let numerator = (2.0 * mean1 * mean2 + c1) * (2.0 * covar + c2);
    let denominator = (mean1.powi(2) + mean2.powi(2) + c1) * (var1 + var2 + c2);
    if denominator == 0.0 {
        return 1.0;
    }
    numerator / denominator
}

/// Convert an RGBA pixel buffer to grayscale via ITU-R BT.601 luma coefficients.
pub fn rgba_to_grayscale(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .map(|p| {
            let r = p[0] as f64;
            let g = p[1] as f64;
            let b = p[2] as f64;
            (0.299 * r + 0.587 * g + 0.114 * b) as u8
        })
        .collect()
}

/// Nearest-neighbour resize of a grayscale buffer.
pub fn resize_grayscale(
    src: &[u8],
    src_width: u32,
    src_height: u32,
    dst_width: u32,
    dst_height: u32,
) -> Vec<u8> {
    let mut dst = vec![0u8; (dst_width * dst_height) as usize];
    for y in 0..dst_height {
        for x in 0..dst_width {
            let src_x = (x as f64 * src_width as f64 / dst_width as f64) as u32;
            let src_y = (y as f64 * src_height as f64 / dst_height as f64) as u32;
            let src_idx = (src_y * src_width + src_x) as usize;
            let dst_idx = (y * dst_width + x) as usize;
            if src_idx < src.len() {
                dst[dst_idx] = src[src_idx];
            }
        }
    }
    dst
}

/// SSIM for two SVG strings. **Stubbed** — returns `Err(..)` until the
/// `eval-ssim` feature (pulling in `resvg` + `image`) is wired up.
pub fn ssim_svg(_candidate: &str, _reference: &str) -> Result<f64, &'static str> {
    Err("SSIM for raw SVGs is not implemented; rasterise externally and call `calculate_ssim`")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_images() {
        let img = vec![100u8; 100];
        let ssim = calculate_ssim(&img, &img);
        assert!((ssim - 1.0).abs() < 0.001);
    }

    #[test]
    fn completely_different() {
        let img1 = vec![0u8; 100];
        let img2 = vec![255u8; 100];
        let ssim = calculate_ssim(&img1, &img2);
        assert!(ssim < 0.1);
    }

    #[test]
    fn similar_images() {
        let img1: Vec<u8> = (0..100).map(|i| (i * 2) as u8).collect();
        let img2: Vec<u8> = (0..100).map(|i| (i * 2 + 5) as u8).collect();
        let ssim = calculate_ssim(&img1, &img2);
        assert!(ssim > 0.9);
    }

    #[test]
    fn rgba_to_grayscale_luma() {
        let rgba = vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255];
        let gray = rgba_to_grayscale(&rgba);
        assert_eq!(gray.len(), 3);
        assert!((gray[0] as i32 - 76).abs() < 2);
        assert!((gray[1] as i32 - 150).abs() < 2);
        assert!((gray[2] as i32 - 29).abs() < 2);
    }
}
