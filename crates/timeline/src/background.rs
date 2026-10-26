//! The reading of a box's `background` and of its settings (`docs/spec/render-command.md`, section 4.6; `docs/spec/notation.md`
//! section 7.4): the layers, each a colour or a gradient, and the size, place and repeat of each. What is not in the form is
//! given back as a message for `FS-0064`, a conic gradient as one for `FS-0035`.

use draw::{Amount, Layer, Paint, Rgba, Shape, Size, Stop};
use project::{colour_value, number_value, Code};

/// Why a value is not read: its code and its message.
pub(crate) type Fault = (Code, String);

/// The parts of a list separated by commas that are not inside brackets, each without its surrounding white space.
fn top_level_parts(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0_i32, 0);
    for (at, character) in value.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(value[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(value[start..].trim());
    parts
}

/// The message of a value of `name` that is not in its form, with what the form is.
fn not_in_form(name: &str, value: &str, why: &str) -> Fault {
    (
        Code::BackgroundNotInForm,
        format!("`{name}=\"{}\"` is not in the form of a background: {why}", project::shown(value)),
    )
}

/// A colour of a background, as written.
fn rgba(text: &str) -> Option<Rgba> {
    colour_value(text).map(|colour| Rgba {
        red: colour.red,
        green: colour.green,
        blue: colour.blue,
        alpha: colour.alpha,
    })
}

/// A number with an optional `%`, as pixels or a percentage.
fn amount(text: &str) -> Option<Amount> {
    match text.strip_suffix('%') {
        Some(number) => number_value(number).map(Amount::Percent),
        None => number_value(text).map(Amount::Pixels),
    }
}

/// A percentage, written with `%`.
fn percent(text: &str) -> Option<f64> {
    text.strip_suffix('%').and_then(number_value)
}

/// The stops of a gradient: two or more, each a colour and a place.
fn stops(parts: &[&str]) -> Option<Vec<Stop>> {
    if parts.len() < 2 {
        return None;
    }
    parts
        .iter()
        .map(|part| {
            let words: Vec<&str> = part.split_whitespace().collect();
            match words.as_slice() {
                [colour, place] => Some(Stop {
                    colour: rgba(colour)?,
                    place: amount(place)?,
                }),
                _ => None,
            }
        })
        .collect()
}

/// The shape of a radial gradient and its centre (rule 6): `circle`, `circle R`, `ellipse` or `ellipse A% B%`, then
/// ` at X% Y%` or nothing. Radii are above 0.
fn shape(text: &str) -> Option<(Shape, Option<(f64, f64)>)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let (form, centre) = match words.iter().position(|word| *word == "at") {
        Some(at) => match &words[at + 1..] {
            [x, y] => (&words[..at], Some((percent(x)?, percent(y)?))),
            _ => return None,
        },
        None => (&words[..], None),
    };
    let shape = match form {
        ["circle"] => Shape::Circle(None),
        ["circle", radius] => Shape::Circle(Some(number_value(radius).filter(|radius| *radius > 0.0)?)),
        ["ellipse"] => Shape::Ellipse(None),
        ["ellipse", across, down] => {
            let (across, down) = (percent(across)?, percent(down)?);
            if across <= 0.0 || down <= 0.0 {
                return None;
            }
            Shape::Ellipse(Some((across, down)))
        }
        _ => return None,
    };
    Some((shape, centre))
}

/// One layer as written (rules 1, 5 and 6).
fn paint(name: &str, value: &str, layer: &str) -> Result<Paint, Fault> {
    if let Some(colour) = rgba(layer) {
        return Ok(Paint::Colour(colour));
    }
    let inside = |prefix: &str| layer.strip_prefix(prefix).and_then(|rest| rest.strip_suffix(')'));
    if inside("conic-gradient(").is_some() {
        return Err((
            Code::FormNotSupported,
            project::Unsupported::ConicGradient.message(&format!("`{name}=\"{}\"` holds a conic gradient", project::shown(value))),
        ));
    }
    let two_stops = "a gradient needs two stops or more, each a colour and a place, such as #ffffff 40%";
    if let Some(body) = inside("linear-gradient(") {
        let parts = top_level_parts(body);
        let (angle, rest) = match parts[0].strip_suffix("deg").and_then(number_value) {
            Some(angle) => (angle, &parts[1..]),
            None => (180.0, &parts[..]),
        };
        let stops = stops(rest).ok_or_else(|| not_in_form(name, value, two_stops))?;
        return Ok(Paint::Linear { angle, stops });
    }
    if let Some(body) = inside("radial-gradient(") {
        let parts = top_level_parts(body);
        let (shape, centre) = shape(parts[0]).ok_or_else(|| {
            not_in_form(
                name,
                value,
                "the shape of a radial gradient is circle, circle R (pixels above 0), ellipse or ellipse A% B% (above 0), then at X% Y% or nothing",
            )
        })?;
        let stops = stops(&parts[1..]).ok_or_else(|| not_in_form(name, value, two_stops))?;
        return Ok(Paint::Radial { shape, centre, stops });
    }
    Err(not_in_form(
        name,
        value,
        "each layer is a colour #rrggbb or #rrggbbaa, a linear-gradient(...) or a radial-gradient(...)",
    ))
}

/// The layers of a `background`, the first listed first.
pub(crate) fn layers(value: &str) -> Result<Vec<Paint>, Fault> {
    top_level_parts(value)
        .into_iter()
        .map(|layer| paint("background", value, layer))
        .collect()
}

/// One entry of a `background-size`.
fn size(entry: &str) -> Option<Size> {
    let words: Vec<&str> = entry.split_whitespace().collect();
    match words.as_slice() {
        ["auto"] => Some(Size::Auto),
        [across, down] => {
            let (across, down) = (amount(across)?, amount(down)?);
            let below = |amount: Amount| matches!(amount, Amount::Pixels(v) | Amount::Percent(v) if v < 0.0);
            (!below(across) && !below(down)).then_some(Size::Given(across, down))
        }
        _ => None,
    }
}

/// One entry of a `background-position`.
fn position(entry: &str) -> Option<(Amount, Amount)> {
    let words: Vec<&str> = entry.split_whitespace().collect();
    match words.as_slice() {
        [x, y] => Some((amount(x)?, amount(y)?)),
        _ => None,
    }
}

/// The entries of a `background-size`: `auto` or `W H`, each pixels or a percentage, 0 or more.
pub(crate) fn sizes(value: &str) -> Result<Vec<Size>, Fault> {
    let why = "each entry is auto, or a width and a height, each a number of pixels or a percentage, 0 or more";
    top_level_parts(value)
        .into_iter()
        .map(|entry| size(entry).ok_or_else(|| not_in_form("background-size", value, why)))
        .collect()
}

/// The entries of a `background-position`: `X Y`, each pixels or a percentage.
pub(crate) fn positions(value: &str) -> Result<Vec<(Amount, Amount)>, Fault> {
    let why = "each entry is an x and a y, each a number of pixels or a percentage";
    top_level_parts(value)
        .into_iter()
        .map(|entry| position(entry).ok_or_else(|| not_in_form("background-position", value, why)))
        .collect()
}

/// The entries of a `background-repeat`: `repeat` or `no-repeat`.
pub(crate) fn repeats(value: &str) -> Result<Vec<bool>, Fault> {
    top_level_parts(value)
        .into_iter()
        .map(|entry| match entry {
            "repeat" => Ok(true),
            "no-repeat" => Ok(false),
            _ => Err(not_in_form("background-repeat", value, "each entry is repeat or no-repeat")),
        })
        .collect()
}

/// The entry of a list for each of `count` layers: the one entry for all, or one for each (rule 2).
pub(crate) fn for_each<T: Clone>(name: &str, value: &str, entries: Vec<T>, count: usize) -> Result<Vec<T>, Fault> {
    match entries.len() {
        1 => Ok(vec![entries[0].clone(); count]),
        written if written == count => Ok(entries),
        written => Err((
            Code::BackgroundListLength,
            format!(
                "`{name}=\"{}\"` has {written} entries for {count} {}: write one entry for all the layers or one for each",
                project::shown(value),
                if count == 1 { "layer" } else { "layers" }
            ),
        )),
    }
}

/// The layers with their settings, each list taken for every layer.
pub(crate) fn settle(paints: Vec<Paint>, sizes: Vec<Size>, positions: Vec<(Amount, Amount)>, repeats: Vec<bool>) -> Vec<Layer> {
    paints
        .into_iter()
        .zip(sizes)
        .zip(positions)
        .zip(repeats)
        .map(|(((paint, size), position), repeat)| Layer {
            paint,
            size,
            position,
            repeat,
        })
        .collect()
}
