//! The root `<composition>` of a composition file: `docs/spec/notation.md` section 6.1, with the
//! wording of `docs/spec/check-command.md` section 4.

use crate::problem::Problem;
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

/// Reads the root of a composition file from its bytes, and judges it.
///
/// The first problem found is returned, looking in this order: the text is UTF-8, it is XML, its
/// root is `<composition>`, then `version`, `width`, `height`, `fps`, `duration`, `background`. The
/// children of the root are not read.
pub fn read_root(source: &[u8]) -> Result<Root, Problem> {
    let text = match std::str::from_utf8(source) {
        Ok(text) => text,
        Err(error) => {
            let line = 1 + source[..error.valid_up_to()].iter().filter(|&&byte| byte == b'\n').count();
            return Err(Problem::new(
                line as u32,
                "the file is not well-formed XML: it is not valid UTF-8 text".to_string(),
            ));
        }
    };
    let document = roxmltree::Document::parse(text)
        .map_err(|error| Problem::new(error.pos().row, format!("the file is not well-formed XML: {error}")))?;

    let element = document.root_element();
    let line_at = |offset: usize| document.text_pos_at(offset).row;
    let element_line = line_at(element.range().start);
    let name = element.tag_name().name();
    if name != "composition" {
        return Err(Problem::new(
            element_line,
            format!("the root element is <{name}>, not <composition>"),
        ));
    }

    // The written value of an attribute with the line it is on; the attribute has no namespace.
    let attribute = |name: &str| {
        element
            .attributes()
            .find(|attribute| attribute.namespace().is_none() && attribute.name() == name)
            .map(|attribute| (trimmed(attribute.value()), line_at(attribute.range().start)))
    };

    let version = attribute("version");
    let Some((written, line)) = version else {
        return Err(Problem::new(
            element_line,
            "<composition> needs the attribute `version`; the versions read are: 1".to_string(),
        ));
    };
    if value::whole_number(written) != Some(VERSION) {
        return Err(Problem::new(
            line,
            format!(
                "`version=\"{}\"` is not a version this program reads; the versions read are: 1",
                shown(written)
            ),
        ));
    }

    let width = size(attribute("width"), "width", element_line)?;
    let height = size(attribute("height"), "height", element_line)?;

    let fps = match attribute("fps") {
        None => DEFAULT_FPS,
        Some((written, line)) => match value::whole_number(written) {
            Some(fps) if (i64::from(MIN_FPS)..=i64::from(MAX_FPS)).contains(&fps) => fps as u32,
            _ => {
                return Err(Problem::new(
                    line,
                    format!("`fps` must be a whole number from {MIN_FPS} to {MAX_FPS}, not {}", shown(written)),
                ))
            }
        },
    };

    let duration = match attribute("duration") {
        None => return Err(Problem::new(element_line, "<composition> needs a `duration` above 0".to_string())),
        Some((written, line)) => match value::time(written) {
            None => {
                return Err(Problem::new(
                    line,
                    format!(
                        "`duration=\"{}\"` is not a time in seconds: digits with an optional point, such as 5 or 1.25",
                        shown(written)
                    ),
                ))
            }
            Some(time) if !time.is_positive() => {
                return Err(Problem::new(line, format!("`duration=\"{}\"` must be above 0", shown(written))))
            }
            Some(time) => time,
        },
    };

    let background = match attribute("background") {
        None => Colour::BLACK,
        Some((written, line)) => match value::colour(written) {
            Some([red, green, blue, alpha]) => Colour { red, green, blue, alpha },
            None => {
                return Err(Problem::new(
                    line,
                    format!(
                        "`background=\"{}\"` is not a colour written as #rrggbb or #rrggbbaa",
                        shown(written)
                    ),
                ))
            }
        },
    };

    Ok(Root {
        id: attribute("id").map(|(written, _)| written.to_string()),
        width,
        height,
        fps,
        duration,
        frames: frame_count(duration, fps),
        background,
    })
}

/// A `width` or a `height`: an even whole number from 2 to 8192.
fn size(written: Option<(&str, u32)>, name: &str, element_line: u32) -> Result<u32, Problem> {
    let Some((written, line)) = written else {
        return Err(Problem::new(element_line, format!("<composition> needs the attribute `{name}`")));
    };
    match value::whole_number(written) {
        Some(size) if (MIN_SIZE..=MAX_SIZE).contains(&size) && size % 2 == 0 => Ok(size as u32),
        _ => Err(Problem::new(
            line,
            format!(
                "`{name}` must be an even whole number from {MIN_SIZE} to {MAX_SIZE}, not {}",
                shown(written)
            ),
        )),
    }
}

/// A written value without its surrounding white space.
fn trimmed(value: &str) -> &str {
    value.trim_matches([' ', '\t', '\n', '\r'])
}

/// A written value as a message shows it: one line, whatever the value holds.
fn shown(value: &str) -> String {
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
