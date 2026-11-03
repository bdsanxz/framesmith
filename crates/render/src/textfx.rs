//! Split-text, typewriter, and count display values (Sprint 7 motion,
//! CAP-009).
//!
//! Pure, deterministic string math shared by frame descriptions and
//! render preparation: splitting a text value into individually
//! addressable pieces, revealing them across the clip window
//! (typewriter), and rendering the interpolated counted number. Motion
//! stagger and expressions address pieces by these same indexes; the
//! cascade timing itself belongs to the stagger task.

use notation::model::{SplitMode, TextClip};

/// Split a text value into its individually addressable pieces:
/// one entry per Unicode scalar value (`chars`), per whitespace run
/// (`words`), or per newline-separated row (`lines`). An empty value
/// splits into no pieces in every mode.
pub fn split_pieces(text: &str, mode: SplitMode) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    match mode {
        SplitMode::Chars => text.chars().map(|c| c.to_string()).collect(),
        SplitMode::Words => text.split_whitespace().map(str::to_string).collect(),
        SplitMode::Lines => text.split('\n').map(str::to_string).collect(),
    }
}

/// Join revealed pieces back into one display string, in the mode's
/// own separator: characters concatenate, words join with single
/// spaces, lines join with newlines.
pub fn join_pieces(pieces: &[String], mode: SplitMode) -> String {
    match mode {
        SplitMode::Chars => pieces.concat(),
        SplitMode::Words => pieces.join(" "),
        SplitMode::Lines => pieces.join("\n"),
    }
}

/// Number of individually addressable pieces of a text clip, for the
/// clip's own split mode; `None` when the clip carries no split.
pub fn piece_count(tc: &TextClip) -> Option<usize> {
    tc.split.map(|mode| split_pieces(&tc.value, mode).len())
}

/// Clamp a window progress to the unit interval; a non-positive
/// window counts as finished so a degenerate clip shows its end.
fn clamp_progress(t: f64, start: f64, duration: f64) -> f64 {
    if !duration.is_finite() || duration <= 0.0 {
        return 1.0;
    }
    ((t - start) / duration).clamp(0.0, 1.0)
}

/// The typewriter reveal at unit progress `p`: the first
/// `ceil(p * n)` pieces joined, clamped to all `n` pieces. Progress 0
/// shows nothing; progress 1 shows every piece.
pub fn typewriter_value(text: &str, mode: SplitMode, progress: f64) -> String {
    let pieces = split_pieces(text, mode);
    if pieces.is_empty() {
        return String::new();
    }
    let p = progress.clamp(0.0, 1.0);
    let shown = (p * pieces.len() as f64).ceil() as usize;
    let shown = shown.min(pieces.len());
    join_pieces(&pieces[..shown], mode)
}

/// The counted number at unit progress `p`: the linear interpolation
/// from `from` to `to` rendered with `decimals` decimal places.
/// Progress 0 shows `from` exactly; progress 1 shows `to` exactly.
pub fn count_value(from: f64, to: f64, decimals: u32, progress: f64) -> String {
    let p = progress.clamp(0.0, 1.0);
    let mut v = from + (to - from) * p;
    if v == 0.0 {
        v = 0.0;
    }
    format!("{v:.decimals$}", decimals = decimals as usize)
}

/// Whether a text clip shows a frame-varying string: a typewriter
/// reveal or a count range. Still clips keep their authored value.
pub fn is_dynamic(tc: &TextClip) -> bool {
    tc.typewriter || tc.count_to.is_some()
}

/// The visible string of a text clip at composition time `t`:
/// the interpolated counted number, the typewriter prefix across the
/// clip window, or the authored value. Pure in `t`, so every frame
/// time maps to exactly one string.
pub fn display_value(tc: &TextClip, t: f64) -> String {
    if let (Some(from), Some(to)) = (tc.count_from, tc.count_to) {
        let p = clamp_progress(t, tc.start, tc.duration);
        return count_value(from, to, tc.decimals, p);
    }
    if tc.typewriter {
        if let Some(mode) = tc.split {
            let p = clamp_progress(t, tc.start, tc.duration);
            return typewriter_value(&tc.value, mode, p);
        }
    }
    tc.value.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use notation::model::{Align, FontStyle, Rgb, TrackKind, Wrap};

    fn clip(value: &str, start: f64, duration: f64) -> TextClip {
        TextClip {
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            hidden: false,
            value: value.to_string(),
            start,
            duration,
            x: 0.0,
            y: 0.0,
            size: 24.0,
            color: Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            align: Align::Left,
            wrap: Wrap::Off,
            style: FontStyle::Regular,
            order: 0,
            line_height: None,
            letter_spacing: None,
            font_family: None,
            split: None,
            typewriter: false,
            count_from: None,
            count_to: None,
            decimals: 0,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
        }
    }

    #[test]
    fn each_split_mode_addresses_its_own_pieces() {
        assert_eq!(split_pieces("abc", SplitMode::Chars), vec!["a", "b", "c"]);
        assert_eq!(
            split_pieces("hi there you", SplitMode::Words),
            vec!["hi", "there", "you"]
        );
        assert_eq!(
            split_pieces("one\ntwo\nthree", SplitMode::Lines),
            vec!["one", "two", "three"]
        );
        assert!(split_pieces("", SplitMode::Chars).is_empty());
        assert!(split_pieces("", SplitMode::Words).is_empty());
        assert!(split_pieces("", SplitMode::Lines).is_empty());
    }

    #[test]
    fn typewriter_reveals_pieces_across_the_window() {
        assert_eq!(typewriter_value("abcd", SplitMode::Chars, 0.0), "");
        assert_eq!(typewriter_value("abcd", SplitMode::Chars, 0.25), "a");
        assert_eq!(typewriter_value("abcd", SplitMode::Chars, 0.5), "ab");
        assert_eq!(typewriter_value("abcd", SplitMode::Chars, 1.0), "abcd");
        assert_eq!(
            typewriter_value("hi there you", SplitMode::Words, 0.5),
            "hi there"
        );
        assert_eq!(
            typewriter_value("one\ntwo", SplitMode::Lines, 1.0),
            "one\ntwo"
        );
        assert_eq!(typewriter_value("", SplitMode::Chars, 1.0), "");
    }

    #[test]
    fn count_renders_the_interpolated_number_with_declared_decimals() {
        assert_eq!(count_value(0.0, 100.0, 0, 0.0), "0");
        assert_eq!(count_value(0.0, 100.0, 0, 0.5), "50");
        assert_eq!(count_value(0.0, 100.0, 0, 1.0), "100");
        assert_eq!(count_value(0.0, 1.0, 2, 0.5), "0.50");
        assert_eq!(count_value(10.0, 20.0, 1, 0.25), "12.5");
        assert_eq!(count_value(5.0, 5.0, 3, 0.7), "5.000");
    }

    #[test]
    fn display_value_routes_by_clip_effect() {
        let mut tw = clip("abcd", 0.0, 4.0);
        tw.split = Some(SplitMode::Chars);
        tw.typewriter = true;
        assert_eq!(display_value(&tw, 0.0), "");
        assert_eq!(display_value(&tw, 2.0), "ab");
        assert_eq!(display_value(&tw, 4.0), "abcd");
        assert_eq!(display_value(&tw, 9.0), "abcd");

        let mut count = clip("ignored", 1.0, 3.0);
        count.count_from = Some(0.0);
        count.count_to = Some(100.0);
        assert_eq!(display_value(&count, 0.0), "0");
        assert_eq!(display_value(&count, 2.5), "50");
        assert_eq!(display_value(&count, 4.0), "100");

        let still = clip("Hi", 0.0, 2.0);
        assert_eq!(display_value(&still, 1.0), "Hi");
        assert!(!is_dynamic(&still));
        assert!(is_dynamic(&tw));
        assert!(is_dynamic(&count));
    }
}
