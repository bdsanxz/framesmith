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
}

impl Code {
    /// Every code, in the order of the table of the catalogue.
    pub const ALL: [Code; 26] = [
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
        }
    }

    /// The kind of the code.
    pub fn kind(self) -> Kind {
        match self {
            Code::CompositionUnreadable | Code::ProjectFileUnreadable | Code::NamedCompositionUnreadable => Kind::Failure,
            Code::KeyNotSupported => Kind::Unsupported,
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
            Code::NotAFolder => "Give the folder that holds `composition.xml`, not a file.",
            Code::NoComposition => "Put a `composition.xml` in the folder, or give the folder that holds one.",
            Code::CompositionUnreadable => "Make `composition.xml` a file this user may read, then run the command again.",
            Code::CommandLineNotRead => "Run `framesmith check <folder>` with the folder of one project.",
            Code::ProjectFileNotUtf8 => "Save the project file as UTF-8 text.",
            Code::ProjectFileNotYaml => "Correct the YAML at the line given: indent with spaces and write each key as `key: value`.",
            Code::ProjectFileUnreadable => "Make `framesmith.yaml` a file this user may read, then run the command again.",
            Code::ProjectFileNotAMapping => "Make the file one mapping of keys, such as `name: launch-film`, or leave it empty.",
            Code::KeyNotListed => "Remove the key, or write it as one of the keys the message lists.",
            Code::KeyNotSupported => "Remove the key, or wait for the sprint that brings it.",
            Code::ValueNotAccepted => "Write the value in the form the message gives.",
            Code::NamedCompositionMissing => "Set `composition` to the path of a composition file that exists, or put the file there.",
            Code::NamedCompositionUnreadable => "Make the composition file one this user may read, then run the command again.",
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
