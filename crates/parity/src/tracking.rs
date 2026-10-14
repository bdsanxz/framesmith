//! The timing of visible events and the tracked position of one element, in two videos read with
//! frx7 (`docs/dev/comparison.md`, section 5). An element is marked by a flat colour that nothing
//! else in the fixture uses; the fixture's record names it, and the tool takes it on the command line.

use crate::frames::parse_colour;
use crate::report::{Report, Verdict};
use crate::timing::centre;
use measure::{Frame, Rgb};
use serde_json::json;

/// An event of one render is within this many frames of the same event of the other.
pub const EVENT_LIMIT_FRAMES: u64 = 1;
/// The tracked element of one render is within this many pixels of the other's on every frame.
pub const TRACK_LIMIT_PIXELS: f64 = 1.0;
/// How far each channel of a pixel may be from the element's colour and still count as it.
pub const TOLERANCE: u8 = 24;
/// How many pixels of the element's colour make the element "there" on a frame.
pub const MIN_PIXELS: usize = 20;
/// The centre moves by more than this many pixels from one frame to the next: the element moves.
pub const MOVING_PIXELS: f64 = 0.25;

/// One element a fixture marks by its colour.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub name: String,
    pub colour: Rgb,
}

/// The elements a fixture marks, and the colour behind them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Marks {
    /// The flat colour behind the elements.
    pub background: Option<Rgb>,
    /// Elements whose events are compared.
    pub elements: Vec<Element>,
    /// The one element that is also followed frame by frame. Its events are compared too.
    pub tracked: Option<Element>,
}

/// Reads `NAME=RRGGBB`.
pub fn parse_element(text: &str) -> Result<Element, String> {
    let (name, colour) = text
        .split_once('=')
        .ok_or_else(|| format!("an element is written NAME=RRGGBB, not '{text}'"))?;
    if name.is_empty() || name.contains(char::is_whitespace) {
        return Err(format!("an element needs a name without spaces, not '{text}'"));
    }
    Ok(Element {
        name: name.to_string(),
        colour: parse_colour(colour)?,
    })
}

impl Marks {
    /// Every marked element: the tracked one first.
    pub fn all(&self) -> Vec<&Element> {
        self.tracked.iter().chain(&self.elements).collect()
    }

    /// Refuses marks that cannot be judged: no background, a name or a colour used twice.
    pub fn check(&self) -> Result<(), String> {
        let all = self.all();
        if all.is_empty() {
            return Ok(());
        }
        let Some(background) = self.background else {
            return Err("--background RRGGBB is needed with --element and --track".to_string());
        };
        for (at, element) in all.iter().enumerate() {
            if element.colour == background {
                return Err(format!("the element {} has the colour of the background", element.name));
            }
            if let Some(other) = all[..at].iter().find(|o| o.name == element.name || o.colour == element.colour) {
                return Err(format!(
                    "the elements {} and {} share a name or a colour; each element is marked by a colour of its own",
                    other.name, element.name
                ));
            }
        }
        Ok(())
    }
}

/// What was seen of one element on each frame of one video: its centre, or `None` when it is not there.
pub type Seen = Vec<Option<(f64, f64)>>;

/// Looks for every marked element on one frame and adds what it saw to `seen` (one list per element,
/// in the order of `Marks::all`).
pub fn observe(marks: &Marks, frame: &Frame, seen: &mut Vec<Seen>) {
    let all = marks.all();
    seen.resize(all.len(), Vec::new());
    let background = marks.background.unwrap_or([0, 0, 0]);
    for (element, list) in all.iter().zip(seen.iter_mut()) {
        let there = frame.count(element.colour, TOLERANCE) >= MIN_PIXELS;
        list.push(if there {
            centre(frame, background, element.colour, TOLERANCE, MIN_PIXELS)
        } else {
            None
        });
    }
}

/// What an event is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Appears,
    Disappears,
    StartsMoving,
    StopsMoving,
}

impl Kind {
    pub fn words(self) -> &'static str {
        match self {
            Kind::Appears => "appears",
            Kind::Disappears => "disappears",
            Kind::StartsMoving => "starts moving",
            Kind::StopsMoving => "stops moving",
        }
    }
}

/// The frames on which each kind of event happens to one element, in order.
pub fn events(seen: &Seen) -> Vec<(Kind, u64)> {
    let mut out = Vec::new();
    let mut moving = false;
    for (at, now) in seen.iter().enumerate() {
        let before = if at == 0 { None } else { seen[at - 1] };
        let frame = at as u64;
        match (before, now) {
            (None, Some(_)) => out.push((Kind::Appears, frame)),
            (Some(_), None) => {
                if moving {
                    out.push((Kind::StopsMoving, frame));
                    moving = false;
                }
                out.push((Kind::Disappears, frame));
            }
            (Some(p), Some(q)) => {
                let moves = (p.0 - q.0).hypot(p.1 - q.1) > MOVING_PIXELS;
                if moves && !moving {
                    // It moves from frame at - 1 to frame at: the first frame it is seen elsewhere.
                    out.push((Kind::StartsMoving, frame));
                } else if !moves && moving {
                    out.push((Kind::StopsMoving, frame));
                }
                moving = moves;
            }
            (None, None) => {}
        }
    }
    out
}

fn ordinal(n: usize) -> String {
    match n {
        0 => String::new(),
        _ => format!(" (time {})", n + 1),
    }
}

/// The event check: every event of every marked element, ours against the reference's.
pub fn event_check(report: &mut Report, marks: &Marks, ours: &[Seen], theirs: &[Seen]) {
    let all = marks.all();
    if all.is_empty() {
        report.check(
            "events",
            Verdict::NotApplicable,
            "the fixture marks no element (--element, --track), so no event is compared",
        );
        return;
    }
    let mut lines = Vec::new();
    let mut failed = Vec::new();
    let mut listed = Vec::new();
    let mut count = 0usize;
    for (at, element) in all.iter().enumerate() {
        let a = events(&ours[at]);
        let b = events(&theirs[at]);
        for kind in [Kind::Appears, Kind::Disappears, Kind::StartsMoving, Kind::StopsMoving] {
            let first: Vec<u64> = a.iter().filter(|e| e.0 == kind).map(|e| e.1).collect();
            let second: Vec<u64> = b.iter().filter(|e| e.0 == kind).map(|e| e.1).collect();
            for n in 0..first.len().max(second.len()) {
                count += 1;
                let name = format!("{} {}{}", element.name, kind.words(), ordinal(n));
                let (line, passes, frames) = match (first.get(n), second.get(n)) {
                    (Some(&x), Some(&y)) => {
                        let apart = x.abs_diff(y);
                        let passes = apart <= EVENT_LIMIT_FRAMES;
                        (
                            format!("{name}: ours frame {x}, reference frame {y}, {apart} frame(s) apart"),
                            passes,
                            (Some(x), Some(y)),
                        )
                    }
                    (Some(&x), None) => (format!("{name}: ours frame {x}, reference never"), false, (Some(x), None)),
                    (None, Some(&y)) => (format!("{name}: ours never, reference frame {y}"), false, (None, Some(y))),
                    (None, None) => unreachable!(),
                };
                lines.push(format!("{line}  {}", if passes { "pass" } else { "fail" }));
                listed.push(json!({"event": name, "ours": frames.0, "reference": frames.1, "passes": passes}));
                if !passes {
                    failed.push(line);
                }
            }
        }
    }
    if count == 0 {
        report.check(
            "events",
            Verdict::Fail,
            "no marked element appears in either video, so there is nothing to compare",
        );
    } else if failed.is_empty() {
        report.check(
            "events",
            Verdict::Pass,
            format!("{count} event(s), each within {EVENT_LIMIT_FRAMES} frame of the reference's"),
        );
    } else {
        report.check(
            "events",
            Verdict::Fail,
            format!(
                "{} of {count} event(s) more than {EVENT_LIMIT_FRAMES} frame apart or missing; first: {}",
                failed.len(),
                failed[0]
            ),
        );
    }
    report.section("events (frames count from 0)", lines);
    report.data.push(("events".to_string(), json!(listed)));
}

/// A distance in pixels, rounded to the nearest tenth: the measure is judged at that precision.
pub fn rounded(distance: f64) -> f64 {
    (distance * 10.0).round() / 10.0
}

/// The tracking check: the tracked element's centre, ours against the reference's, on every frame.
pub fn tracking_check(report: &mut Report, marks: &Marks, ours: &[Seen], theirs: &[Seen], per_frame: bool) {
    let Some(tracked) = &marks.tracked else {
        report.check(
            "tracking",
            Verdict::NotApplicable,
            "the fixture marks no tracked element (--track NAME=RRGGBB)",
        );
        return;
    };
    // The tracked element is the first of `Marks::all`.
    let (a, b) = (&ours[0], &theirs[0]);
    let mut over = Vec::new();
    let mut every = Vec::new();
    let mut listed = Vec::new();
    let mut largest: Option<(f64, usize)> = None;
    let mut compared = 0usize;
    let at = |(x, y): (f64, f64)| format!("({x:.2}, {y:.2})");
    for (frame, (p, q)) in a.iter().zip(b).enumerate() {
        match (p, q) {
            (Some(p), Some(q)) => {
                compared += 1;
                let distance = rounded((p.0 - q.0).hypot(p.1 - q.1));
                if largest.is_none_or(|(d, _)| distance > d) {
                    largest = Some((distance, frame));
                }
                let line = format!("frame {frame}: {distance:.1} px apart, ours {} reference {}", at(*p), at(*q));
                if distance > TRACK_LIMIT_PIXELS {
                    over.push(line.clone());
                }
                every.push(line);
                listed.push(json!({"frame": frame, "ours": [p.0, p.1], "reference": [q.0, q.1], "distance": distance}));
            }
            (None, None) => {}
            _ => {
                let line = format!(
                    "frame {frame}: the element is {} in ours and {} in the reference",
                    if p.is_some() { "there" } else { "missing" },
                    if q.is_some() { "there" } else { "missing" }
                );
                over.push(line.clone());
                every.push(line);
                listed.push(json!({"frame": frame, "ours": p.map(|p| [p.0, p.1]), "reference": q.map(|q| [q.0, q.1]), "distance": null}));
            }
        }
    }
    let name = "tracking";
    let limit = format!("limit {TRACK_LIMIT_PIXELS} px");
    if !over.is_empty() {
        report.check(
            name,
            Verdict::Fail,
            format!(
                "{}: {} of {} frame(s) over the limit ({limit}); first: {}",
                tracked.name,
                over.len(),
                every.len(),
                over[0]
            ),
        );
    } else if let Some((distance, frame)) = largest {
        report.check(
            name,
            Verdict::Pass,
            format!(
                "{}: within {TRACK_LIMIT_PIXELS} px on all {compared} frame(s) it is seen; largest {distance:.1} px at frame {frame}",
                tracked.name
            ),
        );
    } else {
        report.check(
            name,
            Verdict::Fail,
            format!("{}: the tracked element is not seen on any frame of either video", tracked.name),
        );
    }
    report.section(
        format!("tracked frames over {TRACK_LIMIT_PIXELS} px"),
        if over.is_empty() { vec!["none".to_string()] } else { over },
    );
    if per_frame {
        report.section("tracked position of every frame", every);
    }
    report
        .data
        .push(("tracking".to_string(), json!({"element": tracked.name, "frames": listed})));
}
