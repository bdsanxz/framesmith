//! Which entry of the support table (`docs/spec/notation.md` section 14) stops an attribute that the notation lists for an
//! element and this version does not draw on it. Every refusal of the kind unsupported names its entry; an attribute no entry
//! names is not refused as unsupported (section 14: it would be invalid).

use project::{shown, Attribute, Code, Element, Unsupported};

/// The entry of the support table that names the attribute `name` on `element`, which this version does not take there;
/// `None` when no entry names it.
pub(crate) fn attribute_entry(element: Element, name: &str) -> Option<Unsupported> {
    let found = match name {
        "border" | "border-radius" | "overflow" if matches!(element, Element::Text | Element::Span) => Unsupported::TextAndSpanLook,
        "blur" | "brightness" if element == Element::Span => Unsupported::TextAndSpanLook,
        "blur" | "brightness" => Unsupported::AnimatedBlurBrightness,
        "mix-blend-mode" => Unsupported::Blends,
        "displace" | "grain" => Unsupported::TornEdgesAndGrain,
        "clip-path" => Unsupported::PolygonCut,
        "transition" | "reveal" => Unsupported::Transitions,
        "count" | "count-format" | "typed" => Unsupported::CountingTypingSpanMotion,
        "draw" => Unsupported::Shapes,
        "background" | "background-size" | "background-position" | "background-repeat" if element != Element::Box => {
            Unsupported::BackgroundOrPaddingNotOnBox
        }
        "padding" if element == Element::Text => Unsupported::BackgroundOrPaddingNotOnBox,
        _ => return per_element(element, name),
    };
    Some(found)
}

/// The entries that depend on the element: the forms of a span, the movement and the item forms of a shown composition, and
/// the forms of footage.
fn per_element(element: Element, name: &str) -> Option<Unsupported> {
    let moving = matches!(
        name,
        "x" | "y" | "scale" | "scale-x" | "rotation" | "skew-y" | "rotate-x" | "rotate-y" | "transform-origin" | "perspective"
    );
    match element {
        Element::Span => match name {
            "start" | "duration" | "text-shadow" | "text-stroke" | "font-variant-numeric" => Some(Unsupported::SpanWindowAndDressing),
            "text-align" => Some(Unsupported::SpanTextAlign),
            "opacity" => Some(Unsupported::TextAndSpanLook),
            _ if moving => Some(Unsupported::CountingTypingSpanMotion),
            _ => None,
        },
        Element::Nested => match name {
            "max-width" | "align-self" | "flex-grow" => Some(Unsupported::ItemFormsOfLeaf),
            "opacity" => Some(Unsupported::PictureMotion),
            _ if moving => Some(Unsupported::PictureMotion),
            _ => None,
        },
        Element::Video => match name {
            "lut" | "filter" => Some(Unsupported::FootageFilter),
            "max-width" | "align-self" | "flex-grow" => Some(Unsupported::ItemFormsOfLeaf),
            _ if moving => Some(Unsupported::PictureMotion),
            _ => None,
        },
        _ => None,
    }
}

/// The code and the message of an attribute that this version does not take on its element: `FS-0035` with the entry that
/// names it, or, when no entry does, the attribute is reported as one this version does not take (`FS-0030`).
pub(crate) fn refused_attribute(element: Element, name: &str, what: String) -> (Code, String) {
    match attribute_entry(element, name) {
        Some(entry) => (Code::FormNotSupported, entry.message(&what)),
        None => (Code::AttributeNotTaken, format!("{what}, which this version does not take there")),
    }
}

/// What an author wrote, for the message of a refusal: the element and the attribute with its value, and the style that gives
/// the attribute when one does, since the line is then the element's (`docs/spec/check-command.md` section 3c).
pub(crate) fn written(element: Element, attribute: &Attribute) -> String {
    let what = format!("{} has `{}=\"{}\"`", element.tag(), shown(attribute.name()), attribute.shown());
    match attribute.given_by() {
        None => what,
        Some(style) => format!("{what}, which the style `{}` gives it", shown(style)),
    }
}

/// The code and the message of an animated property (`docs/spec/notation.md` section 8.1) that this version does not move,
/// with the entry of the support table that names it.
pub(crate) fn refused_property(property: &str, what: String) -> (Code, String) {
    refused_attribute(Element::Box, property, what)
}
