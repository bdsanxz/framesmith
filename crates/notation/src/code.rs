//! The codes of the refusals and failures, with their kinds and remedies:
//! `docs/spec/error-catalogue.md`.
//!
//! This is the one place where a code is named. A code is `FS-` and four digits; it names the kind
//! of a problem and never changes its meaning or is given to another one. The remedy sentence and
//! the kind of each code are fixed in the catalogue, and a test holds this file to it.

use std::fmt;

/// The three kinds of a refusal or failure (`docs/spec/error-catalogue.md` section 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// What the author gave is wrong: a value, a file or a command line that is not accepted.
    Invalid,
    /// What the author gave is known and is not supported in this version.
    Unsupported,
    /// What the author gave may be right, and the program or the machine could not carry it out.
    Failure,
}

impl Kind {
    /// Every kind, in the order of the catalogue.
    pub const ALL: [Kind; 3] = [Kind::Invalid, Kind::Unsupported, Kind::Failure];

    /// The kind as it is printed: one lower-case word.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Invalid => "invalid",
            Kind::Unsupported => "unsupported",
            Kind::Failure => "failure",
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The code of one kind of problem.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Code {
    /// The file is not valid UTF-8 text.
    NotUtf8,
    /// The file is not well-formed XML.
    NotXml,
    /// The root element is not `<composition>`.
    RootNotComposition,
    /// The root has no `version`.
    VersionMissing,
    /// The root's `version` is not one that is read.
    VersionNotRead,
    /// The root has no `width` or no `height`.
    SizeMissing,
    /// The root's `width` or `height` is not an even whole number from 2 to 8192.
    SizeNotAccepted,
    /// The root's `fps` is not a whole number from 1 to 240.
    FpsNotAccepted,
    /// The root has no `duration`.
    DurationMissing,
    /// The root's `duration` is not a time.
    DurationNotATime,
    /// The root's `duration` is not above 0.
    DurationNotAboveZero,
    /// The root's `background` is not one colour.
    BackgroundNotAColour,
    /// The folder given does not exist.
    FolderMissing,
    /// The path given is not a folder.
    NotAFolder,
    /// The folder holds no `composition.xml`.
    NoComposition,
    /// The `composition.xml` of the folder could not be read.
    CompositionUnreadable,
    /// The command line cannot be read.
    CommandLineNotRead,
    /// The project file is not valid UTF-8 text.
    ProjectFileNotUtf8,
    /// The project file is not well-formed YAML.
    ProjectFileNotYaml,
    /// The `framesmith.yaml` of the folder exists and could not be read.
    ProjectFileUnreadable,
    /// The project file is not one mapping of keys.
    ProjectFileNotAMapping,
    /// A key of the project file is not one that the project file has.
    KeyNotListed,
    /// A key of the project file is known and is not supported in this version.
    KeyNotSupported,
    /// A value in the project file is of the wrong kind or outside the values its key takes.
    ValueNotAccepted,
    /// The composition named by the project file does not exist.
    NamedCompositionMissing,
    /// The composition named by the project file exists and could not be read.
    NamedCompositionUnreadable,
    /// The path given to the command does not exist.
    PathMissing,
    /// The path given to the command exists and could not be read.
    PathUnreadable,
    /// An element is not one the notation has.
    ElementNotKnown,
    /// An element has an attribute that it does not take.
    AttributeNotTaken,
    /// An element stands where the notation does not allow it.
    ElementNotHere,
    /// A value that must be a number is not one.
    NotANumber,
    /// A value that must be a time in seconds is not one.
    NotATime,
    /// A length is not a number of pixels or a percentage, or is below 0 where 0 or more is taken.
    LengthNotAccepted,
    /// The project uses a form that is known and that this version does not draw or apply.
    FormNotSupported,
    /// The video could not be made.
    VideoNotMade,
    /// An option of `render`, or its value, is not one the command takes.
    OptionNotTaken,
    /// The author asked for the graphics card (`gpu`), and this machine has no card that frx7 can draw with.
    RendererNotAvailable,
    /// A value that must be one of a list of words (`position`) is not one of them.
    WordNotTaken,
    /// An element that takes `start` and `duration` as a clip has one of them without the other.
    ClipNotPaired,
    /// A `<style>` of the file has no `id`, so no element can take it.
    StyleWithoutId,
    /// Two `<style>` elements of the file have one `id`.
    StyleIdTwice,
    /// An element's `class` names a style that the file does not have.
    ClassNamesNoStyle,
    ShownLoop,
    ShownTooDeep,
    ShownFpsDiffers,
    ShownMissing,
    ShownUnreadable,
    /// The font file that `<font src>` names does not exist, at the path it gives from the project folder or, when it
    /// begins with `/`, on the machine.
    FontMissing,
    /// The font file exists and could not be read.
    FontUnreadable,
    /// The file is not a font of a kind this version reads: TrueType (`.ttf`) or font collection (`.ttc`).
    FontNotAFont,
    /// A text attribute's value is not in the form the notation gives for it.
    ValueNotInForm,
    /// A length or size that must be above 0 is 0 or below.
    SizeNotAboveZero,
    /// A `columns` of a `<box>` is not a list of column widths.
    ColumnsNotAList,
    /// A `<box>` has both `columns` and `flex-direction`.
    ColumnsWithDirection,
    /// A character of a text whose picture this version does not draw.
    EmojiNotDrawn,
    /// A font file of a kind this version reads that the font reader refuses as damaged.
    FontDamaged,
    /// A `<tween>` without a `duration` above 0.
    TweenNoDuration,
    /// A `<tween>` with no `from-` or `to-` attribute.
    TweenNoProperty,
    /// A `<tween>` changes a property that the element it moves does not take.
    PropertyNotTaken,
    /// A `<set>` with no property.
    SetNoProperty,
    /// A `<set>` changes a property that the element it moves does not take.
    SetPropertyNotTaken,
    /// The `ease` of a `<tween>` is not an easing the notation takes.
    EaseNotKnown,
    /// A background layer, or a size, place or repeat of one, is not in its form.
    BackgroundNotInForm,
    /// A list of background settings has neither one entry nor one for each layer.
    BackgroundListLength,
    /// A background setting is written on an element with no background.
    BackgroundSettingAlone,
    /// A `border` is not a width and a colour, or a width, `double` and a colour.
    BorderNotInForm,
    /// A `box-shadow` or `drop-shadow` is not `X Y BLUR COLOUR`, a list of them for `box-shadow`.
    ShadowNotInForm,
    /// The folder of the output path cannot be made or written, so `render` refuses the video before the first frame.
    OutputNotWritten,
    /// The machine's own encoder does not take the film's size.
    HardwareSizeNotTaken,
    /// The system's encoder on the processor does not take the film's size.
    SoftwareSizeNotTaken,
    /// An `<audio>` without a `duration` above 0.
    SoundNoDuration,
    /// The sound file that `<audio src>` names does not exist, or the `<audio>` has no `src`.
    SoundMissing,
    /// The sound file exists and could not be read.
    SoundUnreadable,
    /// The file is not sound of a kind this version reads, or is of such a kind in a form it does not read.
    SoundNotRead,
    /// The sound file is of a kind this version reads and cannot be decoded.
    SoundDamaged,
    /// An `<audio>` has a `media-start` at or past the end of its sound file.
    SoundStartPastEnd,
    /// A number that counts is not written as the notation gives it: a `count-format` that is not one of its forms or that
    /// has no `count`, words or a `<span>` in a `<text>` that counts, or a tween or set of `count` on a text without it.
    CountNotWritten,
    /// A `<video>` has no `width` or no `height`, or one below 2.
    VideoSize,
    /// A `<video>` has no `src`.
    VideoSourceMissing,
    /// A `media-start` on a `<video>` below 0.
    VideoMediaStartNegative,
    /// A `media-start` on a `<video>` at or past the end of its source.
    VideoMediaStartPastEnd,
}

impl Code {
    /// Every code, in the order of the table of the catalogue.
    pub const ALL: [Code; 82] = [
        Code::NotUtf8,
        Code::NotXml,
        Code::RootNotComposition,
        Code::VersionMissing,
        Code::VersionNotRead,
        Code::SizeMissing,
        Code::SizeNotAccepted,
        Code::FpsNotAccepted,
        Code::DurationMissing,
        Code::DurationNotATime,
        Code::DurationNotAboveZero,
        Code::BackgroundNotAColour,
        Code::FolderMissing,
        Code::NotAFolder,
        Code::NoComposition,
        Code::CompositionUnreadable,
        Code::CommandLineNotRead,
        Code::ProjectFileNotUtf8,
        Code::ProjectFileNotYaml,
        Code::ProjectFileUnreadable,
        Code::ProjectFileNotAMapping,
        Code::KeyNotListed,
        Code::KeyNotSupported,
        Code::ValueNotAccepted,
        Code::NamedCompositionMissing,
        Code::NamedCompositionUnreadable,
        Code::PathMissing,
        Code::PathUnreadable,
        Code::ElementNotKnown,
        Code::AttributeNotTaken,
        Code::ElementNotHere,
        Code::NotANumber,
        Code::NotATime,
        Code::LengthNotAccepted,
        Code::FormNotSupported,
        Code::VideoNotMade,
        Code::OptionNotTaken,
        Code::RendererNotAvailable,
        Code::WordNotTaken,
        Code::ClipNotPaired,
        Code::StyleWithoutId,
        Code::StyleIdTwice,
        Code::ClassNamesNoStyle,
        Code::ShownLoop,
        Code::ShownTooDeep,
        Code::ShownFpsDiffers,
        Code::ShownMissing,
        Code::ShownUnreadable,
        Code::FontMissing,
        Code::FontUnreadable,
        Code::FontNotAFont,
        Code::ValueNotInForm,
        Code::SizeNotAboveZero,
        Code::ColumnsNotAList,
        Code::ColumnsWithDirection,
        Code::EmojiNotDrawn,
        Code::FontDamaged,
        Code::TweenNoDuration,
        Code::TweenNoProperty,
        Code::PropertyNotTaken,
        Code::SetNoProperty,
        Code::SetPropertyNotTaken,
        Code::EaseNotKnown,
        Code::BackgroundNotInForm,
        Code::BackgroundListLength,
        Code::BackgroundSettingAlone,
        Code::BorderNotInForm,
        Code::ShadowNotInForm,
        Code::OutputNotWritten,
        Code::HardwareSizeNotTaken,
        Code::SoftwareSizeNotTaken,
        Code::SoundNoDuration,
        Code::SoundMissing,
        Code::SoundUnreadable,
        Code::SoundNotRead,
        Code::SoundDamaged,
        Code::SoundStartPastEnd,
        Code::CountNotWritten,
        Code::VideoSize,
        Code::VideoSourceMissing,
        Code::VideoMediaStartNegative,
        Code::VideoMediaStartPastEnd,
    ];

    /// The code as it is printed: `FS-` and four digits.
    pub fn as_str(self) -> &'static str {
        match self {
            Code::NotUtf8 => "FS-0001",
            Code::NotXml => "FS-0002",
            Code::RootNotComposition => "FS-0003",
            Code::VersionMissing => "FS-0004",
            Code::VersionNotRead => "FS-0005",
            Code::SizeMissing => "FS-0006",
            Code::SizeNotAccepted => "FS-0007",
            Code::FpsNotAccepted => "FS-0008",
            Code::DurationMissing => "FS-0009",
            Code::DurationNotATime => "FS-0010",
            Code::DurationNotAboveZero => "FS-0011",
            Code::BackgroundNotAColour => "FS-0012",
            Code::FolderMissing => "FS-0013",
            Code::NotAFolder => "FS-0014",
            Code::NoComposition => "FS-0015",
            Code::CompositionUnreadable => "FS-0016",
            Code::CommandLineNotRead => "FS-0017",
            Code::ProjectFileNotUtf8 => "FS-0018",
            Code::ProjectFileNotYaml => "FS-0019",
            Code::ProjectFileUnreadable => "FS-0020",
            Code::ProjectFileNotAMapping => "FS-0021",
            Code::KeyNotListed => "FS-0022",
            Code::KeyNotSupported => "FS-0023",
            Code::ValueNotAccepted => "FS-0024",
            Code::NamedCompositionMissing => "FS-0025",
            Code::NamedCompositionUnreadable => "FS-0026",
            Code::PathMissing => "FS-0027",
            Code::PathUnreadable => "FS-0028",
            Code::ElementNotKnown => "FS-0029",
            Code::AttributeNotTaken => "FS-0030",
            Code::ElementNotHere => "FS-0031",
            Code::NotANumber => "FS-0032",
            Code::NotATime => "FS-0033",
            Code::LengthNotAccepted => "FS-0034",
            Code::FormNotSupported => "FS-0035",
            Code::VideoNotMade => "FS-0036",
            Code::OptionNotTaken => "FS-0037",
            Code::RendererNotAvailable => "FS-0038",
            Code::WordNotTaken => "FS-0039",
            Code::ClipNotPaired => "FS-0040",
            Code::StyleWithoutId => "FS-0041",
            Code::StyleIdTwice => "FS-0042",
            Code::ClassNamesNoStyle => "FS-0043",
            Code::ShownLoop => "FS-0044",
            Code::ShownTooDeep => "FS-0045",
            Code::ShownFpsDiffers => "FS-0046",
            Code::ShownMissing => "FS-0047",
            Code::ShownUnreadable => "FS-0048",
            Code::FontMissing => "FS-0049",
            Code::FontUnreadable => "FS-0050",
            Code::FontNotAFont => "FS-0051",
            Code::ValueNotInForm => "FS-0052",
            Code::SizeNotAboveZero => "FS-0053",
            Code::ColumnsNotAList => "FS-0054",
            Code::ColumnsWithDirection => "FS-0055",
            Code::EmojiNotDrawn => "FS-0056",
            Code::FontDamaged => "FS-0057",
            Code::TweenNoDuration => "FS-0058",
            Code::TweenNoProperty => "FS-0059",
            Code::PropertyNotTaken => "FS-0060",
            Code::SetNoProperty => "FS-0061",
            Code::SetPropertyNotTaken => "FS-0062",
            Code::EaseNotKnown => "FS-0063",
            Code::BackgroundNotInForm => "FS-0064",
            Code::BackgroundListLength => "FS-0065",
            Code::BackgroundSettingAlone => "FS-0066",
            Code::BorderNotInForm => "FS-0067",
            Code::ShadowNotInForm => "FS-0070",
            Code::OutputNotWritten => "FS-0072",
            Code::HardwareSizeNotTaken => "FS-0073",
            Code::SoftwareSizeNotTaken => "FS-0074",
            Code::SoundNoDuration => "FS-0076",
            Code::SoundMissing => "FS-0077",
            Code::SoundUnreadable => "FS-0078",
            Code::SoundNotRead => "FS-0079",
            Code::SoundDamaged => "FS-0080",
            Code::SoundStartPastEnd => "FS-0081",
            Code::CountNotWritten => "FS-0084",
            Code::VideoSize => "FS-0085",
            Code::VideoSourceMissing => "FS-0086",
            Code::VideoMediaStartNegative => "FS-0089",
            Code::VideoMediaStartPastEnd => "FS-0090",
        }
    }

    /// The kind of the code.
    pub fn kind(self) -> Kind {
        match self {
            Code::CompositionUnreadable
            | Code::ProjectFileUnreadable
            | Code::NamedCompositionUnreadable
            | Code::PathUnreadable
            | Code::VideoNotMade
            | Code::OutputNotWritten
            | Code::ShownUnreadable
            | Code::FontUnreadable
            | Code::SoundUnreadable => Kind::Failure,
            Code::KeyNotSupported
            | Code::FormNotSupported
            | Code::RendererNotAvailable
            | Code::EmojiNotDrawn
            | Code::SoundNotRead
            | Code::HardwareSizeNotTaken
            | Code::SoftwareSizeNotTaken => Kind::Unsupported,
            _ => Kind::Invalid,
        }
    }

    /// The remedy: one sentence on what to change, the same for every report of the code.
    pub fn remedy(self) -> &'static str {
        match self {
            Code::NotUtf8 => "Save the file as UTF-8 text.",
            Code::NotXml => "Correct the XML at the line given: close every element and put every attribute value in quotes.",
            Code::RootNotComposition => "Make the root element of the file `<composition>`.",
            Code::VersionMissing => "Write `version=\"1\"` on the root `<composition>`.",
            Code::VersionNotRead => "Write `version=\"1\"` on the root `<composition>`, the only version read.",
            Code::SizeMissing => "Write `width` and `height` on the root, each an even whole number from 2 to 8192.",
            Code::SizeNotAccepted => "Write the size as an even whole number from 2 to 8192.",
            Code::FpsNotAccepted => "Write `fps` as a whole number from 1 to 240, or leave it out for 30.",
            Code::DurationMissing => "Write `duration` on the root as a number of seconds above 0.",
            Code::DurationNotATime => "Write `duration` as digits with an optional point, such as 5 or 1.25.",
            Code::DurationNotAboveZero => "Write `duration` as a number of seconds above 0.",
            Code::BackgroundNotAColour => "Write `background` as one colour, #rrggbb or #rrggbbaa, or leave it out for black.",
            Code::FolderMissing => "Give the path of a folder that exists.",
            Code::NotAFolder => {
                "Give the path of a folder, or of a `framesmith.yaml` or a composition file; the path of a file has no closing `/`."
            }
            Code::NoComposition => "Put a `composition.xml` in the folder, or give the folder that holds one.",
            Code::CompositionUnreadable => "Make `composition.xml` a file this user may read, then run the command again.",
            Code::CommandLineNotRead => {
                "Run `framesmith check <path>` or `framesmith render <path>` with the folder, the project file or the composition file of one project."
            }
            Code::ProjectFileNotUtf8 => "Save the project file as UTF-8 text.",
            Code::ProjectFileNotYaml => "Correct the YAML at the line given: indent with spaces and write each key as `key: value`.",
            Code::ProjectFileUnreadable => "Make `framesmith.yaml` a file this user may read, then run the command again.",
            Code::ProjectFileNotAMapping => "Make the file one mapping of keys, such as `name: launch-film`, or leave it empty.",
            Code::KeyNotListed => "Remove the key, or write it as one of the keys the message lists.",
            Code::KeyNotSupported => "Remove the key, or wait for the sprint that brings it.",
            Code::ValueNotAccepted => "Write the value in the form the message gives.",
            Code::NamedCompositionMissing => "Set `composition` to the path of a composition file that exists, or put the file there.",
            Code::NamedCompositionUnreadable => "Make the composition file one this user may read, then run the command again.",
            Code::PathMissing => "Give the path of a folder, a `framesmith.yaml` or a composition file that exists.",
            Code::PathUnreadable => "Make the path one this user may read, then run the command again.",
            Code::ElementNotKnown => "Remove the element, or write one of the elements the message names.",
            Code::AttributeNotTaken => "Remove the attribute, or write one the element takes; section 6 of the notation spec lists them.",
            Code::ElementNotHere => "Move the element to a place the message names, or remove it.",
            Code::NotANumber => "Write the value as a plain number, such as 12, -4, .5 or 1e3, with no unit and no percent sign.",
            Code::NotATime => "Write the time as digits with an optional point, such as 5, 0.5 or .5, with no sign, exponent or unit.",
            Code::LengthNotAccepted => {
                "Write the value in a form the attribute takes (`notation.md` section 6), and not below 0 where the message says 0 or more."
            }
            Code::FormNotSupported => "Remove what the message names, or wait for the sprint that brings it.",
            Code::VideoNotMade => "Put right what the message names (the folder, the disk or the system's video encoder), then run the command again.",
            Code::OptionNotTaken => "Give one of the options the message lists, with one of the values it names, or leave the option out.",
            Code::WordNotTaken => "Write one of the words the message names, in lower case, or leave the attribute out.",
            Code::ClipNotPaired => "Write both `start` and `duration` on the element, or neither.",
            Code::StyleWithoutId => "Write an `id` on each `<style>`, the name that elements take it by.",
            Code::StyleIdTwice => "Give each `<style>` of the file its own `id`.",
            Code::ClassNamesNoStyle => "Name a `<style>` of this file in `class`, or write its attributes on the element.",
            Code::ShownLoop => "Take out the `<composition src>` that closes the loop, so that no composition shows itself.",
            Code::ShownTooDeep => "Show fewer compositions in one chain: at most 16 with the root, each shown by the one before it.",
            Code::ShownFpsDiffers => "Write in the shown composition the same `fps` as in the one that shows it, a missing `fps` counting as 30.",
            Code::ShownMissing => "Give the path of a composition file in the project folder, or put the file there.",
            Code::ShownUnreadable => "Make the shown composition file readable by this user, then run the command again.",
            Code::FontMissing => "Give the path of a font file that exists, from the project folder or a path that begins with `/`, or put the file there.",
            Code::FontUnreadable => "Make the font file readable by this user, then run the command again.",
            Code::FontNotAFont => "Give in `src` the path of a TrueType font file (`.ttf`) or a font collection file (`.ttc`).",
            Code::ValueNotInForm => "Write the value in the form the message names.",
            Code::SizeNotAboveZero => "Write the value as a number above 0, or leave the attribute out for its default.",
            Code::ColumnsNotAList => "Write the widths as numbers of pixels, such as 640, or as shares, such as 1fr, separated by spaces.",
            Code::ColumnsWithDirection => "Remove `columns` to lay the children out in a row or a column, or `flex-direction` to lay them out in columns.",
            Code::EmojiNotDrawn => "Give the text a font that has a PNG picture for the character, or remove the character.",
            Code::FontDamaged => "Replace the font file with a copy that is not damaged, such as the one the font came in.",
            Code::TweenNoDuration => "Write `duration` on the tween as a number of seconds above 0.",
            Code::TweenNoProperty => "Write at least one `from-` or `to-` attribute on the tween, such as `to-x=\"100\"`.",
            Code::PropertyNotTaken => "Move the tween to an element that takes the property, or change a property the element takes.",
            Code::SetNoProperty => "Write at least one property on the set, such as `x=\"300\"`.",
            Code::SetPropertyNotTaken => "Move the set to an element that takes the property, or change a property the element takes.",
            Code::EaseNotKnown => "Write one of the easings the message lists, in lower case, such as `ease=\"out-cubic\"`.",
            Code::BackgroundNotInForm => "Write the value in the form the message names.",
            Code::BackgroundListLength => "Write one entry for all the layers, or one for each layer in the order of `background`.",
            Code::BackgroundSettingAlone => "Write a `background` on the element, or remove the setting.",
            Code::BorderNotInForm => "Write the border as a width and a colour, such as `6 #ffffff`, or as a width, `double` and a colour, such as `9 double #ffffff`.",
            Code::ShadowNotInForm => "Write the shadow as an offset across, an offset down, a blur and a colour, such as `10 10 8 #000000`.",
            Code::OutputNotWritten => {
                "Give `--output` a path in a folder that can be written, or make the folder writable, then run the command again."
            }
            Code::HardwareSizeNotTaken => {
                "Set the setting the message names: `render.codec` (or `--codec`) to `hevc` where the message offers it, or `render.encoder` (or `--encoder`) to `software`."
            }
            Code::SoftwareSizeNotTaken => {
                "Make the film smaller, then run the command again."
            }
            Code::SoundNoDuration => "Write `duration` on the `<audio>` as the seconds the sound plays, above 0.",
            Code::SoundMissing => "Write `src` as the path of a sound file that exists, from the project folder or from `/`.",
            Code::SoundUnreadable => "Check that the path names a file, not a folder, and that it may be read.",
            Code::SoundNotRead => "Convert the sound to one of the kinds the message lists, such as WAV of 16-bit samples.",
            Code::SoundDamaged => "Replace the sound file with a copy that is not damaged.",
            Code::SoundStartPastEnd => "Write a `media-start` below the length of the sound file, or leave it out to start at the beginning.",
            Code::CountNotWritten => {
                "Write the counting text as the notation's section 7.12 gives it: `count` on a `<text>` that holds no words, with `count-format` in one of the forms the message lists."
            }
            Code::VideoSize => "Write `width` and `height` on the <video>, each a whole number of pixels of 2 or more.",
            Code::VideoSourceMissing => "Write `src` on the <video>: the path of its file from the project folder.",
            Code::VideoMediaStartNegative => {
                "Write a `media-start` of 0 or more, or leave it out to start at the source's first picture."
            }
            Code::VideoMediaStartPastEnd => {
                "Write a `media-start` below the length of the source file, or leave it out to start at the beginning."
            }
            Code::RendererNotAvailable => {
                "Set `render.backend` (or `--backend`) to `auto` or `cpu`, which draw on the processor when no card is there."
            }
        }
    }

    /// One report of the code as one line of plain text, without the place it is at:
    /// `FS-0007: invalid: <what is wrong>. <remedy>`. The message is the sentence on what is wrong,
    /// with or without its full stop; the remedy is the code's own.
    pub fn statement(self, message: &str) -> String {
        let stop = if message.ends_with('.') { "" } else { "." };
        format!("{self}: {}: {message}{stop} {}", self.kind(), self.remedy())
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
