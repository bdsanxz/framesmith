//! A text that counts and a text that types itself, drawn on the processor (`docs/spec/text.md`, sections 7.1 and 7.2;
//! `docs/spec/notation.md` section 7.12 and Q4). Each chosen frame of a fixture is held to the picture of a composition
//! that writes, as plain words, what the frame must show: the number of its time written in the text's form, or the
//! characters typed so far. Where the characters not yet typed take their room in the layout, the reference writes them
//! in the colour of the background, so that they take the same room and show nothing. The fixtures' checked-in pictures
//! are those references' pictures, and the frames drawn equal them to the byte.

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture, project_with};
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;
use std::fs;

const FONT: &str = "<font src=\"fonts/framesmith-test-stroke-400.ttf\" family=\"Framesmith Test Stroke\"/>";
const LOOK: &str = "font-family=\"Framesmith Test Stroke\" font-size=\"20\"";
/// The background of every fixture: a character written in it takes its room and shows nothing.
const HIDDEN: &str = "<span color=\"#ffffff\">";

fn scene_of(folder: &std::path::Path) -> Scene {
    Scene::from_project(&Project::open(folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

/// Frame 0 of a still composition of `width` by `height` on a white ground, with the test font and `inside`.
fn reference(name: &str, width: u32, height: u32, inside: &str) -> Picture {
    let text = format!(
        "<composition version=\"1\" width=\"{width}\" height=\"{height}\" fps=\"10\" duration=\"0.1\" background=\"#ffffff\">\n{FONT}\n{inside}\n</composition>\n"
    );
    let folder = project_with("count-and-typed-pictures", name, &text);
    fs::create_dir_all(folder.join("fonts")).unwrap();
    let font = "framesmith-test-stroke-400.ttf";
    fs::copy(common::fixtures().join("fonts").join(font), folder.join("fonts").join(font)).unwrap();
    scene_of(&folder).picture(0, &mut CpuRenderer).unwrap()
}

/// What frame `number` of `fixture` must show, written as plain words.
fn reference_of(fixture: &str, number: u64) -> Picture {
    let name = format!("{fixture}-{number}");
    match fixture {
        "count-a-number" => {
            // 0 to 100 and 0 to 12345.68, linear over the first second: the value at frame n is n/10 of the way.
            let (first, second) = match number {
                0 => ("0", "0.00"),
                3 => ("30", "3,703.70"),
                5 => ("50", "6,172.84"),
                _ => ("100", "12,345.68"),
            };
            reference(
                &name,
                160,
                60,
                &format!(
                    "<text left=\"10\" top=\"4\" {LOOK} color=\"#202020\">{first}</text>\n<text left=\"10\" top=\"32\" {LOOK} color=\"#2050c0\">{second}</text>"
                ),
            )
        }
        "type-a-text" => {
            // floor(10 n / 10) and floor(6 n / 10) characters at frame n of the first second, all of them after it.
            let (first, centre) = match number {
                0 => (None, None),
                5 => (Some(format!("TYPED{HIDDEN} TEXT!!</span>")), Some(format!("CEN{HIDDEN}TRE</span>"))),
                _ => (Some(format!("TYPED TEXT{HIDDEN}!!</span>")), Some("CENTRE".to_string())),
            };
            let mut inside = String::new();
            if let Some(first) = first {
                inside += &format!("<text left=\"10\" top=\"4\" {LOOK} color=\"#202020\">{first}</text>\n");
            }
            if let Some(centre) = centre {
                inside +=
                    &format!("<text left=\"10\" top=\"32\" width=\"140\" text-align=\"center\" {LOOK} color=\"#2050c0\">{centre}</text>\n");
            }
            inside += &format!("<text left=\"10\" top=\"60\" {LOOK} color=\"#202020\">WHOLE TEXT</text>");
            reference(&name, 160, 90, &inside)
        }
        "type-a-span" => {
            // The text shows from 0.8 s. The span's tween runs from 0.8 + 0.2 = 1.0 s to 1.5 s, 5 characters: floor(5 (t -
            // 1.0) / 0.5) at time t. The characters the span does not show take no room, so " END" follows what it shows.
            let span = match number {
                0 => return reference(&name, 180, 40, ""),
                8 | 10 => "",
                12 => " W",
                13 => " WO",
                _ => " WORD",
            };
            let words = if span.is_empty() {
                "HELLO END".to_string()
            } else {
                format!("HELLO<span color=\"#c03020\">{span}</span> END")
            };
            reference(
                &name,
                180,
                40,
                &format!("<text left=\"10\" top=\"8\" {LOOK} color=\"#202020\">{words}</text>"),
            )
        }
        _ => unreachable!("{fixture}"),
    }
}

/// Holds the chosen frames of `fixture` to what they must show, and to the fixture's checked-in pictures, to the byte.
fn frames_show_what_is_worked_out(fixture: &str, chosen: &[u64]) {
    let scene = scene_of(&project_copied_from_fixture("count-and-typed-pictures", fixture, fixture));
    assert_eq!(
        numbers_with_a_picture(fixture),
        chosen,
        "{fixture}: the chosen frames have a picture"
    );
    for &number in chosen {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        let want = reference_of(fixture, number);
        compare(number, &drawn, &want).unwrap_or_else(|difference| panic!("{fixture}, drawn: {difference}"));
        compare(number, &expected(fixture, number), &want).unwrap_or_else(|difference| panic!("{fixture}, checked in: {difference}"));
    }
}

#[test]
fn a_counting_text_shows_the_number_of_each_frame_s_time_in_its_form() {
    frames_show_what_is_worked_out("count-a-number", &[0, 3, 5, 10, 14]);
}

#[test]
fn a_typed_text_shows_its_first_characters_where_the_whole_text_has_them_and_typed_alone_shows_all() {
    frames_show_what_is_worked_out("type-a-text", &[0, 5, 10, 14]);
}

#[test]
fn a_span_types_its_own_words_in_its_text_s_time_and_the_words_after_it_close_up() {
    frames_show_what_is_worked_out("type-a-span", &[0, 8, 10, 12, 13, 15, 19]);
}

#[test]
fn a_counting_number_keeps_its_box_still_and_stands_in_it_by_its_alignment() {
    // A row holds a counting text, right-aligned, and a square after it. The text's box is as wide as "100", the widest
    // number it shows, so the square is where it stands after the words "100", in every frame; and the number stands at
    // the right of its box, so "5" ends where "100" ends.
    let inside = |text: &str| {
        format!(
            "<box left=\"10\" top=\"10\" flex-direction=\"row\" align-items=\"flex-start\">{text}<box width=\"10\" height=\"10\" background=\"#c03020\"/></box>"
        )
    };
    let counting =
        format!("<text {LOOK} color=\"#202020\" text-align=\"right\" count=\"0\"><tween duration=\"1\" to-count=\"100\"/></text>");
    let text = format!(
        "<composition version=\"1\" width=\"120\" height=\"40\" fps=\"10\" duration=\"1.5\" background=\"#ffffff\">\n{FONT}\n{}\n</composition>\n",
        inside(&counting)
    );
    let folder = project_with("count-and-typed-pictures", "still-box", &text);
    fs::create_dir_all(folder.join("fonts")).unwrap();
    let font = "framesmith-test-stroke-400.ttf";
    fs::copy(common::fixtures().join("fonts").join(font), folder.join("fonts").join(font)).unwrap();
    let scene = scene_of(&folder);
    // At frame 0 the number is 0; at frame 10 and after, 100. The references are still texts with the same alignment in
    // the same row.
    let widest = reference(
        "still-box-100",
        120,
        40,
        &inside(&format!("<text {LOOK} color=\"#202020\" text-align=\"right\">100</text>")),
    );
    compare(10, &scene.picture(10, &mut CpuRenderer).unwrap(), &widest).unwrap();
    compare(14, &scene.picture(14, &mut CpuRenderer).unwrap(), &widest).unwrap();
    // At frame 0, "0" at the right of a box as wide as "100": written as "10" in the background's colour, then "0".
    let zero = reference(
        "still-box-0",
        120,
        40,
        &inside(&format!(
            "<text {LOOK} color=\"#202020\" text-align=\"right\">{HIDDEN}10</span>0</text>"
        )),
    );
    compare(0, &scene.picture(0, &mut CpuRenderer).unwrap(), &zero).unwrap();
}
