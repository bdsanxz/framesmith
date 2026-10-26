//! The motion of an element (`docs/spec/render-command.md`, section 4.5): its movement attributes and its tweens as this
//! version reads them, and its map and opacity at a time.

use draw::{Blend, Filter, Map, Shadow, Turn};
use layout::Rect;
use motion::{Ease, ElementMotion, Kind, Prop, Tween, Values, PROPS, PROP_COUNT};
use project::{Attribute, Code, Element, Length, Node, Number};
use timebase::Time;

use crate::support;

/// The attributes of a `<box>` or a `<text>` that move it: the nine properties, the origin they turn about, the
/// `perspective` of a turn in space, and the `blur` and `brightness` of its picture, and the blend its finished picture
/// is laid with (`docs/spec/blends.md`).
pub(crate) const MOVEMENT: [&str; 14] = [
    "x",
    "y",
    "scale",
    "scale-x",
    "rotation",
    "skew-y",
    "opacity",
    "rotate-x",
    "rotate-y",
    "transform-origin",
    "perspective",
    "blur",
    "brightness",
    "mix-blend-mode",
];

/// A refusal found while reading motion: its code, its line and its message.
pub(crate) type Found = (Code, u32, String);

/// What an element writes of its motion: its own value of each property (the attribute, or the starting value of
/// `docs/spec/notation.md` section 8.1), its origin, its `perspective` if it writes one, the blur and brightness of its
/// picture, and the lines of the attributes that write them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Movement {
    pub(crate) own: [f64; PROP_COUNT],
    pub(crate) origin: (Length, Length),
    pub(crate) perspective: Option<f64>,
    pub(crate) blend: Blend,
    pub(crate) lines: Vec<u32>,
    /// The `drop-shadow` the element writes, and its line (`docs/spec/render-command.md` section 4.7, rule 5).
    pub(crate) shadow: Option<(Option<Shadow>, u32)>,
}

impl Movement {
    /// Whether the element writes any movement attribute.
    pub(crate) fn written(&self) -> bool {
        !self.lines.is_empty() || self.shadow.is_some()
    }
}

/// The refusal of a written `blur` or `brightness` below 0 (`FS-0034`), on its attribute's line, for an attribute named
/// `name` that holds `value` (`docs/spec/notation.md`, section 8.1). `None` when the attribute is not one of these two, or
/// holds 0 or more, or holds no number (the form check's).
fn below_zero(name: &str, line: u32, shown: &str, value: Option<f64>) -> Option<Found> {
    let reason = match name {
        "blur" | "from-blur" | "to-blur" => "a blur is a standard deviation in pixels, 0 or more",
        "brightness" | "from-brightness" | "to-brightness" => "a brightness is a factor the colours are multiplied by, 0 or more",
        _ => return None,
    };
    value
        .filter(|value| *value < 0.0)
        .map(|_| (Code::LengthNotAccepted, line, format!("`{name}=\"{shown}\"` is below 0: {reason}")))
}

/// Reads the movement attributes of an element. A `transform-origin` that is not two lengths is reported (`FS-0034`), and a
/// `perspective` of 0 or less (`FS-0053`), and a `blur` or a `brightness` below 0 (`FS-0034`); a value that is not a number
/// is the form check's, and is left at its starting value here (a `perspective` that is not a number, at none).
pub(crate) fn read_movement(node: &Node) -> (Movement, Vec<Found>) {
    let mut movement = Movement {
        own: PROPS.map(Prop::own_value),
        origin: (Length::Percent(Number::whole(50)), Length::Percent(Number::whole(50))),
        perspective: None,
        blend: Blend::Normal,
        lines: Vec::new(),
        shadow: None,
    };
    let mut found = Vec::new();
    for attribute in node.attributes() {
        let name = attribute.name();
        if name == "drop-shadow" {
            match crate::shadows::drop_shadow(&support::written(node.element(), attribute), attribute.value()) {
                Ok(shadow) => movement.shadow = Some((shadow, attribute.line())),
                Err((code, message)) => found.push((code, attribute.line(), message)),
            }
            continue;
        }
        if !MOVEMENT.contains(&name) {
            continue;
        }
        if name == "mix-blend-mode" {
            match Blend::named(attribute.value()) {
                Some(Blend::Normal) => {}
                Some(blend) => {
                    movement.blend = blend;
                    movement.lines.push(attribute.line());
                }
                None => found.push((
                    Code::WordNotTaken,
                    attribute.line(),
                    format!(
                        "`mix-blend-mode=\"{}\"` is not one of: {}",
                        attribute.shown(),
                        Blend::ALL.map(Blend::name).join(", ")
                    ),
                )),
            }
            continue;
        }
        movement.lines.push(attribute.line());
        if name == "transform-origin" {
            match attribute.lengths().as_deref() {
                Some([x, y]) => movement.origin = (x.clone(), y.clone()),
                _ => found.push((
                    Code::LengthNotAccepted,
                    attribute.line(),
                    format!(
                        "`transform-origin=\"{}\"` is not two lengths: numbers of pixels or percentages, such as 50% 50%",
                        attribute.shown()
                    ),
                )),
            }
            continue;
        }
        if name == "perspective" {
            match attribute.number() {
                Some(distance) if distance > 0.0 => movement.perspective = Some(distance),
                Some(_) => found.push((
                    Code::SizeNotAboveZero,
                    attribute.line(),
                    format!(
                        "`perspective=\"{}\"` is not above 0: a perspective is the distance of the eye from the frame, in pixels",
                        attribute.shown()
                    ),
                )),
                None => {}
            }
            continue;
        }
        if let Some(refusal) = below_zero(name, attribute.line(), &attribute.shown(), attribute.number()) {
            found.push(refusal);
        }
        if let (Some(prop), Some(value)) = (Prop::from_name(name), attribute.number()) {
            movement.own[prop.index()] = value;
        }
    }
    (movement, found)
}

/// Reads the `blur` and the `brightness` an element writes: a standard deviation in pixels of 0 or more, and a factor of
/// 0 or more. A value below 0 is reported (`FS-0034`); a value that is not a number is the form check's, and is left at its
/// starting value here.
fn read_filter(attribute: &Attribute, filter: &mut Filter, found: &mut Vec<Found>) {
    let name = attribute.name();
    match attribute.number() {
        Some(value) if value >= 0.0 => match name {
            "blur" => filter.blur = value,
            _ => filter.brightness = value,
        },
        Some(_) => found.push((
            Code::LengthNotAccepted,
            attribute.line(),
            format!(
                "`{name}=\"{}\"` is below 0: {}",
                attribute.shown(),
                if name == "blur" {
                    "a blur is a standard deviation in pixels, 0 or more"
                } else {
                    "a brightness is a factor the colours are multiplied by, 0 or more"
                }
            ),
        )),
        None => {}
    }
}

/// Reads the look of a `<video>`, which does not move: its `opacity`, `blur` and `brightness`
/// (`docs/spec/render-command.md`, section 4.8). A value that is not a number is the form check's, and is left at its
/// starting value here, as in [`read_movement`].
pub(crate) fn read_look(node: &Node) -> (f64, Filter, Vec<Found>) {
    let mut opacity = Prop::Opacity.own_value();
    let mut filter = Filter::NONE;
    let mut found = Vec::new();
    for attribute in node.attributes() {
        let name = attribute.name();
        if name == "opacity" {
            if let Some(value) = attribute.number() {
                opacity = value;
            }
            continue;
        }
        if name == "blur" || name == "brightness" {
            read_filter(attribute, &mut filter, &mut found);
        }
    }
    (opacity, filter, found)
}

/// Reads a `<tween>` that moves a `<box>` or a `<text>`, with `origin` the start of the time its `at` counts in (the time of
/// the parent of the element it moves). Gives the tween, or `None` when it has no duration above 0 (the form check reports
/// that, `FS-0058`), and the refusal of a property other than the seven, on its attribute's line (`FS-0035`). The tween is
/// eased by its `ease` (`docs/spec/notation.md` section 8.4).
pub(crate) fn read_tween(node: &Node, origin: Time) -> (Option<Tween>, Vec<Found>) {
    let mut found = Vec::new();
    let mut from: Values = [None; PROP_COUNT];
    let mut to: Values = [None; PROP_COUNT];
    let mut at = Time::ZERO;
    let mut duration = None;
    let mut ease = Ease::Linear;
    for attribute in node.attributes() {
        let name = attribute.name();
        match name {
            "at" => at = attribute.time().unwrap_or(Time::ZERO),
            "duration" => duration = attribute.time(),
            // The form check reports an `ease` that is not an easing (`FS-0063`); none is left to read here.
            "ease" => {
                ease = match project::easing(attribute.value()) {
                    Some(project::Easing::Named(name)) => Ease::from_name(name).unwrap_or(Ease::Linear),
                    Some(project::Easing::OutBack(n)) => Ease::OutBack(n),
                    None => Ease::Linear,
                }
            }
            _ => {
                let (values, property) = match (name.strip_prefix("from-"), name.strip_prefix("to-")) {
                    (Some(property), _) => (&mut from, property),
                    (_, Some(property)) => (&mut to, property),
                    _ => continue,
                };
                match Prop::from_name(property) {
                    Some(prop) => {
                        values[prop.index()] = attribute.number();
                        if let Some(refusal) = below_zero(name, attribute.line(), &attribute.shown(), attribute.number()) {
                            found.push(refusal);
                        }
                    }
                    None => {
                        // `from-` and `to-` name the one property: the refusal of it is reported once on the line.
                        let (code, message) =
                            support::refused_property(property, format!("<tween> changes `{}`", project::shown(property)));
                        let line = attribute.line();
                        if !found.iter().any(|(_, at, text)| *at == line && *text == message) {
                            found.push((code, line, message));
                        }
                    }
                }
            }
        }
    }
    let tween = duration.filter(|duration| *duration > Time::ZERO).map(|duration| Tween {
        target: String::new(),
        // A value not given for a property is the value in effect at the tween's start, at either end, so one kind covers
        // `from-` alone, `to-` alone and both (`docs/spec/notation.md` section 8.2).
        kind: Kind::FromTo,
        from,
        to,
        at: origin + at,
        duration,
        ease,
    });
    (tween, found)
}

/// Reads a `<set>` that moves a `<box>` or a `<text>`, with `origin` the start of the time its `at` counts in, as a tween of
/// no length (`docs/spec/motion.md`, rule 3; `docs/spec/notation.md` section 8.3). Gives the set, or `None` when it changes
/// none of the seven properties (the form check reports a set with no property, `FS-0061`), and the refusals of a property
/// other than the seven that it changes, each on its attribute's line (`FS-0035`). An attribute that is not an animated
/// property is the form check's (`FS-0030`), and is passed over here.
pub(crate) fn read_set(node: &Node, origin: Time) -> (Option<Tween>, Vec<Found>) {
    let mut found = Vec::new();
    let mut to: Values = [None; PROP_COUNT];
    let mut at = Time::ZERO;
    for attribute in node.attributes() {
        let name = attribute.name();
        if name == "at" {
            at = attribute.time().unwrap_or(Time::ZERO);
            continue;
        }
        if !Element::Set.takes(name) {
            continue;
        }
        match Prop::from_name(name) {
            Some(prop) => {
                to[prop.index()] = attribute.number();
                if let Some(refusal) = below_zero(name, attribute.line(), &attribute.shown(), attribute.number()) {
                    found.push(refusal);
                }
            }
            None => {
                let (code, message) = support::refused_property(name, format!("<set> changes `{}`", project::shown(name)));
                found.push((code, attribute.line(), message));
            }
        }
    }
    let set = to.iter().any(Option::is_some).then(|| Tween {
        target: String::new(),
        kind: Kind::Set,
        from: [None; PROP_COUNT],
        to,
        at: origin + at,
        duration: Time::ZERO,
        ease: Ease::Linear,
    });
    (set, found)
}

/// Whether an element of this kind is moved by this version.
pub(crate) fn moves(element: Element) -> bool {
    matches!(element, Element::Box | Element::Text)
}

/// An element that moves, as the scene keeps it (`docs/spec/timeline.md` section 5): its entries, from `first` up to `end`,
/// its own values, its rectangle and its origin in the frame, its `perspective`, the blur and brightness of its picture, its
/// tracks, and the rectangle of the canvas it stands on when that is shown from another file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Mover {
    pub(crate) first: usize,
    pub(crate) end: usize,
    pub(crate) own: [f64; PROP_COUNT],
    pub(crate) rect: Rect,
    pub(crate) origin: (f64, f64),
    pub(crate) perspective: Option<f64>,
    pub(crate) motion: Option<ElementMotion>,
    pub(crate) clip: Option<Rect>,
    /// The shadow that follows the element's finished picture.
    pub(crate) shadow: Option<Shadow>,
    /// How the element's finished picture is mixed with what is under it (`docs/spec/blends.md`).
    pub(crate) blend: Blend,
}

impl Mover {
    /// The element's origin in the frame: its `transform-origin` from the top left corner of its rectangle.
    pub(crate) fn origin_in(rect: Rect, (x, y): (Length, Length)) -> (f64, f64) {
        let along = |length: Length, size: u32| match length {
            Length::Pixels(pixels) => pixels.to_f64(),
            Length::Percent(percent) => percent.to_f64() / 100.0 * f64::from(size),
        };
        (
            f64::from(rect.left) + along(x, rect.width),
            f64::from(rect.top) + along(y, rect.height),
        )
    }

    /// The value of each property at `time`: the tween's, or the element's own.
    fn values(&self, time: Time) -> [f64; PROP_COUNT] {
        let mut values = self.own;
        if let Some(motion) = &self.motion {
            for (value, found) in values.iter_mut().zip(motion.values(time)) {
                if let Some(found) = found {
                    *value = found;
                }
            }
        }
        values
    }

    /// The blur and brightness of the element's picture at `time`: the tween's value, or its own (`docs/spec/render-command.md`,
    /// section 4.5, rule 11).
    pub(crate) fn filter_at(&self, time: Time) -> Filter {
        let values = self.values(time);
        Filter {
            blur: values[Prop::Blur.index()],
            brightness: values[Prop::Brightness.index()],
        }
    }

    /// The element's map, opacity and turn in space at `time` (`docs/spec/render-command.md`, section 4.5, rules 3, 6 and
    /// 10). The turn is `None` without a `perspective`, and when both its angles are 0.
    pub(crate) fn at(&self, time: Time) -> (Map, f64, Option<Turn>) {
        let values = self.values(time);
        let value = |prop: Prop| values[prop.index()];
        let (across, down) = (value(Prop::Scale) * value(Prop::ScaleX), value(Prop::Scale));
        let slant = value(Prop::SkewY).to_radians().tan();
        let (sin, cos) = value(Prop::Rotation).to_radians().sin_cos();
        // The linear part, turn after slant after scale: the slant sends (u, v) to (u, v + slant u).
        let a = (cos - sin * slant) * across;
        let b = (sin + cos * slant) * across;
        let c = -sin * down;
        let d = cos * down;
        let (ox, oy) = self.origin;
        let map = Map {
            a,
            b,
            c,
            d,
            e: value(Prop::X) + (ox - (a * ox + c * oy)),
            f: value(Prop::Y) + (oy - (b * ox + d * oy)),
        };
        let opacity = value(Prop::Opacity);
        let turn = self
            .perspective
            .map(|perspective| Turn {
                rect: self.rect,
                origin: self.origin,
                perspective,
                rotate_x: value(Prop::RotateX),
                rotate_y: value(Prop::RotateY),
            })
            .filter(|turn| !turn.is_none());
        (map, if opacity.is_nan() { 0.0 } else { opacity.clamp(0.0, 1.0) }, turn)
    }
}
