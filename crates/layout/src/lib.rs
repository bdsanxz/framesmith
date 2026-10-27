//! Places and sizes (`docs/spec/render-command.md` sections 3, 4.1, 4.3 and 4.4, `docs/spec/notation.md`
//! section 7).
//!
//! A box is placed in its parent's rectangle. Across, its `left`, `right` and `width` are worked out
//! with the parent's width; down, its `top`, `bottom` and `height` with the parent's height. Each
//! written amount is a number of pixels, or a percentage of that parent size. The place is worked out
//! exactly first, then each edge is rounded to a whole pixel (a half up), so the rectangle covers
//! whole pixels.
//!
//! The children of a row or a column are placed by [`flow`]: along the main axis they follow each
//! other with a gap, or share the room left over; across the cross axis they are aligned, and a row
//! aligned by its baselines lines up their first lines. A row or column with no size of its own on an
//! axis is as large as its children there. The cells of a set of columns are placed by [`columns`]
//! (module `set`, `render-command.md` section 4.4).

mod exact;
mod set;

pub use exact::Decimal;

pub use set::{columns, inside_columns, measure_columns, Cell, Columns, Track};

/// The most pixels an edge or a size may be in this version: beyond it a value is not placed.
pub const LIMIT: i64 = 1_000_000;

/// A rectangle in the plane of the frame, in whole pixels. `left` and `top` may be negative or
/// beyond the frame; the part inside the frame is what is seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

/// The pixels of a rectangle that lie inside the frame: columns `left` to `right` less one, and
/// rows `top` to `bottom` less one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Visible {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl Rect {
    /// The part of the rectangle that lies inside a frame of `width` by `height` pixels, or `None`
    /// when no pixel of it does (a size of 0 has none).
    pub fn visible(&self, width: u32, height: u32) -> Option<Visible> {
        let left = i64::from(self.left).max(0);
        let top = i64::from(self.top).max(0);
        let right = (i64::from(self.left) + i64::from(self.width)).min(i64::from(width));
        let bottom = (i64::from(self.top) + i64::from(self.height)).min(i64::from(height));
        if left >= right || top >= bottom {
            return None;
        }
        Some(Visible {
            left: left as u32,
            top: top as u32,
            right: right as u32,
            bottom: bottom as u32,
        })
    }
}

/// An amount written for one axis: a number of pixels, or a percentage of the parent's size on that
/// axis (`notation.md` section 4: a length).
#[derive(Clone, Debug, PartialEq)]
pub enum Amount {
    /// A number of pixels, as written.
    Pixels(Decimal),
    /// A percentage of the parent's width (across) or height (down): `Percent(50.0)` is half of it.
    Percent(Decimal),
}

/// What a box writes of its place and size. A value not written is `None`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Written {
    pub left: Option<Amount>,
    pub top: Option<Amount>,
    pub right: Option<Amount>,
    pub bottom: Option<Amount>,
    pub width: Option<Amount>,
    pub height: Option<Amount>,
    /// The widest the box may be, in pixels above 0 (`max-width`, `render-command.md` section 4.1, rule 7).
    /// It caps a width that does not come from the box's own written `width`.
    pub max_width: Option<f64>,
}

/// A place that cannot be worked out within the limit: an edge or a size lies beyond [`LIMIT`]
/// pixels, or an amount is not a finite number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Beyond;

/// The rectangle of a frame of `width` by `height` pixels: its corner is the origin.
pub fn frame(width: u32, height: u32) -> Rect {
    Rect {
        left: 0,
        top: 0,
        width,
        height,
    }
}

/// The place of a box in its parent's rectangle `parent`, in whole pixels (`render-command.md`
/// section 4.1). A size below 0 is 0: the scene reports a written one before it is placed, and a
/// size that two distances leave below 0 is 0.
pub fn place(parent: Rect, written: &Written) -> Result<Rect, Beyond> {
    let (left, width) = Span::across(parent, written).place(f64::from(parent.width))?;
    let (top, height) = Span::down(parent, written).place(f64::from(parent.height))?;
    Ok(Rect { left, top, width, height })
}

/// The direction of a row or a column (`render-command.md` section 4.3). Its main axis is across for
/// a row, down for a column; the cross axis is the other one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// Children side by side, from left to right.
    #[default]
    Row,
    /// Children one under another, from top to bottom.
    Column,
}

/// Where the group of children sits along the main axis (`justify-content`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Along {
    /// At the start edge of the content box.
    #[default]
    Start,
    /// Centred: the same room on both sides of the group.
    Center,
    /// At the end edge of the content box.
    End,
    /// The first child at the start edge, the last at the end edge, and the room left over shared
    /// equally between each two neighbours, on top of the gap. With one child, or with no room left
    /// over, it is the same as `Start`.
    SpaceBetween,
}

/// Where a child sits across the cross axis (`align-items`, `align-self`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Across {
    /// At the start edge of the content box.
    Start,
    /// Centred across the content box.
    Center,
    /// At the end edge of the content box.
    End,
    /// Filling the content box across, when the child writes no size there. The content box is the one
    /// of the row or column's own size across, or of its measured size when it has none.
    #[default]
    Stretch,
    /// The baselines of the first lines of the children lie on one line, and the highest of them touches
    /// the content box's top edge. In a column it is `Start` (`render-command.md` section 4.3, rule 7).
    Baseline,
}

/// The four sides of a box in pixels: `top`, `right`, `bottom` and `left`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sides {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Sides {
    /// The sides of one to four numbers, in the order of `notation.md` section 4: one number for all
    /// four sides; two for the top and bottom, then the left and right; three for the top, then the
    /// left and right, then the bottom; four for the top, right, bottom and left. `None` for any other
    /// count.
    pub fn from_values(values: &[f64]) -> Option<Self> {
        let (top, right, bottom, left) = match *values {
            [all] => (all, all, all, all),
            [vertical, horizontal] => (vertical, horizontal, vertical, horizontal),
            [top, horizontal, bottom] => (top, horizontal, bottom, horizontal),
            [top, right, bottom, left] => (top, right, bottom, left),
            _ => return None,
        };
        Some(Sides { top, right, bottom, left })
    }

    /// The sides split for a direction: the (start, end) pair on the main axis, then the (start, end)
    /// pair on the cross axis.
    fn split(self, direction: Direction) -> ((f64, f64), (f64, f64)) {
        let across = (self.left, self.right);
        let down = (self.top, self.bottom);
        match direction {
            Direction::Row => (across, down),
            Direction::Column => (down, across),
        }
    }
}

/// How a row or a column lays out its children: its direction, `justify-content`, `align-items`,
/// `gap` and `padding`. The default is a row with the children at its start and stretched across, no
/// gap and no padding.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flex {
    pub direction: Direction,
    /// Where the group of children sits along the main axis.
    pub along: Along,
    /// Where each child sits across, unless the child has its own `align-self`.
    pub across: Across,
    /// The space between two neighbouring children along the main axis, in pixels. Zero or more: the
    /// caller reports a smaller value before it is placed.
    pub gap: f64,
    /// The space inside the container around the children, in pixels. Zero or more.
    pub padding: Sides,
}

/// A child of a row or a column: what it writes, its margin, its own `align-self`, and whether it is
/// placed absolutely.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Item {
    /// What the child writes. An absolute child is placed by all of it; a flowed child uses only its
    /// `width` and `height`, and takes no notice of its own `left`, `top`, `right` and `bottom`.
    pub written: Written,
    /// The margin of the child, in pixels. An absolute child has none (`notation.md` N10).
    pub margin: Sides,
    /// The child's own `align-self`, or `None` to take the container's `align-items`.
    pub across: Option<Across>,
    /// Whether the child is `position="absolute"`: it takes no room, is not counted in the gaps, and
    /// is placed against the container's outer rectangle.
    pub absolute: bool,
    /// The size across that the child's own items give it, when it is a row or column with no size of its
    /// own across (`render-command.md` section 4.3, rules 1 and 3), or `None`. Where the child writes no size
    /// across and does not stretch, it is placed at this size; where it stretches, it fills the content box
    /// and this size counts in its container's size across.
    pub measured_across: Option<f64>,
    /// The baseline of the child from its top edge, for `align-items="baseline"`: that of its first line when
    /// it is a text, that of its first item when it is a row, a column or a set of columns. `None` for its
    /// bottom edge, which is what a child of any other kind has.
    pub baseline: Option<f64>,
    /// The share of the spare room along the main axis (`flex-grow`), 0 for none (`render-command.md` section
    /// 4.3, rule 12).
    pub grow: f64,
    /// The widest the child may be, in pixels, across a row (its width) or down a column (its height is not
    /// capped; its width is, across a column). The caller gives it only for a child that writes no `width` of
    /// its own (`render-command.md` section 4.3, rule 13). `None` for no cap.
    pub max_width: Option<f64>,
}

/// The rectangles of a row or column and of its children, the children in the order they were given.
#[derive(Clone, Debug, PartialEq)]
pub struct Flowed {
    /// The rectangle of the row or column itself, placed in its parent as a box is.
    pub container: Rect,
    /// The rectangle of each child, in the order of the children given to [`flow`].
    pub items: Vec<Rect>,
    /// The baseline of the row or column from its top edge: that of its first child, counted from its top
    /// (`render-command.md` section 4.3, rule 7), or `None` when it has no child, which leaves its bottom edge
    /// as its baseline.
    pub baseline: Option<f64>,
}

/// The size of a row or column worked out from its children on each axis (section 4.3, rule 1): the
/// width and the height it takes when it has no size of its own there.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    /// The width in pixels.
    pub width: f64,
    /// The height in pixels.
    pub height: f64,
}

/// The axes on which a row or column has no size of its own, so that its children give it one. A row or
/// column placed by [`inside`] says so for each axis: a percentage of a child is 0 on an axis it is
/// measured on, as [`measure`] works it out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Measured {
    /// Whether the width is measured from the children.
    pub width: bool,
    /// Whether the height is measured from the children.
    pub height: bool,
}

/// Places a row or a column in `parent` and its children in it (`render-command.md` section 4.3).
///
/// The row or column is placed as a box is (section 4.1), except that an axis on which it has no size
/// of its own (no written size, and not both edges) is as large as its children there: along the main
/// axis, the children's outer sizes with the gaps between them and the padding; across, the largest
/// child's outer size with the padding. A child that writes no size across counts its margins only, or,
/// when it is a row or column with no size of its own across, the size its own items give it, with its
/// margins (see [`measure`]). Percentages of a child are of the row or column's own size on that axis, and
/// are 0 on an axis with none.
///
/// The children that are not absolute follow each other along the main axis with `flex.gap` between
/// neighbours, the group placed as `flex.along` says inside the padded content box. Across, each sits
/// as its `across` says, and one that writes no size there stretches to fill the content box less its
/// margins. An absolute child is placed as [`place`] places a box, in the row or column's outer
/// rectangle, and takes no room.
pub fn flow(parent: Rect, written: &Written, flex: &Flex, items: &[Item]) -> Result<Flowed, Beyond> {
    let across = Span::across(parent, written);
    let down = Span::down(parent, written);
    let (main, cross) = match flex.direction {
        Direction::Row => (across, down),
        Direction::Column => (down, across),
    };
    // An axis the container has a size of its own on is placed first: its length is the base of the
    // children's percentages, and the room the children may grow into. An axis without one is measured
    // from the children, below.
    let main_known = main.known().then(|| main.place(0.0)).transpose()?;
    let cross_known = cross.known().then(|| cross.place(0.0)).transpose()?;
    let main_base = main_known.map(|(_, length)| f64::from(length));
    let cross_base = cross_known.map(|(_, length)| f64::from(length));

    let mut children = children(flex, items, main_base, cross_base)?;
    let (pad_main, _) = flex.padding.split(flex.direction);
    let content = main_known.map(|(_, length)| f64::from(length) - pad_main.0 - pad_main.1);
    settle(flex, &mut children, content);
    let (main_measured, cross_measured) = measured_sizes(flex, &children);
    let main_placed = match main_known {
        Some(placed) => placed,
        None => main.place(main_measured)?,
    };
    let cross_placed = match cross_known {
        Some(placed) => placed,
        None => cross.place(cross_measured)?,
    };
    let container = match flex.direction {
        Direction::Row => rect(main_placed, cross_placed),
        Direction::Column => rect(cross_placed, main_placed),
    };
    let (rects, baseline) = place_items(container, flex, &children, items)?;
    Ok(Flowed {
        container,
        items: rects,
        baseline,
    })
}

/// The size of a row or a column with no size of its own, worked out from its children (section 4.3,
/// rule 1). Along the main axis, the group of children with the gaps between them and the padding; across,
/// the largest outer size of a child with the padding. A child that writes no size across counts its margins
/// only, unless it is a row or column with no size of its own across: it then counts the size its own items
/// give it there, whether or not it stretches across. A child that is itself a row or column is given its own
/// size first, from the inside out. A percentage of a child is 0 on each axis, the row or column having no size
/// there to be a percentage of.
pub fn measure(flex: &Flex, items: &[Item]) -> Result<Size, Beyond> {
    let mut children = children(flex, items, None, None)?;
    settle(flex, &mut children, None);
    let (main, cross) = measured_sizes(flex, &children);
    Ok(match flex.direction {
        Direction::Row => Size {
            width: main,
            height: cross,
        },
        Direction::Column => Size {
            width: cross,
            height: main,
        },
    })
}

/// The children of a row or a column placed in the rectangle `rect` (section 4.3). An axis on which it is
/// `measured` has no size of its own for the percentages of its children, which are then 0 as [`measure`]
/// has them. The children are laid out inside `rect` as [`flow`] lays them out in the container it places.
pub fn inside(rect: Rect, measured: Measured, flex: &Flex, items: &[Item]) -> Result<Flowed, Beyond> {
    let (main_length, cross_length, main_measured, cross_measured) = match flex.direction {
        Direction::Row => (rect.width, rect.height, measured.width, measured.height),
        Direction::Column => (rect.height, rect.width, measured.height, measured.width),
    };
    let main_base = (!main_measured).then(|| f64::from(main_length));
    let cross_base = (!cross_measured).then(|| f64::from(cross_length));
    let mut children = children(flex, items, main_base, cross_base)?;
    let (pad_main, _) = flex.padding.split(flex.direction);
    let content = (!main_measured).then(|| f64::from(main_length) - pad_main.0 - pad_main.1);
    settle(flex, &mut children, content);
    let (rects, baseline) = place_items(rect, flex, &children, items)?;
    Ok(Flowed {
        container: rect,
        items: rects,
        baseline,
    })
}

/// The children that are not absolute, each with its sizes and margins worked out for its row or column.
fn children(flex: &Flex, items: &[Item], main_base: Option<f64>, cross_base: Option<f64>) -> Result<Vec<Child>, Beyond> {
    items
        .iter()
        .filter(|item| !item.absolute)
        .map(|item| Child::new(item, flex, main_base, cross_base))
        .collect()
}

/// Settles the main sizes of the children. When the content box along the main axis is known (`content`),
/// the spare room, the content less the group of children, is shared among the children with a `flex-grow`
/// above 0, in proportion to their values (rule 12). Then each child's size is capped by its `max-width`
/// (rule 13). With `content` unknown no child grows, since the size of the row or column is their sum.
fn settle(flex: &Flex, children: &mut [Child], content: Option<f64>) {
    if let Some(content) = content {
        let spare = content - group(flex, children);
        let total: f64 = children.iter().map(|child| child.grow).sum();
        if spare > 0.0 && total > 0.0 {
            for child in children.iter_mut() {
                child.main += spare * child.grow / total;
            }
        }
    }
    for child in children.iter_mut() {
        if let Some(limit) = child.main_cap {
            child.main = child.main.min(limit);
        }
    }
}

/// The main and the cross size of a row or column worked out from its children, padding included.
fn measured_sizes(flex: &Flex, children: &[Child]) -> (f64, f64) {
    let (pad_main, pad_cross) = flex.padding.split(flex.direction);
    let main = group(flex, children) + pad_main.0 + pad_main.1;
    let outer = children
        .iter()
        .filter(|child| child.across != Across::Baseline)
        .map(Child::outer_cross)
        .fold(0.0, f64::max);
    let block = baseline_block(children).map_or(0.0, |(above, below)| above + below);
    let cross = outer.max(block) + pad_cross.0 + pad_cross.1;
    (main, cross)
}

/// The length of the group of children along the main axis: their outer sizes, and a gap between each two.
fn group(flex: &Flex, children: &[Child]) -> f64 {
    children.iter().map(Child::outer_main).sum::<f64>() + flex.gap * children.len().saturating_sub(1) as f64
}

/// The length of the group of children along the main axis, summed exactly: their outer sizes, and a gap between each two.
fn group_exact(flex: &Flex, children: &[Child]) -> Option<Decimal> {
    let mut total = Decimal::whole(0);
    for child in children {
        total = total
            .add(Decimal::of(child.main)?)?
            .add(Decimal::of(child.main_margin.0)?)?
            .add(Decimal::of(child.main_margin.1)?)?;
    }
    let gaps = Decimal::whole(children.len().saturating_sub(1) as i128);
    total.add(Decimal::of(flex.gap)?.times(gaps)?)
}

/// The part of the cross axis above and below the common baseline of the children that lie on one
/// (`align-items="baseline"`): the highest part above the baseline and the lowest part below it. `None`
/// when no child is aligned by its baseline.
fn baseline_block(children: &[Child]) -> Option<(f64, f64)> {
    let mut found = false;
    let (mut above_max, mut below_max) = (0.0_f64, 0.0_f64);
    for child in children.iter().filter(|child| child.across == Across::Baseline) {
        let above = child.above();
        let below = child.outer_cross() - above;
        found = true;
        above_max = above_max.max(above);
        below_max = below_max.max(below);
    }
    found.then_some((above_max, below_max))
}

/// The rectangles of the items of a row or column placed in `container`, in the order of `items`: the
/// children that are not absolute, placed in the content box of the container, and each absolute item,
/// placed as [`place`] places a box in the container. Also the baseline of the first child from the
/// container's top, when it has one.
fn place_items(container: Rect, flex: &Flex, children: &[Child], items: &[Item]) -> Result<(Vec<Rect>, Option<f64>), Beyond> {
    let (pad_main, pad_cross) = flex.padding.split(flex.direction);
    let (main_start, main_size, cross_start, cross_size) = match flex.direction {
        Direction::Row => (container.left, container.width, container.top, container.height),
        Direction::Column => (container.top, container.height, container.left, container.width),
    };
    // The content box: the container less its padding. Along the main axis the sums are exact (`render-command.md`,
    // section 4.1, rule 3): a share of divided room is the one amount worked in floats.
    let main_from = Decimal::whole(i128::from(main_start))
        .add(Decimal::of(pad_main.0).ok_or(Beyond)?)
        .ok_or(Beyond)?;
    let main_length = Decimal::whole(i128::from(main_size))
        .sub(Decimal::of(pad_main.0).ok_or(Beyond)?)
        .and_then(|rest| rest.sub(Decimal::of(pad_main.1)?))
        .ok_or(Beyond)?;
    let cross_from = f64::from(cross_start) + pad_cross.0;
    let cross_length = f64::from(cross_size) - pad_cross.0 - pad_cross.1;
    let room = main_length.sub(group_exact(flex, children).ok_or(Beyond)?).ok_or(Beyond)?;
    // The place of the first child along the main axis, and the space between two neighbours.
    let gap = Decimal::of(flex.gap).ok_or(Beyond)?;
    let (offset, spacing) = match flex.along {
        Along::Start => (Decimal::whole(0), gap),
        Along::Center => (Decimal::of(room.to_f64() / 2.0).ok_or(Beyond)?, gap),
        Along::End => (room.clone(), gap),
        Along::SpaceBetween => {
            let extra = if children.len() > 1 && room.is_above_zero() {
                Decimal::of(room.to_f64() / (children.len() - 1) as f64).ok_or(Beyond)?
            } else {
                Decimal::whole(0)
            };
            (Decimal::whole(0), gap.add(extra).ok_or(Beyond)?)
        }
    };
    let line = baseline_block(children).map_or(0.0, |(above, _)| above);

    let mut cursor = main_from.add(offset).ok_or(Beyond)?;
    let mut flowed = Vec::with_capacity(children.len());
    for child in children {
        let start = cursor.clone().add(Decimal::of(child.main_margin.0).ok_or(Beyond)?).ok_or(Beyond)?;
        let size = Decimal::of(child.main).ok_or(Beyond)?;
        let (across_start, across_size) = across_place(child, cross_from, cross_length, line);
        flowed.push(to_rect(flex.direction, (start.clone(), size.clone()), (across_start, across_size))?);
        cursor = start
            .add(size)
            .and_then(|end| end.add(Decimal::of(child.main_margin.1)?))
            .and_then(|end| end.add(spacing.clone()))
            .ok_or(Beyond)?;
    }

    let baseline = match (children.first(), flowed.first()) {
        (Some(child), Some(first)) => {
            let top = f64::from(first.top - container.top);
            Some(top + child.baseline.unwrap_or(f64::from(first.height)))
        }
        _ => None,
    };
    let mut flowed = flowed.into_iter();
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let rect = if item.absolute {
            place(container, &item.written)?
        } else {
            flowed.next().ok_or(Beyond)?
        };
        out.push(rect);
    }
    Ok((out, baseline))
}

/// A child that is not absolute, with its sizes and margins worked out for its row or column.
#[derive(Clone, Debug)]
struct Child {
    /// The size on the main axis: as written, or 0 when none is written.
    main: f64,
    /// The size across the cross axis, when the child writes one.
    cross: Option<f64>,
    /// The size across that the child's own items give it, when it is a row or column with no size of its
    /// own across and writes none.
    measured_across: Option<f64>,
    /// The margin before and after on the main axis.
    main_margin: (f64, f64),
    /// The margin before and after on the cross axis.
    cross_margin: (f64, f64),
    /// Where the child sits across: its `align-self`, or the container's `align-items`; `Start` for a
    /// baseline in a column.
    across: Across,
    /// The baseline of the child from its top edge, or `None` for its bottom edge.
    baseline: Option<f64>,
    /// The share of the spare room along the main axis.
    grow: f64,
    /// The widest the child may be on the main axis, in a row, when the caller gave one.
    main_cap: Option<f64>,
    /// The widest the child may be across, in a column, when the caller gave one.
    cross_cap: Option<f64>,
}

impl Child {
    fn new(item: &Item, flex: &Flex, main_base: Option<f64>, cross_base: Option<f64>) -> Result<Self, Beyond> {
        let (main_amount, cross_amount) = match flex.direction {
            Direction::Row => (item.written.width.clone(), item.written.height.clone()),
            Direction::Column => (item.written.height.clone(), item.written.width.clone()),
        };
        let (main_margin, cross_margin) = item.margin.split(flex.direction);
        // A baseline in a column is the start, since a column has no line of baselines (rule 7).
        let across = match (flex.direction, item.across.unwrap_or(flex.across)) {
            (Direction::Column, Across::Baseline) => Across::Start,
            (_, across) => across,
        };
        // `max-width` caps the width: the main size of a row, the cross size of a column.
        let (main_cap, cross_cap) = match flex.direction {
            Direction::Row => (item.max_width, None),
            Direction::Column => (None, item.max_width),
        };
        Ok(Child {
            main: written_size(main_amount, main_base)?.unwrap_or(0.0),
            cross: written_size(cross_amount, cross_base)?,
            measured_across: item.measured_across.map(|size| cap(size, cross_cap)),
            main_margin,
            cross_margin,
            across,
            baseline: item.baseline,
            grow: item.grow.max(0.0),
            main_cap,
            cross_cap,
        })
    }

    /// The size on the main axis with its margins on both sides.
    fn outer_main(&self) -> f64 {
        self.main + self.main_margin.0 + self.main_margin.1
    }

    /// The size across the cross axis: the size written there, else the size the child's own items give it,
    /// else 0.
    fn cross_size(&self) -> f64 {
        self.cross.or(self.measured_across).unwrap_or(0.0)
    }

    /// The size across with its margins on both sides.
    fn outer_cross(&self) -> f64 {
        self.cross_size() + self.cross_margin.0 + self.cross_margin.1
    }

    /// The distance from the outer edge before the cross axis to the baseline: the margin before, and the
    /// child's baseline from its top edge, or its size across when its baseline is its bottom edge.
    fn above(&self) -> f64 {
        self.cross_margin.0 + self.baseline.unwrap_or_else(|| self.cross_size())
    }
}

/// A size capped at `limit` when there is one.
fn cap(size: f64, limit: Option<f64>) -> f64 {
    match limit {
        Some(limit) => size.min(limit),
        None => size,
    }
}

/// A size as written, in pixels and 0 or more. A percentage is of `base`, the row or column's size on
/// that axis; with no base (it has no size of its own there) a percentage is 0. `None` when the size
/// is not written.
fn written_size(amount: Option<Amount>, base: Option<f64>) -> Result<Option<f64>, Beyond> {
    match (amount, base) {
        (None, _) => Ok(None),
        (Some(Amount::Percent(_)), None) => Ok(Some(0.0)),
        (Some(amount), base) => {
            let base = Decimal::of(base.unwrap_or(0.0)).ok_or(Beyond)?;
            let size = pixels(&amount, &base)?;
            Ok(Some(if size.is_below_zero() { 0.0 } else { size.to_f64() }))
        }
    }
}

/// The start and size across the content box of a child, where `align-items` or `align-self` puts it
/// (section 4.3, rule 7). A child that writes no size across stretches to fill the content box less its
/// margins, whatever size its own items give it, and is capped by its `max-width` when it has one; the content
/// box of a row or column with no size of its own across is as large as its children (rule 1). A child that
/// does not stretch is placed at the size it writes there, or the size its own items give it. `line` is the
/// highest part above the common baseline, for a child aligned by its baseline.
fn across_place(child: &Child, start: f64, length: f64, line: f64) -> (f64, f64) {
    let (before, after) = child.cross_margin;
    let size = child.cross_size();
    match child.across {
        Across::Start => (start + before, size),
        Across::End => (start + length - after - size, size),
        Across::Center => (start + (length - (size + before + after)) / 2.0 + before, size),
        Across::Baseline => (start + (line - child.above()) + before, size),
        Across::Stretch => match child.cross {
            Some(size) => (start + before, size),
            None => (start + before, cap((length - before - after).max(0.0), child.cross_cap)),
        },
    }
}

/// The rectangle of a child from its span on the main axis and on the cross axis.
fn to_rect(direction: Direction, main: (Decimal, Decimal), cross: (f64, f64)) -> Result<Rect, Beyond> {
    let main = span_exact(main.0, main.1)?;
    let cross = span(cross.0, cross.1)?;
    let ((left, width), (top, height)) = match direction {
        Direction::Row => (main, cross),
        Direction::Column => (cross, main),
    };
    Ok(Rect { left, top, width, height })
}

/// The rectangle of a container from its placed span across and its placed span down.
fn rect(across: (i32, u32), down: (i32, u32)) -> Rect {
    Rect {
        left: across.0,
        top: down.0,
        width: across.1,
        height: down.1,
    }
}

/// One axis of a box: where its parent lies on that axis, and what the box writes there.
#[derive(Clone, Debug)]
struct Span {
    start: i32,
    length: u32,
    near: Option<Amount>,
    far: Option<Amount>,
    size: Option<Amount>,
    /// The widest the box may be on this axis, when it has no size of its own there (`max-width`).
    max: Option<f64>,
}

impl Span {
    /// The horizontal axis of a box in `parent`: `left`, `right`, `width` and `max-width`.
    fn across(parent: Rect, written: &Written) -> Self {
        Span {
            start: parent.left,
            length: parent.width,
            near: written.left.clone(),
            far: written.right.clone(),
            size: written.width.clone(),
            max: written.max_width,
        }
    }

    /// The vertical axis of a box in `parent`: `top`, `bottom` and `height`.
    fn down(parent: Rect, written: &Written) -> Self {
        Span {
            start: parent.top,
            length: parent.height,
            near: written.top.clone(),
            far: written.bottom.clone(),
            size: written.height.clone(),
            max: None,
        }
    }

    /// Whether the box has a size of its own on this axis: a written size, or both edges.
    fn known(&self) -> bool {
        self.size.is_some() || (self.near.is_some() && self.far.is_some())
    }

    /// The whole-pixel start and size on this axis, with `fallback` pixels when the box has no size of
    /// its own there.
    fn place(&self, fallback: f64) -> Result<(i32, u32), Beyond> {
        axis(
            self.start,
            self.length,
            self.near.as_ref(),
            self.far.as_ref(),
            self.size.as_ref(),
            fallback,
            self.max,
        )
    }
}

/// One axis of a box: its whole-pixel start and size, in a parent that starts at `start` and is
/// `length` pixels long. `near` is the distance of the near edge (`left` or `top`), `far` that of
/// the far edge (`right` or `bottom`). Without a size written, or two edges, the size is `fallback`
/// pixels, or the space between the two edges, and `max` caps it (`max-width`, section 4.1, rule 7);
/// a size written is kept as it is.
fn axis(
    start: i32,
    length: u32,
    near: Option<&Amount>,
    far: Option<&Amount>,
    size: Option<&Amount>,
    fallback: f64,
    max: Option<f64>,
) -> Result<(i32, u32), Beyond> {
    let length = Decimal::whole(i128::from(length));
    let near = near.map(|amount| pixels(amount, &length)).transpose()?;
    let far = far.map(|amount| pixels(amount, &length)).transpose()?;
    let written = size.map(|amount| pixels(amount, &length)).transpose()?;
    let max = max.map(|limit| Decimal::of(limit).ok_or(Beyond)).transpose()?;
    let fallback = Decimal::of(fallback).ok_or(Beyond)?;
    // The size: as written, or the space between two written edges, or the fallback; capped unless written.
    let size = match (written, near.clone(), far.clone()) {
        (Some(size), _, _) => size,
        (None, Some(near), Some(far)) => cap_exact(length.clone().sub(near).and_then(|rest| rest.sub(far)), max)?,
        _ => cap_exact(Some(fallback), max)?,
    };
    let size = size.max(Decimal::whole(0)).ok_or(Beyond)?;
    // The near edge: as written from the parent's near edge; or, with only a far edge, the far edge
    // lies that distance from the parent's far edge; or the parent's near edge.
    let relative = match (near, far) {
        (Some(near), _) => near,
        (None, Some(far)) => length.clone().sub(far).and_then(|rest| rest.sub(size.clone())).ok_or(Beyond)?,
        (None, None) => Decimal::whole(0),
    };
    span_exact(Decimal::whole(i128::from(start)).add(relative).ok_or(Beyond)?, size)
}

/// A size capped at `limit` when there is one, the cap as the amount is worked out (`None` when it does not fit).
fn cap_exact(size: Option<Decimal>, limit: Option<Decimal>) -> Result<Decimal, Beyond> {
    let size = size.ok_or(Beyond)?;
    match limit {
        Some(limit) => size.min(limit).ok_or(Beyond),
        None => Ok(size),
    }
}

/// The whole-pixel start and size of a span that starts at `start` and is `size` pixels long. Each
/// of its two edges is rounded (`whole`), so two neighbours that meet meet on one pixel edge.
fn span(start: f64, size: f64) -> Result<(i32, u32), Beyond> {
    let start = Decimal::of(start).ok_or(Beyond)?;
    let size = Decimal::of(size).ok_or(Beyond)?;
    span_exact(start, size)
}

/// [`span`] for amounts worked out exactly (`render-command.md`, section 4.1, rule 3).
fn span_exact(start: Decimal, size: Decimal) -> Result<(i32, u32), Beyond> {
    let near = whole(start.clone())?;
    let far = whole(start.add(size).ok_or(Beyond)?)?;
    let width = (far - near).max(0);
    if width > LIMIT {
        return Err(Beyond);
    }
    Ok((near as i32, width as u32))
}

/// An amount in pixels: a percentage of the parent's length, `percent` times `length` divided by 100, exactly.
fn pixels(amount: &Amount, length: &Decimal) -> Result<Decimal, Beyond> {
    let value = match amount {
        Amount::Pixels(pixels) => Some(pixels.clone()),
        Amount::Percent(percent) => percent.clone().times(length.clone()).and_then(|share| share.per_hundred()),
    };
    value.ok_or(Beyond)
}

/// A position in pixels rounded to a whole pixel, a half up: the larger whole number when it lies
/// exactly half way. Beyond the limit it is not placed.
fn whole(position: Decimal) -> Result<i64, Beyond> {
    let rounded = position.round_half_up().ok_or(Beyond)?;
    if rounded.abs() <= i128::from(LIMIT) {
        Ok(rounded as i64)
    } else {
        Err(Beyond)
    }
}
