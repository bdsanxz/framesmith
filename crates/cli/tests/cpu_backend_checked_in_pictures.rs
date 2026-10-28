//! The renderer that `framesmith render --backend cpu` starts draws every frame of the first fixture
//! to its checked-in picture, to the byte (`docs/spec/render-command.md` sections 4 and 7). The
//! settings come from the same options the command reads, and the renderer from `renderer_for`, the
//! function the command calls to start it.
//!
//! The fixture's composition says that all its frames are alike, and it has checked-in pictures for
//! frames 0, 4 and 9 only. A frame with no picture of its own is held to the picture of frame 0.

// The PNG reader of the render crate's tests, read by its path so that the checked-in pictures are
// read with the same code and no outside crate.
#[path = "../../render/tests/common/png.rs"]
mod png;

use std::fs;
use std::path::{Path, PathBuf};

use cli::{renderer_for, Chosen};
use draw::Picture;
use project::{Backend, Options, Project};
use render::Scene;

const FIXTURE: &str = "first-render";

/// The folder of the checked-in fixtures, or the folder `FRAMESMITH_FIXTURES` names.
fn fixtures() -> PathBuf {
    std::env::var_os("FRAMESMITH_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures"))
}

/// The checked-in picture that frame `number` is held to: its own, where the fixture has one.
fn held_to(number: u64) -> PathBuf {
    let folder = fixtures().join(FIXTURE).join("expected");
    let own = folder.join(format!("frame-{number:04}.png"));
    if own.exists() {
        own
    } else {
        folder.join("frame-0000.png")
    }
}

fn read_picture(path: &Path) -> Picture {
    let bytes = fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let (width, height, pixels) = png::decode(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    Picture::new(width, height, pixels).unwrap()
}

/// Holds the picture drawn for frame `number` to the checked-in picture, to the byte. A difference
/// is told with the frame's number, the first pixel that differs and how many pixels differ.
fn compare(number: u64, drawn: &Picture, expected: &Picture) -> Result<(), String> {
    if (drawn.width(), drawn.height()) != (expected.width(), expected.height()) {
        return Err(format!(
            "frame {number}: the picture drawn is {}x{} and the checked-in picture is {}x{}",
            drawn.width(),
            drawn.height(),
            expected.width(),
            expected.height()
        ));
    }
    let differing: Vec<usize> = (0..drawn.pixels().len() / 4)
        .filter(|&at| drawn.pixels()[at * 4..at * 4 + 4] != expected.pixels()[at * 4..at * 4 + 4])
        .collect();
    let Some(&first) = differing.first() else {
        return Ok(());
    };
    let (x, y) = (first as u32 % drawn.width(), first as u32 / drawn.width());
    Err(format!(
        "frame {number}: pixel ({x}, {y}) is {:?} and the checked-in picture has {:?}; {} pixels differ",
        drawn.pixel(x, y).unwrap(),
        expected.pixel(x, y).unwrap(),
        differing.len()
    ))
}

#[test]
fn the_cpu_backend_draws_every_frame_to_its_checked_in_picture() {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("cpu-backend-checked-in-pictures")
        .join(FIXTURE);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    fs::copy(fixtures().join(FIXTURE).join("composition.xml"), folder.join("composition.xml")).unwrap();

    let mut options = Options::default();
    options.set("--backend", "cpu").unwrap();
    let Ok(mut project) = Project::open(&folder) else {
        panic!("{}: the project does not open", folder.display());
    };
    project.apply(&options);
    assert!(matches!(project.settings().backend(), Backend::Cpu));

    // `cpu` never starts the card: the starter panics if it is asked to start one.
    let card_must_not_start =
        || -> Result<Box<dyn draw::Renderer>, draw::Error> { panic!("`--backend cpu` must not start the graphics card") };
    let mut choice = renderer_for(project.settings(), &card_must_not_start).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(choice.name(), "processor");
    assert_eq!(choice.chosen(), Chosen::Setting);

    let Ok(scene) = Scene::from_project(&project, &render::system_codecs()) else {
        panic!("{}: the composition is not a valid scene", folder.display());
    };
    assert_eq!(scene.frames(), 10);
    for number in 0..scene.frames() {
        let drawn = scene
            .picture(number, choice.renderer_mut())
            .unwrap_or_else(|error| panic!("frame {number}: {error:?}"));
        if let Err(message) = compare(number, &drawn, &read_picture(&held_to(number))) {
            panic!("{message}");
        }
    }
}
