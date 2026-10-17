//! Paragraphs of text (`docs/spec/text.md`). Fonts come only from the `<font>` elements of a composition:
//! a [`Fonts`] collection is made with no font of the machine in it, and a font file is registered in it
//! under its family name. A paragraph is shaped, broken into lines and placed by [`layout`], which gives
//! the glyph runs of the lines with their places. No type of the engine is part of this crate's interface.

mod fonts;
mod paragraph;

pub use fonts::{FontError, Fonts};
pub use paragraph::{layout, layout_fitted, Align, Block, Line, LineHeight, PositionedGlyph, PositionedRun, Style, TextError};
