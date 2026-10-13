//! The picture format the encoder is given: 8-bit YUV 4:2:0 in planes, BT.709, limited range.
//! See `docs/spec/native-render-pipeline.md`, section 2.

/// The share of each of R, G and B in Y, Cb and Cr, times 65536.
const Y: [i32; 3] = [11_966, 40_254, 4_064];
const CB: [i32; 3] = [-6_596, -22_188, 28_784];
const CR: [i32; 3] = [28_784, -26_145, -2_639];

/// How many threads share one frame.
const THREADS: usize = 4;
/// A frame with fewer rows than this for each thread is not worth sharing out.
const ROWS_WORTH_A_THREAD: usize = 64;

/// The bytes of a `width` by `height` picture in this format.
pub fn size(width: usize, height: usize) -> usize {
    width * height + 2 * (width / 2) * (height / 2)
}

/// Whether a picture of this size can be 4:2:0: both sides even.
pub fn fits(width: usize, height: usize) -> bool {
    width > 0 && height > 0 && width.is_multiple_of(2) && height.is_multiple_of(2)
}

/// Turns RGBA pixels, top row first, into the three planes: all of Y, then Cb, then Cr.
/// `width` and `height` are even, and `out` holds `size(width, height)` bytes.
pub fn from_rgba(rgba: &[u8], width: usize, height: usize, out: &mut [u8]) {
    assert!(fits(width, height), "a 4:2:0 picture has even sides");
    assert_eq!(rgba.len(), width * height * 4);
    assert_eq!(out.len(), size(width, height));
    let across = width / 2;
    let (luma, chroma) = out.split_at_mut(width * height);
    let (cb, cr) = chroma.split_at_mut(across * (height / 2));
    // Each thread takes a band of whole row pairs.
    let threads = (height / ROWS_WORTH_A_THREAD).clamp(1, THREADS);
    let band = (height / 2).div_ceil(threads) * 2;
    let parts = rgba
        .chunks(band * width * 4)
        .zip(luma.chunks_mut(band * width))
        .zip(cb.chunks_mut(band / 2 * across).zip(cr.chunks_mut(band / 2 * across)));
    if threads == 1 {
        for ((rgba, luma), (cb, cr)) in parts {
            rows(rgba, width, luma, cb, cr);
        }
        return;
    }
    std::thread::scope(|scope| {
        for ((rgba, luma), (cb, cr)) in parts {
            scope.spawn(move || rows(rgba, width, luma, cb, cr));
        }
    });
}

/// A whole number of row pairs.
fn rows(rgba: &[u8], width: usize, luma: &mut [u8], cb: &mut [u8], cr: &mut [u8]) {
    for (pixel, y) in rgba.as_chunks::<4>().0.iter().zip(luma.iter_mut()) {
        let (r, g, b) = (i32::from(pixel[0]), i32::from(pixel[1]), i32::from(pixel[2]));
        *y = ((Y[0] * r + Y[1] * g + Y[2] * b + (16 << 16) + (1 << 15)) >> 16) as u8;
    }
    let across = width / 2;
    let pairs = rgba.chunks_exact(width * 8);
    for (pair, (cb, cr)) in pairs.zip(cb.chunks_exact_mut(across).zip(cr.chunks_exact_mut(across))) {
        let (top, bottom) = pair.split_at(width * 4);
        for (block, (cb, cr)) in cb.iter_mut().zip(cr.iter_mut()).enumerate() {
            // The sample stands at the block's left column: that column counts twice, the
            // columns beside it once, over both rows. Eight shares in all.
            let middle = block * 2;
            let (left, right) = (middle.saturating_sub(1), middle + 1);
            let sum = |channel: usize| {
                let of = |row: &[u8], column: usize| i32::from(row[column * 4 + channel]);
                of(top, left) + 2 * of(top, middle) + of(top, right) + of(bottom, left) + 2 * of(bottom, middle) + of(bottom, right)
            };
            let (r, g, b) = (sum(0), sum(1), sum(2));
            *cb = ((CB[0] * r + CB[1] * g + CB[2] * b + (128 << 19) + (1 << 18)) >> 19) as u8;
            *cr = ((CR[0] * r + CR[1] * g + CR[2] * b + (128 << 19) + (1 << 18)) >> 19) as u8;
        }
    }
}

/// What a pixel is in RGB, from its Y and from its Cb and Cr each times eight.
#[inline]
fn rgb(y: u8, cb: i32, cr: i32) -> [u8; 3] {
    let light = 76_309 * (i32::from(y) - 16) * 8 + (1 << 18);
    let (cb, cr) = (cb - 128 * 8, cr - 128 * 8);
    [
        ((light + 117_489 * cr) >> 19).clamp(0, 255) as u8,
        ((light - 13_975 * cb - 34_925 * cr) >> 19).clamp(0, 255) as u8,
        ((light + 138_438 * cb) >> 19).clamp(0, 255) as u8,
    ]
}

/// Turns the three planes of a `width` by `height` picture into opaque RGBA pixels, top row
/// first, on the thread that calls it. Chroma is spread between its samples, which stand as
/// `from_rgba` puts them. `finish` is given each row of pixels once it is made.
pub fn to_rgba(planes: &[u8], width: usize, height: usize, out: &mut [u8], finish: impl Fn(&mut [u8])) {
    assert!(fits(width, height), "a 4:2:0 picture has even sides");
    assert_eq!(planes.len(), size(width, height));
    assert_eq!(out.len(), width * height * 4);
    rows_to_rgba(planes, width, height, 0, out, &finish);
}

/// The rows of `out`, of which the first is row `first` of the picture.
fn rows_to_rgba(planes: &[u8], width: usize, height: usize, first: usize, out: &mut [u8], finish: &impl Fn(&mut [u8])) {
    let (across, down) = (width / 2, height / 2);
    let (luma, chroma) = planes.split_at(width * height);
    let (cb, cr) = chroma.split_at(across * down);
    // The chroma of one row of pixels, already spread between the two rows of samples it lies
    // between: three shares of the nearer, one of the farther.
    let mut between = vec![(0_i32, 0_i32); across];
    for (index, row) in out.chunks_exact_mut(width * 4).enumerate() {
        let y = first + index;
        let near = y / 2;
        let far = if y.is_multiple_of(2) {
            near.saturating_sub(1)
        } else {
            (near + 1).min(down - 1)
        };
        let (near, far) = (near * across, far * across);
        for (x, between) in between.iter_mut().enumerate() {
            *between = (
                3 * i32::from(cb[near + x]) + i32::from(cb[far + x]),
                3 * i32::from(cr[near + x]) + i32::from(cr[far + x]),
            );
        }
        let luma = &luma[y * width..(y + 1) * width];
        for (block, pixels) in row.as_chunks_mut::<8>().0.iter_mut().enumerate() {
            // The left pixel of a block stands on a sample; the right one half way to the next.
            let here = between[block];
            let next = between[(block + 1).min(across - 1)];
            let left = rgb(luma[block * 2], here.0 * 2, here.1 * 2);
            let right = rgb(luma[block * 2 + 1], here.0 + next.0, here.1 + next.1);
            *pixels = [left[0], left[1], left[2], 255, right[0], right[1], right[2], 255];
        }
        finish(row);
    }
}
