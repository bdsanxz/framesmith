//! Checked-in pictures held to their worked values. A worked picture is a PNG in the fixture's `worked` folder, one per
//! frame, `frame-NNNN.png`, whose alpha marks the pixels held: 255 where the picture is held to the worked level, 0 where
//! the spec leaves it to another ruling. A held pixel of the processor's frame lies within the tolerance of the worked level in
//! each channel.

use super::{fixtures, png, project_copied_from_fixture};
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

/// The processor's frame `number` of a fixture, drawn from a copy of the fixture's composition.
pub fn drawn(group: &str, fixture: &str, number: u64) -> Picture {
    let folder = project_copied_from_fixture(group, fixture, fixture);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(number, &mut CpuRenderer).unwrap()
}

/// The worked picture of frame `number` of a fixture, and which of its pixels are held.
pub fn worked(fixture: &str, number: u64) -> (Picture, Vec<bool>) {
    let path = fixtures().join(fixture).join("worked").join(format!("frame-{number:04}.png"));
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let (width, height, pixels) = png::decode(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let held = pixels.chunks(4).map(|pixel| pixel[3] == 255).collect();
    let opaque = pixels.chunks(4).flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255]).collect();
    (Picture::new(width, height, opaque).unwrap(), held)
}

/// The held pixels of `drawn` within `tolerance` levels of `worked`, in each channel. The error names the worst pixel.
pub fn within_levels_where_held(drawn: &Picture, worked: &Picture, held: &[bool], tolerance: u8) -> Result<usize, String> {
    assert_eq!((drawn.width(), drawn.height()), (worked.width(), worked.height()));
    let mut checked = 0;
    let mut worst = (0u8, 0u32, 0u32);
    for y in 0..drawn.height() {
        for x in 0..drawn.width() {
            if !held[(y * drawn.width() + x) as usize] {
                continue;
            }
            checked += 1;
            let (a, b) = (drawn.pixel(x, y).unwrap(), worked.pixel(x, y).unwrap());
            let difference = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap();
            if difference > worst.0 {
                worst = (difference, x, y);
            }
        }
    }
    if worst.0 > tolerance {
        let (x, y) = (worst.1, worst.2);
        return Err(format!(
            "at ({x}, {y}) the drawn pixel is {:?}, the worked one {:?}: {} levels apart, more than {tolerance}",
            drawn.pixel(x, y).unwrap(),
            worked.pixel(x, y).unwrap(),
            worst.0
        ));
    }
    Ok(checked)
}
