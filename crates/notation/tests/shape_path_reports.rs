//! What a `<shape>` needs for its path (`docs/spec/shape.md`, section 6; `docs/spec/check-command.md`,
//! section 3a): a `d` that is not empty, or a `src` whose file is read where the shape is drawn. A shape with
//! neither is `FS-0088` on the line of the element.

mod common;

/// The codes and lines of the problems of a file: `FS-0088` is reported on the line of the element.
fn places(text: &str) -> Vec<(&'static str, u32)> {
    common::problems_of(text)
        .iter()
        .map(|problem| (problem.code().as_str(), problem.line()))
        .collect()
}

#[test]
fn a_shape_with_no_d_and_no_src_is_reported_on_the_elements_line() {
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<shape width=\"10\" height=\"10\"/>\n</composition>";
    assert_eq!(places(text), [("FS-0088", 2)]);
}

#[test]
fn a_shape_with_an_empty_d_and_no_src_is_reported() {
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<shape d=\"  \" width=\"10\" height=\"10\"/>\n</composition>";
    assert_eq!(places(text), [("FS-0088", 2)]);
}

#[test]
fn a_shape_with_a_d_is_not_reported_for_its_path() {
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<shape d=\"M0 0 L10 10\" width=\"10\" height=\"10\"/>\n</composition>";
    assert_eq!(places(text), []);
}

#[test]
fn a_shape_with_a_src_is_not_reported_for_its_path() {
    // Whether the file holds paths is judged where the file is read; the form check takes the `src` as written.
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<shape src=\"icons/star.svg\" width=\"10\" height=\"10\"/>\n</composition>";
    assert_eq!(places(text), []);
}

#[test]
fn a_shape_with_a_d_given_by_a_style_is_not_reported_for_its_path() {
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<style id=\"line\" d=\"M0 0 L10 10\"/>\n<shape class=\"line\" width=\"10\" height=\"10\"/>\n</composition>";
    assert_eq!(places(text), []);
}
