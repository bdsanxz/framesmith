//! The parts of a `<text>` are its words and its `<span>`s in document order, and a span inside a span is a part of the
//! span that holds it, with its own words (`docs/spec/notation.md` section 6.5; `docs/spec/text.md` section 6.5).

use notation::{read, Element, Node, Part, Tree};

const FILE: &str = "<composition version=\"1\" id=\"demo\" width=\"640\" height=\"360\" duration=\"2\">
  <text id=\"words\">hello <span color=\"#ffffff\">there</span> and <span font-weight=\"700\">more <span color=\"#ff0000\">nested</span> words</span>!</text>
  <text id=\"plain\">just words</text>
  <text id=\"empty\"><span color=\"#ffffff\"/></text>
</composition>
";

/// The text element with the given `id` attribute.
fn text_named<'a>(tree: &'a Tree, id: &str) -> &'a Node {
    tree.nodes()
        .iter()
        .find(|node| node.element() == Element::Text && node.attribute("id").map(|a| a.value()) == Some(id))
        .unwrap_or_else(|| panic!("no text named {id}"))
}

/// The words of a part, or `None` for a span.
fn words_of(part: &Part) -> Option<&str> {
    match part {
        Part::Words(words) => Some(words),
        Part::Span(_) => None,
    }
}

/// The index in the tree of a span part.
fn span_of(part: &Part) -> usize {
    match part {
        Part::Span(index) => *index,
        Part::Words(words) => panic!("expected a span, found the words {words:?}"),
    }
}

#[test]
fn the_parts_of_a_text_are_its_words_and_spans_in_document_order() {
    let composition = read(FILE.as_bytes()).unwrap();
    let tree = &composition.tree;
    let parts = text_named(tree, "words").parts();
    assert_eq!(parts.len(), 5, "{parts:?}");
    assert_eq!(words_of(&parts[0]), Some("hello "));
    assert_eq!(words_of(&parts[2]), Some(" and "));
    assert_eq!(words_of(&parts[4]), Some("!"));
    let first = span_of(&parts[1]);
    let second = span_of(&parts[3]);
    assert!(first < second, "the spans are in document order: {first} {second}");
    assert_eq!(tree.nodes()[first].element(), Element::Span);
    assert_eq!(tree.nodes()[second].element(), Element::Span);
}

#[test]
fn the_words_of_a_span_are_its_parts_and_a_span_inside_it_is_one_of_them() {
    let composition = read(FILE.as_bytes()).unwrap();
    let tree = &composition.tree;
    let parts = text_named(tree, "words").parts();
    let first = span_of(&parts[1]);
    assert_eq!(tree.nodes()[first].parts().len(), 1, "{:?}", tree.nodes()[first].parts());
    assert_eq!(words_of(&tree.nodes()[first].parts()[0]), Some("there"));

    let second = span_of(&parts[3]);
    let inner = tree.nodes()[second].parts();
    assert_eq!(inner.len(), 3, "{inner:?}");
    assert_eq!(words_of(&inner[0]), Some("more "));
    assert_eq!(words_of(&inner[2]), Some(" words"));
    let nested = span_of(&inner[1]);
    assert_eq!(
        tree.nodes()[nested].parent(),
        Some(second),
        "the nested span stands in its outer span"
    );
    assert_eq!(words_of(&tree.nodes()[nested].parts()[0]), Some("nested"));
}

#[test]
fn a_text_with_no_span_has_its_words_as_one_part_and_an_empty_span_has_none() {
    let composition = read(FILE.as_bytes()).unwrap();
    let tree = &composition.tree;
    let plain = text_named(tree, "plain").parts();
    assert_eq!(plain.len(), 1, "{plain:?}");
    assert_eq!(words_of(&plain[0]), Some("just words"));

    let empty = text_named(tree, "empty").parts();
    assert_eq!(empty.len(), 1, "{empty:?}");
    let span = span_of(&empty[0]);
    assert!(tree.nodes()[span].parts().is_empty(), "{:?}", tree.nodes()[span].parts());
}
