//! The render job (`docs/spec/render-command.md`): a project is made into a [`Scene`] that this
//! version can draw, any frame of the scene is drawn on request, and the frames in order are written
//! as a film through frx7.
//!
//! The picture of a frame is a pure function of the scene and the frame's number. A project that
//! uses what this version does not draw or apply has no scene: [`Scene::from_project`] gives the
//! reports of everything it refuses, before the first frame.

mod film;

pub use film::{write_film, Failure, Written};
pub use timeline::{place_reports, PictureError, Scene};
