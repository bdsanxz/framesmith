//! The data model of the engine: properties, kinds and tweens.

use crate::Ease;
use serde::{Deserialize, Serialize};
use timebase::Time;

/// The seven properties a tween can change, in the order used everywhere.
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
}

/// The number of properties.
pub const PROP_COUNT: usize = 7;

pub const PROPS: [Prop; PROP_COUNT] = [
    Prop::X,
    Prop::Y,
    Prop::Scale,
    Prop::ScaleX,
    Prop::Rotation,
    Prop::SkewY,
    Prop::Opacity,
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
        }
    }

    pub fn from_name(name: &str) -> Option<Prop> {
        PROPS.into_iter().find(|prop| prop.name() == name)
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// The value the element has when no tween touches the property.
    pub fn own_value(self) -> f64 {
        match self {
            Prop::X | Prop::Y | Prop::Rotation | Prop::SkewY => 0.0,
            Prop::Scale | Prop::ScaleX | Prop::Opacity => 1.0,
        }
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
