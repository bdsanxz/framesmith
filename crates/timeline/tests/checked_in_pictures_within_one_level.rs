//! Every checked-in picture, drawn again on the processor's renderer, is within one level of 255 on
//! every value, and the largest difference found is printed (`docs/spec/timeline.md` section 3).

#[path = "../../render/tests/common/png.rs"]
mod png;

use draw_cpu::CpuRenderer;
use project::Project;
use std::fs;
use std::path::{Path, PathBuf};
use timeline::Scene;

const LIMIT: u8 = 1;

fn fixtures() -> PathBuf {
    std::env::var_os("FRAMESMITH_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures"))
}

/// The fixtures that have checked-in pictures, each with the frame numbers that have one.
fn checked_in() -> Vec<(String, Vec<(u64, PathBuf)>)> {
    let mut out = Vec::new();
    let mut names: Vec<_> = fs::read_dir(fixtures()).unwrap().map(|entry| entry.unwrap().path()).collect();
    names.sort();
    for folder in names {
        let expected = folder.join("expected");
        if !expected.is_dir() {
            continue;
        }
        let mut frames = Vec::new();
        for entry in fs::read_dir(&expected).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if let Some(number) = name.strip_prefix("frame-").and_then(|rest| rest.strip_suffix(".png")) {
                frames.push((number.parse::<u64>().unwrap(), path));
            }
        }
        frames.sort();
        out.push((folder.file_name().unwrap().to_string_lossy().to_string(), frames));
    }
    out
}

/// A project folder of the test's own, holding a copy of the fixture's files; its `expected` pictures are left out.
fn project_of(fixture: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("within-one-level").join(fixture);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    copy_tree(&fixtures().join(fixture), &dir);
    dir
}

/// Copies the files and folders of `from` into `to`, all but a folder named `expected`.
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            if entry.file_name() != "expected" {
                copy_tree(&path, &target);
            }
        } else {
            fs::copy(&path, &target).unwrap();
        }
    }
}

#[test]
fn every_checked_in_picture_is_drawn_within_one_level_and_the_largest_difference_is_printed() {
    let fixtures = checked_in();
    assert!(
        fixtures.iter().map(|(_, frames)| frames.len()).sum::<usize>() >= 5,
        "the checked-in pictures are found"
    );
    let channels = ["red", "green", "blue", "alpha"];
    let mut largest: (u8, String) = (0, "no value differs".to_string());
    let mut over = Vec::new();
    let mut pictures = 0;
    for (fixture, frames) in &fixtures {
        let project = Project::open(&project_of(fixture)).unwrap();
        let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{fixture}: {reports:?}"));
        for (number, path) in frames {
            let (width, height, want) = png::decode(&fs::read(path).unwrap()).unwrap();
            let drawn = scene.picture(*number, &mut CpuRenderer).unwrap();
            assert_eq!((drawn.width(), drawn.height()), (width, height), "{fixture} frame {number}: size");
            pictures += 1;
            for y in 0..height {
                for x in 0..width {
                    let got = drawn.pixel(x, y).unwrap();
                    let at = ((y * width + x) * 4) as usize;
                    for c in 0..4 {
                        let difference = got[c].abs_diff(want[at + c]);
                        let place = format!("{fixture} frame {number}, pixel ({x}, {y}), {}", channels[c]);
                        if difference > largest.0 {
                            largest = (difference, place.clone());
                        }
                        if difference > LIMIT {
                            over.push(format!("{place}: drawn {} checked in {}", got[c], want[at + c]));
                        }
                    }
                }
            }
        }
    }
    println!("checked-in pictures compared: {pictures}");
    println!("largest difference found: {} of 255 ({})", largest.0, largest.1);
    assert!(
        over.is_empty(),
        "{} value(s) differ by more than {LIMIT} of 255; first: {}",
        over.len(),
        over[0]
    );
}
