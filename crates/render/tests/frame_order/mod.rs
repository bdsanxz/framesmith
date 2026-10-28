//! What the order proof shares (`docs/spec/render-command.md`, section 8): the scene of the fixture `frame-order`, the
//! marks of its frames drawn in a given order with one renderer, the orders asked for, the comparison that names the first
//! frame whose mark differs from its mark in order, and the control that depends on drawing order on purpose.

#![allow(dead_code)]

use draw::{DisplayList, Error, Frame, Picture, Renderer};
use project::Project;
use render::Scene;

use crate::common::project_copied_from_fixture;

/// The scene of the fixture `frame-order`, copied into a folder of its own under `group`.
pub fn scene(group: &str) -> Scene {
    let folder = project_copied_from_fixture(group, "frame-order", "frame-order");
    Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

/// The frame number and mark of each frame of `numbers`, drawn in that order by the one renderer given.
pub fn marks(scene: &Scene, renderer: &mut dyn Renderer, numbers: &[u64]) -> Vec<(u64, u64)> {
    numbers
        .iter()
        .map(|&number| (number, scene.picture(number, renderer).unwrap().mark()))
        .collect()
}

/// Every frame in order.
pub fn in_order(scene: &Scene) -> Vec<u64> {
    (0..scene.frames()).collect()
}

/// Every frame in reverse order.
pub fn reversed(scene: &Scene) -> Vec<u64> {
    (0..scene.frames()).rev().collect()
}

/// Frames of `frame-order` in a mixed order: from the end to the start and back, frame 21 asked for three times and 59
/// twice, and jumps forward and back across the starts of the green clip (frame 20) and the purple clip (frame 36).
pub const MIXED: [u64; 16] = [59, 3, 21, 19, 20, 44, 45, 36, 35, 0, 21, 59, 30, 21, 36, 19];

/// `Ok` when each frame asked for has the mark it has in order; otherwise the first frame, in the order asked, whose mark
/// differs, with both marks.
pub fn same_as_in_order(in_order: &[u64], asked: &[(u64, u64)]) -> Result<(), String> {
    for &(number, mark) in asked {
        let expected = in_order[number as usize];
        if mark != expected {
            return Err(format!("frame {number:04}: mark {mark:016x} as asked, {expected:016x} in order"));
        }
    }
    Ok(())
}

/// The control: a renderer made to depend on drawing order on purpose. It draws as the renderer it wraps, then writes into
/// the red of the top left pixel the lowest byte of the mark of the picture it drew before, so a frame's picture depends on
/// the frame drawn before it. The proof must fail on it.
pub struct CarriesAValue<R: Renderer> {
    pub inner: R,
    before: Option<u8>,
}

impl<R: Renderer> CarriesAValue<R> {
    pub fn new(inner: R) -> Self {
        CarriesAValue { inner, before: None }
    }

    fn carry(&mut self, picture: Picture) -> Result<Picture, Error> {
        let drawn = picture.mark() as u8;
        let mut pixels = picture.pixels().to_vec();
        if let Some(before) = self.before {
            pixels[0] = before;
        }
        self.before = Some(drawn);
        Picture::new(picture.width(), picture.height(), pixels).ok_or_else(|| Error::new("the picture could not be made"))
    }
}

impl<R: Renderer> Renderer for CarriesAValue<R> {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn draw(&mut self, list: &DisplayList) -> Result<Picture, Error> {
        let picture = self.inner.draw(list)?;
        self.carry(picture)
    }

    fn draws_text(&self) -> bool {
        self.inner.draws_text()
    }

    fn draw_frame(&mut self, frame: &Frame) -> Result<Picture, Error> {
        let picture = self.inner.draw_frame(frame)?;
        self.carry(picture)
    }
}
