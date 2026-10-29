//! The support table of `docs/spec/notation.md` section 14: the forms an author can write that this version knows and does
//! not support. Each refusal of the kind unsupported names its entry, with the entry's reason and the sprint it waits for,
//! so that `check` and `render` say which entry stops a form. A form no entry names is invalid (section 14).

use std::fmt;

use crate::schema::Element;

/// An entry of the support table, by what it names. Its number is the entry's number in the spec.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Unsupported {
    RelativeStart = 1,
    AutoDuration = 2,
    RatioFrameRate = 3,
    CanvasPreset = 4,
    StackingOrder = 5,
    MediaEnd = 6,
    PlaybackRate = 7,
    FootageSound = 8,
    LoopOrHold = 9,
    FittingFootage = 10,
    SeveralCompositions = 11,
    Keyframes = 12,
    Stagger = 13,
    Labels = 14,
    Repeats = 15,
    MoreEasings = 16,
    ConicGradient = 17,
    PictureMask = 18,
    RestOfFlexAndGrid = 19,
    BordersBySide = 20,
    RightToLeft = 21,
    MotionPath = 22,
    Expressions = 23,
    AnimatedSizeAndColour = 24,
    BackfaceVisibility = 25,
    TurnedStillPicture = 26,
    OtherFootageKinds = 27,
    OtherSoundKinds = 28,
    FootageAlphaOrHdr = 29,
    AnimatedPictures = 30,
    RemoteSource = 31,
    EditListOrFragments = 32,
    EllipticalCorner = 33,
    FilmNotMp4 = 34,
    TextAndSpanLook = 35,
    Blends = 38,
    TornEdgesAndGrain = 39,
    PolygonCut = 41,
    Shapes = 42,
    CountingTypingSpanMotion = 43,
    Transitions = 44,
    FootageFilter = 46,

    Workers = 50,
    ElementInsideLeaf = 51,
    FootageInFlow = 52,
    MotionAroundShown = 53,
    TextBetweenEdgesInFlow = 54,
    GapPercentage = 55,
    FractionOfAPixel = 56,
    BeyondTheLimit = 57,
    OtherFontKinds = 58,
    BackgroundOrPaddingNotOnBox = 59,
    Rect = 60,
    AlphaOutsideBoxBackground = 61,
    SoundBitRate = 62,
    SpanTextAlign = 63,
    PictureMotion = 64,
    SpanWindowAndDressing = 65,
    ItemFormsOfLeaf = 66,
    SpanRoomForm = 68,
    SpanRoomWithoutPlace = 69,
    TextPercentInFlow = 70,
    EmojiPicture = 72,
    InnerShadow = 73,
    SpreadShadow = 74,
    AlphaShadow = 75,
}

impl Unsupported {
    /// Every entry, in the order of the table.
    pub const ALL: [Unsupported; 66] = [
        Unsupported::RelativeStart,
        Unsupported::AutoDuration,
        Unsupported::RatioFrameRate,
        Unsupported::CanvasPreset,
        Unsupported::StackingOrder,
        Unsupported::MediaEnd,
        Unsupported::PlaybackRate,
        Unsupported::FootageSound,
        Unsupported::LoopOrHold,
        Unsupported::FittingFootage,
        Unsupported::SeveralCompositions,
        Unsupported::Keyframes,
        Unsupported::Stagger,
        Unsupported::Labels,
        Unsupported::Repeats,
        Unsupported::MoreEasings,
        Unsupported::ConicGradient,
        Unsupported::PictureMask,
        Unsupported::RestOfFlexAndGrid,
        Unsupported::BordersBySide,
        Unsupported::RightToLeft,
        Unsupported::MotionPath,
        Unsupported::Expressions,
        Unsupported::AnimatedSizeAndColour,
        Unsupported::BackfaceVisibility,
        Unsupported::TurnedStillPicture,
        Unsupported::OtherFootageKinds,
        Unsupported::OtherSoundKinds,
        Unsupported::FootageAlphaOrHdr,
        Unsupported::AnimatedPictures,
        Unsupported::RemoteSource,
        Unsupported::EditListOrFragments,
        Unsupported::EllipticalCorner,
        Unsupported::FilmNotMp4,
        Unsupported::TextAndSpanLook,
        Unsupported::Blends,
        Unsupported::TornEdgesAndGrain,
        Unsupported::PolygonCut,
        Unsupported::Shapes,
        Unsupported::CountingTypingSpanMotion,
        Unsupported::Transitions,
        Unsupported::FootageFilter,
        Unsupported::Workers,
        Unsupported::ElementInsideLeaf,
        Unsupported::FootageInFlow,
        Unsupported::MotionAroundShown,
        Unsupported::TextBetweenEdgesInFlow,
        Unsupported::GapPercentage,
        Unsupported::FractionOfAPixel,
        Unsupported::BeyondTheLimit,
        Unsupported::OtherFontKinds,
        Unsupported::BackgroundOrPaddingNotOnBox,
        Unsupported::Rect,
        Unsupported::AlphaOutsideBoxBackground,
        Unsupported::SoundBitRate,
        Unsupported::SpanTextAlign,
        Unsupported::PictureMotion,
        Unsupported::SpanWindowAndDressing,
        Unsupported::ItemFormsOfLeaf,
        Unsupported::SpanRoomForm,
        Unsupported::SpanRoomWithoutPlace,
        Unsupported::TextPercentInFlow,
        Unsupported::EmojiPicture,
        Unsupported::InnerShadow,
        Unsupported::SpreadShadow,
        Unsupported::AlphaShadow,
    ];

    /// The entry's number in the table: 35 for S35.
    pub fn number(self) -> u32 {
        self as u32
    }

    /// The entry's name in the table: `S35`.
    pub fn name(self) -> String {
        format!("S{:02}", self.number())
    }

    /// Why the form is not supported, as the table's column "Reason" words it.
    pub fn reason(self) -> &'static str {
        match self {
            Unsupported::RelativeStart => "Needs unique, stable ids and a worked-out timeline",
            Unsupported::AutoDuration => "Needs the same timeline",
            Unsupported::RatioFrameRate => "The notation and the encoder settings take whole rates; the ratio waits for them",
            Unsupported::CanvasPreset => "The presets are not chosen",
            Unsupported::StackingOrder => "Drawing order is document order; a second order changes the drawing rules",
            Unsupported::MediaEnd => "Needs the timeline of S01 and S02",
            Unsupported::PlaybackRate => "Needs resampling of pictures and sound at a rate",
            Unsupported::FootageSound => "The sound of footage is not mixed; write an `<audio>`",
            Unsupported::LoopOrHold => "Needs S06 and S07",
            Unsupported::FittingFootage => "Not in the proofs (decision A21)",
            Unsupported::SeveralCompositions => "The project file names one composition",
            Unsupported::Keyframes => "The motion engine has tweens and sets only",
            Unsupported::Stagger => "Needs several targets for one tween",
            Unsupported::Labels => "Needs a timeline of named points",
            Unsupported::Repeats => "Needs a repeat in the evaluator",
            Unsupported::MoreEasings => "The engine has the nineteen curves and `out-back(n)`",
            Unsupported::ConicGradient => "Not drawn by either renderer",
            Unsupported::PictureMask => "Needs a picture as a mask in both renderers",
            Unsupported::RestOfFlexAndGrid => "Layout has rows, columns and a set of columns only",
            Unsupported::BordersBySide => "One border round the box only",
            Unsupported::RightToLeft => "Lines are ordered inside, the paragraph runs left to right; the whole-paragraph direction is Sprint 5's (decision B36)",
            Unsupported::MotionPath => "The motion engine moves along straight lines",
            Unsupported::Expressions => "A composition carries no code",
            Unsupported::AnimatedSizeAndColour => "Layout does not follow motion and colours are not mixed by the engine",
            Unsupported::BackfaceVisibility => "A turned element is drawn mirrored (decision A23)",
            Unsupported::TurnedStillPicture => "The turn must be applied to the picture (decision P10)",
            Unsupported::OtherFootageKinds => "Their codecs are not written yet",
            Unsupported::OtherSoundKinds => "Their decoders are not written yet",
            Unsupported::FootageAlphaOrHdr => "Both are held to 8 bits and opaque in this work",
            Unsupported::AnimatedPictures => "A still picture is one frame",
            Unsupported::RemoteSource => "Nothing is read from a network (decision P6)",
            Unsupported::EditListOrFragments => "Refused by name (decision P12)",
            Unsupported::EllipticalCorner => "One circular radius per corner only",
            Unsupported::FilmNotMp4 => "A film is written as an MP4 file only; a QuickTime movie is Sprint 9's, and no other kind of file is planned (decision B40)",
            Unsupported::TextAndSpanLook => "Not drawn on a text or a span yet",
            Unsupported::Blends => "Blending with what is under is not drawn yet",
            Unsupported::TornEdgesAndGrain => "Not drawn yet",
            Unsupported::PolygonCut => "Not drawn yet",
            Unsupported::Shapes => "Path data is not drawn yet",
            Unsupported::CountingTypingSpanMotion => "Counting, typing and the motion of a span are not built yet",
            Unsupported::Transitions => "Openings over the scene are not drawn yet",
            Unsupported::FootageFilter => "Not applied yet",
            Unsupported::Workers => "So far one worker draws and encodes",
            Unsupported::ElementInsideLeaf => "A shown composition is drawn from its file, footage is a picture, a text is its words",
            Unsupported::FootageInFlow => "Footage is placed by its own edges",
            Unsupported::MotionAroundShown => "A shown composition moves only as itself",
            Unsupported::TextBetweenEdgesInFlow => "Its width cannot be worked from both edges inside a flow",
            Unsupported::GapPercentage => "Gaps are whole pixels",
            Unsupported::FractionOfAPixel => "Edges lie on whole pixels",
            Unsupported::BeyondTheLimit => "The limit of a place",
            Unsupported::OtherFontKinds => "Only `.ttf` and `.ttc` are read",
            Unsupported::BackgroundOrPaddingNotOnBox => "Backgrounds are drawn behind boxes only, and a text is not padded",
            Unsupported::Rect => "`<rect>` is not read yet, though section 6.3 names it",
            Unsupported::AlphaOutsideBoxBackground => "Only a box's background is laid over what is under it; a film's ground is opaque",
            Unsupported::SoundBitRate => "The system's sound encoder takes 64 to 320 kbit/s",
            Unsupported::SpanTextAlign => "Lines are aligned by their text, and a span lies inside a line",
            Unsupported::PictureMotion => "So far footage and a shown composition are drawn in their place only",
            Unsupported::SpanWindowAndDressing => "So far a span is drawn for its text's whole life and in its plain ink only",
            Unsupported::ItemFormsOfLeaf => "So far these are taken on boxes and texts only",
            Unsupported::SpanRoomForm => "A span's room is a number of pixels, 0 or more",
            Unsupported::SpanRoomWithoutPlace => "The room has no place in the line there",
            Unsupported::TextPercentInFlow => "Inside a flow a text is measured in whole pixels",
            Unsupported::EmojiPicture => "Only PNG colour pictures are drawn",
            Unsupported::InnerShadow => "Only shadows outside the box are drawn (ROADMAP 5.13)",
            Unsupported::SpreadShadow => "Only the four values `X Y BLUR COLOUR` are drawn (ROADMAP 5.13)",
            Unsupported::AlphaShadow => "Only opaque shadow colours are drawn",
        }
    }

    /// When the form is supported, as the table's column "Waits until" words it.
    pub fn waits(self) -> &'static str {
        match self {
            Unsupported::RelativeStart => "Sprint 2",
            Unsupported::AutoDuration => "Sprint 2",
            Unsupported::RatioFrameRate => "Sprint 2",
            Unsupported::CanvasPreset => "Sprint 2",
            Unsupported::StackingOrder => "Sprint 2",
            Unsupported::MediaEnd => "Sprint 2",
            Unsupported::PlaybackRate => "Sprint 2",
            Unsupported::FootageSound => "Sprint 2",
            Unsupported::LoopOrHold => "Sprint 2",
            Unsupported::FittingFootage => "Sprint 5",
            Unsupported::SeveralCompositions => "Sprint 2",
            Unsupported::Keyframes => "Sprint 4",
            Unsupported::Stagger => "Sprint 4",
            Unsupported::Labels => "Sprint 4",
            Unsupported::Repeats => "Sprint 4",
            Unsupported::MoreEasings => "Sprint 4",
            Unsupported::ConicGradient => "Sprint 5",
            Unsupported::PictureMask => "Sprint 5",
            Unsupported::RestOfFlexAndGrid => "Sprint 5",
            Unsupported::BordersBySide => "Sprint 5",
            Unsupported::RightToLeft => "Sprint 5",
            Unsupported::MotionPath => "Sprint 4",
            Unsupported::Expressions => "Sprint 4",
            Unsupported::AnimatedSizeAndColour => "Sprint 4",
            Unsupported::BackfaceVisibility => "Sprint 4",
            Unsupported::TurnedStillPicture => "Sprint 7",
            Unsupported::OtherFootageKinds => "Sprint 7",
            Unsupported::OtherSoundKinds => "Sprint 7",
            Unsupported::FootageAlphaOrHdr => "Sprint 7",
            Unsupported::AnimatedPictures => "Sprint 7",
            Unsupported::RemoteSource => "Sprint 7",
            Unsupported::EditListOrFragments => "Sprint 7",
            Unsupported::EllipticalCorner => "Sprint 5",
            Unsupported::FilmNotMp4 => "Sprint 9 (`.mov`); not assigned (any other)",
            Unsupported::TextAndSpanLook => "Sprint 5 (n05-layout-text-look), acceptance criterion 1: every attribute value of the notation has a fixture with a checked-in picture on the processor and agreement on the card (PLAN.md); roadmap 5.23",
            Unsupported::Blends => "Sprint 1, T057",
            Unsupported::TornEdgesAndGrain => "Sprint 1, T058",
            Unsupported::PolygonCut => "Sprint 1, T061",
            Unsupported::Shapes => "Sprint 1, T062",
            Unsupported::CountingTypingSpanMotion => "Sprint 1, T069",
            Unsupported::Transitions => "Sprint 1, T072",
            Unsupported::FootageFilter => "Sprint 1, T078",
            Unsupported::Workers => "Sprint 1, T096",
            Unsupported::ElementInsideLeaf => "Not assigned",
            Unsupported::FootageInFlow => "Not assigned",
            Unsupported::MotionAroundShown => "Not assigned",
            Unsupported::TextBetweenEdgesInFlow => "Not assigned",
            Unsupported::GapPercentage => "Not assigned",
            Unsupported::FractionOfAPixel => "Not assigned",
            Unsupported::BeyondTheLimit => "Not assigned",
            Unsupported::OtherFontKinds => "Not assigned",
            Unsupported::BackgroundOrPaddingNotOnBox => "Not assigned",
            Unsupported::Rect => "Not assigned",
            Unsupported::AlphaOutsideBoxBackground => "Not assigned",
            Unsupported::SoundBitRate => "Not assigned",
            Unsupported::SpanTextAlign => "Not assigned",
            Unsupported::PictureMotion => "Sprint 1, the order picture-motion",
            Unsupported::SpanWindowAndDressing => "Sprint 1, the order span-forms",
            Unsupported::ItemFormsOfLeaf => "Sprint 1, the order item-forms",
            Unsupported::SpanRoomForm => "Not assigned",
            Unsupported::SpanRoomWithoutPlace => "Not assigned",
            Unsupported::TextPercentInFlow => "Not assigned",
            Unsupported::EmojiPicture => "Not assigned",
            Unsupported::InnerShadow => "Sprint 5",
            Unsupported::SpreadShadow => "Sprint 5",
            Unsupported::AlphaShadow => "Not assigned",
        }
    }

    /// The message of a refusal of this entry: what the author wrote, then the entry, its reason and the sprint it
    /// waits for, or "not yet planned" for an entry whose sprint is not assigned (`docs/spec/check-command.md` section 3d).
    pub fn message(self, what: &str) -> String {
        let waits = match self.waits() {
            "Not assigned" => "not yet planned".to_string(),
            sprint => format!("waits until {sprint}"),
        };
        format!(
            "{what}, which this version does not support (support table {}: {}; {waits})",
            self.name(),
            self.reason()
        )
    }
}

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

/// The elements an author draws with: the ones that take the forms of a place, a layout and a turn.
fn drawn(element: Element) -> bool {
    matches!(
        element,
        Element::Box | Element::Rect | Element::Text | Element::Span | Element::Image | Element::Video | Element::Shape | Element::Nested
    )
}

/// The entry that names an attribute `element` does not take, as the table's column "How `check` recognises it" words it, when
/// one does (S04, S05, S06 to S10, S13 to S15, S19 to S22, S24, S25).
pub(crate) fn attribute(element: Element, name: &str) -> Option<Unsupported> {
    // A property is an attribute of the element tables that is not a time: a clip's `start` and `duration` are not changed
    // over time, and a `<set>` that writes one is a mistake in its own form.
    let property = |name: &str| !matches!(name, "start" | "duration") && Element::ALL.iter().any(|element| element.takes(name));
    let found = match (element, name) {
        (Element::Composition, "canvas") => Unsupported::CanvasPreset,
        (Element::Tween, "stagger") => Unsupported::Stagger,
        (Element::Tween | Element::Set, "label") => Unsupported::Labels,
        (Element::Tween, "repeat" | "yoyo") => Unsupported::Repeats,
        (Element::Tween, "motion-path") => Unsupported::MotionPath,
        (Element::Tween, _) => {
            let changed = name.strip_prefix("from-").or_else(|| name.strip_prefix("to-"))?;
            if !property(changed) {
                return None;
            }
            Unsupported::AnimatedSizeAndColour
        }
        (Element::Set, _) if property(name) => Unsupported::AnimatedSizeAndColour,
        (Element::Video | Element::Audio, "media-end") => Unsupported::MediaEnd,
        (Element::Video | Element::Audio, "playback-rate") => Unsupported::PlaybackRate,
        (Element::Video, "volume" | "muted") => Unsupported::FootageSound,
        (Element::Video | Element::Audio, "loop" | "hold") => Unsupported::LoopOrHold,
        (Element::Video, "object-fit" | "object-position") => Unsupported::FittingFootage,
        (Element::Text | Element::Span, "direction" | "dir") => Unsupported::RightToLeft,
        (_, "z-index") if drawn(element) => Unsupported::StackingOrder,
        (_, "flex-wrap" | "flex-basis" | "flex-shrink" | "order" | "display" | "grid-template-columns" | "row-gap" | "column-gap")
            if drawn(element) =>
        {
            Unsupported::RestOfFlexAndGrid
        }
        (_, "border-top" | "border-right" | "border-bottom" | "border-left" | "border-width" | "border-color" | "border-style")
            if drawn(element) =>
        {
            Unsupported::BordersBySide
        }
        (_, "backface-visibility") if drawn(element) => Unsupported::BackfaceVisibility,
        _ => return None,
    };
    Some(found)
}

/// The entry that names a time written in a form the notation does not list, when one does: a `start` or an `at` that
/// begins with a letter or `@` (S01, S14), and a `duration` of `auto` (S02).
pub(crate) fn time(element: Element, name: &str, value: &str) -> Option<Unsupported> {
    let named = value
        .trim()
        .starts_with(|character: char| character.is_alphabetic() || character == '@');
    match name {
        "duration" if value.trim() == "auto" => Some(Unsupported::AutoDuration),
        "at" if named && matches!(element, Element::Tween | Element::Set) => Some(Unsupported::Labels),
        "start" if named => Some(Unsupported::RelativeStart),
        _ => None,
    }
}

/// The entry that names a value of a `<tween>` or a `<set>` that begins with `=` (S23).
pub(crate) fn expression(element: Element, value: &str) -> Option<Unsupported> {
    (matches!(element, Element::Tween | Element::Set) && value.trim().starts_with('=')).then_some(Unsupported::Expressions)
}

/// The entry that names a number written as a percentage where the notation has a number of pixels: a `gap` (S55).
pub(crate) fn percentage(name: &str, value: &str) -> Option<Unsupported> {
    (name == "gap" && value.trim().ends_with('%')).then_some(Unsupported::GapPercentage)
}

/// The entry that names an `ease` that is not on the list of section 8.4 and is built from the names of the easings that
/// wait (S16).
pub(crate) fn ease(value: &str) -> Option<Unsupported> {
    let value = value.trim();
    let bare = value
        .strip_prefix("in-out-")
        .or_else(|| value.strip_prefix("in-"))
        .or_else(|| value.strip_prefix("out-"))
        .unwrap_or(value);
    // A curve of the names that wait, alone or after `in-`, `out-` or `in-out-`; a value that merely begins so, such as
    // `out-back()`, is not one and is reported as an easing not known; so is a bare `out-back`, which is the notation's
    // `out-back(n)` without its number.
    let built = (["bounce", "elastic", "back", "circ"].contains(&bare) && value != "out-back")
        || value.starts_with("steps(")
        || value.starts_with("cubic-bezier(");
    built.then_some(Unsupported::MoreEasings)
}

/// The entry that names an element the notation does not have, when one does: `<keyframes>` in an element (S12).
pub(crate) fn element(name: &str, parent: Element) -> Option<Unsupported> {
    (name == "keyframes" && parent != Element::Composition).then_some(Unsupported::Keyframes)
}

/// The entry that names a frame rate written as a ratio (S03).
pub(crate) fn frame_rate(value: &str) -> Option<Unsupported> {
    value.contains('/').then_some(Unsupported::RatioFrameRate)
}

/// The entry that names a `src` that begins with a scheme and `://` (S31).
pub fn remote(src: &str) -> Option<Unsupported> {
    let (scheme, rest) = src.trim().split_once("://")?;
    let scheme_form = scheme.starts_with(|character: char| character.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.'));
    (scheme_form && !rest.is_empty()).then_some(Unsupported::RemoteSource)
}
