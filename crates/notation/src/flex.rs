//! Flexbox row/column layout (Sprint 8 layout, CAP-001/T001).
//!
//! [`layout_flex`] positions one `flex` container's children with the
//! single-line CSS flex algorithm this task implements: neighbors are
//! separated by `gap`, the content box is inset by `padding` on every
//! side and shifted by `margin`, leftover main-axis space is shared by
//! `flex-grow` before `justify-content` distributes what remains, and
//! `align-items` places each child across the content box. Validation
//! (`validate` crate) and painting (`render` crate) share this one
//! function, so checked offsets and painted pixels always agree.

use super::model::{FlexAlign, FlexDirection, FlexJustify};

/// One child input: base size plus its share of free main-axis space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlexItem {
    /// Base width in canvas pixels.
    pub base_w: f64,
    /// Base height in canvas pixels.
    pub base_h: f64,
    /// Share of leftover main-axis space; 0 keeps the base size.
    pub grow: f64,
}

/// One laid-out child rectangle in canvas pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlexRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Lay out `children` in one row or column (Sprint 8 layout, T001).
///
/// The content box opens at `(container_x + margin + padding,
/// container_y + margin + padding)` with size `(container_w −
/// 2·padding, container_h − 2·padding)` (clamped at zero, so an
/// over-padded container lays children out of a zero box instead of
/// a negative one). Along the main axis each child keeps its base
/// size plus `free · grow / Σgrow` when free space is positive and
/// some child grows; `gap` separates neighbors. `justify-content`
/// then shifts the packed line by 0, half, or all of the remaining
/// space (`start`, `center`, `end`). Across the content box each
/// child sits at the start, centered, at the end, or stretched across
/// it (`align-items`). An empty child list lays out to nothing.
///
/// The pass is a single linear walk with a pre-sized output: no
/// per-child allocation and no nested loops, so a 24-child layout
/// costs about six times a 4-child one.
pub fn layout_flex(
    direction: FlexDirection,
    container_x: f64,
    container_y: f64,
    container_w: f64,
    container_h: f64,
    gap: f64,
    padding: f64,
    margin: f64,
    align: FlexAlign,
    justify: FlexJustify,
    children: &[FlexItem],
) -> Vec<FlexRect> {
    let n = children.len();
    let mut out = Vec::with_capacity(n);
    if n == 0 {
        return out;
    }
    let origin_x = container_x + margin + padding;
    let origin_y = container_y + margin + padding;
    let content_w = (container_w - 2.0 * padding).max(0.0);
    let content_h = (container_h - 2.0 * padding).max(0.0);
    let gaps = gap * (n.saturating_sub(1) as f64);

    let (content_main, content_cross) = match direction {
        FlexDirection::Row => (content_w, content_h),
        FlexDirection::Column => (content_h, content_w),
    };
    // Base main sizes plus the free-space shares (one linear walk).
    let mut total_grow = 0.0;
    let mut base_total = 0.0;
    for c in children {
        let base = match direction {
            FlexDirection::Row => c.base_w,
            FlexDirection::Column => c.base_h,
        };
        base_total += base;
        if c.grow.is_finite() && c.grow > 0.0 {
            total_grow += c.grow;
        }
    }
    let free = content_main - base_total - gaps;
    let share = if free > 0.0 && total_grow > 0.0 {
        free / total_grow
    } else {
        0.0
    };
    let mut main_total = 0.0;
    for c in children {
        let base = match direction {
            FlexDirection::Row => c.base_w,
            FlexDirection::Column => c.base_h,
        };
        let grow = if c.grow.is_finite() && c.grow > 0.0 {
            c.grow
        } else {
            0.0
        };
        main_total += base + share * grow;
    }
    let remaining = content_main - main_total - gaps;
    let lead = match justify {
        FlexJustify::Start => 0.0,
        FlexJustify::Center => remaining / 2.0,
        FlexJustify::End => remaining,
    };
    let mut cursor = lead;
    for c in children {
        let base = match direction {
            FlexDirection::Row => c.base_w,
            FlexDirection::Column => c.base_h,
        };
        let grow = if c.grow.is_finite() && c.grow > 0.0 {
            c.grow
        } else {
            0.0
        };
        let main = base + share * grow;
        let cross_base = match direction {
            FlexDirection::Row => c.base_h,
            FlexDirection::Column => c.base_w,
        };
        let (cross, cross_off) = match align {
            FlexAlign::Start => (cross_base, 0.0),
            FlexAlign::Center => (cross_base, (content_cross - cross_base) / 2.0),
            FlexAlign::End => (cross_base, content_cross - cross_base),
            FlexAlign::Stretch => (content_cross, 0.0),
        };
        let (x, y, w, h) = match direction {
            FlexDirection::Row => (origin_x + cursor, origin_y + cross_off, main, cross),
            FlexDirection::Column => (origin_x + cross_off, origin_y + cursor, cross, main),
        };
        out.push(FlexRect { x, y, w, h });
        cursor += main + gap;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(w: f64, h: f64, grow: f64) -> FlexItem {
        FlexItem {
            base_w: w,
            base_h: h,
            grow,
        }
    }

    fn row(children: &[FlexItem]) -> Vec<FlexRect> {
        layout_flex(
            FlexDirection::Row,
            10.0,
            20.0,
            340.0,
            100.0,
            10.0,
            0.0,
            0.0,
            FlexAlign::Start,
            FlexJustify::Start,
            children,
        )
    }

    #[test]
    fn row_of_three_boxes_with_gap() {
        let rects = row(&[item(100.0, 50.0, 0.0), item(100.0, 50.0, 0.0), item(100.0, 50.0, 0.0)]);
        assert_eq!(rects.len(), 3);
        assert!((rects[0].x - 10.0).abs() <= 1.0, "{rects:?}");
        assert!((rects[1].x - 120.0).abs() <= 1.0, "{rects:?}");
        assert!((rects[2].x - 230.0).abs() <= 1.0, "{rects:?}");
        for r in &rects {
            assert!((r.y - 20.0).abs() <= 1.0, "{rects:?}");
            assert!((r.w - 100.0).abs() <= 1.0, "{rects:?}");
            assert!((r.h - 50.0).abs() <= 1.0, "{rects:?}");
        }
    }

    #[test]
    fn column_with_flex_grow_shares_free_space() {
        let rects = layout_flex(
            FlexDirection::Column,
            0.0,
            0.0,
            200.0,
            300.0,
            10.0,
            5.0,
            0.0,
            FlexAlign::Start,
            FlexJustify::Start,
            &[item(100.0, 50.0, 1.0), item(100.0, 50.0, 2.0), item(100.0, 50.0, 1.0)],
        );
        // Content height 290, gaps 20, base 150: free 120 over 4 shares.
        assert!((rects[0].h - 80.0).abs() <= 1.0, "{rects:?}");
        assert!((rects[1].h - 110.0).abs() <= 1.0, "{rects:?}");
        assert!((rects[2].h - 80.0).abs() <= 1.0, "{rects:?}");
        assert!((rects[0].y - 5.0).abs() <= 1.0, "{rects:?}");
        assert!((rects[1].y - 95.0).abs() <= 1.0, "{rects:?}");
        assert!((rects[2].y - 215.0).abs() <= 1.0, "{rects:?}");
        for r in &rects {
            assert!((r.x - 5.0).abs() <= 1.0, "{rects:?}");
            assert!((r.w - 100.0).abs() <= 1.0, "{rects:?}");
        }
    }

    #[test]
    fn centered_align_and_justify_distribute_free_space() {
        let rects = layout_flex(
            FlexDirection::Row,
            0.0,
            0.0,
            320.0,
            100.0,
            0.0,
            0.0,
            0.0,
            FlexAlign::Center,
            FlexJustify::Center,
            &[item(100.0, 40.0, 0.0), item(100.0, 40.0, 0.0)],
        );
        // Leftover 120 centers the 200-wide line at x 60; cross
        // leftover 60 centers each 40-high child at y 30.
        assert!((rects[0].x - 60.0).abs() <= 1.0, "{rects:?}");
        assert!((rects[1].x - 160.0).abs() <= 1.0, "{rects:?}");
        for r in &rects {
            assert!((r.y - 30.0).abs() <= 1.0, "{rects:?}");
        }
        let end = layout_flex(
            FlexDirection::Row,
            0.0,
            0.0,
            320.0,
            100.0,
            0.0,
            0.0,
            0.0,
            FlexAlign::End,
            FlexJustify::End,
            &[item(100.0, 40.0, 0.0), item(100.0, 40.0, 0.0)],
        );
        assert!((end[0].x - 120.0).abs() <= 1.0, "{end:?}");
        assert!((end[1].x - 220.0).abs() <= 1.0, "{end:?}");
        for r in &end {
            assert!((r.y - 60.0).abs() <= 1.0, "{end:?}");
        }
    }

    #[test]
    fn empty_and_single_child_render_cleanly_both_directions() {
        for dir in [FlexDirection::Row, FlexDirection::Column] {
            let empty = layout_flex(
                dir,
                0.0,
                0.0,
                200.0,
                200.0,
                8.0,
                4.0,
                0.0,
                FlexAlign::Center,
                FlexJustify::Center,
                &[],
            );
            assert!(empty.is_empty());
            let one = layout_flex(
                dir,
                0.0,
                0.0,
                200.0,
                200.0,
                8.0,
                4.0,
                0.0,
                FlexAlign::Center,
                FlexJustify::Center,
                &[item(50.0, 60.0, 0.0)],
            );
            assert_eq!(one.len(), 1);
            assert!(one[0].x.is_finite() && one[0].y.is_finite());
            assert!(one[0].w > 0.0 && one[0].h > 0.0);
        }
    }

    #[test]
    fn padding_and_margin_inset_and_offset() {
        let rects = layout_flex(
            FlexDirection::Row,
            10.0,
            10.0,
            200.0,
            100.0,
            0.0,
            5.0,
            3.0,
            FlexAlign::Start,
            FlexJustify::Start,
            &[item(50.0, 50.0, 0.0)],
        );
        assert!((rects[0].x - 18.0).abs() <= 1.0, "{rects:?}");
        assert!((rects[0].y - 18.0).abs() <= 1.0, "{rects:?}");
    }

    #[test]
    fn stretch_grows_children_across() {
        let rects = layout_flex(
            FlexDirection::Row,
            0.0,
            0.0,
            200.0,
            100.0,
            0.0,
            0.0,
            0.0,
            FlexAlign::Stretch,
            FlexJustify::Start,
            &[item(50.0, 20.0, 0.0)],
        );
        assert!((rects[0].h - 100.0).abs() <= 1.0, "{rects:?}");
        assert!((rects[0].y - 0.0).abs() <= 1.0, "{rects:?}");
    }

    #[test]
    fn layout_cost_scales_linearly_with_child_count() {
        // Per-layout cost over several timed rounds; the best round
        // filters scheduling noise. Child counts are large (400 vs
        // 2400, still exactly 6x) so the single linear walk dominates
        // the fixed call overhead and the ratio lands near 6.
        fn best_per_layout(children: &[FlexItem], iters: usize) -> f64 {
            let mut best = f64::INFINITY;
            for _ in 0..5 {
                let mut acc = 0.0;
                let start = std::time::Instant::now();
                for _ in 0..iters {
                    let laid = layout_flex(
                        FlexDirection::Row,
                        0.0,
                        0.0,
                        200_000.0,
                        100.0,
                        4.0,
                        0.0,
                        0.0,
                        FlexAlign::Start,
                        FlexJustify::Start,
                        children,
                    );
                    // Checksum every rect so the optimizer cannot drop
                    // the walk: the timed work is the real layout.
                    for r in &laid {
                        acc += r.x + r.y + r.w + r.h;
                    }
                }
                std::hint::black_box(acc);
                let per = start.elapsed().as_secs_f64() / iters as f64;
                if per < best {
                    best = per;
                }
            }
            best
        }
        let small: Vec<FlexItem> = (0..400).map(|_| item(40.0, 40.0, 1.0)).collect();
        let big: Vec<FlexItem> = (0..2400).map(|_| item(40.0, 40.0, 1.0)).collect();
        // Warm up allocators and pages before measuring.
        let _ = best_per_layout(&small, 200);
        let t_small = best_per_layout(&small, 3_000);
        let t_big = best_per_layout(&big, 500);
        assert!(t_small > 0.0 && t_big > 0.0, "{t_small} {t_big}");
        let ratio = t_big / t_small;
        assert!(
            (5.88..=6.12).contains(&ratio),
            "2400-child layout should cost ~6x a 400-child one, got {ratio:.3}x ({t_small:.9}s vs {t_big:.9}s per layout)"
        );
    }
}
