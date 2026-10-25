//! The pure evaluator: the value of every animated property at a time.

use crate::{Ease, Kind, Prop, Tween, Values, PROPS, PROP_COUNT};
use std::collections::BTreeMap;
use timebase::Time;

#[derive(Clone, Debug, PartialEq)]
struct Segment {
    at: Time,
    end: Time,
    from: f64,
    to: f64,
    ease: Ease,
}

impl Segment {
    fn value(&self, t: Time) -> f64 {
        if t >= self.end {
            return self.to;
        }
        let progress = (t - self.at).ratio(self.end - self.at);
        self.from + (self.to - self.from) * self.ease.apply(progress)
    }
}

/// Every tween on one property of one element, ready to be put at a time.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    prop: Prop,
    /// The element's own value of the property: what it holds before and without a tween.
    own: f64,
    /// Ordered by start time; tweens with the same start keep document order.
    segments: Vec<Segment>,
    /// Whether the first tween's starting value holds before it starts.
    holds_before: bool,
}

impl Track {
    fn build(prop: Prop, own: f64, tweens: &[&Tween], fps: u32) -> Track {
        let mut ordered: Vec<(Time, &Tween)> = tweens.iter().map(|tween| (tween.at.snapped(fps), *tween)).collect();
        ordered.sort_by_key(|(at, _)| *at);
        let mut segments: Vec<Segment> = Vec::with_capacity(ordered.len());
        for (at, tween) in &ordered {
            let in_effect = match segments.last() {
                Some(previous) => previous.value(*at),
                None => own,
            };
            let given_from = tween.from[prop.index()];
            let given_to = tween.to[prop.index()];
            let (from, to) = match tween.kind {
                Kind::To => (in_effect, given_to.unwrap_or(in_effect)),
                Kind::From => (given_from.unwrap_or(in_effect), in_effect),
                Kind::FromTo => (given_from.unwrap_or(in_effect), given_to.unwrap_or(in_effect)),
                Kind::Set => {
                    let value = given_to.unwrap_or(in_effect);
                    (value, value)
                }
            };
            segments.push(Segment {
                at: *at,
                end: tween.end().snapped(fps).max(*at),
                from,
                to,
                ease: tween.ease,
            });
        }
        let holds_before = ordered
            .first()
            .is_some_and(|(_, tween)| matches!(tween.kind, Kind::From | Kind::FromTo));
        Track {
            prop,
            own,
            segments,
            holds_before,
        }
    }

    /// The property's value at time `t`.
    pub fn value(&self, t: Time) -> f64 {
        let started = self.segments.partition_point(|segment| segment.at <= t);
        match started {
            0 => match self.segments.first() {
                Some(first) if self.holds_before => first.from,
                _ => self.own,
            },
            n => self.segments[n - 1].value(t),
        }
    }
}

/// The tracks of one element.
#[derive(Clone, Debug, PartialEq)]
pub struct ElementMotion {
    /// The caller's key for the element.
    pub element: usize,
    /// One track per property, indexed by `Prop::index`; `None` when no tween touches it.
    pub tracks: [Option<Track>; PROP_COUNT],
}

impl ElementMotion {
    /// The value of each animated property at `t`; `None` for properties left alone.
    pub fn values(&self, t: Time) -> Values {
        let mut values = [None; PROP_COUNT];
        for (slot, track) in values.iter_mut().zip(&self.tracks) {
            *slot = track.as_ref().map(|track| track.value(t));
        }
        values
    }
}

impl ElementMotion {
    /// The tracks of one element at `fps`, from its tweens in the order written and its own value of each property (the
    /// value it holds before and without a tween), indexed by `Prop::index`.
    pub fn build(element: usize, tweens: &[Tween], own: [f64; PROP_COUNT], fps: u32) -> ElementMotion {
        let tracks = PROPS.map(|prop| {
            let touching: Vec<&Tween> = tweens.iter().filter(|tween| tween.touches(prop)).collect();
            (!touching.is_empty()).then(|| Track::build(prop, own[prop.index()], &touching, fps))
        });
        ElementMotion { element, tracks }
    }
}

/// Every animated element of a composition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timeline {
    /// In order of first appearance in the tween list.
    pub elements: Vec<ElementMotion>,
}

impl Timeline {
    /// Builds the timeline for a run at `fps`. `targets[i]` holds the keys of
    /// the elements that tween `i`'s target matches.
    pub fn build(tweens: &[Tween], targets: &[Vec<usize>], fps: u32) -> Timeline {
        let mut order: Vec<usize> = Vec::new();
        let mut per_element: BTreeMap<usize, Vec<&Tween>> = BTreeMap::new();
        for (tween, matched) in tweens.iter().zip(targets) {
            for &element in matched {
                let list = per_element.entry(element).or_insert_with(|| {
                    order.push(element);
                    Vec::new()
                });
                list.push(tween);
            }
        }
        let elements = order
            .into_iter()
            .map(|element| {
                let tweens = &per_element[&element];
                let tracks = PROPS.map(|prop| {
                    let touching: Vec<&Tween> = tweens.iter().copied().filter(|tween| tween.touches(prop)).collect();
                    (!touching.is_empty()).then(|| Track::build(prop, prop.own_value(), &touching, fps))
                });
                ElementMotion { element, tracks }
            })
            .collect();
        Timeline { elements }
    }
}

/// The latest end time of any tween (a set tween ends at its own time), or
/// `None` when there is no tween.
pub fn motion_end(tweens: &[Tween]) -> Option<Time> {
    tweens.iter().map(Tween::end).max()
}
