//! Makes the checked-in pictures of a fixture: draws the frames named on the processor and writes
//! them as PNG files to `fixtures/<name>/expected/frame-NNNN.png`.
//!
//! `cargo run -p render --example write_expected_pictures -- first-render 0 4 9`
//!
//! A picture made here is the correct picture only once it has been looked at and found to show
//! what the composition says (`docs/spec/render-command.md` section 9).

#[path = "../tests/common/png.rs"]
mod png;

use draw_cpu::CpuRenderer;
use project::Project;
use render::{Scene, Sources};
use std::path::Path;

fn main() {
    let mut words = std::env::args().skip(1);
    let Some(name) = words.next() else {
        eprintln!("usage: write_expected_pictures <fixture> <frame>...");
        std::process::exit(2);
    };
    let numbers: Vec<u64> = words
        .map(|word| word.parse().unwrap_or_else(|_| panic!("`{word}` is not a frame number")))
        .collect();
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(&name);
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut sources = Sources::open(&scene).unwrap_or_else(|error| panic!("{error}"));
    std::fs::create_dir_all(folder.join("expected")).unwrap();
    for number in numbers {
        let picture = scene
            .picture_with(number, &mut CpuRenderer, &mut sources)
            .unwrap_or_else(|error| panic!("{error}"));
        let path = folder.join("expected").join(format!("frame-{number:04}.png"));
        std::fs::write(&path, png::encode(picture.width(), picture.height(), picture.pixels())).unwrap();
        println!("{}", path.display());
    }
}
