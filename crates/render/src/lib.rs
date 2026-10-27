//! The render job (`docs/spec/render-command.md`): a project is made into a [`Scene`] that this
//! version can draw, any frame of the scene is drawn on request, and the frames in order are written
//! as a film through frx7.
//!
//! The picture of a frame is a pure function of the scene and the frame's number. A project that
//! uses what this version does not draw or apply has no scene: [`Scene::from_project`] gives the
//! reports of everything it refuses, before the first frame.

mod film;
mod sound;

pub use film::{
    check_place, process_runs, remove_stale_staging, staging_folder_of, staging_of, write_film, write_film_marked, Failure,
    Refusal, Written,
};
pub use sound::open_sounds;
pub use timeline::{place_reports, place_reports_and_notes, sounds, PictureError, Scene, Sound};
