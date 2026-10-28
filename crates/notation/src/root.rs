//! A composition file: the root `<composition>` (`docs/spec/notation.md` section 6.1) and the form of
//! everything under it (`structure`), with the wording of `docs/spec/check-command.md` section 4.

use crate::code::Code;
use crate::problem::Problem;
use crate::schema::Element;
use crate::structure;
use crate::styles;
use crate::support::{self, Unsupported};
use crate::tree::{self, Tree};
use crate::value;
use timebase::{frame_count, Time, DEFAULT_FPS, MAX_FPS, MIN_FPS};

/// The most characters of a written value a message shows.
const SHOWN_CHARACTERS: usize = 80;
/// The size limits of the frame, in pixels.
const MIN_SIZE: i64 = 2;
const MAX_SIZE: i64 = 8192;
/// The only version of the notation that is read.
const VERSION: i64 = 1;

/// A colour: red, green, blue and alpha, each from 0 to 255.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colour {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Colour {
    /// Black, fully opaque: the canvas of a root that names no `background`.
    pub const BLACK: Colour = Colour {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 255,
    };
}

/// A valid composition file: its root, and the elements under it as they are written.
#[derive(Clone, Debug, PartialEq)]
pub struct Composition {
    /// The root with the defaults filled in.
    pub root: Root,
    /// Every element with its attributes and lines (`docs/spec/notation.md` section 6).
    pub tree: Tree,
}

/// The root of a valid composition file, with the defaults filled in.
#[derive(Clone, Debug, PartialEq)]
pub struct Root {
    /// The composition's name, when the root has an `id`.
    pub id: Option<String>,
    /// The frame's width in pixels: even, from 2 to 8192.
    pub width: u32,
    /// The frame's height in pixels: even, from 2 to 8192.
    pub height: u32,
    /// Frames a second, from 1 to 240; 30 when the root writes none.
    pub fps: u32,
    /// The length in seconds, as written: above 0.
    pub duration: Time,
    /// The number of frames: the frame grid rule applied to `duration` at `fps`, at least 1.
    pub frames: u64,
    /// The canvas; black when the root writes no `background`.
    pub background: Colour,
}

/// Where something is in the file: its line and its column, both counted from 1.
#[derive(Clone, Copy)]
pub(crate) struct Spot {
    pub(crate) line: u32,
    pub(crate) column: u32,
}

/// A problem with the place it points at, so that the problems of a file can be put in order.
pub(crate) struct Found {
    spot: Spot,
    pub(crate) problem: Problem,
}

pub(crate) fn found(code: Code, spot: Spot, message: String) -> Found {
    Found {
        spot,
        problem: Problem::new(code, spot.line, message),
    }
}

/// An attribute as it is written: its value without the surrounding white space, and where it is.
type Written<'a> = Option<(&'a str, Spot)>;

/// Reads the root of a composition file from its bytes, and judges the file.
///
/// Every problem of the file is returned, in the order of their places in the file (a line, then a
/// column; a missing attribute is at the root's opening `<`). A text that is not UTF-8, a text that
/// is not XML, and a root that is not `<composition>` each give that one problem and no other, since
/// nothing more can be judged. Otherwise `version`, `width`, `height`, `fps`, `duration` and
/// `background` are each judged, whatever the others hold; and so is the form of everything in the
/// file: an element the notation does not have, an attribute an element does not take, an element in
/// a place it may not stand, and a number or a time written in a form the notation does not list.
/// What an attribute means and the range it takes are the business of the part that draws it.
pub fn read_root(source: &[u8]) -> Result<Root, Vec<Problem>> {
    read(source).map(|composition| composition.root)
}

/// Reads a composition file as [`read_root`] does, and gives the elements under the root too.
pub fn read(source: &[u8]) -> Result<Composition, Vec<Problem>> {
    match read_listing(source) {
        (Some(composition), problems) if problems.is_empty() => Ok(composition),
        (_, problems) => Err(problems),
    }
}

/// Reads a composition file as [`read`] does, and gives also the composition of a file with problems, made without the
/// elements they were found on, when its root can be read: so that what this version does not support is listed in the
/// same run (`docs/spec/check-command.md` section 3d). The problems are in the order of the file.
pub fn read_listing(source: &[u8]) -> (Option<Composition>, Vec<Problem>) {
    let text = match std::str::from_utf8(source) {
        Ok(text) => text,
        Err(error) => {
            let line = 1 + source[..error.valid_up_to()].iter().filter(|&&byte| byte == b'\n').count();
            return (
                None,
                vec![Problem::new(
                    Code::NotUtf8,
                    line as u32,
                    "the file is not well-formed XML: it is not valid UTF-8 text".to_string(),
                )],
            );
        }
    };
    let document = match roxmltree::Document::parse(text) {
        Ok(document) => document,
        Err(error) => {
            return (
                None,
                vec![Problem::new(
                    Code::NotXml,
                    error.pos().row,
                    format!("the file is not well-formed XML: {error}"),
                )],
            )
        }
    };

    let element = document.root_element();
    let spot_at = |offset: usize| {
        let position = document.text_pos_at(offset);
        Spot {
            line: position.row,
            column: position.col,
        }
    };
    let root_spot = spot_at(element.range().start);
    let name = element.tag_name().name();
    if name != "composition" {
        return (
            None,
            vec![Problem::new(
                Code::RootNotComposition,
                root_spot.line,
                format!("the root element is <{name}>, not <composition>"),
            )],
        );
    }

    let mut problems = Vec::new();
    // The styles of the file, and the attributes the root reads: its own, and those its `class` gives it.
    let table = styles::styles(element, &spot_at, &mut problems);
    let read = styles::effective(element, text, Element::Composition, &table, &spot_at);

    // The written value of an attribute with the place it is at; the attribute has no namespace.
    let attribute = |name: &str| -> Written {
        read.iter()
            .find(|entry| !entry.prefixed && entry.name == name)
            .map(|entry| (entry.value.as_str(), entry.spot))
    };
    let version = keep(version(attribute("version"), root_spot), &mut problems);
    let width = keep(size(attribute("width"), "width", root_spot), &mut problems);
    let height = keep(size(attribute("height"), "height", root_spot), &mut problems);
    let fps = keep(fps(attribute("fps")), &mut problems);
    let duration = keep(duration(attribute("duration"), root_spot), &mut problems);
    let background = keep(background(attribute("background")), &mut problems);
    let judged = structure::judge(&document, text, &table, &mut problems);

    // Stable: two problems at one place stay in the order the attributes were judged.
    problems.sort_by_key(|found| (found.spot.line, found.spot.column));
    let composition = match (version, width, height, fps, duration, background) {
        (Some(()), Some(width), Some(height), Some(fps), Some(duration), Some(background)) if !judged.faulty.contains(&element.id()) => {
            Some(Composition {
                root: Root {
                    id: attribute("id").map(|(written, _)| written.to_string()),
                    width,
                    height,
                    fps,
                    duration,
                    frames: frame_count(duration, fps),
                    background,
                },
                tree: tree::build(&document, text, &table, &judged),
            })
        }
        _ => None,
    };
    (composition, problems.into_iter().map(|found| found.problem).collect())
}

/// What a composition file shows with `<composition src>`, read without judging the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shows {
    /// The frame rate the file is made at: 30 when it writes none, and `None` when what it writes is not a frame rate the notation takes.
    pub fps: Option<u32>,
    /// Each `<composition src>` the file holds where the notation lets one stand, in document order.
    pub elements: Vec<Show>,
}

/// A `<composition src>` of a file: the line of the element, and the file it names as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Show {
    pub line: u32,
    pub src: String,
}

/// Reads what a composition file shows, from its bytes, without judging the file.
///
/// The problems of the file are not returned here: [`read`] judges them. A text that is not UTF-8,
/// a text that is not XML and a root that is not `<composition>` show nothing and have no frame rate.
/// A chain of files is walked with this, so that a file with problems of its own still names the
/// files it shows, and every problem of the chain is reported in one run.
pub fn shows(source: &[u8]) -> Shows {
    let nothing = Shows {
        fps: None,
        elements: Vec::new(),
    };
    let Ok(text) = std::str::from_utf8(source) else {
        return nothing;
    };
    let Ok(document) = roxmltree::Document::parse(text) else {
        return nothing;
    };
    let element = document.root_element();
    if element.tag_name().name() != "composition" {
        return nothing;
    }
    let spot_at = |offset: usize| {
        let position = document.text_pos_at(offset);
        Spot {
            line: position.row,
            column: position.col,
        }
    };
    // The problems of the styles and of the root are judged by `read`; the values are all this needs.
    let mut ignored = Vec::new();
    let table = styles::styles(element, &spot_at, &mut ignored);
    let read = styles::effective(element, text, Element::Composition, &table, &spot_at);
    let attribute = |name: &str| -> Written {
        read.iter()
            .find(|entry| !entry.prefixed && entry.name == name)
            .map(|entry| (entry.value.as_str(), entry.spot))
    };
    let fps = fps(attribute("fps")).ok();

    let namespace = element.tag_name().namespace();
    let elements = document
        .descendants()
        .filter(|node| node.is_element() && *node != element)
        .filter(|node| node.tag_name().namespace() == namespace && node.tag_name().name() == "composition")
        .filter(|node| {
            // The notation lets the element stand in the root, in a `<box>` or in a `<rect>` (`schema.rs`).
            node.parent_element()
                .is_some_and(|parent| parent == element || matches!(parent.tag_name().name(), "box" | "rect"))
        })
        .filter_map(|node| {
            let src = node.attribute("src")?;
            Some(Show {
                line: document.text_pos_at(node.range().start).row,
                src: src.to_string(),
            })
        })
        .collect();
    Shows { fps, elements }
}

/// The value of a judgement, or `None` after the problem has been put on the list.
fn keep<T>(judged: Result<T, Found>, problems: &mut Vec<Found>) -> Option<T> {
    match judged {
        Ok(value) => Some(value),
        Err(found) => {
            problems.push(found);
            None
        }
    }
}

/// The `version`: written, and the only one that is read.
fn version(written: Written, root: Spot) -> Result<(), Found> {
    let Some((written, spot)) = written else {
        return Err(found(
            Code::VersionMissing,
            root,
            "<composition> needs the attribute `version`; the versions read are: 1".to_string(),
        ));
    };
    if value::whole_number(written) == Some(VERSION) {
        Ok(())
    } else {
        Err(found(
            Code::VersionNotRead,
            spot,
            format!(
                "`version=\"{}\"` is not a version this program reads; the versions read are: 1",
                shown(written)
            ),
        ))
    }
}

/// A `width` or a `height`: an even whole number from 2 to 8192.
fn size(written: Written, name: &str, root: Spot) -> Result<u32, Found> {
    let Some((written, spot)) = written else {
        return Err(found(
            Code::SizeMissing,
            root,
            format!("<composition> needs the attribute `{name}`, an even whole number from {MIN_SIZE} to {MAX_SIZE}"),
        ));
    };
    match value::whole_number(written) {
        Some(size) if (MIN_SIZE..=MAX_SIZE).contains(&size) && size % 2 == 0 => Ok(size as u32),
        _ => Err(found(
            Code::SizeNotAccepted,
            spot,
            format!(
                "`{name}` must be an even whole number from {MIN_SIZE} to {MAX_SIZE}, not {}",
                shown(written)
            ),
        )),
    }
}

/// The `fps`: a whole number from 1 to 240, and 30 when it is not written.
fn fps(written: Written) -> Result<u32, Found> {
    let Some((written, spot)) = written else {
        return Ok(DEFAULT_FPS);
    };
    match value::whole_number(written) {
        Some(fps) if (i64::from(MIN_FPS)..=i64::from(MAX_FPS)).contains(&fps) => Ok(fps as u32),
        None if support::frame_rate(written).is_some() => Err(found(
            Code::FormNotSupported,
            spot,
            Unsupported::RatioFrameRate.message(&format!("<composition> has `fps=\"{}\"`", shown(written))),
        )),
        _ => Err(found(
            Code::FpsNotAccepted,
            spot,
            format!("`fps` must be a whole number from {MIN_FPS} to {MAX_FPS}, not {}", shown(written)),
        )),
    }
}

/// The `duration`: a time above 0, in seconds.
fn duration(written: Written, root: Spot) -> Result<Time, Found> {
    let Some((written, spot)) = written else {
        return Err(found(
            Code::DurationMissing,
            root,
            "<composition> needs a `duration` above 0".to_string(),
        ));
    };
    match value::time(written) {
        None if support::time(Element::Composition, "duration", written).is_some() => Err(found(
            Code::FormNotSupported,
            spot,
            Unsupported::AutoDuration.message(&format!("<composition> has `duration=\"{}\"`", shown(written))),
        )),
        None => Err(found(
            Code::DurationNotATime,
            spot,
            format!(
                "`duration=\"{}\"` is not a time in seconds: digits with an optional point, such as 5 or 1.25",
                shown(written)
            ),
        )),
        Some(time) if !time.is_positive() => Err(found(
            Code::DurationNotAboveZero,
            spot,
            format!("`duration=\"{}\"` must be above 0", shown(written)),
        )),
        Some(time) => Ok(time),
    }
}

/// The `background`: one colour, and black when it is not written.
fn background(written: Written) -> Result<Colour, Found> {
    let Some((written, spot)) = written else {
        return Ok(Colour::BLACK);
    };
    match value::colour(written) {
        Some([red, green, blue, alpha]) => Ok(Colour { red, green, blue, alpha }),
        None => Err(found(
            Code::BackgroundNotAColour,
            spot,
            format!(
                "`background=\"{}\"` is not a colour written as #rrggbb or #rrggbbaa",
                shown(written)
            ),
        )),
    }
}

/// A written value without its surrounding white space.
pub(crate) fn trimmed(value: &str) -> &str {
    value.trim_matches([' ', '\t', '\n', '\r'])
}

/// A written value as a message shows it: one line, whatever the value holds. A control character
/// is shown as `\u{...}` with its code in hexadecimal, and a value longer than 80 characters as its
/// first 80 followed by `...` (`docs/spec/check-command.md` section 4).
pub fn shown(value: &str) -> String {
    let mut shown = String::new();
    for (index, character) in value.chars().enumerate() {
        if index == SHOWN_CHARACTERS {
            shown.push_str("...");
            break;
        }
        if character.is_control() {
            shown.push_str(&format!("\\u{{{:x}}}", character as u32));
        } else {
            shown.push(character);
        }
    }
    shown
}
