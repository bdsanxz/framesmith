//! Whether an `id` must be unique (`docs/spec/notation.md`, rule N7): no. An `id` is a name and refers
//! to nothing in version 1; two elements with one `id` are accepted and nothing is reported.

mod common;

use common::*;
use notation::read_root;

#[test]
fn two_elements_with_one_id_are_accepted_and_nothing_is_reported() {
    let text = document("<box id=\"panel\"/>\n<box id=\"panel\"/>");
    assert!(read_root(text.as_bytes()).is_ok());
    assert!(problems_of(&text).is_empty());
}

#[test]
fn an_element_and_an_audio_with_one_id_are_accepted() {
    let text = document("<audio id=\"bed\" src=\"a.wav\" duration=\"1\"/>\n<image id=\"bed\" src=\"i.png\"/>");
    assert!(read_root(text.as_bytes()).is_ok(), "{:?}", problems_of(&text));
}

#[test]
fn the_root_and_an_element_inside_it_may_share_an_id_and_so_may_nested_elements() {
    let text = "<composition id=\"x\" version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<box id=\"x\">\n<box id=\"x\">\n<text id=\"x\">\n<span id=\"x\"/>\n</text>\n</box>\n</box>\n<composition id=\"x\" src=\"a.xml\"/>\n</composition>";
    assert!(read_root(text.as_bytes()).is_ok(), "{:?}", problems_of(text));
}

#[test]
fn the_same_id_on_many_elements_and_an_empty_id_are_accepted() {
    let boxes: String = (0..50).map(|_| "<box id=\"same\"/>\n").collect();
    assert!(problems_of(&document(&boxes)).is_empty());
    assert!(problems_of(&document("<box id=\"\"/>\n<box id=\"\"/>")).is_empty());
    // An id differing only by case, or by surrounding white space, is no different matter.
    assert!(problems_of(&document("<box id=\"A\"/>\n<box id=\"a\"/>\n<box id=\" a \"/>")).is_empty());
}

#[test]
fn the_name_of_the_root_is_its_id_and_is_not_changed_by_a_repeat() {
    let text = "<composition id=\"demo\" version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<box id=\"demo\"/>\n</composition>";
    assert_eq!(read_root(text.as_bytes()).unwrap().id.as_deref(), Some("demo"));
}
