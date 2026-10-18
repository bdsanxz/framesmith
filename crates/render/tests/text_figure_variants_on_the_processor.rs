//! The processor draws the figures that a text asks for with `font-variant-numeric`: with tabular figures the strings 111
//! and 888 are drawn equally wide, with proportional figures they are not, and with old-style figures a figure that
//! descends is drawn lower in its line than with lining figures (`docs/spec/text.md`, section 6.6, rule 4;
//! `docs/spec/test-font.md`, section 7). Each width is the distance from the left edge of the first bar of a text to the
//! right edge of its last bar, so the bars are the same in every text and the width is the width of the figures.

mod common;

use common::{crop, extent_of, project_copied_from_fixture};
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const GROUP: &str = "text-figure-variants-on-the-processor";
const BLACK: [u8; 4] = [0, 0, 0, 255];

/// The picture the processor draws for frame 0 of a fixture, drawn from a copy of the fixture in a folder of its own.
fn frame_zero(fixture: &str) -> Picture {
    let folder = project_copied_from_fixture(GROUP, fixture, fixture);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, &mut CpuRenderer).unwrap()
}

/// The ink of the black text in the band of rows from `top` to `top + height`, as (left, top, right, bottom) with right and
/// bottom not included, in the whole picture.
fn ink_in_band(picture: &Picture, top: u32, height: u32) -> (u32, u32, u32, u32) {
    let band = crop(picture, 0, top, picture.width(), height);
    let (left, row_top, right, bottom) = extent_of(&band, BLACK).expect("the text has ink in its band");
    (left, top + row_top, right, top + bottom)
}

/// The tops of the four texts of the width fixture, each band 36 rows high.
const WIDTH_TOPS: [u32; 4] = [8, 44, 80, 116];

/// The width of the ink of the text on line `line` of the width fixture.
fn width_of_line(picture: &Picture, line: usize) -> u32 {
    let (left, _, right, _) = ink_in_band(picture, WIDTH_TOPS[line], 36);
    right - left
}

#[test]
fn with_tabular_figures_the_strings_111_and_888_are_drawn_equally_wide() {
    let picture = frame_zero("text-figure-widths");
    let (tabular_111, tabular_888) = (width_of_line(&picture, 0), width_of_line(&picture, 1));
    println!("tabular-nums: 111 drawn {tabular_111} pixels wide, 888 drawn {tabular_888} pixels wide");
    assert!(tabular_111 > 0 && tabular_888 > 0, "both strings are drawn");
    assert_eq!(
        tabular_111, tabular_888,
        "the strings 111 and 888 with tabular figures are drawn {tabular_111} and {tabular_888} pixels wide"
    );
}

#[test]
fn with_proportional_figures_the_strings_111_and_888_are_drawn_unequally_wide() {
    let picture = frame_zero("text-figure-widths");
    let (proportional_111, proportional_888) = (width_of_line(&picture, 2), width_of_line(&picture, 3));
    println!("proportional-nums: 111 drawn {proportional_111} pixels wide, 888 drawn {proportional_888} pixels wide");
    assert!(proportional_111 > 0 && proportional_888 > 0, "both strings are drawn");
    assert_ne!(
        proportional_111, proportional_888,
        "the strings 111 and 888 with proportional figures are both drawn {proportional_111} pixels wide"
    );
}

/// The tops of the four texts of the old-style fixture, each band 44 rows high: lining 3, old-style 3, lining 9, old-style 9.
const OLDSTYLE_TOPS: [u32; 4] = [6, 50, 94, 138];

/// How far below the top of its band the ink of the text on line `line` reaches: its bottom less the top of its band.
fn drop_in_line(picture: &Picture, line: usize) -> u32 {
    let (_, _, _, bottom) = ink_in_band(picture, OLDSTYLE_TOPS[line], 44);
    bottom - OLDSTYLE_TOPS[line]
}

#[test]
fn a_lining_3_and_a_lining_9_stand_on_the_same_baseline_in_their_lines() {
    let picture = frame_zero("text-figure-oldstyle");
    let (lining_3, lining_9) = (drop_in_line(&picture, 0), drop_in_line(&picture, 2));
    println!("lining-nums: 3 reaches {lining_3} pixels below its line top, 9 reaches {lining_9}");
    assert_eq!(
        lining_3, lining_9,
        "the lining 3 and the lining 9 reach the same depth in their lines"
    );
}

#[test]
fn an_old_style_3_descends_below_a_lining_3_in_its_line() {
    let picture = frame_zero("text-figure-oldstyle");
    let (lining_3, oldstyle_3) = (drop_in_line(&picture, 0), drop_in_line(&picture, 1));
    println!("3: lining-nums reaches {lining_3} pixels below its line top, oldstyle-nums {oldstyle_3}");
    assert!(
        oldstyle_3 >= lining_3 + 5,
        "the old-style 3 reaches {oldstyle_3} pixels below its line top, the lining 3 {lining_3}: it must descend at least 5 more"
    );
}

#[test]
fn an_old_style_9_descends_below_a_lining_9_in_its_line() {
    let picture = frame_zero("text-figure-oldstyle");
    let (lining_9, oldstyle_9) = (drop_in_line(&picture, 2), drop_in_line(&picture, 3));
    println!("9: lining-nums reaches {lining_9} pixels below its line top, oldstyle-nums {oldstyle_9}");
    assert!(
        oldstyle_9 >= lining_9 + 5,
        "the old-style 9 reaches {oldstyle_9} pixels below its line top, the lining 9 {lining_9}: it must descend at least 5 more"
    );
}
