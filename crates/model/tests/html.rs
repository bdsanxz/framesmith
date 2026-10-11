use model::html::{after_doctype, parse};

#[test]
fn elements_know_their_parent_line_and_attributes() {
    let dom = parse("<!doctype html>\n<html>\n<body>\n  <div id=\"stage\" class='a b'\n       data-fs-size=1920x1080 hidden>\n    <p>Hello <b>there</b></p>\n  </div>\n</body>\n</html>\n");
    let tags: Vec<&str> = dom.elements.iter().map(|e| e.tag.as_str()).collect();
    assert_eq!(tags, vec!["html", "body", "div", "p", "b"]);
    let div = &dom.elements[2];
    assert_eq!((div.line, div.parent), (4, Some(1)));
    assert_eq!(div.value("id"), Some("stage"));
    assert_eq!(div.value("class"), Some("a b"));
    assert_eq!(div.value("hidden"), Some(""));
    let size = div.attr("data-fs-size").unwrap();
    assert_eq!((size.value.as_str(), size.line), ("1920x1080", 5));
    assert_eq!(dom.elements[4].parent, Some(3));
    assert!(dom.is_inside(4, 2));
    assert!(!dom.is_inside(2, 2));
}

#[test]
fn comments_and_void_elements_do_not_nest() {
    let dom = parse("<div><!-- <p>not here</p> --><img src=a.png><br><span>x</span></div><p>after</p>");
    let tags: Vec<(&str, Option<usize>)> = dom.elements.iter().map(|e| (e.tag.as_str(), e.parent)).collect();
    assert_eq!(
        tags,
        vec![("div", None), ("img", Some(0)), ("br", Some(0)), ("span", Some(0)), ("p", None)]
    );
}

#[test]
fn script_and_style_text_is_kept_whole_with_its_line() {
    let dom = parse("<head>\n<style>\n  p > b { color: red }\n</style>\n<script type=\"application/json\">\n{ \"a\": \"<div>\" }\n</SCRIPT>\n</head><div></div>");
    let style = &dom.elements[1];
    assert_eq!((style.tag.as_str(), style.text_line), ("style", 2));
    assert!(style.text.contains("p > b { color: red }"));
    let script = &dom.elements[2];
    assert_eq!(script.text, "\n{ \"a\": \"<div>\" }\n");
    assert_eq!(script.text_line, 5);
    // The <div> written inside the script is text, not an element.
    assert_eq!(dom.elements.len(), 4);
    assert_eq!(dom.elements[3].parent, None);
}

#[test]
fn implied_end_tags_are_followed() {
    let dom = parse("<ul><li>one<li>two</ul><p>a<p>b<div>c</div>");
    let parents: Vec<(&str, Option<usize>)> = dom.elements.iter().map(|e| (e.tag.as_str(), e.parent)).collect();
    assert_eq!(
        parents,
        vec![
            ("ul", None),
            ("li", Some(0)),
            ("li", Some(0)),
            ("p", None),
            ("p", None),
            ("div", None)
        ]
    );
}

#[test]
fn inline_svg_is_marked_and_self_closing_tags_close() {
    let dom = parse("<div/><svg viewBox=\"0 0 1 1\"><rect width=1 /><circle r=1><animateTransform/></circle></svg><span></span>");
    let facts: Vec<(&str, Option<usize>, bool)> = dom.elements.iter().map(|e| (e.tag.as_str(), e.parent, e.foreign)).collect();
    assert_eq!(
        facts,
        vec![
            // A slash does not close an HTML element.
            ("div", None, false),
            ("svg", Some(0), true),
            ("rect", Some(1), true),
            ("circle", Some(1), true),
            ("animatetransform", Some(3), true),
            ("span", Some(0), false),
        ]
    );
}

#[test]
fn entities_in_attribute_values_are_decoded() {
    let dom = parse("<a title=\"Tom &amp; Jerry &lt;3 &#65;&#x42; &unknown; &\" ID=x>");
    assert_eq!(dom.elements[0].value("title"), Some("Tom & Jerry <3 AB &unknown; &"));
    assert_eq!(dom.elements[0].value("id"), Some("x"));
}

#[test]
fn odd_input_never_stops_the_reader() {
    for text in [
        "",
        "<",
        "<div",
        "<div a=\"unclosed",
        "</nothing>",
        "a < b",
        "<!-- open",
        "<script>never closed",
        "<p =x>",
    ] {
        let _ = parse(text);
    }
}

#[test]
fn the_loader_goes_after_the_document_type() {
    assert_eq!(after_doctype("<!doctype html><html>"), 15);
    assert_eq!(after_doctype("\n <!-- note -->\n<!DOCTYPE html>\n<html>"), 31);
    assert_eq!(after_doctype("<html><!doctype html>"), 0);
    assert_eq!(after_doctype(""), 0);
}
