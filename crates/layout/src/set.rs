//! Sets of columns (`docs/spec/render-command.md` section 4.4): the cells of a `<box>` with `columns`
//! laid out row by row, the first cell in the first column, the next in the second, and after the last
//! column a new row. The rules of section 4.3 for padding, margins, gaps and absolute cells apply as they
//! are stated there.

use super::{
    cap, lone_written, place_absolute, span, static_axes, written_size, Across, Beyond, Flowed, Measured, Rect, Sides, Size, Span, Written,
};

/// The width of one column of a set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Track {
    /// A width in pixels, above 0.
    Pixels(f64),
    /// A share of the room that the content box leaves after the pixel columns and the gaps, in proportion
    /// to the shares of all the columns. Above 0.
    Share(f64),
}

/// How a set of columns lays out its cells.
#[derive(Clone, Debug, PartialEq)]
pub struct Columns {
    /// The columns, from left to right. One column at least; with none the set has one column of no width.
    pub tracks: Vec<Track>,
    /// The space between two columns and between two rows, in pixels; none before the first or after the last.
    pub gap: f64,
    /// The space inside the set around its cells, in pixels. A written size of the set includes it.
    pub padding: Sides,
    /// Where each cell stands across its row, unless the cell has its own `align-self`.
    pub across: Across,
}

/// A cell of a set: what it writes, its margin, its own `align-self`, whether it is absolute, the size its own
/// items give it when it writes none, and its baseline from its top edge (that of its first line, or of its first
/// item, or `None` for its bottom edge).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cell {
    /// What the cell writes. A cell with a written `width` stands at the start edge of its column at that width.
    pub written: Written,
    /// The margin of the cell, in pixels. An absolute cell is moved in by it from each edge it is placed by
    /// (`render-command.md` section 4.1, rule 8).
    pub margin: Sides,
    /// The cell's own `align-self`, or `None` to take the set's `align-items`.
    pub across: Option<Across>,
    /// Whether the cell is `position="absolute"`: it takes no room and is placed against the set's outer rectangle.
    pub absolute: bool,
    /// The size the cell's own items, or its lines, give it, where it writes no size, or `None`.
    pub measured: Option<Size>,
    /// The baseline of the cell from its top edge, or `None` for its bottom edge.
    pub baseline: Option<f64>,
}

/// The widths of the columns and the heights of the rows of a set, worked out for its cells.
struct Grid {
    widths: Vec<f64>,
    rows: Vec<f64>,
}

/// The number of columns of a set: its tracks, or one when it has none.
fn count(set: &Columns) -> usize {
    set.tracks.len().max(1)
}

/// The length of the content box along one axis: the length less the two sides of the padding, and 0 or more.
fn content(length: u32, before: f64, after: f64) -> f64 {
    (f64::from(length) - before - after).max(0.0)
}

/// Places a set of columns in `parent` and its cells in it (`render-command.md` section 4.4). The set is placed as a
/// box is (section 4.1), with a written `width` or `height` that includes its padding, and an axis with no size of its
/// own is as large as its cells need: the pixel columns, the share columns at the widest of their cells, the gaps and
/// the padding.
pub fn columns(parent: Rect, written: &Written, set: &Columns, cells: &[Cell]) -> Result<Flowed, Beyond> {
    columns_absolute(parent, written, Sides::default(), set, cells)
}

/// Places a set of columns that is an absolutely placed item of a plain box, as [`columns`] does, with its own `margin`
/// moving it in from each edge it is placed by, or from its static position on an axis with no edge (section 4.1, rule
/// 8; `notation.md` N10).
pub fn columns_absolute(parent: Rect, written: &Written, margin: Sides, set: &Columns, cells: &[Cell]) -> Result<Flowed, Beyond> {
    let mut across = Span::across(parent, written);
    across.margins = (margin.left, margin.right);
    let mut down = Span::down(parent, written);
    down.margins = (margin.top, margin.bottom);
    let known_w = across.known().then(|| across.place(0.0)).transpose()?;
    let known_h = down.known().then(|| down.place(0.0)).transpose()?;
    let content_w = known_w.map(|(_, length)| content(length, set.padding.left, set.padding.right));
    let content_h = known_h.map(|(_, length)| content(length, set.padding.top, set.padding.bottom));
    let grid = grid(set, cells, content_w, content_h)?;
    let size = grid_size(set, &grid);
    let (left, width) = match known_w {
        Some(placed) => placed,
        None => across.place(size.width)?,
    };
    let (top, height) = match known_h {
        Some(placed) => placed,
        None => down.place(size.height)?,
    };
    let container = Rect { left, top, width, height };
    let (items, baseline) = place_cells(container, set, &grid, cells, content_w, content_h)?;
    Ok(Flowed {
        container,
        items,
        baseline,
    })
}

/// The size of a set of columns with no size of its own, worked out from its cells (section 4.4, rule 2).
pub fn measure_columns(set: &Columns, cells: &[Cell]) -> Result<Size, Beyond> {
    let grid = grid(set, cells, None, None)?;
    Ok(grid_size(set, &grid))
}

/// The cells of a set of columns placed in the rectangle `rect`, which is its outer rectangle (section 4.4). An axis on
/// which the set is `measured` has no size of its own for the percentages of its cells, which are then 0.
pub fn inside_columns(rect: Rect, measured: Measured, set: &Columns, cells: &[Cell]) -> Result<Flowed, Beyond> {
    let content_w = (!measured.width).then(|| content(rect.width, set.padding.left, set.padding.right));
    let content_h = (!measured.height).then(|| content(rect.height, set.padding.top, set.padding.bottom));
    let grid = grid(set, cells, content_w, content_h)?;
    let (items, baseline) = place_cells(rect, set, &grid, cells, content_w, content_h)?;
    Ok(Flowed {
        container: rect,
        items,
        baseline,
    })
}

/// The widths of the columns and the heights of the rows of a set (section 4.4, rules 1 to 3). A pixel column is its
/// width. A share column is its share of the room left after the pixel columns and the gaps, when the content box has a
/// width; without one, it is as wide as its widest cell with its margins. A row is as high as its tallest cell with its
/// margins; when the content box has a height and the rows need less, the room left over is shared equally among them.
fn grid(set: &Columns, cells: &[Cell], content_w: Option<f64>, content_h: Option<f64>) -> Result<Grid, Beyond> {
    let count = count(set);
    let placed: Vec<&Cell> = cells.iter().filter(|cell| !cell.absolute).collect();
    let gaps = set.gap * (count - 1) as f64;
    let fixed: f64 = set
        .tracks
        .iter()
        .map(|track| match track {
            Track::Pixels(pixels) => pixels.max(0.0),
            Track::Share(_) => 0.0,
        })
        .sum();
    let shares: f64 = set
        .tracks
        .iter()
        .map(|track| match track {
            Track::Share(share) => share.max(0.0),
            Track::Pixels(_) => 0.0,
        })
        .sum();

    let mut widths = Vec::with_capacity(count);
    for column in 0..count {
        let width = match set.tracks.get(column) {
            Some(Track::Pixels(pixels)) => pixels.max(0.0),
            Some(Track::Share(share)) => match content_w {
                Some(content) => {
                    let room = content - fixed - gaps;
                    if room > 0.0 && shares > 0.0 {
                        room * share.max(0.0) / shares
                    } else {
                        0.0
                    }
                }
                None => {
                    let mut widest = 0.0_f64;
                    for (index, cell) in placed.iter().enumerate() {
                        if index % count == column {
                            widest = widest.max(outer_width(cell, content_w)?);
                        }
                    }
                    widest
                }
            },
            None => 0.0,
        };
        widths.push(width);
    }

    let rows_count = placed.len().div_ceil(count);
    let mut rows = vec![0.0_f64; rows_count];
    for (index, cell) in placed.iter().enumerate() {
        let row = index / count;
        rows[row] = rows[row].max(outer_height(cell, content_h)?);
    }
    if let Some(content) = content_h {
        let needed = rows.iter().sum::<f64>() + set.gap * rows_count.saturating_sub(1) as f64;
        let left = content - needed;
        if left > 0.0 && rows_count > 0 {
            let share = left / rows_count as f64;
            for row in rows.iter_mut() {
                *row += share;
            }
        }
    }
    Ok(Grid { widths, rows })
}

/// The width a cell takes up in a column that is not fixed by the cells: its own width, with its margins.
fn outer_width(cell: &Cell, content_w: Option<f64>) -> Result<f64, Beyond> {
    Ok(own_width(cell, content_w)? + cell.margin.left + cell.margin.right)
}

/// The height a cell takes up in its row: its own height, with its margins.
fn outer_height(cell: &Cell, content_h: Option<f64>) -> Result<f64, Beyond> {
    Ok(own_height(cell, content_h)? + cell.margin.top + cell.margin.bottom)
}

/// The width a cell writes, or the width its own items give it, capped by its `max-width` when it writes none (rule 5).
/// A percentage is of the content box, and is 0 where the set has no size of its own there.
fn own_width(cell: &Cell, content_w: Option<f64>) -> Result<f64, Beyond> {
    match written_size(cell.written.width.clone(), content_w)? {
        Some(width) => Ok(width),
        None => Ok(cap(cell.measured.map_or(0.0, |size| size.width), cell.written.max_width)),
    }
}

/// The height a cell writes, or the height its own items give it. A percentage is of the content box.
fn own_height(cell: &Cell, content_h: Option<f64>) -> Result<f64, Beyond> {
    match written_size(cell.written.height.clone(), content_h)? {
        Some(height) => Ok(height),
        None => Ok(cell.measured.map_or(0.0, |size| size.height)),
    }
}

/// The size of a set worked out from its grid (rule 2): its columns and rows with the gaps between them, and the padding.
fn grid_size(set: &Columns, grid: &Grid) -> Size {
    let width =
        grid.widths.iter().sum::<f64>() + set.gap * grid.widths.len().saturating_sub(1) as f64 + set.padding.left + set.padding.right;
    let height = grid.rows.iter().sum::<f64>() + set.gap * grid.rows.len().saturating_sub(1) as f64 + set.padding.top + set.padding.bottom;
    Size { width, height }
}

/// The rectangles of the cells of a set placed in `container`, in the order of `cells` (section 4.4, rules 4 and 7).
/// A cell that is not absolute stands in its column and row: at its column's start edge, as wide as the column less its
/// margins unless it writes a width of its own; across its row as its `align-self` (or the set's `align-items`) says,
/// stretched to the row's height when it writes no height, and at its own height otherwise. A set has no baseline
/// alignment, so `baseline` stands as `flex-start`. Also the baseline of the first cell from the set's top, when it has one.
fn place_cells(
    container: Rect,
    set: &Columns,
    grid: &Grid,
    cells: &[Cell],
    content_w: Option<f64>,
    content_h: Option<f64>,
) -> Result<(Vec<Rect>, Option<f64>), Beyond> {
    let count = count(set);
    // The start of each column and each row, from the container's start edge: the padding, then the columns and rows
    // with the gaps between them.
    let mut column_starts = Vec::with_capacity(count);
    let mut from = set.padding.left;
    for width in &grid.widths {
        column_starts.push(from);
        from += width + set.gap;
    }
    let mut row_starts = Vec::with_capacity(grid.rows.len());
    let mut from_top = set.padding.top;
    for height in &grid.rows {
        row_starts.push(from_top);
        from_top += height + set.gap;
    }

    let mut rects = Vec::with_capacity(cells.len());
    let mut flow_index = 0_usize;
    for cell in cells {
        if cell.absolute {
            let placed = place_absolute(container, &cell.written, cell.margin)?;
            rects.push(static_axes(placed, &cell.written, || {
                let lone = Cell {
                    written: lone_written(placed),
                    absolute: false,
                    measured: None,
                    ..cell.clone()
                };
                let flowed = inside_columns(
                    container,
                    Measured {
                        width: false,
                        height: false,
                    },
                    set,
                    &[lone],
                )?;
                flowed.items.first().copied().ok_or(Beyond)
            })?);
            continue;
        }
        let column = flow_index % count;
        let row = flow_index / count;
        flow_index += 1;
        let margin = cell.margin;
        let column_width = grid.widths[column];
        let row_height = grid.rows[row];
        let width = match written_size(cell.written.width.clone(), content_w)? {
            Some(width) => width,
            None => cap((column_width - margin.left - margin.right).max(0.0), cell.written.max_width),
        };
        let across = match cell.across.unwrap_or(set.across) {
            Across::Baseline => Across::Start,
            across => across,
        };
        let own = own_height(cell, content_h)?;
        let row_top = row_starts[row];
        let (top, height) = match (across, cell.written.height.clone()) {
            (Across::Stretch, None) => (row_top + margin.top, (row_height - margin.top - margin.bottom).max(0.0)),
            (Across::Center, _) => (row_top + (row_height - (own + margin.top + margin.bottom)) / 2.0 + margin.top, own),
            (Across::End, _) => (row_top + row_height - margin.bottom - own, own),
            _ => (row_top + margin.top, own),
        };
        let left = column_starts[column] + margin.left;
        let (left, width) = span(f64::from(container.left) + left, width)?;
        let (top, height) = span(f64::from(container.top) + top, height)?;
        rects.push(Rect { left, top, width, height });
    }

    let baseline = cells
        .iter()
        .zip(rects.iter())
        .find(|(cell, _)| !cell.absolute)
        .map(|(cell, rect)| f64::from(rect.top - container.top) + cell.baseline.unwrap_or(f64::from(rect.height)));
    Ok((rects, baseline))
}
