//! `box-shadow` and `drop-shadow` as the scene reads them (`docs/spec/render-command.md`, section 4.7, rules 1 and 8).

use draw::{Colour, Shadow};
use project::{Code, Unsupported};

use crate::paragraph::{colour_token, length_token};

/// `box-shadow`: `none`, or shadows separated by commas, the first listed on top. `what` names what the author wrote
/// (the element and the attribute with its value, and the style that gives it when one does), for the message of a
/// refusal (`docs/spec/check-command.md` section 3c).
pub(crate) fn box_shadows(what: &str, value: &str) -> Result<Vec<Shadow>, (Code, String)> {
    if value.trim() == "none" {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|entry| shadow(what, "box-shadow", value, entry.trim()))
        .collect()
}

/// `drop-shadow`: `none`, or one shadow. `what` is what the message of a refusal names, as for `box_shadows`.
pub(crate) fn drop_shadow(what: &str, value: &str) -> Result<Option<Shadow>, (Code, String)> {
    if value.trim() == "none" {
        return Ok(None);
    }
    shadow(what, "drop-shadow", value, value.trim()).map(Some)
}

/// One shadow, `X Y BLUR COLOUR` (rule 1), with the reports of rule 8: a form this version does not draw names its
/// entry of the support table (S73 to S75); anything else not in form is invalid.
fn shadow(what: &str, name: &str, value: &str, entry: &str) -> Result<Shadow, (Code, String)> {
    let not_in_form = || {
        (
            Code::ShadowNotInForm,
            format!("`{name}=\"{value}\"` is not in the form of a shadow: an offset across, an offset down, a blur and a colour, such as 10 10 8 #000000"),
        )
    };
    let tokens: Vec<&str> = entry.split_whitespace().collect();
    if tokens.contains(&"inset") {
        return Err((Code::FormNotSupported, Unsupported::InnerShadow.message(what)));
    }
    let lengths = |words: &[&str]| words.iter().map(|word| length_token(word)).collect::<Option<Vec<f32>>>();
    let (numbers, colour) = match tokens.as_slice() {
        [x, y, blur, colour] => (lengths(&[x, y, blur]).ok_or_else(not_in_form)?, *colour),
        [x, y, blur, spread, colour] if lengths(&[x, y, blur, spread]).is_some() && colour_token(colour).is_some() => {
            return Err((Code::FormNotSupported, Unsupported::SpreadShadow.message(what)))
        }
        _ => return Err(not_in_form()),
    };
    if numbers[2] < 0.0 {
        return Err((
            Code::LengthNotAccepted,
            format!("`{name}=\"{value}\"` has a blur below 0: it must be 0 or more"),
        ));
    }
    let colour = match colour_token(colour).ok_or_else(not_in_form)? {
        [red, green, blue, 255] => Colour { red, green, blue },
        _ => return Err((Code::FormNotSupported, Unsupported::AlphaShadow.message(what))),
    };
    Ok(Shadow {
        across: numbers[0],
        down: numbers[1],
        blur: numbers[2],
        colour,
    })
}
