//! The checked-in pictures of the fixtures of borders, rounded corners and cut children hold the colours worked by hand from
//! `docs/spec/box-edges.md`, sections 2 to 5: each fixture is painted here again from its shapes, written out from its
//! composition with the plane geometry of the spec, and every pixel that no edge crosses must be that colour exactly. A pixel
//! crossed by a curved or a moved edge takes its share from the rasteriser and is not judged here; the pixels that are judged
//! hold every border, gap, corner and cut of the fixtures. No product code paints these values.

mod common;

use common::expected;

/// Whether a pixel lies wholly inside a region, wholly outside it, or is crossed by its edge.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Side {
    In,
    Out,
    Edge,
}

/// A rectangle from `left` to `right` across and from `top` to `bottom` down, with corners of radius `radius`: section 2's
/// `Q(a, ρ)` written out.
#[derive(Clone, Copy)]
struct Rounded {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    radius: f64,
}

/// How far a curved edge must be from a pixel for the pixel to be judged: half a pixel, within which the rasteriser's
/// anti-aliasing of an arc may leave a level or two short of whole coverage (section 5 leaves those shares to it).
const MARGIN: f64 = 0.5;

impl Rounded {
    /// `Q(a, ρ)` of a box at (`left`, `top`), `width` by `height`, with the written radius `r` and the inset `a`.
    fn of(left: f64, top: f64, width: f64, height: f64, r: f64, inset: f64) -> Rounded {
        let outer = r.min(width / 2.0).min(height / 2.0);
        Rounded {
            left: left + inset,
            top: top + inset,
            right: left + width - inset,
            bottom: top + height - inset,
            radius: (outer - inset).max(0.0),
        }
    }

    /// The distance from a point to the rectangle that the corner circles' centres span.
    fn from_centres(&self, x: f64, y: f64) -> f64 {
        let across = (self.left + self.radius - x).max(x - (self.right - self.radius)).max(0.0);
        let down = (self.top + self.radius - y).max(y - (self.bottom - self.radius)).max(0.0);
        (across * across + down * down).sqrt()
    }

    /// The side of the pixel whose top left corner is (`x`, `y`).
    fn side(&self, x: f64, y: f64) -> Side {
        let (x1, y1) = (x + 1.0, y + 1.0);
        if x1 <= self.left || x >= self.right || y1 <= self.top || y >= self.bottom {
            return Side::Out;
        }
        // The shape is convex: a pixel whose four corners are inside is inside.
        let inside = |px: f64, py: f64| {
            px >= self.left && px <= self.right && py >= self.top && py <= self.bottom && {
                let d = self.from_centres(px, py);
                d == 0.0 || d <= self.radius - MARGIN
            }
        };
        if inside(x, y) && inside(x1, y) && inside(x, y1) && inside(x1, y1) {
            return Side::In;
        }
        // The distance from the pixel to the rectangle of the centres: past the radius, the pixel is outside.
        let across = (self.left + self.radius - x1).max(x - (self.right - self.radius)).max(0.0);
        let down = (self.top + self.radius - y1).max(y - (self.bottom - self.radius)).max(0.0);
        if (across * across + down * down).sqrt() > self.radius + MARGIN {
            return Side::Out;
        }
        Side::Edge
    }
}

/// A region of the picture: a rounded rectangle, one region less another, both regions, or a part of the picture that is
/// not judged (a box that turns).
#[derive(Clone)]
enum Region {
    Shape(Rounded),
    Less(Box<Region>, Box<Region>),
    Both(Box<Region>, Box<Region>),
    Unjudged(Rounded),
}

impl Region {
    fn side(&self, x: f64, y: f64) -> Side {
        match self {
            Region::Shape(shape) => shape.side(x, y),
            Region::Less(a, b) => match (a.side(x, y), b.side(x, y)) {
                (Side::Out, _) | (_, Side::In) => Side::Out,
                (Side::In, Side::Out) => Side::In,
                _ => Side::Edge,
            },
            Region::Both(a, b) => match (a.side(x, y), b.side(x, y)) {
                (Side::Out, _) | (_, Side::Out) => Side::Out,
                (Side::In, Side::In) => Side::In,
                _ => Side::Edge,
            },
            Region::Unjudged(shape) => match shape.side(x, y) {
                Side::Out => Side::Out,
                _ => Side::Edge,
            },
        }
    }

    fn cut(self, to: &Region) -> Region {
        Region::Both(Box::new(self), Box::new(to.clone()))
    }
}

fn shape(shape: Rounded) -> Region {
    Region::Shape(shape)
}

fn ring(outer: Rounded, inner: Rounded) -> Region {
    Region::Less(Box::new(Region::Shape(outer)), Box::new(Region::Shape(inner)))
}

/// A colour `#rrggbb` with an alpha.
fn rgba(hex: u32, alpha: u8) -> [u8; 4] {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8, alpha]
}

fn rgb(hex: u32) -> [u8; 4] {
    rgba(hex, 255)
}

/// Paints the regions in order over the ground by src-over (section 5): each level is `(src × a + dst × (255 − a)) / 255`,
/// rounded to the nearest, halves up. `None` for a pixel that an edge crosses.
fn paint(ground: u32, layers: &[(Region, [u8; 4])], x: u32, y: u32) -> Option<[u8; 4]> {
    let mut pixel = rgb(ground);
    for (region, colour) in layers {
        match region.side(f64::from(x), f64::from(y)) {
            Side::Out => {}
            Side::Edge => return None,
            Side::In => {
                let a = u32::from(colour[3]);
                for channel in 0..3 {
                    let level = u32::from(colour[channel]) * a + u32::from(pixel[channel]) * (255 - a);
                    pixel[channel] = ((level + 127) / 255) as u8;
                }
            }
        }
    }
    Some(pixel)
}

/// Holds the checked-in picture of a frame to the painted colours; gives how many pixels were judged and how many differ.
fn judge(fixture: &str, number: u64, ground: u32, layers: &[(Region, [u8; 4])]) -> usize {
    let picture = expected(fixture, number);
    let (mut judged, mut wrong) = (0, Vec::new());
    for y in 0..180 {
        for x in 0..320 {
            if let Some(worked) = paint(ground, layers, x, y) {
                judged += 1;
                let held = picture.pixel(x, y).unwrap();
                if held != worked {
                    wrong.push((x, y, held, worked));
                }
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{fixture} frame {number}: {} pixels are not the worked colour, the first: {:?}",
        wrong.len(),
        &wrong[..wrong.len().min(8)]
    );
    judged
}

const PAPER: u32 = 0xf0e8d8;
const GROUND: u32 = 0x203040;

/// A box with square corners at whole pixels, its fill and a border of one or two lines.
fn bordered(left: f64, top: f64, width: f64, height: f64, fill: u32, lines: &[(f64, f64)], colour: [u8; 4]) -> Vec<(Region, [u8; 4])> {
    let mut layers = vec![(shape(Rounded::of(left, top, width, height, 0.0, 0.0)), rgb(fill))];
    for &(from, to) in lines {
        layers.push((
            ring(
                Rounded::of(left, top, width, height, 0.0, from),
                Rounded::of(left, top, width, height, 0.0, to),
            ),
            colour,
        ));
    }
    layers
}

fn borders(moved: f64) -> Vec<(Region, [u8; 4])> {
    let mut layers = bordered(20.0, 20.0, 120.0, 70.0, PAPER, &[(0.0, 6.0)], rgb(0xc02020));
    layers.extend(bordered(170.0, 20.0, 130.0, 70.0, PAPER, &[(0.0, 3.0), (6.0, 9.0)], rgb(0x2050c0)));
    layers.extend(bordered(20.0, 110.0, 120.0, 50.0, PAPER, &[(0.0, 8.0)], rgba(0x000000, 0x80)));
    layers.extend(bordered(170.0 + moved, 110.0, 60.0, 50.0, PAPER, &[(0.0, 6.0)], rgb(0x208040)));
    layers
}

#[test]
fn the_pictures_of_borders_hold_the_worked_lines_gaps_and_alpha() {
    // The box at the bottom right moves from 0.5 s to 1.5 s: at frames 0 and 5 it has not moved, at frame 19 it has moved 60.
    let judged = judge("box-borders", 0, GROUND, &borders(0.0));
    assert_eq!(judged, 320 * 180, "every edge of the borders lies on whole pixels");
    judge("box-borders", 5, GROUND, &borders(0.0));
    assert_eq!(judge("box-borders", 19, GROUND, &borders(60.0)), 320 * 180);
}

fn corners(turned: bool) -> Vec<(Region, [u8; 4])> {
    let stripe = |top: f64| (shape(Rounded::of(0.0, top, 320.0, 30.0, 0.0, 0.0)), rgb(0x4a6a8a));
    let red = rgb(0xc02020);
    let mut layers = vec![
        stripe(0.0),
        stripe(60.0),
        stripe(120.0),
        (shape(Rounded::of(20.0, 15.0, 120.0, 70.0, 20.0, 0.0)), rgb(PAPER)),
        (shape(Rounded::of(170.0, 15.0, 130.0, 70.0, 16.0, 0.0)), rgb(PAPER)),
        (
            ring(
                Rounded::of(170.0, 15.0, 130.0, 70.0, 16.0, 0.0),
                Rounded::of(170.0, 15.0, 130.0, 70.0, 16.0, 5.0),
            ),
            red,
        ),
        (shape(Rounded::of(20.0, 110.0, 110.0, 30.0, 400.0, 0.0)), rgb(0xe0a020)),
        (shape(Rounded::of(150.0, 100.0, 50.0, 50.0, 300.0, 0.0)), rgb(0xe0a020)),
        (
            ring(
                Rounded::of(150.0, 100.0, 50.0, 50.0, 300.0, 0.0),
                Rounded::of(150.0, 100.0, 50.0, 50.0, 300.0, 3.0),
            ),
            red,
        ),
    ];
    let turning = |inset: f64| Rounded::of(220.0, 100.0, 80.0, 60.0, 18.0, inset);
    if turned {
        // Turned by 45 degrees about its centre (260, 130): nothing of it lies more than 50 from the centre.
        layers.push((Region::Unjudged(Rounded::of(209.0, 79.0, 102.0, 102.0, 51.0, 0.0)), rgb(0)));
    } else {
        let blue = rgb(0x2050c0);
        layers.push((shape(turning(0.0)), rgb(PAPER)));
        layers.push((ring(turning(0.0), turning(3.0)), blue));
        layers.push((ring(turning(6.0), turning(9.0)), blue));
    }
    layers
}

#[test]
fn the_pictures_of_rounded_corners_show_the_ground_outside_each_worked_corner() {
    let judged = judge("box-corners", 0, GROUND, &corners(false));
    assert!(judged > 320 * 180 * 90 / 100, "{judged} pixels judged");
    judge("box-corners", 5, GROUND, &corners(false));
    judge("box-corners", 19, GROUND, &corners(true));
}

fn cut(bar_down: f64) -> Vec<(Region, [u8; 4])> {
    let red = rgb(0xc02020);
    let blue = rgb(0x2050c0);
    let green = rgb(0x208040);
    let gold = rgb(0xe0a020);
    let plain = |left: f64, top: f64, width: f64, height: f64| shape(Rounded::of(left, top, width, height, 0.0, 0.0));
    // Left: border 4, radius 24, cuts at its inner edge, Q(4, 20).
    let left = |inset: f64| Rounded::of(20.0, 30.0, 100.0, 120.0, 24.0, inset);
    let left_inside = shape(left(4.0));
    // Middle: the same edges, nothing cut.
    let middle = |inset: f64| Rounded::of(140.0, 30.0, 70.0, 120.0, 24.0, inset);
    // Right: radius 12, no border, cuts at its outer edge; inside it a circle of radius 30 that cuts in turn.
    let right = shape(Rounded::of(230.0, 30.0, 80.0, 120.0, 12.0, 0.0));
    let circle = Rounded::of(240.0, 50.0, 60.0, 60.0, 30.0, 0.0);
    vec![
        (shape(left(0.0)), rgb(PAPER)),
        (ring(left(0.0), left(4.0)), red),
        (plain(-10.0, 50.0 + bar_down, 160.0, 24.0).cut(&left_inside), blue),
        (plain(20.0, 30.0, 30.0, 30.0).cut(&left_inside), green),
        (shape(middle(0.0)), rgb(PAPER)),
        (ring(middle(0.0), middle(4.0)), red),
        (plain(125.0, 80.0, 100.0, 24.0), blue),
        (plain(140.0, 30.0, 30.0, 30.0), green),
        (right.clone(), rgb(PAPER)),
        (shape(circle).cut(&right), gold),
        (plain(220.0, 75.0, 100.0, 10.0).cut(&shape(circle)).cut(&right), blue),
        (plain(220.0, 130.0, 100.0, 10.0).cut(&right), green),
    ]
}

#[test]
fn the_pictures_of_cut_children_hold_nothing_of_a_child_outside_the_worked_inner_edge() {
    // The bar of the left box moves down 60 from 0.5 s to 1.5 s.
    let judged = judge("box-cut", 0, GROUND, &cut(0.0));
    assert!(judged > 320 * 180 * 90 / 100, "{judged} pixels judged");
    judge("box-cut", 5, GROUND, &cut(0.0));
    judge("box-cut", 19, GROUND, &cut(60.0));
}
