//! Places and sizes (`docs/spec/render-command.md` sections 3, 4.1 and 4.3, `docs/spec/notation.md`
//! section 7).
//!
//! A box is placed in its parent's rectangle. Across, its `left`, `right` and `width` are worked out
//! with the parent's width; down, its `top`, `bottom` and `height` with the parent's height. Each
//! written amount is a number of pixels, or a percentage of that parent size. The place is worked out
//! exactly first, then each edge is rounded to a whole pixel (a half up), so the rectangle covers
//! whole pixels.
//!
//! The children of a row or a column are placed by [`flow`]: along the main axis they follow each
//! other with a gap, and across the cross axis they are aligned. A row or column with no size of its
//! own on an axis is as large as its children there.

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
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Amount {
    /// A number of pixels, as written.
    Pixels(f64),
    /// A percentage of the parent's width (across) or height (down): `Percent(50.0)` is half of it.
    Percent(f64),
}

/// What a box writes of its place and size. A value not written is `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Written {
    pub left: Option<Amount>,
    pub top: Option<Amount>,
    pub right: Option<Amount>,
    pub bottom: Option<Amount>,
    pub width: Option<Amount>,
    pub height: Option<Amount>,
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
pub fn place(parent: Rect, written: Written) -> Result<Rect, Beyond> {
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
    /// Filling the content box across, when the row or column has a size of its own across and the
    /// child writes no size there.
    #[default]
    Stretch,
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
#[derive(Clone, Copy, Debug, Default, PartialEq)]
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
}

/// The rectangles of a row or column and of its children, the children in the order they were given.
#[derive(Clone, Debug, PartialEq)]
pub struct Flowed {
    /// The rectangle of the row or column itself, placed in its parent as a box is.
    pub container: Rect,
    /// The rectangle of each child, in the order of the children given to [`flow`].
    pub items: Vec<Rect>,
}

/// Places a row or a column in `parent` and its children in it (`render-command.md` section 4.3).
///
/// The row or column is placed as a box is (section 4.1), except that an axis on which it has no size
/// of its own (no written size, and not both edges) is as large as its children there: along the main
/// axis, the children's outer sizes with the gaps between them and the padding; across, the largest
/// child's outer size with the padding. Percentages of a child are of the row or column's own size on
/// that axis, and are 0 on an axis with none.
///
/// The children that are not absolute follow each other along the main axis with `flex.gap` between
/// neighbours, the group placed as `flex.along` says inside the padded content box. Across, each sits
/// as its `across` says. An absolute child is placed as [`place`] places a box, in the row or column's
/// outer rectangle, and takes no room.
pub fn flow(parent: Rect, written: Written, flex: &Flex, items: &[Item]) -> Result<Flowed, Beyond> {
    let across = Span::across(parent, written);
    let down = Span::down(parent, written);
    let (main, cross) = match flex.direction {
        Direction::Row => (across, down),
        Direction::Column => (down, across),
    };
    // An axis the container has a size of its own on is placed first: its length is the base of the
    // children's percentages. An axis without one is measured from the children, below.
    let main_known = main.known().then(|| main.place(0.0)).transpose()?;
    let cross_known = cross.known().then(|| cross.place(0.0)).transpose()?;
    let main_base = main_known.map(|(_, length)| f64::from(length));
    let cross_base = cross_known.map(|(_, length)| f64::from(length));

    let children = items
        .iter()
        .filter(|item| !item.absolute)
        .map(|item| Child::new(item, flex, main_base, cross_base))
        .collect::<Result<Vec<Child>, Beyond>>()?;

    // The container's size on an axis it has none of its own: the children, the gaps and the padding.
    let (pad_main, pad_cross) = flex.padding.split(flex.direction);
    let gaps = flex.gap * children.len().saturating_sub(1) as f64;
    let group = children.iter().map(Child::outer_main).sum::<f64>() + gaps;
    let main_measured = group + pad_main.0 + pad_main.1;
    let cross_measured = children.iter().map(Child::outer_cross).fold(0.0, f64::max) + pad_cross.0 + pad_cross.1;
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

    // The content box: the container less its padding.
    let main_start = f64::from(main_placed.0) + pad_main.0;
    let main_length = f64::from(main_placed.1) - pad_main.0 - pad_main.1;
    let cross_start = f64::from(cross_placed.0) + pad_cross.0;
    let cross_length = f64::from(cross_placed.1) - pad_cross.0 - pad_cross.1;
    let offset = match flex.along {
        Along::Start => 0.0,
        Along::Center => (main_length - group) / 2.0,
        Along::End => main_length - group,
    };

    let stretch = cross_known.is_some();
    let mut cursor = main_start + offset;
    let mut flowed = Vec::with_capacity(children.len());
    for child in &children {
        let start = cursor + child.main_margin.0;
        let (across_start, across_size) = across_place(child, cross_start, cross_length, stretch);
        flowed.push(to_rect(flex.direction, (start, child.main), (across_start, across_size))?);
        cursor = start + child.main + child.main_margin.1 + flex.gap;
    }

    let mut flowed = flowed.into_iter();
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let rect = if item.absolute {
            place(container, item.written)?
        } else {
            flowed.next().ok_or(Beyond)?
        };
        out.push(rect);
    }
    Ok(Flowed { container, items: out })
}

/// A child that is not absolute, with its sizes and margins worked out for its row or column.
#[derive(Clone, Copy, Debug)]
struct Child {
    /// The size on the main axis: as written, or 0.
    main: f64,
    /// The size across the cross axis, when the child writes one.
    cross: Option<f64>,
    /// The margin before and after on the main axis.
    main_margin: (f64, f64),
    /// The margin before and after on the cross axis.
    cross_margin: (f64, f64),
    /// Where the child sits across: its `align-self`, or the container's `align-items`.
    across: Across,
}

impl Child {
    fn new(item: &Item, flex: &Flex, main_base: Option<f64>, cross_base: Option<f64>) -> Result<Self, Beyond> {
        let (main_amount, cross_amount) = match flex.direction {
            Direction::Row => (item.written.width, item.written.height),
            Direction::Column => (item.written.height, item.written.width),
        };
        let (main_margin, cross_margin) = item.margin.split(flex.direction);
        Ok(Child {
            main: written_size(main_amount, main_base)?.unwrap_or(0.0),
            cross: written_size(cross_amount, cross_base)?,
            main_margin,
            cross_margin,
            across: item.across.unwrap_or(flex.across),
        })
    }

    /// The size on the main axis with its margins on both sides.
    fn outer_main(&self) -> f64 {
        self.main + self.main_margin.0 + self.main_margin.1
    }

    /// The size across with its margins on both sides, a child with no size there counting as 0.
    fn outer_cross(&self) -> f64 {
        self.cross.unwrap_or(0.0) + self.cross_margin.0 + self.cross_margin.1
    }
}

/// A size as written, in pixels and 0 or more. A percentage is of `base`, the row or column's size on
/// that axis; with no base (it has no size of its own there) a percentage is 0. `None` when the size
/// is not written.
fn written_size(amount: Option<Amount>, base: Option<f64>) -> Result<Option<f64>, Beyond> {
    match (amount, base) {
        (None, _) => Ok(None),
        (Some(Amount::Percent(_)), None) => Ok(Some(0.0)),
        (Some(amount), base) => Ok(Some(pixels(amount, base.unwrap_or(0.0))?.max(0.0))),
    }
}

/// The start and size across the content box of a child, where `align-items` or `align-self` puts it
/// (section 4.3 rule 7). `stretch` says whether the row or column has a size of its own across; only
/// then does a child that writes no size fill the content box less its margins.
fn across_place(child: &Child, start: f64, length: f64, stretch: bool) -> (f64, f64) {
    let (before, after) = child.cross_margin;
    let size = child.cross.unwrap_or(0.0);
    match child.across {
        Across::Start => (start + before, size),
        Across::End => (start + length - after - size, size),
        Across::Center => (start + (length - (size + before + after)) / 2.0 + before, size),
        Across::Stretch => match child.cross {
            None if stretch => (start + before, (length - before - after).max(0.0)),
            _ => (start + before, size),
        },
    }
}

/// The rectangle of a child from its span on the main axis and on the cross axis.
fn to_rect(direction: Direction, main: (f64, f64), cross: (f64, f64)) -> Result<Rect, Beyond> {
    let (across, down) = match direction {
        Direction::Row => (main, cross),
        Direction::Column => (cross, main),
    };
    let (left, width) = span(across.0, across.1)?;
    let (top, height) = span(down.0, down.1)?;
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
#[derive(Clone, Copy, Debug)]
struct Span {
    start: i32,
    length: u32,
    near: Option<Amount>,
    far: Option<Amount>,
    size: Option<Amount>,
}

impl Span {
    /// The horizontal axis of a box in `parent`: `left`, `right` and `width`.
    fn across(parent: Rect, written: Written) -> Self {
        Span {
            start: parent.left,
            length: parent.width,
            near: written.left,
            far: written.right,
            size: written.width,
        }
    }

    /// The vertical axis of a box in `parent`: `top`, `bottom` and `height`.
    fn down(parent: Rect, written: Written) -> Self {
        Span {
            start: parent.top,
            length: parent.height,
            near: written.top,
            far: written.bottom,
            size: written.height,
        }
    }

    /// Whether the box has a size of its own on this axis: a written size, or both edges.
    fn known(&self) -> bool {
        self.size.is_some() || (self.near.is_some() && self.far.is_some())
    }

    /// The whole-pixel start and size on this axis, with `fallback` pixels when the box has no size of
    /// its own there.
    fn place(&self, fallback: f64) -> Result<(i32, u32), Beyond> {
        axis(self.start, self.length, self.near, self.far, self.size, fallback)
    }
}

/// One axis of a box: its whole-pixel start and size, in a parent that starts at `start` and is
/// `length` pixels long. `near` is the distance of the near edge (`left` or `top`), `far` that of
/// the far edge (`right` or `bottom`). Without a size written, or two edges, the size is `fallback`
/// pixels.
fn axis(
    start: i32,
    length: u32,
    near: Option<Amount>,
    far: Option<Amount>,
    size: Option<Amount>,
    fallback: f64,
) -> Result<(i32, u32), Beyond> {
    let length = f64::from(length);
    let near = near.map(|amount| pixels(amount, length)).transpose()?;
    let far = far.map(|amount| pixels(amount, length)).transpose()?;
    let size = size.map(|amount| pixels(amount, length)).transpose()?;
    // The size: as written, or the space between two written edges, or the fallback.
    let size = match (size, near, far) {
        (Some(size), _, _) => size,
        (None, Some(near), Some(far)) => length - near - far,
        _ => fallback,
    };
    if !size.is_finite() {
        return Err(Beyond);
    }
    let size = size.max(0.0);
    // The near edge: as written from the parent's near edge; or, with only a far edge, the far edge
    // lies that distance from the parent's far edge; or the parent's near edge.
    let relative = match (near, far) {
        (Some(near), _) => near,
        (None, Some(far)) => length - far - size,
        (None, None) => 0.0,
    };
    span(f64::from(start) + relative, size)
}

/// The whole-pixel start and size of a span that starts at `start` and is `size` pixels long. Each
/// of its two edges is rounded (`whole`), so two neighbours that meet meet on one pixel edge.
fn span(start: f64, size: f64) -> Result<(i32, u32), Beyond> {
    let near = whole(start)?;
    let far = whole(start + size)?;
    let width = (far - near).max(0);
    if width > LIMIT {
        return Err(Beyond);
    }
    Ok((near as i32, width as u32))
}

/// An amount in pixels: a percentage of the parent's length, `percent` times `length` divided by 100.
fn pixels(amount: Amount, length: f64) -> Result<f64, Beyond> {
    let value = match amount {
        Amount::Pixels(pixels) => pixels,
        Amount::Percent(percent) => percent * length / 100.0,
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Beyond)
    }
}

/// A position in pixels rounded to a whole pixel, a half up: the larger whole number when it lies
/// exactly half way. Beyond the limit it is not placed.
fn whole(position: f64) -> Result<i64, Beyond> {
    let floor = position.floor();
    let rounded = if position - floor >= 0.5 { floor + 1.0 } else { floor };
    if rounded.abs() <= LIMIT as f64 {
        Ok(rounded as i64)
    } else {
        Err(Beyond)
    }
}
