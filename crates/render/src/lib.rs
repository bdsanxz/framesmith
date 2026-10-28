//! The render job (`docs/spec/render-command.md`): a project is made into a [`Scene`] that this
//! version can draw, any frame of the scene is drawn on request, and the frames in order are written
//! as a film through frx7.
//!
//! The picture of a frame is a pure function of the scene and the frame's number. A project that
//! uses what this version does not draw or apply has no scene: [`Scene::from_project`] gives the
//! reports of everything it refuses, before the first frame.

mod film;
mod footage;
mod sound;

pub use film::{
    check_place, formats, remove_stale_staging, staging_folder_of, staging_of, write_film, write_film_marked, write_film_stepped, Failure,
    Refusal, Step, Written,
};
pub use sound::open_sounds;
pub use timeline::{place_reports, place_reports_and_notes, sounds, Footage, FootageSource, PictureError, Scene, Sound};

/// The codec registry a scene is made with: frx7's codecs with the system's backend registered, so that the emoji pictures of
/// a font collection can be read (`docs/spec/text.md`, section 12.4, point 4). A program calls this once, where a command
/// starts, and hands the registry to every scene it makes; no other crate registers the system's backend for a scene.
pub fn system_codecs() -> codec::Registry {
    let mut codecs = codec::Registry::new();
    codecs.register(backend_os::BACKEND);
    codecs
}

pub use footage::Sources;
