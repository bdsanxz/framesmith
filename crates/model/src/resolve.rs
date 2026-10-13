//! Turns what a page declares into a manifest and diagnostics.
//!
//! The same code runs on the host (from the files) and in the page (from
//! the document), so the two can only disagree when the page really differs
//! from the files.

use crate::{Clip, Diagnostic, Manifest, RawDoc, RawElement, TweenFact};
use motion::{parse_block, Tween};
use std::collections::BTreeMap;
use timebase::{parse_fps, FpsError, Time, DEFAULT_FPS};

/// The `type` of a motion block's `script` element.
pub const MOTION_TYPE: &str = "application/framesmith-motion+json";

const A_COMPOSITION: &str = "data-fs-composition";
const A_SIZE: &str = "data-fs-size";
const A_FPS: &str = "data-fs-fps";
const A_DURATION: &str = "data-fs-duration";
const A_CLIP: &str = "data-fs-clip";
const A_START: &str = "data-fs-start";
const A_TRACK: &str = "data-fs-track";
const VAR_PREFIX: &str = "data-fs-var-";
const RELATIVE_PREFIX: &str = "after:";
const MIN_SIDE: u32 = 2;
const MAX_SIDE: u32 = 16384;
/// How long a still-image clip lasts when no duration is written.
const IMAGE_CLIP_SECONDS: i64 = 3;

/// Why a tween target could not be matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectorProblem {
    /// Not a valid selector.
    Invalid,
    /// A valid selector in a form that cannot be read from the files.
    Unsupported,
}

/// A start written with `after:` and a text, still to be read against the ids of the clips.
#[derive(Clone)]
struct RelativeStart {
    /// What follows `after:`, spaces at its ends and after the colon removed: the id of the clip to
    /// follow in the plain form, or an offset (decided once the ids of all the clips are known).
    text: String,
    /// The value as written, spaces at its ends removed.
    written: String,
    /// The line of the `data-fs-start` attribute.
    line: u32,
}

/// What follows `after:` in a start, trimmed. `None` when the value does not begin with `after:`.
fn relative_text(value: &str) -> Option<&str> {
    Some(value.strip_prefix(RELATIVE_PREFIX)?.trim())
}

/// Where an offset begins in a text that is not the id of a clip, and whether the text is of the offset
/// form (`composition.md`, "Relative start", steps 2 and 3): the base (the id the offset is added to) and
/// the offset text (a sign, or signs, and what follows). `None` when the text is not of the offset form.
fn split_offset(text: &str) -> Option<(&str, &str)> {
    // A `+` always begins the offset; without one, only the last `-` can.
    let mut start = match text.find('+') {
        Some(at) => at,
        None => text.rfind('-')?,
    };
    // Signs that stand directly before it (spaces between them) belong to the offset.
    loop {
        let before = text[..start].trim_end();
        if before.ends_with(['+', '-']) {
            start = before.len() - 1;
        } else {
            break;
        }
    }
    let base = text[..start].trim_end();
    if base.is_empty() {
        return None;
    }
    let offset_text = &text[start..];
    let tail = offset_text.trim_start_matches(|c: char| c == '+' || c == '-' || c.is_whitespace());
    let is_number = tail.chars().any(|c| c.is_ascii_digit()) && tail.chars().all(|c| c.is_ascii_digit() || c == '.');
    (offset_text.contains('+') || is_number).then_some((base, offset_text))
}

/// Reads an offset text: one sign, spaces, and a time as every written time is read (no sign of its own).
/// The result is exact: a plus or minus the time.
fn read_offset(offset_text: &str) -> Option<Time> {
    let mut chars = offset_text.chars();
    let sign = chars.next()?;
    let rest = chars.as_str().trim_start();
    if rest.starts_with(['+', '-']) {
        return None;
    }
    let time = Time::parse(rest)?;
    match sign {
        '+' => Some(time),
        '-' => Some(Time::ZERO - time),
        _ => None,
    }
}

struct ClipDraft {
    element: usize,
    id: Option<String>,
    label: String,
    tag: String,
    line: u32,
    start: Option<Time>,
    /// Set when the start is the plain relative form, still to be resolved.
    relative: Option<RelativeStart>,
    /// `Some(None)`: no duration written. `None`: written wrongly.
    duration: Option<Option<Time>>,
    track: Option<i64>,
    parent: Option<usize>,
}

impl ClipDraft {
    /// The clip's own duration and whether it was written: the written one,
    /// or the fixed one of an image clip.
    fn own_duration(&self) -> Option<(Time, bool)> {
        match self.duration? {
            Some(written) => Some((written, true)),
            None if self.tag == "img" => Some((Time::from_secs(IMAGE_CLIP_SECONDS), false)),
            None => None,
        }
    }

    /// No duration is written and the clip has none of its own, so it lasts
    /// until the end of the composition.
    fn lasts_to_the_end(&self) -> bool {
        self.duration == Some(None) && self.tag != "img"
    }
}

struct CompositionDraft {
    element: usize,
    line: u32,
    name: Option<String>,
    size: Option<(u32, u32)>,
    /// `Some(None)`: not written. `None`: written wrongly.
    fps: Option<Option<u32>>,
    length: Option<Option<Time>>,
}

/// The first phase: everything that can be read without matching selectors.
pub struct Draft {
    file: String,
    diagnostics: Vec<Diagnostic>,
    composition: Option<CompositionDraft>,
    /// The ids of the compositions other than the one being read.
    other_compositions: Vec<String>,
    clips: Vec<ClipDraft>,
    tweens: Vec<(Tween, u32)>,
}

/// The result of resolving a page.
#[derive(Clone, Debug, Default)]
pub struct Resolved {
    /// Present when the composition, every clip and every tween resolved.
    pub manifest: Option<Manifest>,
    pub diagnostics: Vec<Diagnostic>,
    /// For each tween in the manifest, the keys of the elements its target matches.
    pub targets: Vec<Vec<usize>>,
    /// For each clip in the manifest, its index in the raw element list.
    pub clip_elements: Vec<usize>,
}

/// "a", "a and b", "a, b and c".
fn join_names(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

fn is_name(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn parse_size(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.trim().split_once(['x', 'X'])?;
    let side = |s: &str| {
        let s = s.trim();
        if s.is_empty() || s.len() > 6 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        s.parse::<u32>().ok().filter(|n| (MIN_SIDE..=MAX_SIDE).contains(n))
    };
    Some((side(w)?, side(h)?))
}

impl Draft {
    /// Reads the composition, the clips and the motion blocks. `file` names
    /// the page in diagnostics.
    pub fn read(raw: &RawDoc, file: &str) -> Draft {
        let mut draft = Draft {
            file: file.to_string(),
            diagnostics: Vec::new(),
            composition: None,
            other_compositions: Vec::new(),
            clips: Vec::new(),
            tweens: Vec::new(),
        };
        let owner = draft.find_compositions(raw);
        draft.read_composition(raw);
        draft.read_attributes(raw, &owner);
        draft.read_clips(raw, &owner);
        draft.read_motion(raw);
        draft
    }

    fn add(&mut self, code: &str, line: u32, description: impl Into<String>, remedy: impl Into<String>) {
        let diagnostic = Diagnostic::new(code, &self.file, line, description, remedy);
        self.diagnostics.push(diagnostic);
    }

    /// Picks the composition and reports any other. Returns, for every raw
    /// element, the composition element that encloses it (or is it).
    fn find_compositions(&mut self, raw: &RawDoc) -> Vec<Option<usize>> {
        let mut owner: Vec<Option<usize>> = vec![None; raw.elements.len()];
        let mut main: Option<usize> = None;
        for (index, element) in raw.elements.iter().enumerate() {
            let inherited = element.parent.and_then(|parent| owner[parent]);
            let Some(attr) = element.attr(A_COMPOSITION) else {
                owner[index] = inherited;
                continue;
            };
            owner[index] = Some(index);
            if let Some(id) = element.id.as_ref().filter(|id| !id.is_empty()) {
                self.other_compositions.push(id.clone());
            }
            if inherited.is_some() {
                self.add(
                    "FS-U006",
                    attr.line,
                    format!(
                        "The composition \"{}\" is nested inside another composition; nested compositions are not supported in this version.",
                        attr.value
                    ),
                    "Keep one composition per project: remove data-fs-composition from the inner element.",
                );
            } else if main.is_some() {
                self.add(
                    "FS-U005",
                    attr.line,
                    format!(
                        "\"{}\" is a second composition; a project holds one composition in this version.",
                        attr.value
                    ),
                    "Keep one composition per project: move the second composition to its own project folder.",
                );
            } else {
                main = Some(index);
            }
        }
        if let Some(main) = main {
            // The composition being read is not "another" one.
            if let Some(id) = raw.elements[main].id.as_ref() {
                if let Some(at) = self.other_compositions.iter().position(|other| other == id) {
                    self.other_compositions.remove(at);
                }
            }
        }
        match main {
            Some(element) => {
                self.composition = Some(CompositionDraft {
                    element,
                    line: raw.elements[element].line,
                    name: None,
                    size: None,
                    fps: None,
                    length: None,
                });
            }
            None => self.add(
                "FS-E003",
                0,
                "The page has no composition.",
                "Add data-fs-composition=\"main\" and data-fs-size=\"1920x1080\" to the element that holds the picture.",
            ),
        }
        owner
    }

    fn read_composition(&mut self, raw: &RawDoc) {
        let Some(element_index) = self.composition.as_ref().map(|c| c.element) else {
            return;
        };
        let element = &raw.elements[element_index];
        let line = element.line;

        let name_attr = element.attr(A_COMPOSITION).cloned().unwrap_or_default();
        let name = if is_name(name_attr.value.trim()) {
            Some(name_attr.value.trim().to_string())
        } else {
            self.add(
                "FS-E004",
                name_attr.line,
                format!(
                    "The composition name \"{}\" is empty or has characters other than letters, digits, - and _.",
                    name_attr.value
                ),
                "Give the composition a simple name, for example data-fs-composition=\"main\".",
            );
            None
        };

        let size = match element.attr(A_SIZE) {
            None => {
                self.add(
                    "FS-E005",
                    line,
                    "The composition has no size.",
                    "Add data-fs-size with the width and height in pixels, for example data-fs-size=\"1920x1080\".",
                );
                None
            }
            Some(attr) => match parse_size(&attr.value) {
                None => {
                    self.add(
                        "FS-E006",
                        attr.line,
                        format!(
                            "The composition size \"{}\" is not two whole numbers from {MIN_SIDE} to {MAX_SIDE}.",
                            attr.value
                        ),
                        "Write the size as WIDTHxHEIGHT in pixels, for example data-fs-size=\"1920x1080\".",
                    );
                    None
                }
                Some((width, height)) if width % 2 == 1 || height % 2 == 1 => {
                    self.add(
                        "FS-E007",
                        attr.line,
                        format!("The composition is {width}x{height}; the width and the height must both be even."),
                        "Write an even width and an even height, for example data-fs-size=\"1920x1080\".",
                    );
                    None
                }
                Some(size) => Some(size),
            },
        };

        let fps = match element.attr(A_FPS) {
            None => Some(None),
            Some(attr) => match parse_fps(&attr.value) {
                Ok(fps) => Some(Some(fps)),
                Err(FpsError::Ratio) => {
                    self.add(
                        "FS-U004",
                        attr.line,
                        format!(
                            "The frame rate \"{}\" is written as a ratio; ratio frame rates are not supported in this version.",
                            attr.value
                        ),
                        "Write a whole number of frames per second, for example data-fs-fps=\"30\".",
                    );
                    None
                }
                Err(FpsError::Invalid) => {
                    self.add(
                        "FS-E008",
                        attr.line,
                        format!("The frame rate \"{}\" is not a whole number from 1 to 240.", attr.value),
                        "Write a whole number of frames per second, for example data-fs-fps=\"30\", or remove the attribute to use 30.",
                    );
                    None
                }
            },
        };

        let length = match element.attr(A_DURATION) {
            None => Some(None),
            Some(attr) => match Time::parse(&attr.value) {
                None => {
                    self.add(
                        "FS-E009",
                        attr.line,
                        format!("The composition length \"{}\" is not a number of seconds.", attr.value),
                        "Write the length as a plain number of seconds, for example data-fs-duration=\"6\".",
                    );
                    None
                }
                Some(length) if !length.is_positive() => {
                    self.add(
                        "FS-E010",
                        attr.line,
                        format!(
                            "The composition length is {} seconds; it must be greater than zero.",
                            attr.value.trim()
                        ),
                        "Write a length greater than zero, for example data-fs-duration=\"6\".",
                    );
                    None
                }
                Some(length) => Some(Some(length)),
            },
        };

        if let Some(composition) = self.composition.as_mut() {
            composition.name = name;
            composition.size = size;
            composition.fps = fps;
            composition.length = length;
        }
    }

    /// Reports attributes that are unknown, unsupported, or on the wrong element.
    fn read_attributes(&mut self, raw: &RawDoc, owner: &[Option<usize>]) {
        let main = self.composition.as_ref().map(|c| c.element);
        for (index, element) in raw.elements.iter().enumerate() {
            let is_composition = element.attr(A_COMPOSITION).is_some();
            let is_clip = element.attr(A_CLIP).is_some();
            // Attributes of another composition are reported with that composition.
            let foreign = is_composition && Some(index) != main;
            for attr in &element.attrs {
                let name = attr.name.as_str();
                match name {
                    A_COMPOSITION => {}
                    A_SIZE | A_FPS if is_composition => {}
                    A_DURATION if is_composition || is_clip => {}
                    A_CLIP => {
                        if is_composition {
                            self.add(
                                "FS-E026",
                                attr.line,
                                "data-fs-clip is written on the composition element.",
                                "Remove data-fs-clip from the composition; mark elements inside it as clips instead.",
                            );
                        } else if owner[index].is_none() {
                            self.add(
                                "FS-E027",
                                attr.line,
                                format!("The clip <{}> is outside the composition.", element.tag),
                                "Move the clip inside the element that carries data-fs-composition.",
                            );
                        }
                    }
                    A_START | A_TRACK if is_clip && !is_composition => {}
                    A_SIZE | A_FPS | A_DURATION | A_START | A_TRACK => {
                        if !foreign {
                            let home = if matches!(name, A_SIZE | A_FPS) {
                                "the composition element"
                            } else {
                                "a clip (an element with data-fs-clip)"
                            };
                            self.add(
                                "FS-E026",
                                attr.line,
                                format!("{name} is written on <{}>, which is not {home}.", element.tag),
                                format!("Move {name} to {home}, or remove it."),
                            );
                        }
                    }
                    _ if name.starts_with(VAR_PREFIX) => self.add(
                        "FS-U007",
                        attr.line,
                        format!("The attribute {name} declares a variable; variables are not supported in this version."),
                        "Remove the attribute and write the value directly in the page.",
                    ),
                    _ => self.add(
                        "FS-X001",
                        attr.line,
                        format!("The attribute {name} is not one Framesmith knows."),
                        "Check the spelling. The attributes are data-fs-composition, data-fs-size, data-fs-fps, data-fs-duration, data-fs-clip, data-fs-start and data-fs-track.",
                    ),
                }
            }
        }
    }

    fn read_clips(&mut self, raw: &RawDoc, owner: &[Option<usize>]) {
        let Some(main) = self.composition.as_ref().map(|c| c.element) else {
            return;
        };
        // Raw element index to clip index, for finding the enclosing clip.
        let mut clip_of: BTreeMap<usize, usize> = BTreeMap::new();
        for (index, element) in raw.elements.iter().enumerate() {
            if element.attr(A_CLIP).is_none() || index == main || owner[index] != Some(main) {
                continue;
            }
            let number = self.clips.len() + 1;
            let label = match &element.id {
                Some(id) if !id.is_empty() => format!("#{id}"),
                _ => format!("number {number} <{}>", element.tag),
            };
            let (start, relative) = self.read_clip_start(element, &label);
            let duration = self.read_clip_duration(element, &label);
            let track = self.read_clip_track(element, &label);
            let mut parent = None;
            let mut ancestor = element.parent;
            while let Some(a) = ancestor {
                if let Some(&clip) = clip_of.get(&a) {
                    parent = Some(clip);
                    break;
                }
                ancestor = raw.elements[a].parent;
            }
            clip_of.insert(index, self.clips.len());
            self.clips.push(ClipDraft {
                element: index,
                id: element.id.clone().filter(|id| !id.is_empty()),
                label,
                tag: element.tag.clone(),
                line: element.line,
                start,
                relative,
                duration,
                track,
                parent,
            });
        }
    }

    /// Reads a clip's start: the time, or the relative start still to be resolved.
    fn read_clip_start(&mut self, element: &RawElement, label: &str) -> (Option<Time>, Option<RelativeStart>) {
        let Some(attr) = element.attr(A_START) else {
            self.add(
                "FS-E020",
                element.line,
                format!("The clip {label} has no start."),
                "Add data-fs-start with the time in seconds at which the clip appears, for example data-fs-start=\"1\".",
            );
            return (None, None);
        };
        let value = attr.value.trim();
        match relative_text(value) {
            Some("") => {
                self.add(
                    "FS-E021",
                    attr.line,
                    format!(
                        "The start of the clip {label}, \"{value}\", is neither a number of seconds nor the id of one clip after after:."
                    ),
                    "Write the start as a plain number of seconds, or as after: and the id of one clip, for example data-fs-start=\"after:intro\".",
                );
                return (None, None);
            }
            Some(text) => {
                let relative = RelativeStart {
                    text: text.to_string(),
                    written: value.to_string(),
                    line: attr.line,
                };
                return (None, Some(relative));
            }
            None => {}
        }
        let start = match Time::parse(value) {
            None => {
                self.add(
                    "FS-E021",
                    attr.line,
                    format!("The start of the clip {label}, \"{value}\", is not a number of seconds."),
                    "Write the start as a plain number of seconds, for example data-fs-start=\"1.5\".",
                );
                None
            }
            Some(start) if start.is_negative() => {
                self.add(
                    "FS-E022",
                    attr.line,
                    format!("The clip {label} starts at {value} seconds, before the start of the composition."),
                    "Write a start of 0 or more.",
                );
                None
            }
            Some(start) => Some(start),
        };
        (start, None)
    }

    fn read_clip_duration(&mut self, element: &RawElement, label: &str) -> Option<Option<Time>> {
        let Some(attr) = element.attr(A_DURATION) else {
            return Some(None);
        };
        let value = attr.value.trim();
        match Time::parse(value) {
            None => {
                self.add(
                    "FS-E023",
                    attr.line,
                    format!("The duration of the clip {label}, \"{value}\", is not a number of seconds."),
                    "Write the duration as a plain number of seconds, for example data-fs-duration=\"2\".",
                );
                None
            }
            Some(duration) if !duration.is_positive() => {
                self.add(
                    "FS-E024",
                    attr.line,
                    format!("The clip {label} has a duration of {value} seconds; it must be greater than zero."),
                    "Write a duration greater than zero, or remove the attribute to have it worked out.",
                );
                None
            }
            Some(duration) => Some(Some(duration)),
        }
    }

    fn read_clip_track(&mut self, element: &RawElement, label: &str) -> Option<i64> {
        let attr = element.attr(A_TRACK)?;
        let value = attr.value.trim();
        let digits = value.strip_prefix('-').unwrap_or(value);
        let whole = !digits.is_empty() && digits.len() <= 9 && digits.bytes().all(|b| b.is_ascii_digit());
        match value.parse::<i64>() {
            Ok(track) if whole => Some(track),
            _ => {
                self.add(
                    "FS-E028",
                    attr.line,
                    format!("The track of the clip {label}, \"{value}\", is not a whole number."),
                    "Write the track as a whole number, for example data-fs-track=\"1\", or remove it.",
                );
                None
            }
        }
    }

    fn read_motion(&mut self, raw: &RawDoc) {
        let name = self.composition.as_ref().and_then(|c| c.name.clone());
        for block in &raw.motion {
            let parsed = parse_block(&block.text);
            let absolute = |line: u32| if block.line == 0 { 0 } else { block.line + line - 1 };
            for finding in parsed.findings {
                self.add(finding.code, absolute(finding.line), finding.description, finding.remedy);
            }
            if let (Some((written, line)), Some(name)) = (&parsed.composition, &name) {
                if written != name {
                    self.add(
                        "FS-E046",
                        absolute(*line),
                        format!("The motion block is for a composition named \"{written}\", but this composition is \"{name}\"."),
                        format!("Write \"composition\": \"{name}\", or remove the field."),
                    );
                }
            }
            for (tween, line) in parsed.tweens {
                self.tweens.push((tween, absolute(line)));
            }
        }
    }

    /// The raw index of the composition element, when there is one.
    pub fn composition_element(&self) -> Option<usize> {
        self.composition.as_ref().map(|c| c.element)
    }

    /// The distinct tween targets, in order of first use.
    pub fn selectors(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for (tween, _) in &self.tweens {
            if !seen.contains(&tween.target) {
                seen.push(tween.target.clone());
            }
        }
        seen
    }

    /// The second phase: matches every target with `matcher`, fills in what
    /// was not written, and builds the manifest.
    pub fn finish(mut self, matcher: &mut dyn FnMut(&str) -> Result<Vec<usize>, SelectorProblem>) -> Resolved {
        let mut matched: BTreeMap<String, Result<Vec<usize>, SelectorProblem>> = BTreeMap::new();
        for selector in self.selectors() {
            let result = matcher(&selector);
            matched.insert(selector, result);
        }
        let tweens = std::mem::take(&mut self.tweens);
        let mut facts: Vec<TweenFact> = Vec::new();
        let mut targets: Vec<Vec<usize>> = Vec::new();
        let mut all_tweens_ok = true;
        for (tween, line) in &tweens {
            match &matched[&tween.target] {
                Ok(elements) if !elements.is_empty() => {
                    facts.push(TweenFact {
                        tween: tween.clone(),
                        matches: elements.len(),
                    });
                    targets.push(elements.clone());
                }
                Ok(_) => {
                    all_tweens_ok = false;
                    self.add(
                        "FS-E044",
                        *line,
                        format!("The tween target \"{}\" matches no element in the composition.", tween.target),
                        "Correct the selector, or add the element it should match inside the composition.",
                    );
                }
                Err(SelectorProblem::Invalid) => {
                    all_tweens_ok = false;
                    self.add(
                        "FS-E048",
                        *line,
                        format!("The tween target \"{}\" is not a valid CSS selector.", tween.target),
                        "Write a CSS selector, for example \"#title\" or \".card\".",
                    );
                }
                Err(SelectorProblem::Unsupported) => {
                    all_tweens_ok = false;
                    self.add(
                        "FS-U020",
                        *line,
                        format!(
                            "The tween target \"{}\" uses a selector form that is not supported in this version.",
                            tween.target
                        ),
                        "Use type, #id, .class and [attribute] selectors, joined by spaces, > or commas.",
                    );
                }
            }
        }

        // What the motion gives for working the length out. Faulty tweens
        // are already reported; the valid ones are enough to carry on.
        let all_tweens: Vec<Tween> = tweens.iter().map(|(tween, _)| tween.clone()).collect();
        let motion_end = motion::motion_end(&all_tweens).filter(|end| end.is_positive());

        // The length is settled first: a clip with no end of its own lasts until it.
        let mut drafts = std::mem::take(&mut self.clips);
        self.resolve_relative_starts(&mut drafts);
        let last_own_end = drafts.iter().filter_map(|draft| Some(draft.start? + draft.own_duration()?.0)).max();
        let written_length = self.composition.as_ref().map(|composition| composition.length);
        let length = match written_length {
            Some(Some(Some(written))) => Some((written, true)),
            Some(Some(None)) => motion_end.or(last_own_end).map(|end| (end, false)),
            _ => None,
        };
        let no_length_to_work_out = written_length == Some(Some(None)) && length.is_none();

        let mut clips: Vec<Clip> = Vec::new();
        let mut clip_elements: Vec<usize> = Vec::new();
        let mut all_clips_ok = true;
        for draft in &drafts {
            let duration = match (draft.own_duration(), length) {
                (Some(own), _) => Some(own),
                (None, Some((length, _))) if draft.lasts_to_the_end() => {
                    let left = draft
                        .start
                        .filter(|start| *start < length)
                        .map_or(Time::ZERO, |start| length - start);
                    Some((left, false))
                }
                (None, None) if draft.lasts_to_the_end() && no_length_to_work_out => {
                    self.add(
                        "FS-E025",
                        draft.line,
                        format!(
                            "The clip {} has no duration, and the composition has no length for it to last until: none is written, and there is no motion and no clip with its own end to work one out from.",
                            draft.label
                        ),
                        "Write a duration on the clip, for example data-fs-duration=\"2\", or a length on the composition.",
                    );
                    None
                }
                _ => None,
            };
            match (draft.start, duration) {
                (Some(start), Some((duration, duration_written))) => {
                    clips.push(Clip {
                        label: draft.label.clone(),
                        tag: draft.tag.clone(),
                        start,
                        relative_start: draft.relative.as_ref().map(|relative| relative.written.clone()),
                        duration,
                        duration_written,
                        track: draft.track,
                        parent: draft.parent,
                    });
                    clip_elements.push(draft.element);
                }
                _ => all_clips_ok = false,
            }
        }
        if no_length_to_work_out && !drafts.iter().any(ClipDraft::lasts_to_the_end) {
            let line = self.composition.as_ref().map_or(0, |composition| composition.line);
            self.add(
                "FS-E011",
                line,
                "The composition has no length, and no motion and no clip with its own end to work one out from.",
                "Write the length in seconds on the composition, for example data-fs-duration=\"6\".",
            );
        }

        let manifest = if all_clips_ok && all_tweens_ok {
            self.build_manifest(clips, facts, length)
        } else {
            None
        };
        if let Some(manifest) = &manifest {
            for (clip, draft) in manifest.clips.iter().zip(&drafts) {
                if clip.start >= manifest.length {
                    self.add(
                        "FS-W001",
                        draft.line,
                        format!(
                            "The clip {} starts at {} s, at or after the end of the composition ({} s), so it never shows.",
                            clip.label, clip.start, manifest.length
                        ),
                        "Give the clip an earlier start, lengthen the composition, or remove the clip.",
                    );
                }
            }
        }
        let mut diagnostics = self.diagnostics;
        diagnostics.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));
        Resolved {
            manifest,
            diagnostics,
            targets,
            clip_elements,
        }
    }

    /// Gives every clip that follows another its start: the end of the named clip (its start plus its
    /// written duration) plus the offset. The named clip may itself follow another, to any depth; the
    /// starts are worked out from the clip that has a time, down each line, whatever order the clips are
    /// written in. A fault, or a form that is not built yet, is reported and leaves the start open, and a
    /// clip that follows a clip whose start is open gets no report of its own. Clips whose starts depend
    /// on each other in a circle are reported once, together (FS-E053).
    fn resolve_relative_starts(&mut self, drafts: &mut [ClipDraft]) {
        // First, what each relative start names: the index of the clip it follows, and its offset
        // (`None` when the offset is not a time, which is already reported). A start with a fault of its
        // own names nothing.
        let mut links: Vec<Option<(usize, Option<Time>)>> = vec![None; drafts.len()];
        for index in 0..drafts.len() {
            let Some(relative) = drafts[index].relative.clone() else {
                continue;
            };
            let label = drafts[index].label.clone();
            let (text, written, line) = (relative.text.as_str(), relative.written.as_str(), relative.line);
            let by_id = |id: &str| drafts.iter().position(|draft| draft.id.as_deref() == Some(id));
            let remedy_missing = "Write the id of a clip of this composition, give the clip you mean data-fs-clip and that id, or write the start as a time in seconds.";
            let nested_remedy =
                "Write the start as a time in seconds from the start of the composition, for example data-fs-start=\"2.5\".";
            let nested = |id: &str| {
                format!("The clip {label} starts after \"{id}\", a nested composition; following a nested composition is not supported in this version.")
            };

            // Step 1: the whole text is the id of a clip.
            let (named, offset) = if let Some(named) = by_id(text) {
                (named, Some(Time::ZERO))
            } else if self.other_compositions.iter().any(|name| name == text) {
                self.add("FS-U003", line, nested(text), nested_remedy);
                continue;
            } else if let Some((base, offset_text)) = split_offset(text) {
                // Steps 2 and 3: a base and an offset.
                let offset = read_offset(offset_text);
                if offset.is_none() {
                    self.add(
                        "FS-E051",
                        line,
                        format!("The offset \"{offset_text}\" in the start \"{written}\" of the clip {label} is not a time in seconds."),
                        format!("Write the offset as + or - and a number of seconds with at most nine decimals and no unit, for example after:{base}+0.5, or write the start as a time in seconds."),
                    );
                }
                if self.other_compositions.iter().any(|name| name == base) {
                    self.add("FS-U003", line, nested(base), nested_remedy);
                    continue;
                }
                let Some(named) = by_id(base) else {
                    let description = if offset.is_some() {
                        format!("The clip {label} starts after \"{text}\", but no clip with the id \"{text}\" is in this composition; read as the clip \"{base}\" and the offset \"{offset_text}\", no clip with the id \"{base}\" is in it either; only an element marked data-fs-clip can be named.")
                    } else {
                        format!("The clip {label} starts after \"{base}\" (the part of \"{written}\" before the offset), but no clip with the id \"{base}\" is in this composition; only an element marked data-fs-clip can be named.")
                    };
                    self.add("FS-E050", line, description, remedy_missing);
                    continue;
                };
                (named, offset)
            } else {
                // Step 4: an id that no clip has.
                self.add(
                    "FS-E050",
                    line,
                    format!("The clip {label} starts after \"{text}\", but no clip with the id \"{text}\" is in this composition; only an element marked data-fs-clip can be named."),
                    remedy_missing,
                );
                continue;
            };
            if named == index {
                self.add_names_itself(&label, written, line);
                continue;
            }
            links[index] = Some((named, offset));
        }

        // Then the starts, from the clips that have a time down each line of followers. A walk follows
        // the names from one clip until it reaches a clip that is settled (an absolute start, a fault,
        // or one worked out already), or comes back to a clip on its own way, which is a circle.
        // Nothing recurses, so the depth of a line does not matter.
        #[derive(Clone, Copy, PartialEq)]
        enum State {
            Fresh,
            OnWalk,
            Settled,
        }
        let mut state: Vec<State> = drafts
            .iter()
            .map(|draft| if draft.relative.is_some() { State::Fresh } else { State::Settled })
            .collect();
        for first in 0..drafts.len() {
            let mut walk: Vec<usize> = Vec::new();
            let mut current = first;
            let mut closed_at: Option<usize> = None;
            loop {
                match state[current] {
                    State::Settled => break,
                    State::OnWalk => {
                        closed_at = walk.iter().position(|&member| member == current);
                        break;
                    }
                    State::Fresh => {}
                }
                state[current] = State::OnWalk;
                walk.push(current);
                match links[current] {
                    Some((named, _)) => current = named,
                    None => break,
                }
            }
            if let Some(from) = closed_at {
                // The clips of the walk before `from` only lead into the circle: they are settled with no
                // start and no report of their own.
                self.add_circle(drafts, &links, &walk[from..]);
                for &member in &walk {
                    state[member] = State::Settled;
                }
                continue;
            }
            // From the end of the walk back to where it began, each clip's start is worked out from the
            // start of the clip it names, which is settled by then.
            while let Some(index) = walk.pop() {
                state[index] = State::Settled;
                let Some((named, Some(offset))) = links[index] else {
                    continue;
                };
                let label = drafts[index].label.clone();
                let written = drafts[index]
                    .relative
                    .as_ref()
                    .map(|relative| relative.written.clone())
                    .unwrap_or_default();
                let line = drafts[index].relative.as_ref().map_or(0, |relative| relative.line);
                let target_label = drafts[named].label.clone();
                // The named clip's start is open: its own fault is reported with it.
                let Some(start) = drafts[named].start else {
                    continue;
                };
                match drafts[named].duration {
                    // Written wrongly: reported with the named clip.
                    None => {}
                    Some(None) => self.add(
                        "FS-U003",
                        line,
                        format!(
                            "The clip {label} starts after {target_label}, which has no written duration; following a clip whose end is worked out is not supported in this version."
                        ),
                        format!("Write a duration on {target_label}, for example data-fs-duration=\"2\", or write the start of {label} as a time in seconds."),
                    ),
                    Some(Some(duration)) => {
                        let end = start + duration;
                        let sum = end + offset;
                        if sum.is_negative() {
                            self.add(
                                "FS-E052",
                                line,
                                format!(
                                    "The clip {label} starts at {sum} s (\"{written}\", and {target_label} ends at {end} s), before the start of the composition."
                                ),
                                format!("Use an offset of -{end} s or later, or write the start as a time of 0 or more in seconds. The clip is not moved to 0."),
                            );
                        } else {
                            drafts[index].start = Some(sum);
                        }
                    }
                }
            }
        }
    }

    /// Reports a circle of relative starts once: every clip of it, from the one written first, each with
    /// the clip it starts after. `members` are in the order of the names (each starts after the next).
    fn add_circle(&mut self, drafts: &[ClipDraft], links: &[Option<(usize, Option<Time>)>], members: &[usize]) {
        let first = members.iter().enumerate().min_by_key(|(_, &member)| member).map_or(0, |(at, _)| at);
        let ordered: Vec<usize> = members[first..].iter().chain(&members[..first]).copied().collect();
        let label = |index: usize| drafts[index].label.clone();
        let names: Vec<String> = ordered.iter().map(|&member| label(member)).collect();
        let steps: Vec<String> = ordered
            .iter()
            .map(|&member| {
                let (named, _) = links[member].unwrap_or((member, None));
                format!("{} starts after {}", label(member), label(named))
            })
            .collect();
        let line = drafts[ordered[0]].relative.as_ref().map_or(0, |relative| relative.line);
        self.add(
            "FS-E053",
            line,
            format!(
                "The starts of {} depend on each other in a circle: {}, so none of them can be worked out.",
                join_names(&names),
                join_names(&steps)
            ),
            "Break the circle: write the start of one of the clips of the circle as a time in seconds, or make it start after a clip that is not in the circle.",
        );
    }

    fn add_names_itself(&mut self, label: &str, written: &str, line: u32) {
        self.add(
            "FS-E049",
            line,
            format!("The clip {label} starts after itself (\"{written}\"), so its start cannot be worked out."),
            "Name the clip it should follow, for example data-fs-start=\"after:intro\", or write the start as a time in seconds.",
        );
    }

    fn build_manifest(&self, clips: Vec<Clip>, tweens: Vec<TweenFact>, length: Option<(Time, bool)>) -> Option<Manifest> {
        let composition = self.composition.as_ref()?;
        let (width, height) = composition.size?;
        let fps = composition.fps?;
        let (length, length_written) = length?;
        Some(Manifest {
            name: composition.name.clone()?,
            width,
            height,
            fps: fps.unwrap_or(DEFAULT_FPS),
            fps_written: fps.is_some(),
            length,
            length_written,
            clips,
            tweens,
        })
    }
}
