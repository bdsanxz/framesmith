//! Picture similarity (SSIM) between two frames of the same size.

use crate::Frame;

const WINDOW: u32 = 8;
const STEP: u32 = 4;

fn luma(frame: &Frame) -> Vec<f64> {
    frame
        .rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|&[r, g, b]| 0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b))
        .collect()
}

/// The mean structural similarity of the two frames' brightness, over
/// 8x8 windows stepped by 4 pixels. 1 means identical. `None` when the
/// sizes differ or the frames are smaller than one window.
pub fn similarity(first: &Frame, second: &Frame) -> Option<f64> {
    if (first.width, first.height) != (second.width, second.height) || first.width < WINDOW || first.height < WINDOW {
        return None;
    }
    let (a, b) = (luma(first), luma(second));
    let (c1, c2) = ((0.01f64 * 255.0).powi(2), (0.03f64 * 255.0).powi(2));
    let count = f64::from(WINDOW * WINDOW);
    let (mut total, mut windows) = (0.0, 0u64);
    let mut top = 0;
    while top + WINDOW <= first.height {
        let mut left = 0;
        while left + WINDOW <= first.width {
            let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for y in top..top + WINDOW {
                let row = (y * first.width) as usize;
                for x in left..left + WINDOW {
                    let (pa, pb) = (a[row + x as usize], b[row + x as usize]);
                    sa += pa;
                    sb += pb;
                    saa += pa * pa;
                    sbb += pb * pb;
                    sab += pa * pb;
                }
            }
            let (ma, mb) = (sa / count, sb / count);
            let (va, vb, cov) = (saa / count - ma * ma, sbb / count - mb * mb, sab / count - ma * mb);
            total += ((2.0 * ma * mb + c1) * (2.0 * cov + c2)) / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            windows += 1;
            left += STEP;
        }
        top += STEP;
    }
    Some(total / windows as f64)
}
