//! The test font made by us (decision A8): one family of two faces, Regular and Bold, drawn from
//! strokes of our own. It is a development fixture. The files checked in under `fixtures/fonts/` are
//! written by `examples/write_test_font.rs`; no product, starter or command reads them. The facts and
//! the method are in `docs/spec/test-font.md`.

mod design;
mod outline;
mod sfnt;

/// The name of the family, written in the name table of both faces.
pub const FAMILY: &str = "Framesmith Test Stroke";

/// One of the two faces of the family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// Weight class 400, with strokes 80 units wide.
    Regular,
    /// Weight class 700, with strokes 140 units wide.
    Bold,
}

impl Face {
    /// Both faces, Regular first.
    pub const ALL: [Face; 2] = [Face::Regular, Face::Bold];

    /// The width of a stroke, in thousandths of the em.
    pub fn stroke(self) -> i32 {
        match self {
            Face::Regular => 80,
            Face::Bold => 140,
        }
    }

    /// The OS/2 weight class.
    pub fn weight_class(self) -> u16 {
        match self {
            Face::Regular => 400,
            Face::Bold => 700,
        }
    }

    /// The style name, `Regular` or `Bold`.
    pub fn style(self) -> &'static str {
        match self {
            Face::Regular => "Regular",
            Face::Bold => "Bold",
        }
    }

    /// The file name the face is checked in under, in `fixtures/fonts/`.
    pub fn file_name(self) -> &'static str {
        match self {
            Face::Regular => "framesmith-test-stroke-400.ttf",
            Face::Bold => "framesmith-test-stroke-700.ttf",
        }
    }

    /// The bytes of the TrueType file of the face.
    pub fn bytes(self) -> Vec<u8> {
        sfnt::build(self)
    }
}

/// The characters the font maps: U+0020, then the printable ASCII characters U+0021 to U+007E.
pub fn mapped_characters() -> impl Iterator<Item = char> {
    (0x20u8..=0x7E).map(char::from)
}
