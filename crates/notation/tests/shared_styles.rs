//! Shared styles as the reader gives them (`docs/spec/notation.md` section 9 and N5): an element takes
//! the attributes of the styles it names by `class`; its own value wins over a style; of two styles
//! the one named later wins; a value a style gives is read on the element's line; a style attribute
//! the element does not take is reported on the element's line; the reports of the styles themselves
//! carry their lines; and the movement attributes of a style are read as the element's starting value.

mod common;

use common::{document, problems_of};
use notation::{read, Code, Element, Node, Tree};

fn tree_of(text: &str) -> Tree {
    match read(text.as_bytes()) {
        Ok(composition) => composition.tree,
        Err(problems) => panic!("the file is not valid: {problems:#?}"),
    }
}

/// The elements of one kind, in document order.
fn nodes_of(tree: &Tree, element: Element) -> Vec<&Node> {
    tree.nodes().iter().filter(|node| node.element() == element).collect()
}

/// The value of an attribute of an element as it is read, with the line the attribute is read on.
fn read_as(node: &Node, name: &str) -> Option<(String, u32)> {
    node.attribute(name)
        .map(|attribute| (attribute.value().to_string(), attribute.line()))
}

#[test]
fn two_boxes_that_name_one_style_take_its_background_and_size() {
    let text = document(
        "<style id=\"panel\" background=\"#e0a030\" width=\"30\" height=\"20\"/>\n\
         <box class=\"panel\" left=\"10\"/>\n\
         <box class=\"panel\" left=\"50\"/>",
    );
    let tree = tree_of(&text);
    let boxes = nodes_of(&tree, Element::Box);
    assert_eq!(boxes.len(), 2);
    for node in boxes {
        assert_eq!(read_as(node, "background"), Some(("#e0a030".to_string(), node.line())));
        assert_eq!(read_as(node, "width"), Some(("30".to_string(), node.line())));
        assert_eq!(read_as(node, "height"), Some(("20".to_string(), node.line())));
    }
}

#[test]
fn a_value_the_element_writes_itself_wins_over_the_style() {
    let text = document(
        "<style id=\"panel\" background=\"#e0a030\" width=\"30\"/>\n\
         <box class=\"panel\" background=\"#3060ff\"/>",
    );
    let tree = tree_of(&text);
    let node = nodes_of(&tree, Element::Box)[0];
    assert_eq!(read_as(node, "background"), Some(("#3060ff".to_string(), node.line())));
    assert_eq!(read_as(node, "width"), Some(("30".to_string(), node.line())));
}

#[test]
fn of_two_styles_the_one_named_later_in_class_wins() {
    let text = document(
        "<style id=\"first\" background=\"#ff3030\" height=\"12\"/>\n\
         <style id=\"second\" background=\"#30ff60\"/>\n\
         <box class=\"first second\"/>\n\
         <box class=\"second first\"/>",
    );
    let tree = tree_of(&text);
    let boxes = nodes_of(&tree, Element::Box);
    let (named_last, named_first) = (boxes[0], boxes[1]);
    assert_eq!(read_as(named_last, "background"), Some(("#30ff60".to_string(), named_last.line())));
    assert_eq!(read_as(named_last, "height"), Some(("12".to_string(), named_last.line())));
    assert_eq!(
        read_as(named_first, "background"),
        Some(("#ff3030".to_string(), named_first.line()))
    );
}

#[test]
fn a_style_attribute_the_element_does_not_take_is_reported_on_the_element_line() {
    let problems = problems_of(&document(
        "<style id=\"shade\" object-fit=\"cover\"/>\n\
         <text class=\"shade\">HELLO</text>",
    ));
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].code(), Code::AttributeNotTaken);
    assert_eq!(problems[0].line(), 3);
    assert_eq!(
        problems[0].message(),
        "<text> has no attribute `object-fit`, which the style `shade` gives it"
    );
}

#[test]
fn a_style_without_an_id_is_reported_on_its_line() {
    let problems = problems_of(&document("<style background=\"#ffffff\"/>"));
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].code(), Code::StyleWithoutId);
    assert_eq!(problems[0].line(), 2);
}

#[test]
fn a_second_style_with_one_id_is_reported_on_its_id_line() {
    let problems = problems_of(&document(
        "<style id=\"label\" color=\"#000000\"/>\n\
         <style id=\"label\" color=\"#ffffff\"/>",
    ));
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].code(), Code::StyleIdTwice);
    assert_eq!(problems[0].line(), 3);
    assert_eq!(problems[0].message(), "two <style> elements have the id `label`");
}

#[test]
fn a_class_that_names_no_style_of_the_file_is_reported_on_its_line() {
    let problems = problems_of(&document(
        "<style id=\"label\" color=\"#000000\"/>\n\
         <box class=\"label missing\"/>",
    ));
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].code(), Code::ClassNamesNoStyle);
    assert_eq!(problems[0].line(), 3);
    assert_eq!(
        problems[0].message(),
        "`class` names the style `missing`, and this file has no <style> with that id"
    );
}

#[test]
fn a_style_that_holds_an_element_is_reported_on_the_element_line_and_not_read_further() {
    let problems = problems_of(&document(
        "<style id=\"frame\">\n\
         <box width=\"no\"/>\n\
         </style>",
    ));
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].code(), Code::ElementNotHere);
    assert_eq!(problems[0].line(), 3);
    assert_eq!(
        problems[0].message(),
        "the style `frame` holds <box>, and a style holds attributes only; a <box> stands directly in <composition>, <box> or <rect>"
    );
}

#[test]
fn a_style_that_has_a_class_is_reported_on_the_class() {
    let problems = problems_of(&document("<style id=\"chain\" class=\"other\" color=\"#000000\"/>"));
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].code(), Code::AttributeNotTaken);
    assert_eq!(problems[0].line(), 2);
    assert_eq!(
        problems[0].message(),
        "the style `chain` has `class`; a style cannot take another style"
    );
}

#[test]
fn a_motion_attribute_in_a_style_is_the_elements_starting_value() {
    let text = document(
        "<style id=\"moving\" x=\"20\" opacity=\"0.5\"/>\n\
         <box class=\"moving\" left=\"90\" width=\"20\" height=\"16\" background=\"#ffffff\">\n\
         <tween duration=\"1\" to-x=\"60\"/>\n\
         </box>",
    );
    let tree = tree_of(&text);
    let node = nodes_of(&tree, Element::Box)[0];
    assert_eq!(read_as(node, "x"), Some(("20".to_string(), node.line())));
    assert_eq!(read_as(node, "opacity"), Some(("0.5".to_string(), node.line())));
    let tween = nodes_of(&tree, Element::Tween);
    assert_eq!(tween.len(), 1, "the element's own tween is read, and no tween is shared");
}

#[test]
fn a_style_counter_is_checked_on_the_element_that_takes_the_style() {
    // `count` is taken by a `<text>` only: a style that gives it to a `<box>` is reported on the box's line.
    let problems = problems_of(&document(
        "<style id=\"counted\" count=\"3\"/>\n\
         <box class=\"counted\"/>",
    ));
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].code(), Code::AttributeNotTaken);
    assert_eq!(problems[0].line(), 3);
    assert_eq!(
        problems[0].message(),
        "<box> has no attribute `count`, which the style `counted` gives it"
    );
}

#[test]
fn a_tween_is_never_shared_through_a_style() {
    // A `<tween>` is written inside its element; it is not an attribute and cannot be named by `class`.
    let problems = problems_of(&document(
        "<style id=\"move\">\n\
         <tween duration=\"1\" to-x=\"60\"/>\n\
         </style>\n\
         <box class=\"move\"/>",
    ));
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].code(), Code::ElementNotHere);
    assert_eq!(problems[0].line(), 3);
}
