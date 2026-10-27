//! What is shown at a frame (`docs/spec/timeline.md`): a project is made into a [`Scene`] that this
//! version can draw, and any frame of the scene is made into a display list or drawn on request.
//! The crate touches no system interface.

mod background;
mod edges;
mod moving;
mod paragraph;
mod scene;
mod shadows;
mod sound;

pub use scene::{place_reports, place_reports_and_notes, PictureError, Scene};
pub use sound::{sounds, Sound};
