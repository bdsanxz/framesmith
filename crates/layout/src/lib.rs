//! Framesmith text layout: parley shaping and layout over the fonts of a
//! composition and of the machine, with color emoji pictures decoded by
//! FFmpeg.
//!
//! The architecture follows the old engine, with two replacements:
//!
//! * shaping/layout runs on parley (this crate's only layout
//!   dependency); the old engine's `read-fonts` queries are replaced by
//!   the std-only table readers in the `font` crate;
//! * emoji pictures are decoded by the same pinned-or-host FFmpeg binary
//!   that encodes the MP4 (`ffmpeg::picture::decode_memory`), fed the
//!   raw PNG bytes of an `sbix` strike over its stdin — never a linked
//!   media stack (no frx7).
//!
//! A [`Fonts`] collection starts empty: the system's fonts are never
//! read until a text names a family or the fallback takes one. [`layout`]
//! shapes one paragraph in one style and returns a [`Block`] of glyph
//! runs (font bytes, face index, size, skew, color, positioned glyphs)
//! plus decoded emoji pictures; the renderer draws them with the `font`
//! crate's CPU rasterizer.

mod fonts;
mod machine;
mod paragraph;

pub use fonts::{FontError, Fonts, MachineFace};
pub use paragraph::{layout, Align, Block, Glyph, Line, Picture, Run, Style, TextError};
