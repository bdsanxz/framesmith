//! On-canvas sound drawing (Sprint 10 sound, CAP-010).
//!
//! A clip with `waveform="true"` draws its sound shape within its
//! resolved drawn range (defaulting to the clip window): the range
//! maps across the canvas width, each column's height names the peak
//! of the clip's placed contribution there, and the static shape
//! shows on every frame whose time falls in the range. Drawing is a
//! pure pixel overwrite with one fixed ink, so re-renders stay
//! byte-identical on any worker count or backend.

/// Waveform ink in canvas RGB (Sprint 10 sound, CAP-010): a fixed
/// spring green overwriting the pixels it covers, so drawn shapes
/// assert as exact bytes.
pub const WAVEFORM_INK: [u8; 3] = [0, 255, 128];

/// One drawn sound shape (Sprint 10 sound, CAP-010): the clip's peak
/// per canvas column across its drawn range.
#[derive(Debug, Clone)]
pub struct WaveformOverlay {
    /// Drawn range start in composition seconds (inclusive).
    pub from: f64,
    /// Drawn range end in composition seconds (exclusive).
    pub to: f64,
    /// Peak (0..1) of the clip contribution per canvas column, across
    /// the drawn range. `columns.len()` is the canvas width.
    pub columns: Vec<f32>,
}

/// Peak per canvas column of one stereo `f32` contribution slice
/// (Sprint 10 sound, CAP-010): column `i` of `width` spans frames
/// `[i*F/width, (i+1)*F/width)` and peaks the louder channel there,
/// clamped to 0..1. A zero width draws no columns.
pub fn columns_for_contribution(slice: &[f32], width: usize) -> Vec<f32> {
    const CHANNELS: usize = 2;
    if width == 0 {
        return Vec::new();
    }
    let frames = slice.len() / CHANNELS;
    let mut out = vec![0.0f32; width];
    if frames == 0 {
        return out;
    }
    for (x, slot) in out.iter_mut().enumerate() {
        let a = (x * frames / width).min(frames);
        let b = ((x + 1) * frames / width).min(frames);
        let mut peak = 0.0f32;
        for f in a..b {
            let l = slice[f * CHANNELS].abs();
            let r = slice[f * CHANNELS + 1].abs();
            peak = peak.max(l).max(r);
        }
        *slot = peak.clamp(0.0, 1.0);
    }
    out
}

/// Draw every in-range sound shape onto one RGB24 frame (Sprint 10
/// sound, CAP-010): each covering overlay paints its columns as
/// vertical bars centered on the canvas middle in [`WAVEFORM_INK`],
/// overwriting the pixels underneath. Frames outside every drawn
/// range and mismatched buffers pass through untouched.
pub fn draw_waveform_overlay(
    frame: &mut [u8],
    width: u32,
    height: u32,
    t: f64,
    overlays: &[WaveformOverlay],
) {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || frame.len() != w * h * 3 {
        return;
    }
    if !t.is_finite() {
        return;
    }
    let mid = h as f64 / 2.0;
    for over in overlays {
        if !(over.from <= t && t < over.to) || over.columns.len() != w {
            continue;
        }
        for (x, peak) in over.columns.iter().enumerate() {
            let half = (*peak as f64).clamp(0.0, 1.0) * mid;
            if half <= 0.0 {
                continue;
            }
            let y_lo = (mid - half).floor().max(0.0) as usize;
            let y_hi = (mid + half).ceil().min(h as f64) as usize;
            for y in y_lo.min(h)..y_hi.min(h) {
                let at = (y * w + x) * 3;
                frame[at] = WAVEFORM_INK[0];
                frame[at + 1] = WAVEFORM_INK[1];
                frame[at + 2] = WAVEFORM_INK[2];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stereo(levels: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity(levels.len() * 2);
        for l in levels {
            out.push(*l);
            out.push(*l * 0.5);
        }
        out
    }

    #[test]
    fn columns_peak_the_louder_channel_per_span() {
        // Eight frames over four columns: column peaks are the max of
        // either channel in each two-frame span.
        let cols = columns_for_contribution(&stereo(&[0.1, 0.0, 0.0, 0.5, -0.9, 0.0, 0.0, 0.2]), 4);
        assert_eq!(cols.len(), 4);
        assert!((cols[0] - 0.1).abs() < 1e-6);
        assert!((cols[1] - 0.5).abs() < 1e-6);
        assert!((cols[2] - 0.9).abs() < 1e-6);
        assert!((cols[3] - 0.2).abs() < 1e-6);
        // Silence and empty inputs peak zero; zero width draws nothing.
        assert_eq!(
            columns_for_contribution(&stereo(&[0.0; 8]), 4),
            vec![0.0; 4]
        );
        assert_eq!(columns_for_contribution(&[], 4), vec![0.0; 4]);
        assert!(columns_for_contribution(&stereo(&[0.5; 8]), 0).is_empty());
        // Hotter-than-full-scale contributions clamp to 1.
        assert_eq!(
            columns_for_contribution(&stereo(&[2.0; 4]), 2),
            vec![1.0; 2]
        );
    }

    #[test]
    fn overlay_paints_centered_bars_only_in_range() {
        // 4x4 canvas, one column at half peak: mid=2, half=1, rows 1..3.
        let over = WaveformOverlay {
            from: 1.0,
            to: 3.0,
            columns: vec![0.5, 0.0, 1.0, 0.25],
        };
        let paint = |t: f64| {
            let mut frame = vec![0u8; 4 * 4 * 3];
            draw_waveform_overlay(&mut frame, 4, 4, t, std::slice::from_ref(&over));
            frame
        };
        let ink = |f: &[u8], x: usize, y: usize| -> bool {
            let at = (y * 4 + x) * 3;
            f[at..at + 3] == WAVEFORM_INK
        };
        // Outside the range the frame passes through untouched.
        assert!(paint(0.5).iter().all(|b| *b == 0));
        assert!(paint(3.0).iter().all(|b| *b == 0));
        let hit = paint(2.0);
        // Column 0 (half peak) inks rows 1..3 only.
        assert!(!ink(&hit, 0, 0));
        assert!(ink(&hit, 0, 1));
        assert!(ink(&hit, 0, 2));
        assert!(!ink(&hit, 0, 3));
        // Column 1 (silence) inks nothing; column 2 (full) inks all.
        assert!((0..4).all(|y| !ink(&hit, 1, y)));
        assert!((0..4).all(|y| ink(&hit, 2, y)));
        // Column 3 (quarter peak): half=0.5, rows 1..3.
        assert!(!ink(&hit, 3, 0));
        assert!(ink(&hit, 3, 1));
        assert!(ink(&hit, 3, 2));
        assert!(!ink(&hit, 3, 3));
        // Mismatched buffers and degenerate canvases stay untouched.
        let mut short = vec![0u8; 10];
        draw_waveform_overlay(&mut short, 4, 4, 2.0, std::slice::from_ref(&over));
        assert!(short.iter().all(|b| *b == 0));
    }
}
