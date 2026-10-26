//! Paragraphs of text (`docs/spec/text.md`). Fonts come from the `<font>` elements of a composition, and on macOS from
//! the machine's font folders (section 2.7): a [`Fonts`] collection is made with no font in it, a font file is registered
//! in it under its family name, and a face of the machine joins it when a text names its family or the fall-back of
//! section 2.4 takes it. A paragraph is shaped, broken into lines and placed by [`layout`], which gives
//! the glyph runs of the lines with their places. No type of the engine is part of this crate's interface.

mod colour;
mod damage;
mod fonts;
mod machine;
mod paragraph;
mod picture;
mod tables;

pub use fonts::{one_face, FontError, Fonts, MachineFace};
pub use paragraph::{
    layout, layout_fitted, layout_fitted_pieces, layout_pieces, Align, Block, Line, LineHeight, Piece, PositionedGlyph, PositionedPicture, Source,
    PositionedRun, Style, TextError,
};
