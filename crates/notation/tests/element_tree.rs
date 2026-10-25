//! A valid composition file gives the tree of its elements: in document order, each with the
//! attributes it writes, as written, and the lines they are on (`docs/spec/render-command.md`
//! section 11). The value forms a drawing part reads from an attribute: a length and a colour
//! (`docs/spec/notation.md` section 4).

mod common;

use common::document;
use notation::{read, read_root, Colour, Element, Length};

const FILE: &str = "<composition version=\"1\" id=\"demo\" width=\"640\" height=\"360\" duration=\"2\">
  <box id=\"first\" left=\"10\" top=\"20\"
       background=\"#ff0000\">
    <tween at=\"0\" duration=\"1\" to-x=\"10\"/>
  </box>
  <text id=\"words\">hello <span color=\"#ffffff\">there</span></text>
  <box id=\"last\"/>
</composition>
";

#[test]
fn the_elements_come_in_document_order_each_after_the_one_it_stands_in() {
    let composition = read(FILE.as_bytes()).unwrap();
    let nodes = composition.tree.nodes();
    let order: Vec<(Element, Option<usize>, u32)> = nodes.iter().map(|node| (node.element(), node.parent(), node.line())).collect();
    assert_eq!(
        order,
        vec![
            (Element::Composition, None, 1),
            (Element::Box, Some(0), 2),
            (Element::Tween, Some(1), 4),
            (Element::Text, Some(0), 6),
            (Element::Span, Some(3), 6),
            (Element::Box, Some(0), 7),
        ]
    );
    assert_eq!(composition.tree.root().element(), Element::Composition);
}

#[test]
fn the_attributes_are_kept_as_written_in_order_with_their_lines() {
    let composition = read(FILE.as_bytes()).unwrap();
    let first = &composition.tree.nodes()[1];
    let written: Vec<(&str, &str, u32)> = first
        .attributes()
        .iter()
        .map(|attribute| (attribute.name(), attribute.value(), attribute.line()))
        .collect();
    assert_eq!(
        written,
        vec![
            ("id", "first", 2),
            ("left", "10", 2),
            ("top", "20", 2),
            ("background", "#ff0000", 3)
        ]
    );
    assert_eq!(first.attribute("top").map(|attribute| attribute.value()), Some("20"));
    assert!(first.attribute("width").is_none());
    assert_eq!(
        composition.tree.root().attribute("duration").map(|attribute| attribute.value()),
        Some("2")
    );
}

#[test]
fn the_white_space_round_a_written_value_is_not_part_of_it() {
    let text = document("<box left=\"  12\n\" width=\"\t5 \"/>");
    let composition = read(text.as_bytes()).unwrap();
    let node = &composition.tree.nodes()[1];
    assert_eq!(node.attribute("left").unwrap().value(), "12");
    assert_eq!(node.attribute("width").unwrap().value(), "5");
}

#[test]
fn reading_the_tree_gives_the_same_root_as_reading_the_root() {
    assert_eq!(read(FILE.as_bytes()).unwrap().root, read_root(FILE.as_bytes()).unwrap());
}

#[test]
fn a_file_with_a_problem_has_no_tree_and_the_same_problems_as_before() {
    let text = document("<box glow=\"1\"/>");
    assert_eq!(read(text.as_bytes()).unwrap_err(), read_root(text.as_bytes()).unwrap_err());
}

#[test]
fn a_nested_file_gives_each_element_the_place_of_the_one_it_stands_in() {
    let depth = 100;
    let body = format!("{}{}", "<box>".repeat(depth), "</box>".repeat(depth));
    let composition = read(document(&body).as_bytes()).unwrap();
    let nodes = composition.tree.nodes();
    assert_eq!(nodes.len(), depth + 1);
    assert_eq!(nodes[depth].parent(), Some(depth - 1));
}

fn length(value: &str) -> Option<Length> {
    let text = document(&format!("<box width=\"{value}\"/>"));
    let composition = read(text.as_bytes()).unwrap();
    composition.tree.nodes()[1].attribute("width").unwrap().length()
}

#[test]
fn a_length_is_a_number_of_pixels_or_a_percentage() {
    assert_eq!(length("40"), Some(Length::Pixels(40.0)));
    assert_eq!(length("-4"), Some(Length::Pixels(-4.0)));
    assert_eq!(length("+0.5"), Some(Length::Pixels(0.5)));
    assert_eq!(length(".5"), Some(Length::Pixels(0.5)));
    assert_eq!(length("1e3"), Some(Length::Pixels(1000.0)));
    assert_eq!(length("50%"), Some(Length::Percent(50.0)));
    assert_eq!(length("-10%"), Some(Length::Percent(-10.0)));
    assert_eq!(length("2.5e1%"), Some(Length::Percent(25.0)));
}

#[test]
fn a_value_that_is_no_length_is_none() {
    for value in ["abc", "10px", "50 %", "%", "", "inf", "nan", "1e999", "1,5", "50%%", "- 4"] {
        assert_eq!(length(value), None, "{value:?}");
    }
}

#[test]
fn a_colour_is_read_with_its_alpha_and_anything_else_is_none() {
    let text = document(
        "<box background=\"#10a0ff\"/><box background=\"#10a0ff80\"/><box background=\"red\"/><box background=\"#fff\"/><box background=\"10a0ff\"/>",
    );
    let composition = read(text.as_bytes()).unwrap();
    let colours: Vec<Option<Colour>> = composition.tree.nodes()[1..]
        .iter()
        .map(|node| node.attribute("background").unwrap().colour())
        .collect();
    let colour = |red, green, blue, alpha| Some(Colour { red, green, blue, alpha });
    assert_eq!(
        colours,
        vec![colour(0x10, 0xa0, 0xff, 255), colour(0x10, 0xa0, 0xff, 0x80), None, None, None]
    );
}

#[test]
fn a_written_value_is_shown_on_one_line_the_way_a_message_shows_it() {
    let text = document("<box background=\"a&#9;b\"/>");
    let composition = read(text.as_bytes()).unwrap();
    assert_eq!(composition.tree.nodes()[1].attribute("background").unwrap().shown(), "a\\u{9}b");
}
