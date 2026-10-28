//! What a `<shape>` needs of its line and its blur (`docs/spec/notation.md`, section 7.11; `docs/spec/check-command.md`,
//! section 3a): `draw` and `dash` need a `stroke-width` on the shape, as an attribute, in a `<tween>` or `<set>`, or given
//! by a style; a shape with a `src` is not judged for the line; a `blur` below 0 is `FS-0034` on its line.

mod common;

use common::{problems_in, problems_of, worded};

fn places(text: &str) -> Vec<(&'static str, u32)> {
    problems_of(text)
        .iter()
        .map(|problem| (problem.code().as_str(), problem.line()))
        .collect()
}

#[test]
fn dash_on_a_shape_with_no_stroke_width_is_reported_on_the_attributes_line() {
    let problems = problems_in("<shape d=\"M0 0 L10 10\" width=\"10\" height=\"10\" dash=\"10 5\"/>");
    assert_eq!(
        problems.iter().map(worded).collect::<Vec<_>>(),
        ["line 2: FS-0087: `dash` is for a <shape> with `stroke-width`",]
    );
}

#[test]
fn draw_on_a_shape_with_no_stroke_width_is_reported_on_the_attributes_line() {
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<shape d=\"M0 0 L10 10\" width=\"10\" height=\"10\"\n  draw=\"0.5\"/>\n</composition>";
    assert_eq!(places(text), [("FS-0087", 3)]);
}

#[test]
fn dash_and_draw_on_a_shape_with_a_stroke_width_are_not_reported() {
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<shape d=\"M0 0 L10 10\" width=\"10\" height=\"10\"\n  stroke-width=\"2\" dash=\"10 5\" draw=\"0.5\"/>\n</composition>";
    assert_eq!(places(text), []);
}

#[test]
fn a_stroke_width_given_by_a_style_counts_as_written() {
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<style id=\"line\" stroke-width=\"2\"/>\n<shape class=\"line\" d=\"M0 0 L10 10\" width=\"10\" height=\"10\" dash=\"10 5\"/>\n</composition>";
    assert_eq!(places(text), []);
}

#[test]
fn a_shape_with_a_src_is_not_judged_for_its_line() {
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<shape src=\"icons/star.svg\" width=\"10\" height=\"10\" dash=\"10 5\"/>\n</composition>";
    assert_eq!(places(text), []);
}

#[test]
fn a_tween_or_a_set_of_draw_on_a_shape_with_no_stroke_width_is_reported_on_the_attributes_line() {
    let tween = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<shape d=\"M0 0 L10 10\" width=\"10\" height=\"10\">\n<tween duration=\"1\" to-draw=\"1\"/>\n</shape>\n</composition>";
    assert_eq!(places(tween), [("FS-0087", 3)]);
    let set = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<shape d=\"M0 0 L10 10\" width=\"10\" height=\"10\">\n<set draw=\"1\"/>\n</shape>\n</composition>";
    assert_eq!(places(set), [("FS-0087", 3)]);
}

#[test]
fn a_from_draw_or_a_to_draw_alone_on_a_shape_with_no_stroke_width_is_reported() {
    let from = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<shape d=\"M0 0 L10 10\" width=\"10\" height=\"10\">\n<tween duration=\"1\" from-draw=\"0\"/>\n</shape>\n</composition>";
    assert_eq!(places(from), [("FS-0087", 3)]);
    let to = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<shape d=\"M0 0 L10 10\" width=\"10\" height=\"10\">\n<tween duration=\"1\" to-draw=\"1\"/>\n</shape>\n</composition>";
    assert_eq!(places(to), [("FS-0087", 3)]);
}

#[test]
fn dash_is_not_animated_so_a_tween_of_it_is_not_judged_for_the_line() {
    // `dash` is not an animated property (`notation.md` section 8.1), so a `to-dash` is reported as an attribute the tween does
    // not take (`FS-0030`), not as a line problem: section 7.11 names `draw` alone for a tween.
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<shape d=\"M0 0 L10 10\" width=\"10\" height=\"10\">\n<tween duration=\"1\" to-dash=\"4 2\"/>\n</shape>\n</composition>";
    assert!(!places(text).iter().any(|(code, _)| *code == "FS-0087"), "{text}");
}

#[test]
fn dash_on_a_box_is_an_attribute_the_box_does_not_take() {
    assert_eq!(
        places("<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<box dash=\"10 5\"/>\n</composition>"),
        [("FS-0030", 2)]
    );
}

#[test]
fn a_negative_blur_on_a_shape_is_reported_on_its_line() {
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<shape d=\"M0 0 L10 10\" width=\"10\" height=\"10\"\n  blur=\"-2\"/>\n</composition>";
    assert_eq!(places(text), [("FS-0034", 3)]);
}

#[test]
fn a_blur_of_0_or_more_on_a_shape_is_not_reported() {
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<shape d=\"M0 0 L10 10\" width=\"10\" height=\"10\" blur=\"0\"/>\n</composition>";
    assert_eq!(places(text), []);
}
