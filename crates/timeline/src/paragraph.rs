//! The paragraphs of a composition as the timeline places them (`docs/spec/text.md`): the text attributes as
//! they are written and as they pass from a box to the elements inside it, the fonts that a composition
//! declares, and the lines of a `<text>` laid out in its place.

use draw::Colour;
use layout::{place, Amount, Beyond, Rect, Written, LIMIT};
use project::{Attribute, Code, Element, Length, Node, Project};
use text::{Align, Block, FontError, Fonts, LineHeight, Style};

/// The text attributes: a `<box>` and a `<text>` take them, and they pass to the elements inside (`text.md`
/// section 8). A value a `<text>` writes is the one its words are drawn with.
pub(crate) const TEXT_ATTRIBUTES: [&str; 8] = [
    "font-family",
    "font-weight",
    "font-style",
    "font-size",
    "line-height",
    "letter-spacing",
    "color",
    "text-align",
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
    "align-self",
    "flex-grow",
    "max-width",
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
}

/// A paragraph placed in its place: its rectangle in the frame, its lines, and its colour.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Paragraph {
    pub(crate) rect: Rect,
    pub(crate) block: Block,
    pub(crate) colour: Colour,
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
                Some(Length::Pixels(pixels)) => pixels,
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
                    format!("`color=\"{value}\"` has an alpha below ff, which this version does not draw"),
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

/// The style of the text at `index` (`text.md` section 8): each text attribute that the element, or the nearest
/// element above it, writes, and the defaults of the table where none does. A letter spacing in ems is taken in
/// pixels of the font size of the element that writes it. Gives the style and the colour, which is black where
/// none is written.
pub(crate) fn style_of(tree: &[Node], index: usize) -> (Style, Colour) {
    let mut chain: Vec<TextAttrs> = Vec::new();
    let mut at = Some(index);
    while let Some(current) = at {
        chain.push(read_text_attributes(&tree[current], &mut Vec::new()));
        at = tree[current].parent();
    }
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
    let style = Style {
        families,
        size,
        weight,
        italic,
        line_height,
        letter_spacing,
        align,
    };
    (style, colour)
}

/// The fonts a composition declares with `<font>` (`text.md` section 2.1), read from the composition's own tree.
/// Each file is read and registered once for each pair of `src` and `family`. Gives the faults of the files that
/// are missing, unreadable, or not a font this version draws.
pub(crate) fn load_fonts(project: &Project, tree: &[Node]) -> (Fonts, Vec<Fault>) {
    let mut fonts = Fonts::new();
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
                    format!("`src=\"{src}\"` is {kind}, a font of a kind this version does not draw"),
                ),
                FontError::NotAFont => (Code::FontNotAFont, format!("`src=\"{src}\"` is not a TrueType font (`.ttf`)")),
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
/// height is the one written, or the height of its lines. Gives the rectangle and the lines, or the fault that stops it.
pub(crate) fn place_text(fonts: &mut Fonts, tree: &[Node], index: usize, written: Written, parent: Rect) -> Result<Paragraph, Fault> {
    let node = &tree[index];
    let (style, colour) = style_of(tree, index);
    let beyond = |Beyond| Fault {
        code: Code::FormNotSupported,
        line: node.line(),
        message: format!("a place or a size of <text> works out beyond {LIMIT} pixels, which this version does not place"),
        style: false,
    };
    // A width is known with a written `width`, or with both `left` and `right`, which give the width of the
    // container less the two (`layout::place`). Without either, the text is as wide as its widest line.
    let known = written.width.is_some() || (written.left.is_some() && written.right.is_some());
    let wrap = if known {
        // The width the place gives, with the height left out: it depends on the width alone.
        let across = place(
            parent,
            Written {
                height: Some(Amount::Pixels(0.0)),
                ..written
            },
        )
        .map_err(beyond)?;
        Some(across.width as f32)
    } else {
        None
    };
    let block = lay_block(fonts, node, &style, wrap, written.max_width)?;
    let mut sized = written;
    if sized.width.is_none() {
        sized.width = Some(Amount::Pixels(f64::from(block.width).ceil()));
    }
    if sized.height.is_none() {
        sized.height = Some(Amount::Pixels(f64::from(block.height).ceil()));
    }
    let rect = place(parent, sized).map_err(beyond)?;
    Ok(Paragraph { rect, block, colour })
}

/// A `<text>` laid out as an item of a row, a column or a set (`render-command.md` section 4.3, rule 14; `text.md`
/// section 8): its block, its colour, the size its text takes, and its first baseline from its top edge, or `None` for a
/// text with no line. The first baseline lies on a whole pixel, so that the baselines of the texts in one row meet on one
/// line; the glyphs and the lines of the block are moved by the same amount.
pub(crate) struct ItemText {
    pub(crate) block: Block,
    pub(crate) colour: Colour,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) baseline: Option<f64>,
}

/// Lays out the `<text>` at `index` as an item of a row, a column or a set. Its width is the one written, or that of its
/// widest line, which a `max-width` breaks when it writes none (section 5.3); its height is the one written, or that of
/// its lines. A percentage width or height is not drawn in a row or a column (`text.md` section 8). An absolute text with
/// both edges and no width is not drawn either, since its width would need the container's. Gives the layout, or the
/// fault that stops it.
pub(crate) fn lay_out_item(fonts: &mut Fonts, tree: &[Node], index: usize, written: Written, absolute: bool) -> Result<ItemText, Fault> {
    let node = &tree[index];
    let (style, colour) = style_of(tree, index);
    if absolute && written.width.is_none() && written.left.is_some() && written.right.is_some() {
        return Err(Fault {
            code: Code::FormNotSupported,
            line: node.line(),
            message: "a <text> with `left` and `right` and no `width`, inside a row or a column, is not drawn by this version".to_string(),
            style: false,
        });
    }
    let width = flow_pixels(written.width, "width", node)?;
    let height = flow_pixels(written.height, "height", node)?;
    let mut block = lay_block(fonts, node, &style, width.map(|pixels| pixels as f32), written.max_width)?;
    let first = block.lines.first().map(|line| f64::from(line.baseline));
    let baseline = first.map(|first| {
        let whole = (first + 0.5).floor();
        let shift = (whole - first) as f32;
        for run in block.runs.iter_mut() {
            for glyph in run.glyphs.iter_mut() {
                glyph.y += shift;
            }
        }
        for line in block.lines.iter_mut() {
            line.baseline += shift;
        }
        whole
    });
    let width = width.unwrap_or_else(|| f64::from(block.width).ceil());
    let height = height.unwrap_or_else(|| f64::from(block.height).ceil());
    Ok(ItemText {
        block,
        colour,
        width,
        height,
        baseline,
    })
}

/// A `width` or `height` of a `<text>` in a row or a column as pixels: `None` where it is not written. A percentage is not
/// drawn there (`text.md` section 8), and is reported on the element's line.
fn flow_pixels(amount: Option<Amount>, name: &str, node: &Node) -> Result<Option<f64>, Fault> {
    match amount {
        None => Ok(None),
        Some(Amount::Pixels(pixels)) => Ok(Some(pixels)),
        Some(Amount::Percent(percent)) => Err(Fault {
            code: Code::FormNotSupported,
            line: node.line(),
            message: format!(
                "`{name}=\"{percent}%\"` on <text> inside a row or a column is not a form this version supports: a whole number of pixels"
            ),
            style: false,
        }),
    }
}

/// Lays out the words of the `<text>` at `node` in `style`: wrapped at `wrap` where a width is given, else broken so that
/// no line is wider than `max` where a `max-width` is written (`text.md` section 5.3), else one line. Gives the block, or
/// the fault of a character that no family has.
fn lay_block(fonts: &mut Fonts, node: &Node, style: &Style, wrap: Option<f32>, max: Option<f64>) -> Result<Block, Fault> {
    let laid = match (wrap, max) {
        (Some(width), _) => text::layout(fonts, node.words(), style, Some(width)),
        (None, Some(max)) => text::layout_fitted(fonts, node.words(), style, max as f32),
        (None, None) => text::layout(fonts, node.words(), style, None),
    };
    laid.map_err(|error| Fault {
        code: Code::FormNotSupported,
        line: node.line(),
        message: format!("<text> is not drawn by this version: {error}"),
        style: false,
    })
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
