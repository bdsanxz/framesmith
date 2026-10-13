//! Animation as the proof of concept needs it: more properties and one more curve than the
//! motion engine has today, evaluated at the exact time of a frame.

use motion::Ease;

/// The properties a tween can change, in the order used everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prop {
    X,
    Y,
    Scale,
    ScaleX,
    Rotation,
    SkewY,
    Opacity,
    Blur,
    Brightness,
    /// How many characters of a text are shown; a text that is not typed shows all of them.
    Typed,
    /// The turn about a horizontal line and about a vertical one, in degrees.
    RotateX,
    RotateY,
    /// How far an element with a transition has opened: 0 not at all, 1 wholly.
    Reveal,
    /// The number a counting text shows.
    Count,
    /// The share of each outline of a shape that is drawn, from its start.
    Draw,
}

pub const PROPS: [Prop; 15] = [
    Prop::X,
    Prop::Y,
    Prop::Scale,
    Prop::ScaleX,
    Prop::Rotation,
    Prop::SkewY,
    Prop::Opacity,
    Prop::Blur,
    Prop::Brightness,
    Prop::Typed,
    Prop::RotateX,
    Prop::RotateY,
    Prop::Reveal,
    Prop::Count,
    Prop::Draw,
];

/// One value per property, indexed by `Prop::index`.
pub type Values = [f64; PROPS.len()];

impl Prop {
    pub fn name(self) -> &'static str {
        match self {
            Prop::X => "x",
            Prop::Y => "y",
            Prop::Scale => "scale",
            Prop::ScaleX => "scale-x",
            Prop::Rotation => "rotation",
            Prop::SkewY => "skew-y",
            Prop::Opacity => "opacity",
            Prop::Blur => "blur",
            Prop::Brightness => "brightness",
            Prop::Typed => "typed",
            Prop::RotateX => "rotate-x",
            Prop::RotateY => "rotate-y",
            Prop::Reveal => "reveal",
            Prop::Count => "count",
            Prop::Draw => "draw",
        }
    }

    pub fn from_name(name: &str) -> Option<Prop> {
        PROPS.into_iter().find(|prop| prop.name() == name)
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// The value the element has when nothing sets the property.
    pub fn own_value(self) -> f64 {
        match self {
            Prop::X | Prop::Y | Prop::Rotation | Prop::SkewY | Prop::Blur | Prop::RotateX | Prop::RotateY | Prop::Count => 0.0,
            Prop::Scale | Prop::ScaleX | Prop::Opacity | Prop::Brightness | Prop::Reveal | Prop::Draw => 1.0,
            Prop::Typed => f64::INFINITY,
        }
    }
}

pub fn own_values() -> Values {
    PROPS.map(Prop::own_value)
}

/// An easing curve: one of the motion engine's ten, a curve that overshoots its end, or one of
/// the nine of `docs/spec/native-notation.md`, section 10.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    Std(Ease),
    OutBack(f64),
    More(Shape, Way),
}

/// What a curve of section 10 is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Quint,
    Sine,
    Expo,
}

/// Where a curve of section 10 is slow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    In,
    Out,
    InOut,
}

impl Shape {
    /// The curve that starts slowly: how far the value has come when the tween is `p` along.
    fn slow_start(self, p: f64) -> f64 {
        match self {
            Shape::Quint => p.powi(5),
            Shape::Sine => 1.0 - (p * std::f64::consts::FRAC_PI_2).cos(),
            Shape::Expo if p <= 0.0 => 0.0,
            Shape::Expo => 2.0_f64.powf(10.0 * p - 10.0),
        }
    }
}

impl Curve {
    /// `linear`, `out-cubic` and the other names of the motion engine, or `out-back(2)`.
    pub fn from_name(name: &str) -> Option<Curve> {
        if let Some(ease) = Ease::from_name(name) {
            return Some(Curve::Std(ease));
        }
        let shapes = [("quint", Shape::Quint), ("sine", Shape::Sine), ("expo", Shape::Expo)];
        let ways = [("in-out-", Way::InOut), ("in-", Way::In), ("out-", Way::Out)];
        for (prefix, way) in ways {
            let shape = name
                .strip_prefix(prefix)
                .and_then(|rest| shapes.iter().find(|(shape, _)| *shape == rest));
            if let Some((_, shape)) = shape {
                return Some(Curve::More(*shape, way));
            }
        }
        let overshoot = name.strip_prefix("out-back(")?.strip_suffix(')')?;
        overshoot.trim().parse().ok().filter(|v: &f64| v.is_finite()).map(Curve::OutBack)
    }

    pub fn apply(self, p: f64) -> f64 {
        match self {
            Curve::Std(ease) => ease.apply(p),
            Curve::OutBack(overshoot) => {
                let p = p.clamp(0.0, 1.0);
                if p == 0.0 {
                    return 0.0;
                }
                let q = p - 1.0;
                q * q * ((overshoot + 1.0) * q + overshoot) + 1.0
            }
            Curve::More(shape, way) => {
                let p = p.clamp(0.0, 1.0);
                // The ends are exact; a curve that ends slowly is the slow start turned round,
                // and one that does both is half of each.
                if p == 0.0 || p == 1.0 {
                    return p;
                }
                match way {
                    Way::In => shape.slow_start(p),
                    Way::Out => 1.0 - shape.slow_start(1.0 - p),
                    Way::InOut if p < 0.5 => shape.slow_start(2.0 * p) / 2.0,
                    Way::InOut => 1.0 - shape.slow_start(2.0 - 2.0 * p) / 2.0,
                }
            }
        }
    }
}

/// One tween or set as written, for one property. Times are in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub at: f64,
    pub duration: f64,
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub curve: Curve,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Segment {
    at: f64,
    end: f64,
    from: f64,
    to: f64,
    curve: Curve,
}

impl Segment {
    fn value(&self, t: f64) -> f64 {
        if t >= self.end {
            return self.to;
        }
        let progress = (t - self.at) / (self.end - self.at);
        self.from + (self.to - self.from) * self.curve.apply(progress)
    }
}

/// Every tween on one property of one element.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub prop: Prop,
    /// Ordered by start time; tweens with the same start keep document order.
    segments: Vec<Segment>,
    /// Whether the first tween's starting value holds before it starts.
    holds_before: bool,
}

impl Track {
    /// `base` is the value the element has without the tweens.
    pub fn build(prop: Prop, base: f64, keys: &[Key]) -> Track {
        let mut ordered: Vec<&Key> = keys.iter().collect();
        ordered.sort_by(|a, b| a.at.total_cmp(&b.at));
        let mut segments: Vec<Segment> = Vec::with_capacity(ordered.len());
        for key in &ordered {
            let in_effect = segments.last().map_or(base, |previous| previous.value(key.at));
            let (from, to) = match (key.from, key.to) {
                (Some(from), Some(to)) => (from, to),
                (Some(from), None) => (from, in_effect),
                (None, Some(to)) => (in_effect, to),
                (None, None) => (in_effect, in_effect),
            };
            segments.push(Segment {
                at: key.at,
                end: key.at + key.duration.max(0.0),
                from,
                to,
                curve: key.curve,
            });
        }
        let holds_before = ordered.first().is_some_and(|key| key.from.is_some() && key.duration > 0.0);
        Track {
            prop,
            segments,
            holds_before,
        }
    }

    /// The property's value at `t`, or `None` when the track leaves the element's own value.
    pub fn value(&self, t: f64) -> Option<f64> {
        // A hair of tolerance, so a tween written to three decimals starts on its frame.
        let started = self.segments.partition_point(|segment| segment.at <= t + 1e-9);
        match started {
            0 => self.segments.first().filter(|_| self.holds_before).map(|first| first.from),
            n => Some(self.segments[n - 1].value(t)),
        }
    }
}
