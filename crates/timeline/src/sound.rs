//! The sounds of the composition (`docs/spec/sound.md`): each `<audio>` in the root, placed to the sample of the
//! mix. Its file is looked for here, as a font's is; it is opened and judged by the render, which reads sound.

use crate::paragraph::Fault;
use project::{Code, Element, Node, Project};
use std::path::PathBuf;
use timebase::Time;

/// Sample frames a second of the film's sound (`docs/spec/render-command.md` section 5.2).
const MIX_RATE: u32 = 48_000;

/// The level of a sound: its `volume` and the points of its curve, in the order written (`docs/spec/sound.md`, section 3a).
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    /// The `volume` of the `<audio>`: 1 when none is written.
    pub volume: f64,
    /// Each `<volume>` point: its time from the start of the sound (0 when none is written) and its value.
    pub points: Vec<(Time, f64)>,
}

/// A sound of the film: an `<audio>` of the root, with the file it names and where it is heard.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    /// The line of the `<audio>`.
    pub line: u32,
    /// `src` as it is written.
    pub src: String,
    /// The file `src` names, found from the project folder.
    pub path: PathBuf,
    /// The first sample frame of the mix it is heard in.
    pub first: u64,
    /// The sample frame it is heard up to, not on.
    pub end: u64,
    /// `media-start`: the time of its source it is heard from, with the value as written.
    pub media_start: Option<(Time, String)>,
    /// The level the sound is heard at: its `volume` and its `<volume>` points.
    pub level: Level,
}

impl Sound {
    /// The first sample frame of its source that is heard, at the source's `rate`.
    pub fn skip(&self, rate: u32) -> u64 {
        self.media_start
            .as_ref()
            .map_or(0, |(time, _)| time.nearest_count(rate).max(0) as u64)
    }
}

/// The sounds of the project's composition that can be placed, in document order: those `gather` gives.
pub fn sounds(project: &Project) -> Vec<Sound> {
    gather(project, project.tree().nodes()).0
}

/// The sounds of the root's `<audio>` elements in document order, and the faults of the ones that cannot be
/// placed: no `src`, no `duration` above 0, no file at `src`, and a level that is wrong (`docs/spec/sound.md`, section 3a).
pub(crate) fn gather(project: &Project, tree: &[Node]) -> (Vec<Sound>, Vec<Fault>) {
    let mut sounds = Vec::new();
    let mut faults = Vec::new();
    for (index, node) in tree
        .iter()
        .enumerate()
        .filter(|(_, node)| node.element() == Element::Audio && node.parent() == Some(0))
    {
        let line = node.line();
        let fault = |code, message: String| Fault {
            code,
            line,
            message,
            style: false,
        };
        let (level, level_faults) = level_of(tree, index, node);
        faults.extend(level_faults);
        let src = node.attribute("src").map(|attribute| attribute.value());
        if src.is_none() {
            faults.push(fault(
                Code::SoundMissing,
                "`<audio>` needs the attribute `src`, the path of a sound file: from the project folder, or a path that begins with `/`"
                    .to_string(),
            ));
        }
        let duration = node.attribute("duration");
        let length = duration.and_then(|attribute| attribute.time()).filter(|time| time.is_positive());
        if length.is_none() {
            // The sound is named by its `src` as written; an `<audio>` with none is named as the element.
            let named = match src {
                Some(src) => format!("`<audio src=\"{src}\">`"),
                None => "`<audio>`".to_string(),
            };
            let message = match duration {
                Some(attribute) => format!(
                    "{named} needs a `duration` above 0, the seconds the sound plays, not `duration=\"{}\"`",
                    attribute.shown()
                ),
                None => format!("{named} needs a `duration` above 0, the seconds the sound plays"),
            };
            faults.push(fault(Code::SoundNoDuration, message));
        }
        let Some(src) = src else { continue };
        let path = project.resolve(src);
        if !path.is_file() {
            faults.push(fault(
                Code::SoundMissing,
                format!("`src=\"{src}\"` names no file: a sound file, from the project folder or a path that begins with `/`"),
            ));
            continue;
        }
        let Some(length) = length else { continue };
        let start = node.attribute("start").and_then(|attribute| attribute.time()).unwrap_or(Time::ZERO);
        sounds.push(Sound {
            line,
            src: src.to_string(),
            path,
            first: start.nearest_count(MIX_RATE).max(0) as u64,
            end: (start + length).nearest_count(MIX_RATE).max(0) as u64,
            media_start: node
                .attribute("media-start")
                .and_then(|attribute| attribute.time().map(|time| (time, attribute.shown()))),
            level,
        });
    }
    faults.sort_by_key(|fault| fault.line);
    (sounds, faults)
}

/// The level of an `<audio>` (`docs/spec/sound.md`, section 3a): its `volume`, and its `<volume>` points in the order
/// written, with the faults of them. A `volume` or a `value` below 0 is `FS-0034`; a point with no `value` is `FS-0088`;
/// the first point that is before the one before it is `FS-0087`, and no other point of the sound is reported for order.
fn level_of(tree: &[Node], audio: usize, node: &Node) -> (Level, Vec<Fault>) {
    let mut faults = Vec::new();
    let mut fault = |line: u32, code, message: String| {
        faults.push(Fault {
            code,
            line,
            message,
            style: false,
        })
    };
    let volume = match node.attribute("volume") {
        Some(attribute) => match attribute.number() {
            Some(value) if value < 0.0 => {
                fault(
                    attribute.line(),
                    Code::LengthNotAccepted,
                    format!("`volume=\"{}\"` is below 0: a level is 0 or more", attribute.shown()),
                );
                1.0
            }
            Some(value) => value,
            None => 1.0,
        },
        None => 1.0,
    };
    let mut points = Vec::new();
    let mut before: Option<(Time, String)> = None;
    let mut out_of_order_reported = false;
    for point in tree
        .iter()
        .filter(|point| point.element() == Element::Volume && point.parent() == Some(audio))
    {
        let at = point.attribute("at");
        let time = at.and_then(|attribute| attribute.time()).unwrap_or(Time::ZERO);
        let written = at.map_or_else(|| "0".to_string(), |attribute| attribute.shown());
        if let Some((previous, previous_written)) = &before {
            if time < *previous && !out_of_order_reported {
                out_of_order_reported = true;
                fault(
                    point.line(),
                    Code::VolumeOutOfOrder,
                    format!(
                        "the <volume> points of an <audio> must be in order of time: the point at {written} s is before the one at {previous_written} s"
                    ),
                );
            }
        }
        before = Some((time, written));
        match point.attribute("value") {
            None => fault(
                point.line(),
                Code::VolumeNoValue,
                "`<volume>` needs a `value`, the level at its time, 0 or more".to_string(),
            ),
            Some(attribute) => match attribute.number() {
                Some(value) if value < 0.0 => fault(
                    attribute.line(),
                    Code::LengthNotAccepted,
                    format!("`value=\"{}\"` is below 0: a level is 0 or more", attribute.shown()),
                ),
                Some(value) => points.push((time, value)),
                None => {}
            },
        }
    }
    (Level { volume, points }, faults)
}
