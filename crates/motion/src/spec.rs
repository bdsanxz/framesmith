//! The data model of the engine: properties, kinds and tweens.

use crate::Ease;
use serde::{Deserialize, Serialize};
use timebase::Time;

/// The eleven properties a tween can change, in the order used everywhere: the nine that move the element, then the blur and
/// the brightness of its picture (`docs/spec/motion.md`, Properties).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Prop {
    X,
    Y,
    Scale,
    ScaleX,
    Rotation,
    SkewY,
    Opacity,
    RotateX,
    RotateY,
    /// The number a counting text shows (`docs/spec/text.md` section 7.1).
    Count,
    /// How many of its own characters a text or a span shows (`docs/spec/text.md` section 7.2).
    Typed,
    /// The standard deviation of a Gaussian blur in the element's pixels (`docs/spec/motion.md`, Properties).
    Blur,
    /// The factor each colour is multiplied by (`docs/spec/motion.md`, Properties).
    Brightness,
}

/// The number of properties.
pub const PROP_COUNT: usize = 13;

pub const PROPS: [Prop; PROP_COUNT] = [
    Prop::X,
    Prop::Y,
    Prop::Scale,
    Prop::ScaleX,
    Prop::Rotation,
    Prop::SkewY,
    Prop::Opacity,
    Prop::RotateX,
    Prop::RotateY,
    Prop::Count,
    Prop::Typed,
    Prop::Blur,
    Prop::Brightness,
];

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
            Prop::RotateX => "rotate-x",
            Prop::RotateY => "rotate-y",
            Prop::Count => "count",
            Prop::Typed => "typed",
            Prop::Blur => "blur",
            Prop::Brightness => "brightness",
        }
    }

    pub fn from_name(name: &str) -> Option<Prop> {
        PROPS.into_iter().find(|prop| prop.name() == name)
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// The value the element has when no tween touches the property. That of `count` and of `typed` is the element's own:
    /// its `count` attribute, and its `typed` attribute or the number of its characters, which the caller gives; here
    /// `count` is 0, and `typed` is every character, without end.
    pub fn own_value(self) -> f64 {
        match self {
            Prop::X | Prop::Y | Prop::Rotation | Prop::SkewY | Prop::RotateX | Prop::RotateY | Prop::Count => 0.0,
            Prop::Scale | Prop::ScaleX | Prop::Opacity => 1.0,
            Prop::Typed => f64::INFINITY,
            Prop::X | Prop::Y | Prop::Rotation | Prop::SkewY | Prop::RotateX | Prop::RotateY => 0.0,
            Prop::Scale | Prop::ScaleX | Prop::Opacity | Prop::Brightness => 1.0,
            Prop::Blur => 0.0,
        }
    }

    /// Whether the property is a blur or a brightness, which no value may take below 0 (`docs/spec/motion.md`, Properties).
    pub fn is_filter(self) -> bool {
        matches!(self, Prop::Blur | Prop::Brightness)
    }
}

/// One optional value per property, indexed by `Prop::index`.
pub type Values = [Option<f64>; PROP_COUNT];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    To,
    From,
    FromTo,
    Set,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::To => "to",
            Kind::From => "from",
            Kind::FromTo => "from-to",
            Kind::Set => "set",
        }
    }

    pub fn from_name(name: &str) -> Option<Kind> {
        [Kind::To, Kind::From, Kind::FromTo, Kind::Set]
            .into_iter()
            .find(|kind| kind.name() == name)
    }
}

/// One tween, as written.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tween {
    pub target: String,
    pub kind: Kind,
    /// Starting values: given for from and from-to tweens.
    pub from: Values,
    /// End values: given for to, from-to and set tweens.
    pub to: Values,
    pub at: Time,
    pub duration: Time,
    pub ease: Ease,
}

impl Tween {
    pub fn end(&self) -> Time {
        self.at + self.duration
    }

    /// Whether the tween touches `prop`.
    pub fn touches(&self, prop: Prop) -> bool {
        self.from[prop.index()].is_some() || self.to[prop.index()].is_some()
    }
}
