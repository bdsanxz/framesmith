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
    fn distance_to_centres(&self, x: f64, y: f64) -> f64 {
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
                let d = self.distance_to_centres(px, py);
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
    judge("box-corners", 10, GROUND, &corners(true));
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

/// Whether a point of the box's own coordinates lies inside a rounded rectangle.
fn holds(shape: &Rounded, x: f64, y: f64) -> bool {
    x >= shape.left && x <= shape.right && y >= shape.top && y <= shape.bottom && shape.distance_to_centres(x, y) <= shape.radius
}

/// Lays an opaque colour over a pixel with the share `k` of 255, rounded to the nearest level (section 5).
fn laid(under: [f64; 3], colour: [u8; 4], k: f64) -> [f64; 3] {
    let mut out = under;
    for channel in 0..3 {
        out[channel] = (f64::from(colour[channel]) * k + under[channel] * (255.0 - k)) / 255.0;
    }
    out
}

#[test]
fn the_turned_rounded_box_is_weighed_once_at_its_edge() {
    // Frame 10 of box-corners: the box at (220, 100), 80 by 60, radius 18, with `9 double`, turned 22.5 degrees (half of its
    // linear tween to 45) about its centre (260, 130). Each pixel's share of the box's outer edge and of its two lines is
    // measured here by 64 by 64 points across the pixel, taken back through the turn to the box's own coordinates; the
    // background is laid with the share of the outer edge alone, and then each line with its share, over the stripes.
    let picture = expected("box-corners", 10);
    let angle = 22.5_f64.to_radians();
    let (sin, cos) = angle.sin_cos();
    let edge = |inset: f64| Rounded::of(220.0, 100.0, 80.0, 60.0, 18.0, inset);
    let (outer, gap_outer, gap_inner, inner) = (edge(0.0), edge(3.0), edge(6.0), edge(9.0));
    const POINTS: u32 = 64;
    let (mut judged, mut wrong, mut worst) = (0, Vec::new(), 0_u8);
    for y in 75..180_u32 {
        for x in 205..316_u32 {
            let (mut fill, mut first, mut second) = (0_u32, 0_u32, 0_u32);
            for j in 0..POINTS {
                for i in 0..POINTS {
                    let px = f64::from(x) + (f64::from(i) + 0.5) / f64::from(POINTS) - 260.0;
                    let py = f64::from(y) + (f64::from(j) + 0.5) / f64::from(POINTS) - 130.0;
                    // Back through a clockwise turn on the screen, whose y runs down.
                    let (bx, by) = (260.0 + px * cos + py * sin, 130.0 - px * sin + py * cos);
                    if holds(&outer, bx, by) {
                        fill += 1;
                        if !holds(&gap_outer, bx, by) {
                            first += 1;
                        } else if holds(&gap_inner, bx, by) && !holds(&inner, bx, by) {
                            second += 1;
                        }
                    }
                }
            }
            if fill == 0 {
                continue;
            }
            let share = |count: u32| (f64::from(count) * 255.0 / f64::from(POINTS * POINTS)).round();
            let ground = if (120..150).contains(&y) { rgb(0x4a6a8a) } else { rgb(GROUND) };
            let under = [f64::from(ground[0]), f64::from(ground[1]), f64::from(ground[2])];
            let worked = laid(
                laid(laid(under, rgb(PAPER), share(fill)), rgb(0x2050c0), share(first)),
                rgb(0x2050c0),
                share(second),
            );
            let held = picture.pixel(x, y).unwrap();
            judged += 1;
            let apart = (0..3).map(|c| (f64::from(held[c]) - worked[c]).abs().round() as u8).max().unwrap();
            worst = worst.max(apart);
            if apart > 3 {
                wrong.push((x, y, held, worked.map(|v| v.round() as u8)));
            }
        }
    }
    println!("{judged} pixels of the turned box judged; the farthest is {worst} levels from the worked colour");
    assert!(judged > 4000, "{judged} pixels judged");
    assert!(
        wrong.is_empty(),
        "{} pixels lie more than 3 levels from the worked colour, the first: {:?}",
        wrong.len(),
        &wrong[..wrong.len().min(8)]
    );
}

impl Region {
    /// Whether a point lies in the region; `None` in a part that is not judged.
    fn holds(&self, x: f64, y: f64) -> Option<bool> {
        Some(match self {
            Region::Shape(shape) => holds(shape, x, y),
            Region::Less(a, b) => a.holds(x, y)? && !b.holds(x, y)?,
            Region::Both(a, b) => a.holds(x, y)? && b.holds(x, y)?,
            Region::Unjudged(shape) => {
                if holds(shape, x, y) {
                    return None;
                }
                false
            }
        })
    }
}

/// One layer of the every-pixel model: a region laid in its colour, or layers drawn into a picture of their own, transparent
/// at first, that is then weighed by the share of a cut and laid over what is there (section 4).
enum Layer {
    Paint(Region, [u8; 4]),
    Cut(Region, Vec<Layer>),
}

fn painted(layers: Vec<(Region, [u8; 4])>) -> Vec<Layer> {
    layers.into_iter().map(|(region, colour)| Layer::Paint(region, colour)).collect()
}

/// The share of a pixel that a region holds, as a whole level, measured by 32 by 32 points; `None` in a part not judged.
fn share_of(region: &Region, x: u32, y: u32) -> Option<f64> {
    const POINTS: u32 = 32;
    // A pixel wholly inside or outside needs no points.
    match region.side(f64::from(x), f64::from(y)) {
        Side::In => return Some(1.0),
        Side::Out => return Some(0.0),
        Side::Edge => {}
    }
    let mut inside = 0_u32;
    for j in 0..POINTS {
        for i in 0..POINTS {
            let px = f64::from(x) + (f64::from(i) + 0.5) / f64::from(POINTS);
            let py = f64::from(y) + (f64::from(j) + 0.5) / f64::from(POINTS);
            if region.holds(px, py)? {
                inside += 1;
            }
        }
    }
    Some((f64::from(inside) * 255.0 / f64::from(POINTS * POINTS)).round() / 255.0)
}

/// Lays the layers over a premultiplied pixel `[red, green, blue, alpha]`, each value from 0 to 1, by src-over.
fn laid_layers(mut pixel: [f64; 4], layers: &[Layer], x: u32, y: u32) -> Option<[f64; 4]> {
    for layer in layers {
        let (source, weight) = match layer {
            Layer::Paint(region, paint) => {
                let share = share_of(region, x, y)? * f64::from(paint[3]) / 255.0;
                let colour = [0, 1, 2].map(|c| f64::from(paint[c]) / 255.0);
                ([colour[0], colour[1], colour[2], 1.0], share)
            }
            Layer::Cut(region, inside) => {
                let weight = share_of(region, x, y)?;
                (laid_layers([0.0; 4], inside, x, y)?, weight)
            }
        };
        let kept = 1.0 - source[3] * weight;
        pixel = [0, 1, 2, 3].map(|c| source[c] * weight + pixel[c] * kept);
    }
    Some(pixel)
}

/// Holds every pixel of a checked-in picture, edges included, to the layers laid in order over the ground, each with its
/// share of the pixel: at most `levels` apart in any value.
fn judge_every_pixel(fixture: &str, number: u64, layers: &[Layer], levels: f64) {
    let picture = expected(fixture, number);
    let ground = [GROUND >> 16, (GROUND >> 8) & 0xff, GROUND & 0xff].map(|v| f64::from(v) / 255.0);
    let (mut wrong, mut worst, mut judged) = (Vec::new(), 0.0_f64, 0);
    for y in 0..180_u32 {
        for x in 0..320_u32 {
            let Some(colour) = laid_layers([ground[0], ground[1], ground[2], 1.0], layers, x, y) else {
                continue;
            };
            judged += 1;
            let held = picture.pixel(x, y).unwrap();
            let apart = (0..3).map(|c| (f64::from(held[c]) - colour[c] * 255.0).abs()).fold(0.0, f64::max);
            worst = worst.max(apart);
            if apart > levels {
                wrong.push((x, y, held, colour.map(|v| (v * 255.0).round() as u8)));
            }
        }
    }
    println!("{fixture} frame {number}: {judged} pixels judged, the farthest {worst:.1} levels from the worked colour");
    assert!(judged > 320 * 180 * 8 / 10, "{fixture} frame {number}: {judged} pixels judged");
    assert!(
        wrong.is_empty(),
        "{fixture} frame {number}: {} pixels lie more than {levels} levels from the worked colour, the first: {:?}",
        wrong.len(),
        &wrong[..wrong.len().min(8)]
    );
}

/// The layers of box-cut as section 4 draws them: each box that cuts draws its children into a layer of their own.
fn cut_layers(bar_down: f64) -> Vec<Layer> {
    let (red, blue, green, gold) = (rgb(0xc02020), rgb(0x2050c0), rgb(0x208040), rgb(0xe0a020));
    let plain = |left: f64, top: f64, width: f64, height: f64| shape(Rounded::of(left, top, width, height, 0.0, 0.0));
    let left = |inset: f64| Rounded::of(20.0, 30.0, 100.0, 120.0, 24.0, inset);
    let middle = |inset: f64| Rounded::of(140.0, 30.0, 70.0, 120.0, 24.0, inset);
    let right = Rounded::of(230.0, 30.0, 80.0, 120.0, 12.0, 0.0);
    let circle = Rounded::of(240.0, 50.0, 60.0, 60.0, 30.0, 0.0);
    vec![
        Layer::Paint(shape(left(0.0)), rgb(PAPER)),
        Layer::Paint(ring(left(0.0), left(4.0)), red),
        Layer::Cut(
            shape(left(4.0)),
            vec![
                Layer::Paint(plain(-10.0, 50.0 + bar_down, 160.0, 24.0), blue),
                Layer::Paint(plain(20.0, 30.0, 30.0, 30.0), green),
            ],
        ),
        Layer::Paint(shape(middle(0.0)), rgb(PAPER)),
        Layer::Paint(ring(middle(0.0), middle(4.0)), red),
        Layer::Paint(plain(125.0, 80.0, 100.0, 24.0), blue),
        Layer::Paint(plain(140.0, 30.0, 30.0, 30.0), green),
        Layer::Paint(shape(right), rgb(PAPER)),
        Layer::Cut(
            shape(right),
            vec![
                Layer::Paint(shape(circle), gold),
                Layer::Cut(shape(circle), vec![Layer::Paint(plain(220.0, 75.0, 100.0, 10.0), blue)]),
                Layer::Paint(plain(220.0, 130.0, 100.0, 10.0), green),
            ],
        ),
    ]
}

#[test]
fn every_pixel_of_the_rounded_corners_and_cuts_holds_its_worked_share_of_each_edge() {
    for number in [0, 5] {
        judge_every_pixel("box-corners", number, &painted(corners(false)), 3.0);
        judge_every_pixel("box-cut", number, &cut_layers(0.0), 3.0);
    }
    judge_every_pixel("box-corners", 19, &painted(corners(true)), 3.0);
    judge_every_pixel("box-cut", 19, &cut_layers(60.0), 3.0);
}
