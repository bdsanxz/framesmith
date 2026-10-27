//! The reports of a torn edge and grain (`docs/spec/finishes.md`, section 2): octaves outside 1 to 8, an amount of 0 and a
//! form not listed are each reported with the line of the attribute by `check` and `render`, which exit 1 and write no
//! film; the forms listed pass `check`. Two renders of the fixture `finishes-torn-and-grain` print the same frame marks.

mod common;
mod frame_marks;

use std::fs;
use std::path::Path;

use common::{err, framesmith, out, project};
use frame_marks::{fixture_project, mark_lines, marked_render};

const GROUP: &str = "finish-reports";

/// A composition with the attribute `written` on the box on line 4.
fn composition(written: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"64\" height=\"48\" fps=\"10\" duration=\"0.5\" background=\"#000000\">\n\
         <box left=\"0\" top=\"0\" width=\"40\" height=\"40\" background=\"#336699\"/>\n\
         <box left=\"8\" top=\"8\" width=\"40\" height=\"20\" background=\"#cc3300\"\n     {written}/>\n\
         </composition>\n"
    )
}

fn has_a_film(project: &Path) -> bool {
    fs::read_dir(project.join("renders"))
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "mp4"))
        })
        .unwrap_or(false)
}

fn reported(name: &str, written: &str, wanted: &str) {
    let (folder, cwd) = project(GROUP, name, &composition(written));
    for command in ["check", "render"] {
        let output = framesmith(&cwd, [command, folder.to_str().unwrap()], &[]);
        let printed = format!("{}{}", out(&output), err(&output));
        assert_eq!(output.status.code(), Some(1), "{name}: {command}: {printed}");
        let wanted = format!("line 4: {wanted}");
        assert!(printed.contains(&wanted), "{name}: {command}: {wanted}\nin\n{printed}");
    }
    assert!(!has_a_film(&folder), "{name}");
}

#[test]
fn octaves_outside_one_to_eight_are_reported_with_their_line() {
    reported(
        "nine-octaves",
        "grain=\"noise(0.5 9 1) 1 0\"",
        "FS-0071: invalid: `grain=\"noise(0.5 9 1) 1 0\"` has 9 octaves; the octaves are a whole number from 1 to 8",
    );
    reported(
        "no-octaves",
        "displace=\"7 noise(0.1 0 3)\"",
        "FS-0071: invalid: `displace=\"7 noise(0.1 0 3)\"` has 0 octaves; the octaves are a whole number from 1 to 8",
    );
}

#[test]
fn an_amount_of_zero_is_reported_with_its_line() {
    reported(
        "zero-amount",
        "displace=\"0 noise(0.1 3 1)\"",
        "FS-0053: invalid: `displace=\"0 noise(0.1 3 1)\"` has an amount that is not above 0",
    );
    reported(
        "zero-frequency",
        "grain=\"noise(0 3 1) 1 0\"",
        "FS-0053: invalid: `grain=\"noise(0 3 1) 1 0\"` has a frequency that is not above 0",
    );
}

#[test]
fn a_form_not_listed_is_reported_with_its_line() {
    for (name, written, form) in [
        (
            "no-seed",
            "displace=\"7 noise(0.1 3)\"",
            "displace=\"7 noise(0.1 3)\"` is not in the form AMOUNT noise(FREQUENCY OCTAVES SEED)",
        ),
        (
            "other-function",
            "displace=\"7 turbulence(0.1 3 1)\"",
            "displace=\"7 turbulence(0.1 3 1)\"` is not in the form AMOUNT noise(FREQUENCY OCTAVES SEED)",
        ),
        (
            "no-offset",
            "grain=\"noise(0.9 2 5) -1.7\"",
            "grain=\"noise(0.9 2 5) -1.7\"` is not in the form noise(FREQUENCY OCTAVES SEED) SLOPE OFFSET",
        ),
        (
            "fraction-of-a-seed",
            "grain=\"noise(0.9 2 5.5) 1 0\"",
            "grain=\"noise(0.9 2 5.5) 1 0\"` is not in the form noise(FREQUENCY OCTAVES SEED) SLOPE OFFSET",
        ),
    ] {
        reported(name, written, &format!("FS-0071: invalid: `{form}"));
    }
}

#[test]
fn the_forms_listed_pass_check() {
    for (name, written) in [
        ("torn", "displace=\"14 noise(0.035 3 9)\""),
        ("grain", "grain=\"noise(0.9 2 5) -1.7 1.75\""),
        ("both", "displace=\"3 noise(0.2 8 -7)\" grain=\"noise(0.5 1 0) 2 -0.5\""),
    ] {
        let (folder, cwd) = project(GROUP, &format!("listed-{name}"), &composition(written));
        let output = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
        assert_eq!(output.status.code(), Some(0), "{name}: {}{}", out(&output), err(&output));
    }
}

#[test]
fn two_renders_of_a_torn_edge_and_grain_print_the_same_frame_marks() {
    let fixture = "finishes-torn-and-grain";
    let (first, first_cwd) = fixture_project(GROUP, "marks-first", fixture);
    let (second, second_cwd) = fixture_project(GROUP, "marks-second", fixture);
    let first_marks = mark_lines(&framesmith(&first_cwd, marked_render(&first), &[]));
    let second_marks = mark_lines(&framesmith(&second_cwd, marked_render(&second), &[]));
    assert_eq!(first_marks.len(), 10);
    assert_eq!(first_marks, second_marks);
    // The torn square moves, so the frames differ from one another.
    assert_ne!(first_marks[0], first_marks[9]);
}
