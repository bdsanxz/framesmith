//! The geometry of a box's border, its rounded corners and its cut (`docs/spec/box-edges.md`, section 2): the rounded
//! rectangles both renderers fill, worked out in exact numbers before any pixel is made.

use layout::Rect;

use crate::{Mark, Rgba};

/// A ring of a border in its colour (`docs/spec/box-edges.md`, section 3): its coverage lays the colour over the picture by
/// src-over, with the colour's own alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RingMark {
    pub ring: Ring,
    pub colour: Rgba,
}

/// Marks cut to a rounded rectangle (`docs/spec/box-edges.md`, sections 3 and 4): the marks are drawn into a layer of their
/// own, transparent at first, each pixel of the layer is weighed by the coverage of `shape` and the layer is laid over the
/// picture. The shape is in the coordinates of the marks, and is mapped with them.
#[derive(Clone, Debug)]
pub struct Cut {
    pub shape: Rounded,
    pub marks: Vec<Mark>,
}

/// A rectangle with each corner replaced by a quarter circle of `radius` that touches both of its sides, in pixels of the
/// frame before any map. `left < right`, `top < bottom`, and the radius is at most half of the shorter side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rounded {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub radius: f64,
}

impl Rounded {
    /// The outer edge of a box (rule 3), with the radius used (rule 1): the written radius, no more than half of the width
    /// and half of the height. `None` for a box with no width or no height, and for a radius that is not a finite number.
    pub fn of_box(rect: Rect, radius: f64) -> Option<Rounded> {
        if rect.width == 0 || rect.height == 0 || !radius.is_finite() {
            return None;
        }
        let (left, top) = (f64::from(rect.left), f64::from(rect.top));
        let (right, bottom) = (left + f64::from(rect.width), top + f64::from(rect.height));
        let radius = radius.max(0.0).min(f64::from(rect.width) / 2.0).min(f64::from(rect.height) / 2.0);
        Some(Rounded {
            left,
            top,
            right,
            bottom,
            radius,
        })
    }

    /// The rounded rectangle inset by `by` on every side, with the radius less `by` and never below 0 (rule 2); `None` when
    /// nothing is left of it.
    pub fn inset(self, by: f64) -> Option<Rounded> {
        let inset = Rounded {
            left: self.left + by,
            top: self.top + by,
            right: self.right - by,
            bottom: self.bottom - by,
            radius: (self.radius - by).max(0.0),
        };
        (inset.left < inset.right && inset.top < inset.bottom).then_some(inset)
    }

    /// The width of the shorter side.
    pub fn shorter_side(self) -> f64 {
        (self.right - self.left).min(self.bottom - self.top)
    }

    /// Whether every edge lies on a whole pixel and the corners are square: the shape covers each pixel wholly or not at all.
    pub fn is_whole(self) -> bool {
        self.radius == 0.0
            && [self.left, self.top, self.right, self.bottom]
                .iter()
                .all(|value| value.fract() == 0.0)
    }

    /// The rectangle of whole pixels the shape is, when [`Rounded::is_whole`] holds.
    pub fn whole_rect(self) -> Option<Rect> {
        if !self.is_whole() || self.left.abs() > 1.0e9 || self.top.abs() > 1.0e9 {
            return None;
        }
        Some(Rect {
            left: self.left as i32,
            top: self.top as i32,
            width: (self.right - self.left) as u32,
            height: (self.bottom - self.top) as u32,
        })
    }
}

/// The part of a box between two rounded rectangles: everything inside `outer` that is not inside `inner`. With no `inner`
/// the ring is the whole of `outer`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    pub outer: Rounded,
    pub inner: Option<Rounded>,
}

/// A border as the author writes it: its width in pixels, and whether it is a border of two lines (`w double c`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub width: f64,
    pub double: bool,
}

impl Line {
    /// The rings a border draws on the outer edge `outer` (sections 2 and 3): one for a border of one line, two for a border
    /// of two lines, whose outer line, gap and inner line are each a third of the width. A border wider than half the shorter
    /// side is as wide as that. A border of width 0, or one that is not a finite number, draws no ring.
    pub fn rings(self, outer: Rounded) -> Vec<Ring> {
        if !self.width.is_finite() || self.width <= 0.0 {
            return Vec::new();
        }
        let width = self.width.min(outer.shorter_side() / 2.0);
        if !self.double {
            return vec![Ring {
                outer,
                inner: outer.inset(width),
            }];
        }
        let third = width / 3.0;
        let mut rings = vec![Ring {
            outer,
            inner: outer.inset(third),
        }];
        if let Some(line) = outer.inset(2.0 * third) {
            rings.push(Ring {
                outer: line,
                inner: outer.inset(width),
            });
        }
        rings
    }

    /// The inner edge of a box with this border (rule 5): its padding box, which `overflow="hidden"` cuts at. `None` when the
    /// border leaves nothing inside.
    pub fn inner_edge(self, outer: Rounded) -> Option<Rounded> {
        if !self.width.is_finite() || self.width <= 0.0 {
            return Some(outer);
        }
        outer.inset(self.width.min(outer.shorter_side() / 2.0))
    }
}

/// What a box writes of its edges: its corner radius, its border with its colour, and whether it cuts its children
/// (`overflow="hidden"`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edges {
    pub radius: f64,
    pub border: Option<(Line, Rgba)>,
    pub cut: bool,
}

impl Edges {
    /// The marks of a box in the order of `docs/spec/box-edges.md` section 3: its background, cut to its outer edge when the
    /// corners are round; the rings of its border; then its children, cut to its inner edge when the box cuts them. A box with
    /// no size draws no background and no border, and of its children only those it does not cut.
    pub fn marks(&self, rect: Rect, background: Vec<Mark>, children: Vec<Mark>) -> Vec<Mark> {
        let Some(outer) = Rounded::of_box(rect, self.radius) else {
            return if self.cut { Vec::new() } else { children };
        };
        let mut marks = Vec::new();
        if !background.is_empty() {
            if outer.radius == 0.0 {
                marks.extend(background);
            } else {
                marks.push(Mark::Cut(Cut {
                    shape: outer,
                    marks: background,
                }));
            }
        }
        if let Some((line, colour)) = self.border {
            marks.extend(line.rings(outer).into_iter().map(|ring| Mark::Ring(RingMark { ring, colour })));
        }
        if !self.cut {
            marks.extend(children);
            return marks;
        }
        let inner = match self.border {
            Some((line, _)) => line.inner_edge(outer),
            None => Some(outer),
        };
        if let Some(shape) = inner {
            if !children.is_empty() {
                marks.push(Mark::Cut(Cut { shape, marks: children }));
            }
        }
        marks
    }
}
