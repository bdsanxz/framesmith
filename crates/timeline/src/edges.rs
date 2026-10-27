//! A box's border, corner radius and cut, read from its attributes (`docs/spec/box-edges.md`, sections 1 and 6). A value in no
//! form the spec lists is given back with its code, its line and a message that names the forms taken.

use draw::{Edges, Line, Rgba};
use project::{Code, Node};

/// What a box writes of its edges, or every problem with them as a code, a line and a message.
pub(crate) fn read_edges(node: &Node) -> Result<Edges, Vec<(Code, u32, String)>> {
    let mut edges = Edges {
        radius: 0.0,
        border: None,
        cut: false,
    };
    let mut faults = Vec::new();
    for attribute in node.attributes() {
        let value = attribute.value();
        match attribute.name() {
            "border" => match border(value) {
                Some(border) => edges.border = Some(border),
                None => faults.push((
                    Code::BorderNotInForm,
                    attribute.line(),
                    format!(
                        "`border=\"{}\"` is not a width and a colour: write `width colour` or `width double colour`, such as `6 #ffffff` or `9 double #ffffff`",
                        attribute.shown()
                    ),
                )),
            },
            // A value that is not a number is reported by the notation (FS-0032).
            "border-radius" => match attribute.number() {
                Some(radius) if radius >= 0.0 => edges.radius = radius,
                Some(_) => faults.push((
                    Code::LengthNotAccepted,
                    attribute.line(),
                    format!("`border-radius=\"{}\"` must be 0 or more", attribute.shown()),
                )),
                None => {}
            },
            "overflow" => match value {
                "visible" => edges.cut = false,
                "hidden" => edges.cut = true,
                _ => faults.push((
                    Code::WordNotTaken,
                    attribute.line(),
                    format!("`overflow=\"{}\"` is not one of: visible, hidden", attribute.shown()),
                )),
            },
            _ => {}
        }
    }
    if faults.is_empty() {
        Ok(edges)
    } else {
        Err(faults)
    }
}

/// A border written `w c` or `w double c`: a number of 0 or more and a colour, with the one word `double` between them.
fn border(value: &str) -> Option<(Line, Rgba)> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    let (width, double, colour) = match parts[..] {
        [width, colour] => (width, false, colour),
        [width, "double", colour] => (width, true, colour),
        _ => return None,
    };
    let width = project::number_value(width).filter(|width| *width >= 0.0)?;
    let colour = project::colour_value(colour)?;
    Some((
        Line { width, double },
        Rgba {
            red: colour.red,
            green: colour.green,
            blue: colour.blue,
            alpha: colour.alpha,
        },
    ))
}
