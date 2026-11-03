//! Stagger origins and per-piece delays (Sprint 7 motion, CAP-002):
//! a `stagger` amount delays each animated piece by the amount times
//! its order index from the `stagger-from` origin (`start`, `center`,
//! or `end`; 2D grids via `stagger-grid` columns). Pure in the piece
//! index, so every frame maps each piece to exactly one delay.

/// Where piece delays are measured from: the first piece, the middle,
/// or the last piece of the layout order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Start,
    Center,
    End,
}

impl Origin {
    /// Parse a `stagger-from` value; anything else is a validation
    /// error, never a guessed origin.
    pub fn parse(s: &str) -> Option<Origin> {
        match s {
            "start" => Some(Origin::Start),
            "center" => Some(Origin::Center),
            "end" => Some(Origin::End),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Origin::Start => "start",
            Origin::Center => "center",
            Origin::End => "end",
        }
    }

    /// Accepted spellings, for diagnostics.
    pub const ACCEPTED: [&'static str; 3] = ["start", "center", "end"];
}

/// The 1D order rank of piece `index` (0-based) among `count` pieces
/// from `origin`: `start` counts up from the first piece, `end`
/// counts up from the last, `center` measures the distance from the
/// middle (half ranks when the count is even). A single piece always
/// ranks 0, so unstaggered and single-piece motion never shifts.
pub fn rank(index: usize, count: usize, origin: Origin) -> f64 {
    if count <= 1 {
        return 0.0;
    }
    let i = index as f64;
    let n = count as f64;
    match origin {
        Origin::Start => i,
        Origin::End => (n - 1.0) - i,
        Origin::Center => (i - (n - 1.0) / 2.0).abs(),
    }
}

/// The 2D grid rank of piece `index` (row-major: `col = index % cols`,
/// `row = index / cols`) among `count` pieces laid in `cols` columns
/// and `rows` rows from `origin`. `start` and `end` count along the
/// row-major layout order; `center` measures the Manhattan distance
/// from the grid center, so delays spread across rows and columns.
pub fn grid_rank(index: usize, count: usize, cols: usize, rows: usize, origin: Origin) -> f64 {
    if count <= 1 {
        return 0.0;
    }
    let cols = cols.max(1) as f64;
    let rows = rows.max(1) as f64;
    let col = (index % cols as usize) as f64;
    let row = (index / cols as usize) as f64;
    match origin {
        Origin::Start => row * cols + col,
        Origin::End => (count as f64 - 1.0) - (row * cols + col),
        Origin::Center => (col - (cols - 1.0) / 2.0).abs() + (row - (rows - 1.0) / 2.0).abs(),
    }
}

/// Rows a `count`-piece grid needs at `cols` columns, so every piece
/// has a row even when the last row is short.
pub fn grid_rows(count: usize, cols: usize) -> usize {
    if count == 0 {
        return 0;
    }
    count.div_ceil(cols.max(1))
}

/// The delay of piece `index` among `count` pieces: `stagger` seconds
/// times its rank from `origin` (1D layout). Non-positive counts and
/// out-of-range indexes delay nothing, never panic.
pub fn delay(index: usize, count: usize, stagger: f64, origin: Origin) -> f64 {
    if count == 0 || index >= count || !(stagger > 0.0) || !stagger.is_finite() {
        return 0.0;
    }
    stagger * rank(index, count, origin)
}

/// The delay of piece `index` among `count` pieces in a grid of
/// `cols` columns (rows derived): `stagger` seconds times its grid
/// rank from `origin`.
pub fn grid_delay(index: usize, count: usize, stagger: f64, origin: Origin, cols: usize) -> f64 {
    grid_delay_rows(index, count, stagger, origin, cols, grid_rows(count, cols))
}

/// The delay of piece `index` in a grid of `cols` columns and `rows`
/// rows: the declared-rows form of [`grid_delay`]. Fewer declared
/// rows than needed still place every piece; the grid never drops
/// one silently.
pub fn grid_delay_rows(
    index: usize,
    count: usize,
    stagger: f64,
    origin: Origin,
    cols: usize,
    declared_rows: usize,
) -> f64 {
    if count == 0 || index >= count || !(stagger > 0.0) || !stagger.is_finite() {
        return 0.0;
    }
    let rows = declared_rows.max(grid_rows(count, cols.max(1))).max(1);
    stagger * grid_rank(index, count, cols.max(1), rows, origin)
}

/// The largest delay any piece takes: `stagger` times the largest
/// rank, so overflow checks need only one number per directive.
pub fn max_delay(count: usize, stagger: f64, origin: Origin) -> f64 {
    max_delay_grid(count, stagger, origin, None)
}

/// The largest delay any piece takes in a grid (`None` for the 1D
/// layout, `Some((cols, rows))` for grids with declared rows).
pub fn max_delay_grid(
    count: usize,
    stagger: f64,
    origin: Origin,
    grid: Option<(usize, usize)>,
) -> f64 {
    if count == 0 || !(stagger > 0.0) || !stagger.is_finite() {
        return 0.0;
    }
    let top = match grid {
        None => (0..count)
            .map(|i| rank(i, count, origin))
            .fold(0.0f64, f64::max),
        Some((cols, declared_rows)) => {
            let cols = cols.max(1);
            let rows = declared_rows.max(grid_rows(count, cols)).max(1);
            (0..count)
                .map(|i| grid_rank(i, count, cols, rows, origin))
                .fold(0.0f64, f64::max)
        }
    };
    stagger * top
}

/// One pass of a repeated motion (Sprint 7 motion, CAP-004): the pass
/// offset from the motion start and whether the pass runs reversed
/// (`yoyo` on alternate passes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pass {
    /// Seconds from the motion start to this pass start.
    pub offset: f64,
    /// True when the pass plays its values backward (exact time
    /// reversal through the same easing, not a swapped-curve
    /// approximation).
    pub reversed: bool,
}

/// The passes of a repeated motion: `repeat` passes of `duration`
/// seconds separated by `gap` seconds of held end values. Pass `p`
/// starts at `p * (duration + gap)`; with `yoyo`, odd passes run
/// reversed. A single pass is never reversed, so unrepeated motion
/// evaluates exactly as before.
pub fn passes(duration: f64, repeat: u32, gap: f64, yoyo: bool) -> Vec<Pass> {
    let repeat = repeat.max(1) as usize;
    let gap = if gap.is_finite() && gap > 0.0 {
        gap
    } else {
        0.0
    };
    (0..repeat)
        .map(|p| Pass {
            offset: p as f64 * (duration + gap),
            reversed: yoyo && p % 2 == 1,
        })
        .collect()
}

/// The latest animated instant of a repeated motion starting at
/// `start`: the last pass end (gaps sit between passes, never after
/// the last one).
pub fn repeat_end(start: f64, duration: f64, repeat: u32, gap: f64) -> f64 {
    match passes(duration, repeat, gap, false).last() {
        Some(pass) => start + pass.offset + duration,
        None => start + duration,
    }
}
