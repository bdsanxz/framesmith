//! Per-frame evaluation: what is drawn at one frame, and where. A pure function of the frame.
//!
//! The result is a flat list of drawing steps that both renderers read, and the list of footage
//! pictures the frame needs. Whatever lies wholly under an opaque element that fills the frame
//! is left out, so its footage is not even decoded.

use crate::anim::{Prop, Values};
use crate::model::{Blend, Composition, Finish, Fit, GlyphRun, Kind, Node, Opening, Tile};
use crate::outline;
use crate::style::{Paint, Rgba, Shadow};
use std::borrow::Cow;
use timebase::Time;
use vello_cpu::kurbo::{Affine, BezPath, Rect, Shape as _};

pub enum Op<'a> {
    /// Everything up to the matching `Pop` is drawn apart, then cut, blurred, faded and blended
    /// onto the frame as one picture.
    Push {
        transform: Affine,
        clip: Option<Clip>,
        blend: Blend,
        opacity: f32,
        /// The standard deviation of the blur, in the element's own pixels.
        blur: f32,
        /// Multiplies the colours of the finished picture, after the blur. 1 leaves them.
        brightness: f32,
        /// What else is done to the finished picture before it is put on the frame.
        effect: Option<Effect>,
        /// A torn edge, uneven ink and a shadow of the finished picture, before its opacity.
        finish: Option<Finish>,
    },
    Pop,
    Fill {
        transform: Affine,
        rect: Rect,
        radius: f64,
        paint: &'a Paint,
        /// One tile of the paint, when the paint does not simply fill `rect`.
        tile: Option<Tile>,
        opacity: f32,
    },
    /// The shadow of a box: a blurred rectangle.
    Shadow {
        transform: Affine,
        rect: Rect,
        radius: f64,
        deviation: f64,
        color: Rgba,
        opacity: f32,
    },
    Border {
        transform: Affine,
        rect: Rect,
        radius: f64,
        width: f64,
        color: Rgba,
        opacity: f32,
    },
    Shape {
        transform: Affine,
        /// The outlines as the frame shows them: the shape's own, or the part of them drawn.
        paths: Cow<'a, [BezPath]>,
        stroke: Option<f64>,
        color: Rgba,
        /// A shadow under the outlines, in their own units.
        shadow: Option<Shadow>,
        opacity: f32,
    },
    /// The shadows of an element's words. A renderer may keep the picture it makes of them
    /// under `key`, which changes with the element and with how much of its text shows.
    TextShadow {
        transform: Affine,
        key: (usize, usize),
        runs: &'a [GlyphRun],
        node: &'a Node,
        opacity: f32,
    },
    Glyphs {
        transform: Affine,
        run: &'a GlyphRun,
        color: Rgba,
        stroke: Option<(f64, Rgba)>,
        opacity: f32,
    },
    /// A still picture, stretched over `rect`.
    Still {
        transform: Affine,
        still: usize,
        rect: Rect,
        radius: f64,
        opacity: f32,
        fit: Fit,
    },
    Video {
        transform: Affine,
        video: usize,
        rect: Rect,
        radius: f64,
        opacity: f32,
    },
}

/// The step that closes the layer opened at `open`.
pub fn closing(ops: &[Op], open: usize) -> usize {
    let mut depth = 0_usize;
    for (index, op) in ops.iter().enumerate().skip(open) {
        match op {
            Op::Push { .. } => depth += 1,
            Op::Pop => {
                depth -= 1;
                if depth == 0 {
                    return index;
                }
            }
            _ => {}
        }
    }
    ops.len() - 1
}

/// What is done to the finished picture of a group before it is put on the frame. See
/// `docs/spec/native-notation.md`, section 8.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// Turned in space. The matrix takes a point of the flat picture, in the frame, to where
    /// it is seen: its rows are applied to `(x, y, 1)` and the first two results divided by the
    /// third.
    Turn([[f64; 3]; 3]),
    /// Shown through an opening: how it opens, how far (0 to 1), and the element's box in the
    /// frame.
    Reveal { opening: Opening, progress: f64, around: Rect },
}

/// The product of two 3x3 matrices.
fn times(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    std::array::from_fn(|row| std::array::from_fn(|column| (0..3).map(|k| a[row][k] * b[k][column]).sum()))
}

/// The matrix of a turn of `about_x` and then `about_y` degrees around the point `origin` of
/// the frame, seen from `distance` pixels away.
pub fn turn(origin: (f64, f64), about_x: f64, about_y: f64, distance: f64) -> [[f64; 3]; 3] {
    let (a, b) = (about_x.to_radians(), about_y.to_radians());
    // A point `(x, y)` from the origin goes to `(X, Y)` and comes `Z` towards the eye; what is
    // seen is `(X, Y)` divided by `1 - Z / distance`.
    let turned = [
        [b.cos(), a.sin() * b.sin(), 0.0],
        [0.0, a.cos(), 0.0],
        [b.sin() / distance, -a.sin() * b.cos() / distance, 1.0],
    ];
    let to_origin = [[1.0, 0.0, -origin.0], [0.0, 1.0, -origin.1], [0.0, 0.0, 1.0]];
    let back = [[1.0, 0.0, origin.0], [0.0, 1.0, origin.1], [0.0, 0.0, 1.0]];
    times(back, times(turned, to_origin))
}

/// What a layer is cut to, in the element's own space.
pub enum Clip {
    /// A rectangle with rounded corners.
    Rounded(Rect, f64),
    Outline(BezPath),
}

/// One picture of footage a frame needs.
pub struct Footage<'a> {
    pub node: &'a Node,
    pub video: usize,
    /// Which picture of the source, counted from its first.
    pub source_frame: u64,
}

pub struct Frame<'a> {
    pub ops: Vec<Op<'a>>,
    pub footage: Vec<Footage<'a>>,
}

pub struct Evaluator<'a> {
    composition: &'a Composition,
}

struct Walk<'a> {
    fps: u32,
    /// The frame being described.
    index: i64,
    frame: Rect,
    ops: Vec<Op<'a>>,
    footage: Vec<Footage<'a>>,
}

impl<'a> Evaluator<'a> {
    pub fn new(composition: &'a Composition) -> Evaluator<'a> {
        Evaluator { composition }
    }

    /// The drawing steps of `frame`, in drawing order.
    pub fn frame(&self, frame: u64) -> Frame<'a> {
        let frame_rect = Rect::new(0.0, 0.0, f64::from(self.composition.width), f64::from(self.composition.height));
        let mut walk = Walk {
            fps: self.composition.fps,
            index: frame as i64,
            frame: frame_rect,
            ops: Vec::new(),
            footage: Vec::new(),
        };
        for child in &self.composition.root.children {
            walk.node(child, Time::ZERO, Affine::IDENTITY, None);
        }
        let (ops, footage) = cull(walk.ops, walk.footage, frame_rect);
        Frame { ops, footage }
    }
}

/// The time at which frame `index` reads the tweens of an element whose parent's time begins
/// at `offset`. A tween or a set written within a thousandth of a frame after the frame counts
/// as on it.
pub fn moment(index: i64, fps: u32, offset: Time) -> f64 {
    (Time::of_frame(index, fps) - offset).to_f64() + 0.001 / f64::from(fps)
}

/// The values of the animated properties of `node` at `t`, a time of its parent.
pub fn values(node: &Node, t: f64) -> Values {
    let mut values = node.base;
    for track in &node.tracks {
        if let Some(value) = track.value(t) {
            values[track.prop.index()] = value;
        }
    }
    values
}

impl<'a> Walk<'a> {
    /// `offset` is when the time of the node's parent begins on the composition's timeline;
    /// `placed` overrides the node's box (a typed text moves its children).
    fn node(&mut self, node: &'a Node, offset: Time, parent: Affine, placed: Option<Rect>) {
        // The frame the node's own time begins on: its clip's first frame, by the frame grid
        // rule (`docs/spec/frame-time.md`).
        let mut began = offset.grid_frame(self.fps);
        if let Some((start, duration)) = node.window {
            began = (offset + start).grid_frame(self.fps);
            if self.index < began || self.index >= (offset + start + duration).grid_frame(self.fps) {
                return;
            }
        }
        let values = values(node, moment(self.index, self.fps, offset));
        let value = |prop: Prop| values[prop.index()];
        let opacity = value(Prop::Opacity).clamp(0.0, 1.0) as f32;
        if opacity <= 0.0 {
            return;
        }
        let rect = placed.unwrap_or(node.rect);
        let (width, height) = (rect.width(), rect.height());
        let origin = (node.origin.0.of(width), node.origin.1.of(height));
        let scale = value(Prop::Scale);
        let transform = parent
            * Affine::translate((rect.x0 + value(Prop::X), rect.y0 + value(Prop::Y)))
            * Affine::translate(origin)
            * Affine::rotate(value(Prop::Rotation).to_radians())
            * Affine::skew(0.0, value(Prop::SkewY).to_radians().tan())
            * Affine::scale_non_uniform(scale * value(Prop::ScaleX), scale)
            * Affine::translate((-origin.0, -origin.1));
        let own = Rect::new(0.0, 0.0, width, height);
        let blur = value(Prop::Blur).max(0.0) as f32;
        let brightness = value(Prop::Brightness).max(0.0) as f32;
        let lit = (brightness - 1.0).abs() > 0.001;
        // Turned in space, or opening: one of the two.
        let (about_x, about_y) = (value(Prop::RotateX), value(Prop::RotateY));
        let turned = node.look.perspective.filter(|_| about_x.abs() > 1e-6 || about_y.abs() > 1e-6);
        let reveal = value(Prop::Reveal).clamp(0.0, 1.0);
        let effect = match (turned, node.look.transition) {
            (Some(distance), _) => {
                let centre = transform * vello_cpu::kurbo::Point::new(origin.0, origin.1);
                Some(Effect::Turn(turn((centre.x, centre.y), about_x, about_y, distance)))
            }
            (None, Some(opening)) if reveal < 1.0 - 1e-6 && opening != Opening::Cut => Some(Effect::Reveal {
                opening,
                progress: reveal,
                around: transform.transform_rect_bbox(own),
            }),
            _ => None,
        };

        // How much of a typed text shows, and the layout that goes with it.
        let typed = (!node.typed.is_empty()).then(|| {
            let shown = (value(Prop::Typed) + 1e-6).floor().clamp(0.0, (node.typed.len() - 1) as f64) as usize;
            (shown, node.typed[shown].as_ref())
        });
        // The number a counting text shows in this frame, among those it was laid out for.
        let counted = node.count.and_then(|format| {
            let number = format.write(value(Prop::Count));
            node.counted.binary_search_by(|(written, _)| written.as_str().cmp(&number)).ok()
        });
        let runs: &'a [GlyphRun] = match (typed, counted) {
            (_, Some(place)) => &node.counted[place].1,
            (Some((_, Some(layout))), None) => &layout.runs,
            _ => &node.runs,
        };

        // What the element itself draws, before its children.
        let look = &node.look;
        let has_text = !runs.is_empty();
        let own_steps = look.shadows.len()
            + look.background.len()
            + usize::from(look.border.is_some()) * (1 + usize::from(look.double))
            + usize::from(has_text) * (1 + usize::from(!node.text.shadows.is_empty()))
            + usize::from(matches!(node.kind, Kind::Image { .. } | Kind::Video { .. } | Kind::Shape { .. }));
        let alone = own_steps <= 1 && node.children.is_empty();
        let outline = look.clip_path.as_ref().map(|corners| {
            let mut path = BezPath::new();
            for (index, (x, y)) in corners.iter().enumerate() {
                let point = (x * width, y * height);
                if index == 0 {
                    path.move_to(point);
                } else {
                    path.line_to(point);
                }
            }
            path.close_path();
            path
        });
        // An opening with a hard edge is a cut of the box at the share that is open.
        let opens = look.transition == Some(Opening::Cut) && reveal < 1.0 - 1e-6;
        let outline = outline.or_else(|| opens.then(|| Rect::new(0.0, 0.0, width * reveal, height).to_path(0.1)));
        let cut = outline.is_some();
        // A box that covers the whole frame cuts nothing off.
        let clips = look.clip && !(look.radius == 0.0 && covers(transform, own, self.frame));
        let layered = cut
            || blur > 0.02
            || lit
            || effect.is_some()
            || look.finish.any()
            || clips
            || node.isolates
            || look.blend != Blend::Normal
            || (opacity < 1.0 && !alone);
        // Opacity goes to the layer when there is one, and to the single step when there is not.
        let step_opacity = if layered { 1.0 } else { opacity };
        if layered {
            self.ops.push(Op::Push {
                transform,
                clip: match outline {
                    Some(outline) => Some(Clip::Outline(outline)),
                    None => clips.then_some(Clip::Rounded(own, look.radius)),
                },
                blend: look.blend,
                opacity,
                blur: if blur > 0.02 { blur } else { 0.0 },
                brightness: if lit { brightness } else { 1.0 },
                effect,
                finish: look.finish.any().then_some(look.finish),
            });
        }
        // A box cut to an outline shows no shadow: the cut takes it away.
        for shadow in look.shadows.iter().rev().filter(|_| !cut) {
            self.ops.push(Op::Shadow {
                transform,
                rect: own + vello_cpu::kurbo::Vec2::new(shadow.x, shadow.y),
                radius: look.radius,
                deviation: shadow.blur / 2.0,
                color: shadow.color,
                opacity: step_opacity,
            });
        }
        for layer in look.background.iter().rev() {
            self.ops.push(Op::Fill {
                transform,
                rect: own,
                radius: look.radius,
                paint: &layer.paint,
                tile: layer.tile(width, height),
                opacity: step_opacity,
            });
        }
        match &node.kind {
            Kind::Image { still, fit } => self.ops.push(Op::Still {
                transform,
                still: *still,
                rect: own,
                radius: look.radius,
                opacity: step_opacity,
                fit: *fit,
            }),
            Kind::Video { video, media_start, .. } => {
                let since = (self.index - began).max(0) as u64;
                let first = media_start.grid_frame(self.fps).max(0) as u64;
                self.footage.push(Footage {
                    node,
                    video: *video,
                    source_frame: first + since,
                });
                self.ops.push(Op::Video {
                    transform,
                    video: *video,
                    rect: own,
                    radius: look.radius,
                    opacity: step_opacity,
                });
            }
            Kind::Shape {
                paths,
                view,
                stroke,
                color,
                shadow,
                dash,
                dashed,
            } => {
                // The outlines keep their shape: scaled to fit the box, and centred in it.
                let fit = (width / view.width()).min(height / view.height());
                let offset = ((width - view.width() * fit) / 2.0, (height - view.height() * fit) / 2.0);
                // A line drawn in part is cut to its share first, and dashed after.
                // Under a thousandth nothing is there, not even the round end of the line.
                let share = Some(value(Prop::Draw)).filter(|share| *share >= 0.001).unwrap_or(0.0);
                let whole = stroke.is_none() || share >= 1.0;
                let paths: Cow<'a, [BezPath]> = match (whole, dash.is_empty()) {
                    (true, true) => Cow::Borrowed(paths.as_slice()),
                    (true, false) => Cow::Borrowed(dashed.as_slice()),
                    (false, plain) => {
                        let part = paths.iter().map(|path| outline::drawn(path, share));
                        Cow::Owned(match plain {
                            true => part.collect(),
                            false => part.map(|path| outline::dashed(&path, dash)).collect(),
                        })
                    }
                };
                self.ops.push(Op::Shape {
                    transform: transform * Affine::translate(offset) * Affine::scale(fit) * Affine::translate((-view.x0, -view.y0)),
                    paths,
                    stroke: *stroke,
                    color: *color,
                    shadow: *shadow,
                    opacity: step_opacity,
                });
            }
            Kind::Box | Kind::Text | Kind::Span => {}
        }
        if let Some((border, color)) = look.border {
            // A border of two lines: the outer and the inner third of its width.
            let lines = if look.double {
                vec![(0.0, border / 3.0), (border * 2.0 / 3.0, border / 3.0)]
            } else {
                vec![(0.0, border)]
            };
            for (inside, width) in lines {
                self.ops.push(Op::Border {
                    transform,
                    rect: own.inset(-inside),
                    radius: (look.radius - inside).max(0.0),
                    width,
                    color,
                    opacity: step_opacity,
                });
            }
        }
        if has_text {
            if !node.text.shadows.is_empty() {
                self.ops.push(Op::TextShadow {
                    transform,
                    key: (node.key, counted.or(typed.map(|(shown, _)| shown)).unwrap_or(usize::MAX)),
                    runs,
                    node,
                    opacity: step_opacity,
                });
            }
            for run in runs {
                self.ops.push(Op::Glyphs {
                    transform,
                    run,
                    color: node.text.color,
                    stroke: node.text.stroke,
                    opacity: step_opacity,
                });
            }
        }
        // The children count their times from the start of this element when it is a clip.
        let inner = node.window.map_or(offset, |(start, _)| offset + start);
        for (index, child) in node.children.iter().enumerate() {
            let placed = match typed {
                Some((_, Some(layout))) => layout.children.get(index).copied(),
                _ => None,
            };
            self.node(child, inner, transform, placed);
        }
        if layered {
            self.ops.push(Op::Pop);
        }
    }
}

/// Whether `transform` keeps rectangles upright and puts `rect` over all of `frame`.
fn covers(transform: Affine, rect: Rect, frame: Rect) -> bool {
    let [a, b, c, d, _, _] = transform.as_coeffs();
    if b.abs() > 1e-9 || c.abs() > 1e-9 || a <= 0.0 || d <= 0.0 {
        return false;
    }
    let placed = transform.transform_rect_bbox(rect);
    placed.x0 <= 0.01 && placed.y0 <= 0.01 && placed.x1 >= frame.x1 - 0.01 && placed.y1 >= frame.y1 - 0.01
}

/// Drops every step that lies wholly under an opaque step filling the frame, and the footage
/// only those steps showed.
fn cull<'a>(ops: Vec<Op<'a>>, footage: Vec<Footage<'a>>, frame: Rect) -> (Vec<Op<'a>>, Vec<Footage<'a>>) {
    // The open layers at each step, and whether everything drawn in them reaches the frame
    // unchanged: no fade, no blur, no blend, no cut inside the frame.
    let mut open: Vec<usize> = Vec::new();
    let mut plain: Vec<bool> = Vec::new();
    let mut cover: Option<(usize, Vec<usize>)> = None;
    for (index, op) in ops.iter().enumerate() {
        let direct = plain.iter().all(|plain| *plain);
        match op {
            Op::Push {
                transform,
                clip,
                blend,
                opacity,
                blur,
                // A brighter or darker layer covers what a plain one covers.
                brightness: _,
                effect,
                finish,
            } => {
                let uncut = match clip {
                    None => true,
                    Some(Clip::Rounded(rect, radius)) => *radius == 0.0 && covers(*transform, *rect, frame),
                    Some(Clip::Outline(_)) => false,
                };
                open.push(index);
                plain.push(uncut && *blend == Blend::Normal && *opacity >= 1.0 && *blur == 0.0 && effect.is_none() && finish.is_none());
            }
            Op::Pop => {
                open.pop();
                plain.pop();
            }
            Op::Fill {
                transform,
                rect,
                radius,
                paint: Paint::Solid(color),
                tile: None,
                opacity,
                ..
            } if direct && color[3] == 255 && *opacity >= 1.0 && *radius == 0.0 && covers(*transform, *rect, frame) => {
                cover = Some((index, open.clone()));
            }
            Op::Video {
                transform,
                rect,
                radius,
                opacity,
                ..
            } if direct && *opacity >= 1.0 && *radius == 0.0 && covers(*transform, *rect, frame) => {
                cover = Some((index, open.clone()));
            }
            _ => {}
        }
    }
    let Some((from, layers)) = cover else { return (ops, footage) };
    if from == 0 {
        return (ops, footage);
    }
    let mut kept: Vec<Op<'a>> = Vec::with_capacity(ops.len() - from + layers.len());
    let mut shown: Vec<usize> = Vec::new();
    for (index, op) in ops.into_iter().enumerate() {
        if index >= from || layers.contains(&index) {
            if let Op::Video { video, .. } = &op {
                shown.push(*video);
            }
            kept.push(op);
        }
    }
    let footage = footage.into_iter().filter(|footage| shown.contains(&footage.video)).collect();
    (kept, footage)
}
