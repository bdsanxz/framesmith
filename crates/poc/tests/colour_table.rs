//! A colour table on footage: reading a `.cube` text, the colour a colour becomes, its place
//! before the filter, and what is said of a table that cannot be used.

use poc::lut::Lut;
use poc::model::{Kind, Node};
use poc::style::{apply_color_ops, ColorOp};
use poc::xml;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The text of a table of `size` points a side that turns the colour `(r, g, b)`, each from 0
/// to 1, into `colour(r, g, b)`.
fn cube(size: usize, colour: impl Fn(f32, f32, f32) -> [f32; 3]) -> String {
    let mut text = format!("# a table of our own\nTITLE \"test\"\nLUT_3D_SIZE {size}\nDOMAIN_MIN 0.0 0.0 0.0\nDOMAIN_MAX 1.0 1.0 1.0\n\n");
    let at = |point: usize| point as f32 / (size - 1) as f32;
    for blue in 0..size {
        for green in 0..size {
            for red in 0..size {
                let [r, g, b] = colour(at(red), at(green), at(blue));
                text.push_str(&format!("{r} {g} {b}\n"));
            }
        }
    }
    text
}

/// A spread of colours: greys, the corners of the cube and mixed ones.
fn colours() -> Vec<[u8; 3]> {
    let mut all = Vec::new();
    for red in (0..=255).step_by(15) {
        for green in (0..=255).step_by(17) {
            for blue in (0..=255).step_by(51) {
                all.push([red as u8, green as u8, blue as u8]);
            }
        }
    }
    all
}

#[test]
fn a_table_that_changes_nothing_leaves_every_colour() {
    for size in [2, 5, 33] {
        let table = Lut::read(&cube(size, |r, g, b| [r, g, b])).expect("the table is read");
        assert_eq!(table.size(), size);
        for colour in colours() {
            assert_eq!(table.at(colour), colour, "with {size} points a side");
        }
    }
}

#[test]
fn a_colour_between_points_takes_its_place_between_them() {
    // Each result is a straight line of one channel, so a place between points is exact:
    // red is turned round, green halved, blue takes red's value.
    let table = Lut::read(&cube(3, |r, g, _| [1.0 - r, g * 0.5, r])).expect("the table is read");
    for [red, green, blue] in colours() {
        let expected = [255 - red, (f32::from(green) * 0.5).round() as u8, red];
        let found = table.at([red, green, blue]);
        for channel in 0..3 {
            assert!(
                found[channel].abs_diff(expected[channel]) <= 1,
                "({red}, {green}, {blue}) became {found:?}, not {expected:?}"
            );
        }
    }
    // A table with a bend: the middle point of each channel is lifted, the ends are not.
    let bent = Lut::read(&cube(3, |r, g, b| [r, g, b].map(|c| if c == 0.5 { 0.75 } else { c }))).expect("the table is read");
    assert_eq!(bent.at([0, 0, 0]), [0, 0, 0]);
    assert_eq!(bent.at([255, 255, 255]), [255, 255, 255]);
    // A quarter of the way is half of the way to the lifted middle point.
    let quarter = bent.at([64, 64, 64]);
    assert!(quarter.iter().all(|value| value.abs_diff(96) <= 1), "{quarter:?}");
}

#[test]
fn a_result_outside_the_range_is_brought_to_its_nearer_end() {
    let table = Lut::read(&cube(2, |r, g, b| [r * 1.5, g - 0.5, b])).expect("the table is read");
    assert_eq!(table.at([255, 0, 128]), [255, 0, 128]);
    assert_eq!(table.at([200, 100, 0]), [255, 0, 0]);
}

#[test]
fn the_table_comes_before_the_steps_of_the_filter() {
    let turned_round = Arc::new(Lut::read(&cube(2, |r, g, b| [1.0 - r, 1.0 - g, 1.0 - b])).expect("the table is read"));
    let mut pixels = [255, 0, 100, 255, 0, 255, 100, 77];
    apply_color_ops(&[ColorOp::Table(turned_round), ColorOp::Brightness(0.5)], &mut pixels);
    // Turned round first, then halved: the other order would give 128, 0, 205.
    assert_eq!(pixels, [0, 128, 78, 255, 128, 0, 78, 77]);
}

#[test]
fn what_is_wrong_with_a_table_is_named() {
    let whole = cube(2, |r, g, b| [r, g, b]);
    let reason = |text: &str| Lut::read(text).expect_err("the table is refused");
    assert!(reason("LUT_1D_SIZE 4\n0 0 0\n").contains("line 1: this is a table of one dimension"));
    assert!(reason(&whole.replace("DOMAIN_MAX 1.0 1.0 1.0", "DOMAIN_MAX 2.0 2.0 2.0")).contains("only tables for colours from 0 to 1"));
    assert!(reason(&whole.replace("TITLE \"test\"", "SHAPER 4")).contains("line 2: `SHAPER` is not known"));
    assert!(reason(&format!("{whole}0.5 0.5\n")).contains("`0.5 0.5` is not three numbers"));
    assert!(reason(&format!("{whole}0.5 0.5 0.5\n")).contains("it has 9 rows of colours; a table of size 2 has 8"));
    assert!(reason("0 0 0\n").contains("it has no `LUT_3D_SIZE`"));
    assert!(reason("LUT_3D_SIZE 1\n0 0 0\n").contains("the size must be a whole number from 2 to 128"));
}

/// A new, empty folder for one test, under the build's own temporary folder.
fn folder(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("colour-table").join(name);
    if folder.exists() {
        std::fs::remove_dir_all(&folder).expect("the old test folder is removed");
    }
    std::fs::create_dir_all(&folder).expect("the test folder is made");
    folder
}

fn filters(node: &Node, found: &mut Vec<Vec<ColorOp>>) {
    if let Kind::Video { filter, .. } = &node.kind {
        found.push(filter.clone());
    }
    for child in &node.children {
        filters(child, found);
    }
}

#[test]
fn a_video_names_its_table_and_the_table_is_its_filters_first_step() {
    let folder = folder("named");
    std::fs::write(folder.join("grade.cube"), cube(2, |r, g, b| [b, g, r])).expect("the table is written");
    let composition = xml::parse(
        r#"<composition width="64" height="32" fps="30" duration="1">
  <video src="a.mp4" left="0" top="0" width="32" height="32" lut="grade.cube" filter="brightness(0.5)"/>
  <video src="a.mp4" left="32" top="0" width="32" height="32" lut="grade.cube"/>
  <video src="a.mp4" left="32" top="0" width="32" height="32"/>
</composition>"#,
        &folder,
    )
    .expect("the scene is valid");
    let mut found = Vec::new();
    filters(&composition.root, &mut found);
    let [both, table, none] = &found[..] else {
        panic!("three videos, not {}", found.len());
    };
    let [ColorOp::Table(first), ColorOp::Brightness(_)] = &both[..] else {
        panic!("the table and then the filter, not {both:?}");
    };
    assert_eq!(first.at([255, 10, 0]), [0, 10, 255]);
    // The file is read once for both videos.
    assert!(matches!(&table[..], [ColorOp::Table(second)] if Arc::ptr_eq(first, second)));
    assert!(none.is_empty());
}

#[test]
fn a_table_that_cannot_be_used_stops_the_composition_with_its_line_and_file() {
    let folder = folder("faulty");
    std::fs::write(folder.join("short.cube"), "LUT_3D_SIZE 2\n0 0 0\n").expect("the table is written");
    let scene = |table: &str| {
        format!(
            "<composition width=\"64\" height=\"32\" fps=\"30\" duration=\"1\">\n  <video src=\"a.mp4\" left=\"0\" top=\"0\" width=\"32\" height=\"32\" lut=\"{table}\"/>\n</composition>"
        )
    };
    let refused = |table: &str| xml::parse(&scene(table), &folder).err().expect("the scene is refused");
    let problems = refused("short.cube");
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].line, 2);
    assert!(
        problems[0].message.contains("short.cube") && problems[0].message.contains("it has 1 rows of colours; a table of size 2 has 8"),
        "{}",
        problems[0].message
    );
    let problems = refused("missing.cube");
    assert!(
        problems[0].message.contains("the colour table")
            && problems[0].message.contains("missing.cube")
            && problems[0].message.contains("could not be read"),
        "{}",
        problems[0].message
    );
}
