//! The border of a box: one line, or two with a gap, as `docs/spec/native-notation.md`,
//! section 18, says.

use poc::cpu::Cpu;
use poc::scene::Evaluator;
use poc::text::Shaper;
use poc::{layout, xml, Backend};
use std::path::Path;

const WIDTH: usize = 80;

/// The colours along the row through the middle of a box of 60 by 40 at (10, 10) with `border`.
fn row(border: &str) -> Vec<[u8; 3]> {
    let source = format!(
        r##"<composition width="{WIDTH}" height="60" fps="10" duration="1" background="#ffffff">
  <box left="10" top="10" width="60" height="40" border="{border}"/>
</composition>"##
    );
    let mut composition = xml::parse(&source, Path::new(".")).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    let mut cpu = Cpu::new(&composition, &[]);
    cpu.draw(0, &Evaluator::new(&composition).frame(0).ops).expect("the frame is drawn");
    let pixels = cpu.pixels().expect("the frame is read");
    (0..WIDTH)
        .map(|x| {
            [
                pixels[(30 * WIDTH + x) * 4],
                pixels[(30 * WIDTH + x) * 4 + 1],
                pixels[(30 * WIDTH + x) * 4 + 2],
            ]
        })
        .collect()
}

const INK: [u8; 3] = [0, 0, 255];
const PAPER: [u8; 3] = [255, 255, 255];

#[test]
fn a_border_of_one_line_fills_its_width_inside_the_box() {
    let found = row("9 #0000ff");
    assert_eq!(found[9], PAPER);
    assert!((10..19).all(|x| found[x] == INK), "{:?}", &found[8..21]);
    assert_eq!(found[19], PAPER);
    assert!((61..70).all(|x| found[x] == INK) && found[60] == PAPER && found[70] == PAPER);
}

#[test]
fn a_double_border_is_two_lines_and_a_gap_of_a_third_each() {
    let found = row("9 double #0000ff");
    let expected = [PAPER, INK, INK, INK, PAPER, PAPER, PAPER, INK, INK, INK, PAPER];
    assert_eq!(&found[9..20], &expected, "from the left edge inwards");
    let mirrored: Vec<[u8; 3]> = (60..71).rev().map(|x| found[x]).collect();
    assert_eq!(&mirrored[..], &expected, "from the right edge inwards");
}

#[test]
fn a_border_with_another_word_or_no_colour_is_refused_with_its_line() {
    let source = "<composition width=\"8\" height=\"8\" fps=\"10\" duration=\"1\">\n<box border=\"9 dotted #0000ff\"/>\n<box border=\"9 double\"/>\n<box border=\"x double #0000ff\"/>\n</composition>";
    let problems = xml::parse(source, Path::new(".")).err().expect("the borders are refused");
    let found: Vec<String> = problems
        .iter()
        .map(|problem| format!("{}: {}", problem.line, problem.message))
        .collect();
    assert_eq!(
        found,
        [
            "2: `border=\"9 dotted #0000ff\"` is not a width and a colour",
            "3: `border=\"9 double\"` is not a width and a colour",
            "4: `border=\"x double #0000ff\"` is not a width and a colour",
        ]
    );
}
