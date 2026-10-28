//! The paragraphs of a composition as the timeline places them (`docs/spec/text.md`): the text attributes as
//! they are written and as they pass from a box to the elements inside it, the fonts that a composition
//! declares, and the lines of a `<text>` laid out in its place.

use std::collections::HashMap;

use draw::{Colour, Outline, Shadow};
use layout::{place_absolute, Amount, Beyond, Rect, Sides, Written, LIMIT};
use motion::{ElementMotion, Prop, Track, PROPS};
use project::CountFormat;
use project::{Attribute, Code, Element, Length, Node, Part, Project, Unsupported};
use text::{Align, Block, FontError, Fonts, LineHeight, Piece, Style};
use timebase::Time;

use crate::moving;
use crate::support;

/// The text attributes: a `<box>` and a `<text>` take them, and they pass to the elements inside (`text.md`
/// section 8). A value a `<text>` writes is the one its words are drawn with. The last five are those of section 6.6.
pub(crate) const TEXT_ATTRIBUTES: [&str; 13] = [
    "font-family",
    "font-weight",
    "font-style",
    "font-size",
    "line-height",
    "letter-spacing",
    "color",
    "text-align",
    "white-space",
    "text-transform",
    "font-variant-numeric",
    "text-shadow",
    "text-stroke",
];

/// What a `<text>` takes: its id and class, its place and its clip, the text attributes, and `white-space` and
/// `text-transform`, each in the one value this version draws (`text.md` section 6.4). In a row or a column it
/// also takes its `align-self`, `flex-grow` and `max-width` (`render-command.md` section 4.3, rule 14).
pub(crate) const TEXT_TAKES: &[&str] = &[
    "id",
    "class",
    "left",
    "top",
    "right",
    "bottom",
    "width",
    "height",
    "position",
    "margin",
    "start",
    "duration",
    "font-family",
    "font-weight",
    "font-style",
    "font-size",
    "line-height",
    "letter-spacing",
    "color",
    "text-align",
    "white-space",
    "text-transform",
    "font-variant-numeric",
    "text-shadow",
    "text-stroke",
    "align-self",
    "flex-grow",
    "max-width",
    "count",
    "count-format",
    "typed",
];

/// What a `<span>` takes (`docs/spec/text.md`, section 6.5): its id and class, its margin and padding, and the text
/// attributes that section 6.4 draws. Any other attribute is a fault on its line, and the span is not drawn.
pub(crate) const SPAN_TAKES: &[&str] = &[
    "id",
    "class",
    "margin",
    "padding",
    "font-family",
    "font-weight",
    "font-style",
    "font-size",
    "line-height",
    "letter-spacing",
    "color",
    "white-space",
    "text-transform",
    "typed",
];

/// A fault of a text or a font, as the timeline reports it: its code, the line it is on, its message, and whether
/// a `<style>` gives the attribute it is about (`docs/spec/check-command.md`, section 3c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fault {
    pub(crate) code: Code,
    pub(crate) line: u32,
    pub(crate) message: String,
    pub(crate) style: bool,
}

/// A letter spacing as written: a number of pixels, or a number of ems of the font size of the element that writes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Spacing {
    Pixels(f32),
    Ems(f32),
}

/// The text attributes one element writes, each as it is read; `None` for one it does not write.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TextAttrs {
    families: Option<Vec<String>>,
    weight: Option<f32>,
    italic: Option<bool>,
    size: Option<f32>,
    line_height: Option<LineHeight>,
    spacing: Option<Spacing>,
    colour: Option<Colour>,
    align: Option<Align>,
    uppercase: Option<bool>,
    nowrap: Option<bool>,
    figures: Option<Vec<[u8; 4]>>,
    shadows: Option<Vec<ShadowAs>>,
    outline: Option<Outline>,
}

/// A shadow as a `<text>` writes it: its offsets and its blur in pixels, and its colour, where it writes one; a shadow with
/// no colour takes the colour of its text (`docs/spec/text.md`, section 6.6, rule 5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShadowAs {
    across: f32,
    down: f32,
    blur: f32,
    colour: Option<Colour>,
}

/// What a `<text>` draws around its glyphs (`docs/spec/text.md`, section 6.6): its shadows, the first listed first, and its
/// outline, where the outline has a width above 0.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Dressing {
    pub(crate) shadows: Vec<Shadow>,
    pub(crate) outline: Option<Outline>,
}

/// A paragraph placed in its place: its rectangle in the frame, its lines and its shadows and outline. The colour of each
/// run is in the block.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Paragraph {
    pub(crate) rect: Rect,
    pub(crate) block: Block,
    pub(crate) dressing: Dressing,
    pub(crate) content: Content,
}

/// The time a text's content is put at: the frame rate, the number of frames of the composition, the time the tweens of
/// the text count their `at` from (the start of its parent's time), and the time the tweens of its spans count theirs
/// from (the text's own start, Q4) (`docs/spec/text.md`, section 7).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Clock {
    pub(crate) fps: u32,
    pub(crate) frames: u64,
    pub(crate) origin: Time,
    pub(crate) inner: Time,
}

/// What a text shows from frame to frame (`docs/spec/text.md`, section 7).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Content {
    /// The same words in every frame.
    Still,
    /// A number that counts (section 7.1): the block of each different number, the number shown in each frame of the
    /// composition as an index of `blocks`, and where it stands in the text's width.
    Count {
        blocks: Vec<Block>,
        shown: Vec<u32>,
        align: Align,
        width: f32,
    },
    /// Typed text (section 7.2): the track of `typed` of each element that types, or its own value when it has no tween,
    /// and for each piece of the block, the element that types it and the characters of that element before the piece.
    Typed {
        typing: Vec<Typing>,
        pieces: Vec<Option<(usize, usize)>>,
    },
}

/// The value of `typed` of one element: its own value, and its track when a tween or a set changes it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Typing {
    own: f64,
    track: Option<Track>,
}

impl Paragraph {
    /// The block drawn at frame `number`, at `time`, and how far it is moved to the right in the text's box: the block of the
    /// number shown, standing in the box by `text-align` (section 7.1, point 3); the block typed so far (section 7.2); or the
    /// block itself.
    pub(crate) fn block_at(&self, number: u64, time: Time) -> (std::borrow::Cow<'_, Block>, f32) {
        match &self.content {
            Content::Still => (std::borrow::Cow::Borrowed(&self.block), 0.0),
            Content::Count {
                blocks,
                shown,
                align,
                width,
            } => {
                let index = shown.get(number as usize).or(shown.last()).copied().unwrap_or(0) as usize;
                let Some(block) = blocks.get(index) else {
                    return (std::borrow::Cow::Borrowed(&self.block), 0.0);
                };
                let room = width - block.width;
                let offset = match align {
                    Align::Left => 0.0,
                    Align::Center => room / 2.0,
                    Align::Right => room,
                };
                (std::borrow::Cow::Borrowed(block), offset)
            }
            Content::Typed { typing, pieces } => {
                let drawn: Vec<usize> = typing
                    .iter()
                    .map(|typing| {
                        let value = typing.track.as_ref().map_or(typing.own, |track| track.value(time));
                        whole_characters(value)
                    })
                    .collect();
                let shown: Vec<usize> = pieces
                    .iter()
                    .map(|piece| match piece {
                        Some((element, before)) => drawn[*element].saturating_sub(*before),
                        None => usize::MAX,
                    })
                    .collect();

                (std::borrow::Cow::Owned(self.block.typed(&shown)), 0.0)
            }
        }
    }
}

/// The number of characters a value of `typed` draws: rounded down, 0 at least, and every character for a value too large
/// to count (section 7.2, point 2).
fn whole_characters(value: f64) -> usize {
    if value.is_nan() || value <= 0.0 {
        0
    } else if value >= usize::MAX as f64 {
        usize::MAX
    } else {
        value.floor() as usize
    }
}

/// Reads the text attributes that an element writes. A value that is not in the form the notation gives is a fault
/// on the attribute's line; the attributes that are right are read all the same.
pub(crate) fn read_text_attributes(node: &Node, faults: &mut Vec<Fault>) -> TextAttrs {
    let mut attrs = TextAttrs::default();
    for attribute in node.attributes() {
        if !TEXT_ATTRIBUTES.contains(&attribute.name()) {
            continue;
        }
        if let Err((code, message)) = read_text_attribute(attribute, &mut attrs) {
            faults.push(Fault {
                code,
                line: attribute.line(),
                message,
                style: attribute.given_by().is_some(),
            });
        }
    }
    attrs
}

/// Reads one text attribute into `attrs`, or gives the code and the message of the fault it has.
fn read_text_attribute(attribute: &Attribute, attrs: &mut TextAttrs) -> Result<(), (Code, String)> {
    let name = attribute.name();
    let value = attribute.value();
    match name {
        "font-family" => {
            let mut families = Vec::new();
            for entry in value.split(',') {
                let family = unquote(entry.trim());
                if family.is_empty() {
                    return Err((Code::ValueNotInForm, format!("`font-family=\"{value}\"` has an empty family")));
                }
                families.push(family.to_string());
            }
            attrs.families = Some(families);
        }
        "font-weight" => match number(value) {
            Some(weight) => attrs.weight = Some(weight as f32),
            None => return Err((Code::NotANumber, not_a_number(name, value))),
        },
        "font-style" => {
            attrs.italic = Some(match value {
                "normal" => false,
                "italic" => true,
                _ => {
                    return Err((
                        Code::WordNotTaken,
                        format!("`font-style=\"{value}\"` is not one of: normal, italic"),
                    ))
                }
            })
        }
        "font-size" => {
            let size = match attribute.length() {
                Some(Length::Pixels(pixels)) => pixels.to_f64(),
                Some(Length::Percent(_)) => {
                    return Err((Code::ValueNotInForm, format!("`font-size=\"{value}\"` is not a number of pixels")))
                }
                None => return Err((Code::NotANumber, not_a_number(name, value))),
            };
            if size <= 0.0 {
                return Err((Code::SizeNotAboveZero, format!("`font-size=\"{value}\"` must be above 0")));
            }
            attrs.size = Some(size as f32);
        }
        "line-height" => attrs.line_height = Some(line_height(value)?),
        "letter-spacing" => attrs.spacing = Some(spacing(value)?),
        "text-transform" => {
            attrs.uppercase = Some(match value {
                "none" => false,
                "uppercase" => true,
                _ => {
                    return Err((
                        Code::WordNotTaken,
                        format!("`text-transform=\"{value}\"` is not one of: none, uppercase"),
                    ))
                }
            })
        }
        "white-space" => {
            attrs.nowrap = Some(match value {
                "normal" => false,
                "nowrap" => true,
                _ => {
                    return Err((
                        Code::WordNotTaken,
                        format!("`white-space=\"{value}\"` is not one of: normal, nowrap"),
                    ))
                }
            })
        }
        "font-variant-numeric" => attrs.figures = Some(figures(value)?),
        "text-shadow" => attrs.shadows = Some(shadows(value)?),
        "text-stroke" => attrs.outline = Some(outline(value)?),
        "color" => match attribute.colour() {
            Some(found) if found.alpha == 255 => {
                attrs.colour = Some(Colour {
                    red: found.red,
                    green: found.green,
                    blue: found.blue,
                })
            }
            Some(_) => {
                return Err((
                    Code::FormNotSupported,
                    Unsupported::AlphaOutsideBoxBackground.message(&format!("`color=\"{value}\"` has an alpha below ff")),
                ))
            }
            None => {
                return Err((
                    Code::ValueNotInForm,
                    format!("`color=\"{value}\"` is not a colour written as #rrggbb or #rrggbbaa"),
                ))
            }
        },
        "text-align" => {
            attrs.align = Some(match value {
                "left" => Align::Left,
                "center" => Align::Center,
                "right" => Align::Right,
                _ => {
                    return Err((
                        Code::WordNotTaken,
                        format!("`text-align=\"{value}\"` is not one of: left, center, right"),
                    ))
                }
            })
        }
        _ => {}
    }
    Ok(())
}

/// `font-variant-numeric`: `normal`, or a list of the words of the figures, each asking the font for one feature: where a list
/// holds both words of a pair, the one written last is asked for (`docs/spec/text.md`, section 6.6, rule 4).
fn figures(value: &str) -> Result<Vec<[u8; 4]>, (Code, String)> {
    let not_taken = || {
        (
            Code::WordNotTaken,
            format!("`font-variant-numeric=\"{value}\"` is not normal or a list of: lining-nums, oldstyle-nums, tabular-nums, proportional-nums"),
        )
    };
    if value == "normal" {
        return Ok(Vec::new());
    }
    // Each pair of words asks for one kind of the feature: lining or old-style figures, or tabular or proportional ones.
    let mut found: Vec<([u8; 4], u8)> = Vec::new();
    for word in value.split_whitespace() {
        let (tag, pair) = match word {
            "lining-nums" => (*b"lnum", 0),
            "oldstyle-nums" => (*b"onum", 0),
            "tabular-nums" => (*b"tnum", 1),
            "proportional-nums" => (*b"pnum", 1),
            _ => return Err(not_taken()),
        };
        found.retain(|(_, other)| *other != pair);
        found.push((tag, pair));
    }
    if found.is_empty() {
        return Err(not_taken());
    }
    Ok(found.into_iter().map(|(tag, _)| tag).collect())
}

/// `text-shadow`: `none`, or shadows separated by commas, each of two lengths with an optional blur and an optional colour, in
/// either order (`docs/spec/text.md`, section 6.6, rule 5).
fn shadows(value: &str) -> Result<Vec<ShadowAs>, (Code, String)> {
    if value == "none" {
        return Ok(Vec::new());
    }
    value.split(',').map(|entry| shadow(value, entry.trim())).collect()
}

/// One shadow of a `text-shadow` list (section 6.6, rule 5), read from its words.
fn shadow(value: &str, entry: &str) -> Result<ShadowAs, (Code, String)> {
    let not_a_list = || {
        (
            Code::ValueNotInForm,
            format!("`text-shadow=\"{value}\"` is not a list of shadows: two lengths, an optional blur and an optional colour, such as 2px 2px 4px #000000"),
        )
    };
    let mut lengths: Vec<f32> = Vec::new();
    let mut colour: Option<[u8; 4]> = None;
    for token in entry.split_whitespace() {
        if token.starts_with('#') {
            if colour.is_some() {
                return Err(not_a_list());
            }
            colour = Some(colour_token(token).ok_or_else(not_a_list)?);
        } else {
            lengths.push(length_token(token).ok_or_else(not_a_list)?);
        }
    }
    if !(2..=3).contains(&lengths.len()) {
        return Err(not_a_list());
    }
    let blur = lengths.get(2).copied().unwrap_or(0.0);
    if blur < 0.0 {
        return Err((Code::LengthNotAccepted, format!("`text-shadow=\"{value}\"` must be 0 or more")));
    }
    let colour = match colour {
        None => None,
        Some([red, green, blue, 255]) => Some(Colour { red, green, blue }),
        Some(_) => {
            return Err((
                Code::FormNotSupported,
                Unsupported::AlphaOutsideBoxBackground.message(&format!("`text-shadow=\"{value}\"` has an alpha below ff")),
            ))
        }
    };
    Ok(ShadowAs {
        across: lengths[0],
        down: lengths[1],
        blur,
        colour,
    })
}

/// `text-stroke`: a width and a colour, in that order (`docs/spec/text.md`, section 6.6, rule 6). A width of 0 draws no outline.
fn outline(value: &str) -> Result<Outline, (Code, String)> {
    let not_taken = || {
        (
            Code::ValueNotInForm,
            format!("`text-stroke=\"{value}\"` is not a width and a colour, such as 2px #000000"),
        )
    };
    let tokens: Vec<&str> = value.split_whitespace().collect();
    let (width, colour) = match tokens.as_slice() {
        [width, colour] => (
            length_token(width).ok_or_else(not_taken)?,
            colour_token(colour).ok_or_else(not_taken)?,
        ),
        _ => return Err(not_taken()),
    };
    if width < 0.0 {
        return Err((Code::LengthNotAccepted, format!("`text-stroke=\"{value}\"` must be 0 or more")));
    }
    let [red, green, blue, alpha] = colour;
    if alpha != 255 {
        return Err((
            Code::FormNotSupported,
            Unsupported::AlphaOutsideBoxBackground.message(&format!("`text-stroke=\"{value}\"` has an alpha below ff")),
        ));
    }
    Ok(Outline {
        width,
        colour: Colour { red, green, blue },
    })
}

/// A length of a shadow or an outline: a number of pixels with an optional `px` after it (notation N9).
pub(crate) fn length_token(token: &str) -> Option<f32> {
    let text = token.strip_suffix("px").unwrap_or(token);
    number(text).map(|pixels| pixels as f32)
}

/// A colour written `#rrggbb` or `#rrggbbaa`, as red, green, blue and alpha (section 6.3).
pub(crate) fn colour_token(token: &str) -> Option<[u8; 4]> {
    let hex = token.strip_prefix('#')?;
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut parts = [0, 0, 0, 255];
    for (part, pair) in parts.iter_mut().zip(hex.as_bytes().chunks(2)) {
        *part = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(parts)
}

/// `line-height`: `normal`, a number of times the font size, or a number of pixels above 0 (`text.md` section 4).
fn line_height(value: &str) -> Result<LineHeight, (Code, String)> {
    if value == "normal" {
        return Ok(LineHeight::Normal);
    }
    let not_taken = || {
        (
            Code::ValueNotInForm,
            format!("`line-height=\"{value}\"` is not `normal`, a number or pixels"),
        )
    };
    let (text, pixels) = match value.strip_suffix("px") {
        Some(text) => (text, true),
        None => (value, false),
    };
    let amount = number(text).ok_or_else(not_taken)?;
    if amount <= 0.0 {
        return Err((Code::SizeNotAboveZero, format!("`line-height=\"{value}\"` must be above 0")));
    }
    Ok(if pixels {
        LineHeight::Pixels(amount as f32)
    } else {
        LineHeight::Multiple(amount as f32)
    })
}

/// `letter-spacing`: a number of pixels, or a number of ems; a negative value is allowed (`text.md` section 6.1).
fn spacing(value: &str) -> Result<Spacing, (Code, String)> {
    let not_taken = || (Code::ValueNotInForm, format!("`letter-spacing=\"{value}\"` is not a number or ems"));
    match value.strip_suffix("em") {
        Some(text) => Ok(Spacing::Ems(number(text).ok_or_else(not_taken)? as f32)),
        None => Ok(Spacing::Pixels(number(value).ok_or_else(not_taken)? as f32)),
    }
}

/// The text attributes of the element at `index` and of each element above it, nearest first.
fn chain_of(tree: &[Node], index: usize) -> Vec<TextAttrs> {
    let mut chain: Vec<TextAttrs> = Vec::new();
    let mut at = Some(index);
    while let Some(current) = at {
        chain.push(read_text_attributes(&tree[current], &mut Vec::new()));
        at = tree[current].parent();
    }
    chain
}

/// The shadows and the outline of the `<text>` at `index` (`docs/spec/text.md`, section 6.6): each takes the nearest element
/// above the text that writes it, and a shadow with no colour of its own takes the colour of the text.
pub(crate) fn dressing_of(tree: &[Node], index: usize) -> Dressing {
    let chain = chain_of(tree, index);
    let colour = chain
        .iter()
        .find_map(|attrs| attrs.colour)
        .unwrap_or(Colour { red: 0, green: 0, blue: 0 });
    let shadows = chain
        .iter()
        .find_map(|attrs| attrs.shadows.clone())
        .unwrap_or_default()
        .into_iter()
        .map(|shadow| Shadow {
            across: shadow.across,
            down: shadow.down,
            blur: shadow.blur,
            colour: shadow.colour.unwrap_or(colour),
        })
        .collect();
    // A width of 0 draws no outline (section 6.6, rule 6).
    let outline = chain.iter().find_map(|attrs| attrs.outline).filter(|outline| outline.width > 0.0);
    Dressing { shadows, outline }
}

/// The style of the text at `index` (`text.md` section 8): each text attribute that the element, or the nearest
/// element above it, writes, and the defaults of the table where none does. A letter spacing in ems is taken in
/// pixels of the font size of the element that writes it. Gives the style and the colour, which is black where
/// none is written.
pub(crate) fn style_of(tree: &[Node], index: usize) -> (Style, Colour) {
    let chain = chain_of(tree, index);
    let families = chain
        .iter()
        .find_map(|attrs| attrs.families.clone())
        .unwrap_or_else(|| vec!["sans-serif".to_string()]);
    let size = chain.iter().find_map(|attrs| attrs.size).unwrap_or(16.0);
    let weight = chain.iter().find_map(|attrs| attrs.weight).unwrap_or(400.0);
    let italic = chain.iter().find_map(|attrs| attrs.italic).unwrap_or(false);
    let line_height = chain.iter().find_map(|attrs| attrs.line_height).unwrap_or(LineHeight::Normal);
    let align = chain.iter().find_map(|attrs| attrs.align).unwrap_or(Align::Left);
    let colour = chain
        .iter()
        .find_map(|attrs| attrs.colour)
        .unwrap_or(Colour { red: 0, green: 0, blue: 0 });
    let letter_spacing = match chain
        .iter()
        .enumerate()
        .find_map(|(at, attrs)| attrs.spacing.map(|spacing| (at, spacing)))
    {
        None => 0.0,
        Some((_, Spacing::Pixels(pixels))) => pixels,
        Some((writer, Spacing::Ems(ems))) => {
            // The writer's own font size, or the nearest one above it.
            let writer_size = chain[writer..].iter().find_map(|attrs| attrs.size).unwrap_or(16.0);
            ems * writer_size
        }
    };
    let uppercase = chain.iter().find_map(|attrs| attrs.uppercase).unwrap_or(false);
    let nowrap = chain.iter().find_map(|attrs| attrs.nowrap).unwrap_or(false);
    let figures = chain.iter().find_map(|attrs| attrs.figures.clone()).unwrap_or_default();
    let style = Style {
        families,
        size,
        weight,
        italic,
        line_height,
        letter_spacing,
        align,
        uppercase,
        nowrap,
        figures,
    };
    (style, colour)
}

/// The fonts a composition declares with `<font>` (`text.md` section 2.1), read from the composition's own tree.
/// Each file is read and registered once for each pair of `src` and `family`. Gives the faults of the files that
/// are missing, unreadable, or not a font this version draws.
pub(crate) fn load_fonts(project: &Project, tree: &[Node], codecs: &codec::Registry) -> (Fonts, Vec<Fault>) {
    let mut fonts = Fonts::new(codecs);
    let mut faults = Vec::new();
    let mut loaded: Vec<(String, Option<String>)> = Vec::new();
    for node in tree
        .iter()
        .filter(|node| node.element() == Element::Font && node.parent() == Some(0))
    {
        let line = node.line();
        let Some(src) = node.attribute("src") else {
            faults.push(Fault {
                code: Code::FontMissing,
                line,
                message:
                    "`<font>` needs the attribute `src`, the path of a font file: from the project folder, or a path that begins with `/`"
                        .to_string(),
                style: false,
            });
            continue;
        };
        let src = src.value();
        let family = node.attribute("family").map(|attribute| attribute.value().to_string());
        let key = (src.to_string(), family.clone());
        if loaded.contains(&key) {
            continue;
        }
        loaded.push(key);
        // Nothing is read from a network (support table S31).
        if let Some(entry) = project::remote(src) {
            faults.push(Fault {
                code: Code::FormNotSupported,
                line,
                message: entry.message(&format!("<font> has `src=\"{src}\"`")),
                style: false,
            });
            continue;
        }
        let path = project.resolve(src);
        if !path.is_file() {
            faults.push(Fault {
                code: Code::FontMissing,
                line,
                message: format!("`src=\"{src}\"` names no file: a font file, from the project folder or a path that begins with `/`"),
                style: false,
            });
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                faults.push(Fault {
                    code: Code::FontUnreadable,
                    line,
                    message: format!("`src=\"{src}\"` could not be read: {}", reason(&error)),
                    style: false,
                });
                continue;
            }
        };
        if let Err(error) = fonts.register(bytes, family.as_deref()) {
            let (code, message) = match error {
                FontError::Unsupported(kind) => (
                    Code::FormNotSupported,
                    Unsupported::OtherFontKinds.message(&format!("`src=\"{src}\"` is {kind}")),
                ),
                FontError::Collection(reason) => (
                    Code::FormNotSupported,
                    Unsupported::OtherFontKinds.message(&format!("`src=\"{src}\"` is a font collection that cannot be read: {reason}")),
                ),
                FontError::Damaged(reason) => (
                    Code::FontDamaged,
                    format!("`src=\"{src}\"` is damaged: the font reader refuses it: {reason}"),
                ),
                FontError::NotAFont => (
                    Code::FontNotAFont,
                    format!(
                        "`src=\"{src}\"` is not a font of a kind this version takes. The kinds taken are TrueType (`.ttf`) and font collection (`.ttc`)"
                    ),
                ),
            };
            faults.push(Fault {
                code,
                line,
                message,
                style: false,
            });
        }
    }
    (fonts, faults)
}

/// Lays out the `<text>` at `index` in its place (`text.md` sections 4, 5 and 8). Its width is the one written, or the
/// one that `left` and `right` leave in its container, and its lines wrap inside it. Without either it is as wide as
/// its widest line, or, with a `max-width`, its lines are broken so that none is wider than that (section 5.3). Its
/// height is the one written, or the height of its lines. An absolute text is moved in by its `margin` from each edge it
/// is placed by (`render-command.md` section 4.1, rule 8). Gives the rectangle and the lines, or the fault that stops it.
pub(crate) fn place_text(
    fonts: &mut Fonts,
    tree: &[Node],
    index: usize,
    written: Written,
    margin: Sides,
    parent: Rect,
    clock: Clock,
) -> Result<Paragraph, Fault> {
    let node = &tree[index];
    let beyond = |Beyond| Fault {
        code: Code::FormNotSupported,
        line: node.line(),
        message: Unsupported::BeyondTheLimit.message(&format!(
            "a place or a size of <text> works out beyond {LIMIT} pixels, or needs more digits than this version works with"
        )),
        style: false,
    };
    // A width is known with a written `width`, or with both `left` and `right`, which give the width of the
    // container less the two (`layout::place`). Without either, the text is as wide as its widest line.
    let known = written.width.is_some() || (written.left.is_some() && written.right.is_some());
    let wrap = if known {
        // The width the place gives, with the height left out: it depends on the width alone.
        let across = place_absolute(
            parent,
            &Written {
                height: Some(Amount::Pixels(0.0.into())),
                ..written.clone()
            },
            margin,
        )
        .map_err(beyond)?;
        Some(across.width as f32)
    } else {
        None
    };
    let (block, content) = lay_block(fonts, tree, index, wrap, written.max_width, clock)?;
    let mut sized = written;
    if sized.width.is_none() {
        sized.width = Some(Amount::Pixels(f64::from(block.width).ceil().into()));
    }
    if sized.height.is_none() {
        sized.height = Some(Amount::Pixels(f64::from(block.height).ceil().into()));
    }
    let rect = place_absolute(parent, &sized, margin).map_err(beyond)?;
    Ok(Paragraph {
        rect,
        block,
        dressing: dressing_of(tree, index),
        content,
    })
}

/// A `<text>` laid out as an item of a row, a column or a set (`render-command.md` section 4.3, rule 14; `text.md`
/// section 8): its block, its colour, the size its text takes, and its first baseline from its top edge, or `None` for a
/// text with no line. The first baseline lies on a whole pixel, so that the baselines of the texts in one row meet on one
/// line; the glyphs and the lines of the block are moved by the same amount.
pub(crate) struct ItemText {
    pub(crate) block: Block,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) baseline: Option<f64>,
    pub(crate) dressing: Dressing,
    pub(crate) content: Content,
}

/// Lays out the `<text>` at `index` as an item of a row, a column or a set. Its width is the one written, or that of its
/// widest line, which a `max-width` breaks when it writes none (section 5.3); its height is the one written, or that of
/// its lines. A percentage width or height is not drawn in a row or a column (`text.md` section 8). An absolute text with
/// both edges and no width is not drawn either, since its width would need the container's. Gives the layout, or the
/// fault that stops it.
pub(crate) fn lay_out_item(
    fonts: &mut Fonts,
    tree: &[Node],
    index: usize,
    written: Written,
    absolute: bool,
    clock: Clock,
) -> Result<ItemText, Fault> {
    let node = &tree[index];
    if absolute && written.width.is_none() && written.left.is_some() && written.right.is_some() {
        return Err(Fault {
            code: Code::FormNotSupported,
            line: node.line(),
            message: Unsupported::TextBetweenEdgesInFlow
                .message("a <text> with `left` and `right` and no `width` is inside a row, a column or a set of columns"),
            style: false,
        });
    }
    let width = flow_pixels(written.width, "width", node)?;
    let height = flow_pixels(written.height, "height", node)?;
    let (mut block, mut content) = lay_block(fonts, tree, index, width.map(|pixels| pixels as f32), written.max_width, clock)?;
    let first = block.lines.first().map(|line| f64::from(line.baseline));
    let baseline = first.map(|first| {
        let whole = (first + 0.5).floor();
        let shift = (whole - first) as f32;
        lower(&mut block, shift);
        if let Content::Count { blocks, .. } = &mut content {
            for block in blocks.iter_mut() {
                lower(block, shift);
            }
        }
        whole
    });
    let width = width.unwrap_or_else(|| f64::from(block.width).ceil());
    let height = height.unwrap_or_else(|| f64::from(block.height).ceil());
    Ok(ItemText {
        block,
        width,
        height,
        baseline,
        dressing: dressing_of(tree, index),
        content,
    })
}

/// Moves the glyphs, the pictures and the lines of a block down by `shift`.
fn lower(block: &mut Block, shift: f32) {
    for run in block.runs.iter_mut() {
        for glyph in run.glyphs.iter_mut() {
            glyph.y += shift;
        }
    }
    for picture in block.pictures.iter_mut() {
        picture.y += shift;
    }
    for line in block.lines.iter_mut() {
        line.baseline += shift;
    }
}

/// A `width` or `height` of a `<text>` in a row or a column as pixels: `None` where it is not written. A percentage is not
/// drawn there (`text.md` section 8), and is reported on the element's line.
fn flow_pixels(amount: Option<Amount>, name: &str, node: &Node) -> Result<Option<f64>, Fault> {
    match amount {
        None => Ok(None),
        Some(Amount::Pixels(pixels)) => Ok(Some(pixels.to_f64())),
        Some(Amount::Percent(percent)) => Err(Fault {
            code: Code::FormNotSupported,
            line: node.line(),
            message: Unsupported::TextPercentInFlow
                .message(&format!("<text> inside a row or a column has `{name}=\"{}%\"`", percent.to_f64())),
            style: false,
        }),
    }
}

/// Lays out the paragraph of the `<text>` at `index` (`docs/spec/text.md`, section 6.5), its pieces in the style of each
/// element: wrapped at `wrap` where a width is given, else broken so that no line is wider than `max` where a `max-width`
/// is written (section 5.3), else one line. Gives the block, or the fault of a character that no family has.
fn lay_block(
    fonts: &mut Fonts,
    tree: &[Node],
    index: usize,
    wrap: Option<f32>,
    max: Option<f64>,
    clock: Clock,
) -> Result<(Block, Content), Fault> {
    let node = &tree[index];
    if let Some(value) = node.attribute("count").and_then(|attribute| attribute.number()) {
        return count_blocks(fonts, tree, index, value, wrap, clock);
    }
    let (style, _) = style_of(tree, index);
    let (pieces, sources, owners) = pieces_of(tree, index);
    let laid = match (wrap, max) {
        (Some(width), _) => text::layout_pieces(fonts, &pieces, style.align, Some(width)),
        (None, Some(max)) => text::layout_fitted_pieces(fonts, &pieces, style.align, max as f32),
        (None, None) => text::layout_pieces(fonts, &pieces, style.align, None),
    };
    let block = laid.map_err(|error| text_fault(node, &sources, error))?;
    let content = typing_of(tree, &owners, &block, clock);
    Ok((block, content))
}

/// The fault of a paragraph that cannot be laid out, on the line it is about.
fn text_fault(node: &Node, sources: &[Option<RoomSource>], error: text::TextError) -> Fault {
    match error {
        // A room where a line may end before its span is reported on the line of the attribute that writes the room.
        text::TextError::RoomAtALineBreak { piece } => {
            let source = sources.get(piece).copied().flatten().unwrap_or(RoomSource { line: node.line() });
            Fault {
                code: Code::FormNotSupported,
                line: source.line,
                message: Unsupported::SpanRoomWithoutPlace.message(
                    "<text> holds a margin or a padding of a <span> where the line may break before the span, after a character other than a space or a hyphen",
                ),
                style: false,
            }
        }
        // An emoji with no picture this version draws is refused on the line of its `<text>` (`docs/spec/text.md`,
        // section 12.4, point 9).
        text::TextError::NoPicture(_) | text::TextError::PictureRefused { .. } => Fault {
            code: Code::EmojiNotDrawn,
            line: node.line(),
            message: Unsupported::EmojiPicture.message(&format!("<text> holds a character whose picture is not drawn: {error}")),
            style: false,
        },
        text::TextError::RoomWithoutWords => Fault {
            code: Code::FormNotSupported,
            line: node.line(),
            message: Unsupported::SpanRoomWithoutPlace
                .message("<text> holds a margin or a padding of a <span> in a <text> with no word"),
            style: false,
        },
        text::TextError::NoFamily | text::TextError::MissingCharacter(_) => Fault {
            code: Code::FormNotSupported,
            line: node.line(),
            // Not reached on macOS, where the machine's fonts are read; it has no entry of the support table (S71 is left free).
            message: format!("<text> is written so that {error}, which this version does not support on this system"),
            style: false,
        },
    }
}

/// The tweens and sets written inside the element at `index`, in the order written, with their `at` counted from `origin`.
fn tweens_in(tree: &[Node], index: usize, origin: Time) -> Vec<motion::Tween> {
    tree.iter()
        .filter(|node| node.parent() == Some(index))
        .filter_map(|node| match node.element() {
            Element::Tween => moving::read_tween(node, origin).0,
            Element::Set => moving::read_set(node, origin).0,
            _ => None,
        })
        .collect()
}

/// The track of one property of the element at `index` from the tweens inside it, with its own value `own`; `None` when no
/// tween changes it.
fn track_of(tree: &[Node], index: usize, prop: Prop, own: f64, clock: Clock) -> Option<Track> {
    let origin = if tree[index].element() == Element::Span {
        clock.inner
    } else {
        clock.origin
    };
    let tweens = tweens_in(tree, index, origin);
    let mut owns = PROPS.map(Prop::own_value);
    owns[prop.index()] = own;
    let [.., count, typed] = ElementMotion::build(index, &tweens, owns, clock.fps).tracks;
    match prop {
        Prop::Count => count,
        Prop::Typed => typed,
        _ => None,
    }
}

/// The blocks of a number that counts (`docs/spec/text.md`, section 7.1): the number of each frame of the composition
/// written in its form and laid out on one line in the text's style; the block of the widest number gives the size.
fn count_blocks(
    fonts: &mut Fonts,
    tree: &[Node],
    index: usize,
    own: f64,
    wrap: Option<f32>,
    clock: Clock,
) -> Result<(Block, Content), Fault> {
    let node = &tree[index];
    let format = node
        .attribute("count-format")
        .and_then(|attribute| CountFormat::read(attribute.value()))
        .unwrap_or(CountFormat::Number {
            decimals: 0,
            thousands: false,
        });
    let (style, colour) = style_of(tree, index);
    let align = style.align;
    let style = Style { nowrap: true, ..style };
    let track = track_of(tree, index, Prop::Count, own, clock);
    let mut words: Vec<String> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut shown: Vec<u32> = Vec::with_capacity(clock.frames as usize);
    for number in 0..clock.frames.max(1) {
        let value = track
            .as_ref()
            .map_or(own, |track| track.value(Time::of_frame(number as i64, clock.fps)));
        let written = format.write(value);
        let at = match index.get(&written) {
            Some(&at) => at,
            None => {
                index.insert(written.clone(), words.len());
                words.push(written);
                words.len() - 1
            }
        };
        shown.push(at as u32);
    }
    let mut blocks = Vec::with_capacity(words.len());
    for written in words {
        let piece = Piece::Words {
            words: written,
            style: style.clone(),
            colour: rgba(colour),
        };
        let block = text::layout_pieces(fonts, &[piece], Align::Left, None).map_err(|error| text_fault(node, &[], error))?;
        blocks.push(block);
    }
    let widest = blocks.iter().enumerate().fold(
        0,
        |widest, (at, block)| if block.width > blocks[widest].width { at } else { widest },
    );
    let sizing = blocks[widest].clone();
    // The numbers stand in the text's width (section 6.2): its own, or the widest number's, before it is rounded up to
    // the pixel.
    let width = wrap.unwrap_or(sizing.width);
    Ok((
        sizing,
        Content::Count {
            blocks,
            shown,
            align,
            width,
        },
    ))
}

/// How a text is typed (`docs/spec/text.md`, section 7.2): `Still` when neither the text nor a span inside it holds a
/// tween or a set of `typed`. `owners` gives the element that holds each piece of `block`.
fn typing_of(tree: &[Node], owners: &[Option<usize>], block: &Block, clock: Clock) -> Content {
    let mut elements: Vec<usize> = Vec::new();
    let mut typing: Vec<Typing> = Vec::new();
    let mut pieces: Vec<Option<(usize, usize)>> = Vec::with_capacity(owners.len());
    let mut before: Vec<usize> = Vec::new();
    let mut types = false;
    for (piece, owner) in owners.iter().enumerate() {
        let Some(element) = *owner else {
            pieces.push(None);
            continue;
        };
        let at = match elements.iter().position(|known| *known == element) {
            Some(at) => at,
            None => {
                // The attribute is the starting value of the element's tweens and sets of `typed`, and the number drawn
                // when it stands alone; with neither, every character is drawn (section 7.2, point 2; B58).
                let written = tree[element].attribute("typed").and_then(|attribute| attribute.number());
                let track = track_of(tree, element, Prop::Typed, written.unwrap_or(f64::INFINITY), clock);
                types |= track.is_some() || written.is_some();
                elements.push(element);
                typing.push(Typing {
                    own: written.unwrap_or(f64::INFINITY),
                    track,
                });
                before.push(0);
                elements.len() - 1
            }
        };
        pieces.push(Some((at, before[at])));
        before[at] += block.characters.get(piece).copied().unwrap_or(0);
    }
    if types {
        Content::Typed { typing, pieces }
    } else {
        Content::Still
    }
}

/// The pieces of the `<text>` at `index` (`docs/spec/text.md`, section 6.5), in document order: its words and those of its
/// `<span>`s, each in the style and the colour of the element that holds it (section 8), and the room of a span's margin and
/// padding at each of its edges. A text with no span is one piece of words, as before. Gives the pieces, and beside each the
/// attribute that gives its room (`None` for words), so that a refusal of a room is reported on the line of its attribute.
pub(crate) fn pieces_of(tree: &[Node], index: usize) -> (Vec<Piece>, Vec<Option<RoomSource>>, Vec<Option<usize>>) {
    // The work still to do, last first: the parts of an element from a position on, or the room after a span.
    enum Work {
        Parts(usize, usize),
        Room(f32, RoomSource),
    }
    let mut pieces: Vec<Piece> = Vec::new();
    let mut sources: Vec<Option<RoomSource>> = Vec::new();
    let mut owners: Vec<Option<usize>> = Vec::new();
    let mut work = vec![Work::Parts(index, 0)];
    while let Some(next) = work.pop() {
        match next {
            Work::Room(width, source) => {
                pieces.push(Piece::Room { width });
                sources.push(Some(source));
                owners.push(None);
            }
            Work::Parts(element, at) => {
                let Some(part) = tree[element].parts().get(at) else {
                    continue;
                };
                work.push(Work::Parts(element, at + 1));
                match part {
                    Part::Words(words) => {
                        let (style, colour) = style_of(tree, element);
                        pieces.push(Piece::Words {
                            words: words.clone(),
                            style,
                            colour: rgba(colour),
                        });
                        sources.push(None);
                        owners.push(Some(element));
                    }
                    Part::Span(span) => {
                        let (left, right) = edge_room(&tree[*span]);
                        if left > 0.0 {
                            pieces.push(Piece::Lead { width: left });
                            sources.push(Some(room_source(&tree[*span], true)));
                            owners.push(None);
                        }
                        if right > 0.0 {
                            work.push(Work::Room(right, room_source(&tree[*span], false)));
                        }
                        work.push(Work::Parts(*span, 0));
                    }
                }
            }
        }
    }
    (pieces, sources, owners)
}

/// The spans inside the element at `index`, in document order: each is before the spans written inside it.
fn spans_in(tree: &[Node], index: usize) -> Vec<usize> {
    let mut found = Vec::new();
    let mut pending: Vec<usize> = Vec::new();
    let push_spans_of = |element: usize, pending: &mut Vec<usize>| {
        let spans: Vec<usize> = tree[element]
            .parts()
            .iter()
            .filter_map(|part| match part {
                Part::Span(span) => Some(*span),
                Part::Words(_) => None,
            })
            .collect();
        pending.extend(spans.into_iter().rev());
    };
    push_spans_of(index, &mut pending);
    while let Some(span) = pending.pop() {
        found.push(span);
        push_spans_of(span, &mut pending);
    }
    found
}

/// The room a span holds at its left and at its right edge (`docs/spec/text.md`, section 6.5, rule 4): its margin and its
/// padding on that side, in pixels. A value that is not drawn holds no room, and its fault is reported with the text.
fn edge_room(node: &Node) -> (f32, f32) {
    let (margin_left, margin_right) = horizontal_sides(node, "margin");
    let (padding_left, padding_right) = horizontal_sides(node, "padding");
    ((margin_left + padding_left) as f32, (margin_right + padding_right) as f32)
}

/// Where the room of a span at one edge comes from (`docs/spec/text.md`, section 10): the line of the attribute that writes it,
/// and whether a `<style>` gives that attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RoomSource {
    pub(crate) line: u32,
}

/// The attribute that gives the room of a span at one edge: its `margin` where the margin holds room on that side, and its
/// `padding` otherwise (`edge_room`). Without the attribute, the line of the span.
fn room_source(node: &Node, left: bool) -> RoomSource {
    let (margin_left, margin_right) = horizontal_sides(node, "margin");
    let margin = if left { margin_left } else { margin_right };
    let name = if margin > 0.0 { "margin" } else { "padding" };
    match node.attribute(name) {
        Some(attribute) => RoomSource { line: attribute.line() },
        None => RoomSource { line: node.line() },
    }
}

/// The left and the right side of a `margin` or a `padding` written in one to four numbers, in CSS's shorthand order (top,
/// right, bottom, left): one number is every side, two are the vertical and the horizontal, three are top, the horizontal and
/// bottom, and four are all four. Only the horizontal sides are drawn (`docs/spec/text.md`, section 6.5, rule 4). A negative
/// side is taken as 0, its fault being reported with the text.
fn horizontal_sides(node: &Node, name: &str) -> (f64, f64) {
    let Some(values) = node.attribute(name).and_then(Attribute::sides) else {
        return (0.0, 0.0);
    };
    let (left, right) = match values.as_slice() {
        [all] => (*all, *all),
        [_, horizontal] => (*horizontal, *horizontal),
        [_, horizontal, _] => (*horizontal, *horizontal),
        [_, right, _, left] => (*left, *right),
        _ => (0.0, 0.0),
    };
    (left.max(0.0), right.max(0.0))
}

/// The faults of the `<span>`s inside the `<text>` at `index`, in document order (`docs/spec/text.md`, section 6.5): an
/// attribute a span does not take, a margin or a padding that is not drawn, and a text attribute that is not in the form the
/// notation gives. Each is on the line of its attribute. Gives no fault for a text with no span.
pub(crate) fn span_faults(tree: &[Node], index: usize) -> Vec<Fault> {
    let mut faults = Vec::new();
    for span in spans_in(tree, index) {
        for attribute in tree[span].attributes() {
            if let Some(fault) = span_attribute_fault(attribute) {
                faults.push(fault);
            }
        }
    }
    faults
}

/// The fault of one attribute of a `<span>`, if it has one.
fn span_attribute_fault(attribute: &Attribute) -> Option<Fault> {
    let name = attribute.name();
    let value = attribute.value();
    let fault = |code: Code, message: String| Fault {
        code,
        line: attribute.line(),
        message,
        style: false,
    };
    if !SPAN_TAKES.contains(&name) {
        let (code, message) = support::refused_attribute(Element::Span, name, support::written(Element::Span, attribute));
        return Some(fault(code, message));
    }
    match name {
        // The notation's check judges `typed` (`docs/spec/notation.md` section 7.12).
        "typed" => None,
        "margin" | "padding" => sides_refusal(name, attribute).map(|(code, message)| fault(code, message)),
        "white-space" if value != "normal" => Some(fault(
            Code::FormNotSupported,
            Unsupported::SpanWindowAndDressing.message(&support::written(Element::Span, attribute)),
        )),
        "text-transform" if value != "none" => Some(fault(
            Code::FormNotSupported,
            Unsupported::SpanWindowAndDressing.message(&support::written(Element::Span, attribute)),
        )),
        _ => match read_text_attribute(attribute, &mut TextAttrs::default()) {
            Err((code, message)) => Some(fault(code, message)),
            Ok(()) => None,
        },
    }
}

/// Why a `margin` or a `padding` of a span is not drawn: a value that is not one to four numbers, a percentage, and a
/// negative margin are refused; a negative padding is refused as a box's is (`docs/spec/notation.md`, section 7.2).
fn sides_refusal(name: &str, attribute: &Attribute) -> Option<(Code, String)> {
    let value = attribute.value();
    if value.contains('%') {
        return Some((
            Code::FormNotSupported,
            Unsupported::SpanRoomForm.message(&format!("<span> has `{name}=\"{value}\"`")),
        ));
    }
    let Some(values) = attribute.sides() else {
        return Some((
            Code::NotANumber,
            format!("`{name}=\"{value}\"` is not one to four numbers, such as 4 or 4 8"),
        ));
    };
    if !values.iter().any(|side| *side < 0.0) {
        return None;
    }
    if name == "padding" {
        Some((Code::LengthNotAccepted, format!("`padding=\"{value}\"` must be 0 or more")))
    } else {
        Some((
            Code::FormNotSupported,
            Unsupported::SpanRoomForm.message(&format!("<span> has `margin=\"{value}\"`, below 0")),
        ))
    }
}

/// A colour of the notation (opaque, as a text's is) as the four bytes the text layout takes.
fn rgba(colour: Colour) -> [u8; 4] {
    [colour.red, colour.green, colour.blue, 255]
}

/// A number as the notation writes it: an optional sign, digits with an optional point, and an optional exponent,
/// which is finite (`notation.md` section 13).
pub(crate) fn number(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    let mut at = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        at += 1;
    }
    let mut digits = 0;
    while at < bytes.len() && bytes[at].is_ascii_digit() {
        at += 1;
        digits += 1;
    }
    if at < bytes.len() && bytes[at] == b'.' {
        at += 1;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if at < bytes.len() && (bytes[at] == b'e' || bytes[at] == b'E') {
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let exponent = at;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
        }
        if at == exponent {
            return None;
        }
    }
    if at != bytes.len() {
        return None;
    }
    text.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn not_a_number(name: &str, value: &str) -> String {
    format!(
        "`{name}=\"{value}\"` is not a number: an optional sign, digits with an optional point and an optional exponent, such as 12, -4, .5 or 2.5e-1"
    )
}

/// A family name written in quotes, without the quotes (`'IBM Plex Mono'`).
fn unquote(name: &str) -> &str {
    let bytes = name.as_bytes();
    if name.len() >= 2 && (bytes[0] == b'\'' || bytes[0] == b'"') && bytes[name.len() - 1] == bytes[0] {
        &name[1..name.len() - 1]
    } else {
        name
    }
}

/// The reason the system gives for an error, without the number of the error.
fn reason(error: &std::io::Error) -> String {
    let text = error.to_string();
    match text.rfind(" (os error ") {
        Some(at) => text[..at].to_string(),
        None => text,
    }
}
