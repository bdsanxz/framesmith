//! A chain of sixteen files, the root counted, is drawn through every file: the box of the last file is in the
//! picture at the place of the element that shows it through the fifteen before it (`docs/spec/notation.md`
//! section 10).

mod common;

use common::scratch;
use draw_cpu::CpuRenderer;
use project::{Project, MAX_CHAIN_DEPTH};
use render::Scene;
use std::fs;

/// A composition of 96 by 54 pixels, one second long at 30 frames a second, with `inside` in its root.
fn file(inside: &str) -> String {
    format!("<composition version=\"1\" width=\"96\" height=\"54\" fps=\"30\" duration=\"1\">\n{inside}\n</composition>\n")
}

#[test]
fn a_chain_of_sixteen_files_is_drawn_through_every_file() {
    let dir = scratch("shown-chain-drawn", "sixteen");
    fs::write(
        dir.join("composition.xml"),
        file("<composition src=\"f2.xml\" left=\"0\" top=\"0\"/>"),
    )
    .unwrap();
    for number in 2..MAX_CHAIN_DEPTH {
        let inside = format!("<composition src=\"f{}.xml\" left=\"0\" top=\"0\"/>", number + 1);
        fs::write(dir.join(format!("f{number}.xml")), file(&inside)).unwrap();
    }
    let last = format!("f{MAX_CHAIN_DEPTH}.xml");
    fs::write(
        dir.join(&last),
        file("<box left=\"10\" top=\"10\" width=\"20\" height=\"10\" background=\"#ff0000\"/>"),
    )
    .unwrap();
    let project = Project::open(&dir).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let picture = scene.picture(0, &mut CpuRenderer).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        picture.pixel(15, 15),
        Some([255, 0, 0, 255]),
        "the box of {last}, drawn through the chain"
    );
    assert_ne!(picture.pixel(60, 40), Some([255, 0, 0, 255]), "nothing is drawn outside the box");
}
