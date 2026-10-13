//! The codes of the problems the reader reports: `docs/spec/check-command.md` section 4.
//!
//! This is the one place where a code is named. A code is `FS-` and four digits; it names the kind
//! of a problem and never changes its meaning or is given to another one.

use std::fmt;

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
}

impl Code {
    /// Every code, in the order of the table of the spec.
    pub const ALL: [Code; 12] = [
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
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
