//! Paragraphs of text (`docs/spec/text.md`). Fonts come only from the `<font>` elements of a composition:
//! a [`Fonts`] collection is made with no font of the machine in it, and a font file is registered in it
//! under its family name. A paragraph is shaped, broken into lines and placed by [`layout`], which gives
//! the glyph runs of the lines with their places. No type of the engine is part of this crate's interface.

mod colour;
mod fonts;
mod paragraph;
mod picture;
mod tables;

pub use fonts::{FontError, Fonts};
pub use paragraph::{
    layout, layout_fitted, layout_fitted_pieces, layout_pieces, Align, Block, Line, LineHeight, Piece, PositionedGlyph, PositionedPicture,
    PositionedRun, Style, TextError,
};
