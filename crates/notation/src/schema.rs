//! The elements of the notation, the attributes each takes and the elements each holds:
//! `docs/spec/notation.md` section 6. A test holds these tables to the spec's tables.

/// An element of the notation. `<composition>` is two of them: the root of a file, and a
/// composition shown from another file by its `src` (section 6.9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Element {
    /// The root `<composition>` of a file.
    Composition,
    /// A `<composition src>`: another composition file, shown as a clip.
    Nested,
    Style,
    Font,
    Audio,
    Volume,
    Box,
    Rect,
    Text,
    Span,
    Image,
    Video,
    Shape,
    Tween,
    Set,
}

/// The form of a value that the notation reads for every element alike (section 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// A number: `12`, `-4`, `+0.5`, `.5`, `1e3`, `2.5E-1`.
    Number,
    /// A number whose value has no fraction.
    WholeNumber,
    /// A time in seconds: digits with an optional point.
    Time,
    /// Any other form: read by the part of the product that draws the element.
    Other,
}

const NAME: &[&str] = &["id", "class"];
const CLIP: &[&str] = &["start", "duration"];
const EDGES: &[&str] = &["left", "top", "right", "bottom"];
const SIZE: &[&str] = &["width", "height"];
const FLOW_ITEM: &[&str] = &["max-width", "position", "margin", "align-self", "flex-grow"];
const CONTAINER: &[&str] = &["flex-direction", "justify-content", "align-items", "gap", "padding", "columns"];
const TEXT_PROPERTIES: &[&str] = &[
    "font-family",
    "font-weight",
    "font-style",
    "font-size",
    "line-height",
    "letter-spacing",
    "color",
    "text-transform",
    "text-align",
    "text-shadow",
    "white-space",
    "font-variant-numeric",
    "text-stroke",
];
const BACKGROUND: &[&str] = &["background", "background-size", "background-position", "background-repeat"];
const LOOK: &[&str] = &[
    "border-radius",
    "border",
    "box-shadow",
    "overflow",
    "clip-path",
    "mix-blend-mode",
    "displace",
    "grain",
    "drop-shadow",
    "transform-origin",
    "perspective",
    "transition",
];
/// The movement attributes of every drawn element (section 8.1, the first twelve).
const MOVEMENT: &[&str] = &[
    "x",
    "y",
    "scale",
    "scale-x",
    "rotation",
    "skew-y",
    "opacity",
    "blur",
    "brightness",
    "rotate-x",
    "rotate-y",
    "reveal",
];
/// The fifteen animated properties (section 8.1).
const PROPERTIES: &[&str] = &[
    "x",
    "y",
    "scale",
    "scale-x",
    "rotation",
    "skew-y",
    "opacity",
    "blur",
    "brightness",
    "rotate-x",
    "rotate-y",
    "reveal",
    "count",
    "draw",
    "typed",
];
const FROM: &[&str] = &[
    "from-x",
    "from-y",
    "from-scale",
    "from-scale-x",
    "from-rotation",
    "from-skew-y",
    "from-opacity",
    "from-blur",
    "from-brightness",
    "from-rotate-x",
    "from-rotate-y",
    "from-reveal",
    "from-count",
    "from-draw",
    "from-typed",
];
const TO: &[&str] = &[
    "to-x",
    "to-y",
    "to-scale",
    "to-scale-x",
    "to-rotation",
    "to-skew-y",
    "to-opacity",
    "to-blur",
    "to-brightness",
    "to-rotate-x",
    "to-rotate-y",
    "to-reveal",
    "to-count",
    "to-draw",
    "to-typed",
];

/// The names of attributes whose value is a plain number wherever an element takes them. The
/// movement attributes are in the list too; `width` and `height` are lengths, except on a `<video>`.
const NUMBERS: &[&str] = &[
    "max-width",
    "flex-grow",
    "gap",
    "font-weight",
    "font-size",
    "border-radius",
    "perspective",
    "typed",
    "count",
    "stroke-width",
    "draw",
    "x",
    "y",
    "scale",
    "scale-x",
    "rotation",
    "skew-y",
    "opacity",
    "blur",
    "brightness",
    "rotate-x",
    "rotate-y",
    "reveal",
];
/// The names of attributes whose value is a time.
const TIMES: &[&str] = &["start", "duration", "media-start", "at"];

impl Element {
    /// Every element, in the order of the sections of the spec's tables (section 6).
    pub const ALL: [Element; 15] = [
        Element::Composition,
        Element::Box,
        Element::Rect,
        Element::Text,
        Element::Span,
        Element::Image,
        Element::Video,
        Element::Shape,
        Element::Nested,
        Element::Audio,
        Element::Volume,
        Element::Font,
        Element::Style,
        Element::Tween,
        Element::Set,
    ];

    /// The element a name stands for, as written inside a composition. `<composition>` is
    /// [`Element::Nested`] when it has a `src` and `None` when it has not: only the root of a file is
    /// a `<composition>` without one.
    pub fn named(name: &str, has_src: bool) -> Option<Element> {
        Some(match name {
            "composition" if has_src => Element::Nested,
            "style" => Element::Style,
            "font" => Element::Font,
            "audio" => Element::Audio,
            "volume" => Element::Volume,
            "box" => Element::Box,
            "rect" => Element::Rect,
            "text" => Element::Text,
            "span" => Element::Span,
            "image" => Element::Image,
            "video" => Element::Video,
            "shape" => Element::Shape,
            "tween" => Element::Tween,
            "set" => Element::Set,
            _ => return None,
        })
    }

    /// The name the element is written with.
    pub fn name(self) -> &'static str {
        match self {
            Element::Composition | Element::Nested => "composition",
            Element::Style => "style",
            Element::Font => "font",
            Element::Audio => "audio",
            Element::Volume => "volume",
            Element::Box => "box",
            Element::Rect => "rect",
            Element::Text => "text",
            Element::Span => "span",
            Element::Image => "image",
            Element::Video => "video",
            Element::Shape => "shape",
            Element::Tween => "tween",
            Element::Set => "set",
        }
    }

    /// The element as a message names it: `<box>`, and `<composition src>` for a composition shown
    /// from another file.
    pub fn tag(self) -> &'static str {
        match self {
            Element::Composition => "<composition>",
            Element::Nested => "<composition src>",
            Element::Style => "<style>",
            Element::Font => "<font>",
            Element::Audio => "<audio>",
            Element::Volume => "<volume>",
            Element::Box => "<box>",
            Element::Rect => "<rect>",
            Element::Text => "<text>",
            Element::Span => "<span>",
            Element::Image => "<image>",
            Element::Video => "<video>",
            Element::Shape => "<shape>",
            Element::Tween => "<tween>",
            Element::Set => "<set>",
        }
    }

    /// The elements this one holds directly (section 6; words are not elements).
    pub fn children(self) -> &'static [Element] {
        use Element::*;
        match self {
            Composition => &[Style, Font, Audio, Box, Rect, Text, Image, Video, Shape, Nested],
            Box | Rect => &[Box, Rect, Text, Image, Video, Shape, Nested, Tween, Set],
            Text | Span => &[Span, Tween, Set],
            Image | Video | Shape | Nested => &[Tween, Set],
            Audio => &[Volume],
            Style | Font | Volume | Tween | Set => &[],
        }
    }

    /// The elements this one may stand directly in.
    pub fn parents(self) -> Vec<Element> {
        Element::ALL
            .into_iter()
            .filter(|parent| parent.children().contains(&self))
            .collect()
    }

    /// The groups of names the element's table is made of.
    fn groups(self) -> Vec<&'static [&'static str]> {
        match self {
            Element::Composition => vec![&["id", "class", "version", "width", "height", "fps", "duration", "background"]],
            Element::Nested => vec![NAME, CLIP, EDGES, FLOW_ITEM, &["src"], BACKGROUND, LOOK, MOVEMENT],
            Element::Style => {
                // A style holds any attribute of an element that takes a class (section 9).
                let mut groups: Vec<&'static [&'static str]> = vec![&["id"]];
                for element in Element::ALL {
                    if element != Element::Style && element.groups_have("class") {
                        groups.extend(element.groups());
                    }
                }
                groups
            }
            Element::Font => vec![&["class", "src", "family"]],
            Element::Audio => vec![&["id", "class", "src", "start", "duration", "media-start", "volume"]],
            Element::Volume => vec![&["at", "value"]],
            Element::Box | Element::Rect => vec![
                NAME,
                CLIP,
                EDGES,
                SIZE,
                FLOW_ITEM,
                CONTAINER,
                TEXT_PROPERTIES,
                BACKGROUND,
                LOOK,
                MOVEMENT,
            ],
            Element::Text => vec![
                NAME,
                CLIP,
                EDGES,
                SIZE,
                FLOW_ITEM,
                &["padding"],
                TEXT_PROPERTIES,
                &["typed", "count", "count-format"],
                BACKGROUND,
                LOOK,
                MOVEMENT,
            ],
            Element::Span => vec![
                NAME,
                CLIP,
                &["margin", "padding"],
                TEXT_PROPERTIES,
                &["typed"],
                BACKGROUND,
                LOOK,
                MOVEMENT,
            ],
            Element::Image => vec![
                NAME,
                CLIP,
                EDGES,
                SIZE,
                FLOW_ITEM,
                &["src", "object-fit", "object-position", "filter", "mask"],
                BACKGROUND,
                LOOK,
                MOVEMENT,
            ],
            Element::Video => vec![
                NAME,
                CLIP,
                EDGES,
                SIZE,
                FLOW_ITEM,
                &["src", "media-start", "filter", "lut"],
                BACKGROUND,
                LOOK,
                MOVEMENT,
            ],
            Element::Shape => vec![
                NAME,
                CLIP,
                EDGES,
                SIZE,
                FLOW_ITEM,
                &["src", "d", "view-box", "stroke-width", "color", "dash", "draw"],
                BACKGROUND,
                LOOK,
                MOVEMENT,
            ],
            Element::Tween => vec![&["at", "duration", "ease"], FROM, TO],
            Element::Set => vec![&["at"], PROPERTIES],
        }
    }

    fn groups_have(self, name: &str) -> bool {
        self.groups().iter().any(|group| group.contains(&name))
    }

    /// Whether the element takes an attribute of this name (no namespace).
    pub fn takes(self, name: &str) -> bool {
        self.groups_have(name)
    }

    /// Whether `start` and `duration` are written both or neither on the element (`notation.md`
    /// section 5.2). A `<composition src>` and an `<audio>` take `start` with a default, and are not
    /// judged for the pair.
    pub(crate) fn clip_pair(self) -> bool {
        matches!(
            self,
            Element::Box | Element::Rect | Element::Text | Element::Span | Element::Image | Element::Video | Element::Shape
        )
    }

    /// Every attribute the element takes, each once, in the order of the spec's table.
    pub fn attributes(self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = Vec::new();
        for name in self.groups().into_iter().flatten() {
            if !names.contains(name) {
                names.push(name);
            }
        }
        names
    }

    /// The form of the value of an attribute this element takes, for the forms that every element
    /// reads alike. The attributes of the root and the values of a `<style>` are judged elsewhere
    /// (the root by its own rules, a style on the element that takes it), so they are
    /// [`Form::Other`] here.
    pub fn form(self, attribute: &str) -> Form {
        match self {
            Element::Composition | Element::Style => Form::Other,
            Element::Tween if attribute.starts_with("from-") || attribute.starts_with("to-") => Form::Number,
            Element::Tween if attribute == "ease" => Form::Other,
            Element::Set if attribute != "at" => Form::Number,
            Element::Volume if attribute == "value" => Form::Number,
            Element::Audio if attribute == "volume" => Form::Number,
            Element::Video if attribute == "width" || attribute == "height" => Form::WholeNumber,
            _ if TIMES.contains(&attribute) => Form::Time,
            Element::Tween | Element::Set | Element::Volume | Element::Audio => Form::Other,
            _ if NUMBERS.contains(&attribute) => Form::Number,
            _ => Form::Other,
        }
    }
}
