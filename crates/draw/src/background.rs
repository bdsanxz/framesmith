//! The layers of a box's background (`docs/spec/render-command.md`, section 4.6): what is written, the numbers of a layer
//! worked out for a box of a given size, and the colour of a layer at a point. Both renderers take a layer's numbers from
//! here; the processor's renderer finds its colours with [`ResolvedLayer::colour_at`].

use layout::Rect;

/// A colour with its alpha, each value from 0 to 255, not premultiplied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Rgba {
    /// The colour premultiplied by its alpha, each value from 0 to 1.
    pub fn premultiplied(self) -> [f64; 4] {
        let alpha = f64::from(self.alpha) / 255.0;
        [
            f64::from(self.red) / 255.0 * alpha,
            f64::from(self.green) / 255.0 * alpha,
            f64::from(self.blue) / 255.0 * alpha,
            alpha,
        ]
    }
}

/// A length of a background: pixels, or a percentage of what the rule names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Amount {
    Pixels(f64),
    Percent(f64),
}

impl Amount {
    /// The length in pixels, a percentage taken of `whole`.
    pub fn of(self, whole: f64) -> f64 {
        match self {
            Amount::Pixels(pixels) => pixels,
            Amount::Percent(share) => whole * share / 100.0,
        }
    }
}

/// A stop of a gradient: its colour and its place on the gradient's line (rule 7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    pub colour: Rgba,
    pub place: Amount,
}

/// The shape of a radial gradient (rule 6): a circle with a radius in pixels or to the farthest corner, or an ellipse with
/// radii in percentages of the tile or through the farthest corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Circle(Option<f64>),
    Ellipse(Option<(f64, f64)>),
}

/// What a layer draws.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Colour(Rgba),
    /// A linear gradient: its angle in degrees (0 up, 90 right) and its stops.
    Linear {
        angle: f64,
        stops: Vec<Stop>,
    },
    /// A radial gradient: its shape, its centre in percentages of the tile (`None` for the middle) and its stops.
    Radial {
        shape: Shape,
        centre: Option<(f64, f64)>,
        stops: Vec<Stop>,
    },
}

/// The size of a layer's tile: the box's own size, or a width and a height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
    Auto,
    Given(Amount, Amount),
}

/// One layer of a background, with its settings (rule 2).
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub paint: Paint,
    pub size: Size,
    pub position: (Amount, Amount),
    pub repeat: bool,
}

/// The background of a box: its rectangle in the frame and its layers, the first listed on top.
#[derive(Clone, Debug, PartialEq)]
pub struct Background {
    pub rect: Rect,
    pub layers: Vec<Layer>,
}

/// The line or shape of a resolved layer, in pixels of its tile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gradient {
    /// A colour: the one stop holds everywhere.
    Flat,
    /// A linear gradient: `t = (u − cx) sin − (v − cy) cos + half`, with `(cx, cy)` the tile's centre and `half` half the
    /// line's length (rule 5).
    Linear { sin: f64, cos: f64, cx: f64, cy: f64, half: f64 },
    /// A radial gradient: `t = rx √((du / rx)² + (dv / ry)²)` from the centre `(cx, cy)` (rule 6).
    Radial { cx: f64, cy: f64, rx: f64, ry: f64 },
}

/// The numbers of a layer for a box of a given size: its tile from the box's top left corner, whether it repeats, its line or
/// shape, and its stops as places in pixels that never go back with their colours premultiplied (rule 7).
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedLayer {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub repeat: bool,
    pub gradient: Gradient,
    pub stops: Vec<(f64, [f64; 4])>,
}

impl Layer {
    /// The layer's numbers for a box `width` by `height` pixels (rules 2 to 7).
    pub fn resolve(&self, width: f64, height: f64) -> ResolvedLayer {
        let (tile_width, tile_height) = match self.size {
            Size::Auto => (width, height),
            Size::Given(across, down) => (across.of(width).max(0.0), down.of(height).max(0.0)),
        };
        let place = |amount: Amount, room: f64| match amount {
            Amount::Pixels(pixels) => pixels,
            Amount::Percent(share) => room * share / 100.0,
        };
        let left = place(self.position.0, width - tile_width);
        let top = place(self.position.1, height - tile_height);
        let (gradient, length, stops) = match &self.paint {
            Paint::Colour(colour) => (
                Gradient::Flat,
                0.0,
                vec![Stop {
                    colour: *colour,
                    place: Amount::Pixels(0.0),
                }],
            ),
            Paint::Linear { angle, stops } => {
                let (sin, cos) = angle.to_radians().sin_cos();
                let length = (tile_width * sin).abs() + (tile_height * cos).abs();
                let gradient = Gradient::Linear {
                    sin,
                    cos,
                    cx: tile_width / 2.0,
                    cy: tile_height / 2.0,
                    half: length / 2.0,
                };
                (gradient, length, stops.clone())
            }
            Paint::Radial { shape, centre, stops } => {
                let (cx, cy) = match centre {
                    Some((x, y)) => (tile_width * x / 100.0, tile_height * y / 100.0),
                    None => (tile_width / 2.0, tile_height / 2.0),
                };
                let farther_x = cx.max(tile_width - cx);
                let farther_y = cy.max(tile_height - cy);
                let (rx, ry) = match shape {
                    Shape::Circle(Some(radius)) => (*radius, *radius),
                    Shape::Circle(None) => {
                        let corner = farther_x.hypot(farther_y);
                        (corner, corner)
                    }
                    Shape::Ellipse(Some((across, down))) => (tile_width * across / 100.0, tile_height * down / 100.0),
                    Shape::Ellipse(None) => (farther_x * std::f64::consts::SQRT_2, farther_y * std::f64::consts::SQRT_2),
                };
                (Gradient::Radial { cx, cy, rx, ry }, rx, stops.clone())
            }
        };
        let mut least = f64::NEG_INFINITY;
        let stops = stops
            .iter()
            .map(|stop| {
                let place = stop.place.of(length).max(least);
                least = place;
                (place, stop.colour.premultiplied())
            })
            .collect();
        ResolvedLayer {
            left,
            top,
            width: tile_width,
            height: tile_height,
            repeat: self.repeat,
            gradient,
            stops,
        }
    }
}

impl ResolvedLayer {
    /// The layer's premultiplied colour at the point `(x, y)` of the box, from its top left corner (rules 3 to 7); `None`
    /// where the layer does not show.
    pub fn colour_at(&self, x: f64, y: f64) -> Option<[f64; 4]> {
        if !(self.width > 0.0 && self.height > 0.0) {
            return None;
        }
        let (mut u, mut v) = (x - self.left, y - self.top);
        if self.repeat {
            u = u.rem_euclid(self.width);
            v = v.rem_euclid(self.height);
        } else if !(0.0..self.width).contains(&u) || !(0.0..self.height).contains(&v) {
            return None;
        }
        let t = match self.gradient {
            Gradient::Flat => 0.0,
            Gradient::Linear { sin, cos, cx, cy, half } => (u - cx) * sin - (v - cy) * cos + half,
            Gradient::Radial { cx, cy, rx, ry } => {
                let (du, dv) = ((u - cx) / rx, (v - cy) / ry);
                rx * (du * du + dv * dv).sqrt()
            }
        };
        Some(self.colour_on_line(t))
    }

    /// The colour of the stops at the place `t` on the line (rule 7).
    pub fn colour_on_line(&self, t: f64) -> [f64; 4] {
        let first = self.stops[0];
        if t < first.0 {
            return first.1;
        }
        for pair in self.stops.windows(2) {
            let ((p0, c0), (p1, c1)) = (pair[0], pair[1]);
            if p0 <= t && t < p1 {
                let share = (t - p0) / (p1 - p0);
                return [0, 1, 2, 3].map(|at| c0[at] + (c1[at] - c0[at]) * share);
            }
        }
        self.stops[self.stops.len() - 1].1
    }
}

/// A premultiplied value from 0 to 1 as 8 bits: `⌊255 v + 0.5⌋` (rule 8).
pub fn level(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8
}
