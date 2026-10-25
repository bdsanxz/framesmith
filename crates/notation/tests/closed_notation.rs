//! The notation is closed: an element it does not list, an attribute an element does not take and
//! an element in a place it may not stand are each reported with their line, and a composition that
//! differs from a valid one only by such a form is not read (`docs/spec/notation.md` section 3,
//! section 6 and section 13).

mod common;

use common::*;
use notation::{read_root, Code, Element};

#[test]
fn an_element_the_spec_does_not_list_is_reported_with_its_line() {
    let text = document("<box/>\n<circle radius=\"5\"/>\n<box/>");
    let problems = problems_of(&text);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].code(), Code::ElementNotKnown);
    assert_eq!(problems[0].line(), 3);
    assert_eq!(
        problems[0].message(),
        "<circle> is not a known element; inside <composition> the elements taken are <style>, <font>, <audio>, <box>, <rect>, <text>, <image>, <video>, <shape> and <composition src>"
    );
}

#[test]
fn an_unknown_element_names_what_is_taken_where_it_stands() {
    let in_a_box = only_problem_in("<box>\n<circle/>\n</box>");
    assert_eq!(in_a_box.line(), 3);
    assert_eq!(
        in_a_box.message(),
        "<circle> is not a known element; inside <box> the elements taken are <box>, <rect>, <text>, <image>, <video>, <shape>, <composition src>, <tween> and <set>"
    );
    let in_an_audio = only_problem_in("<audio src=\"a.wav\" duration=\"1\">\n<circle/>\n</audio>");
    assert_eq!(
        in_an_audio.message(),
        "<circle> is not a known element; inside <audio> the elements taken are <volume>"
    );
    let in_a_tween = only_problem_in("<box>\n<tween duration=\"1\" to-x=\"1\">\n<circle/>\n</tween>\n</box>");
    assert_eq!(in_a_tween.line(), 4);
    assert_eq!(
        in_a_tween.message(),
        "<circle> is not a known element; no element is taken inside <tween>"
    );
}

#[test]
fn every_name_the_spec_does_not_list_is_unknown_in_every_place() {
    for name in [
        "circle",
        "html",
        "div",
        "Box",
        "BOX",
        "group",
        "svg",
        "keyframes",
        "stagger",
        "scene",
        "p",
        "br",
    ] {
        for parent in Element::ALL {
            let text = inside(parent, &format!("<{name}/>"));
            let problems = problems_of(&text);
            assert_eq!(problems.len(), 1, "<{name}> in {}: {problems:#?}", parent.tag());
            assert_eq!(problems[0].code(), Code::ElementNotKnown, "<{name}> in {}", parent.tag());
            assert_eq!(
                problems[0].line(),
                line_of(&text, &format!("<{name}/>")),
                "<{name}> in {}",
                parent.tag()
            );
            assert!(problems[0].message().starts_with(&format!("<{name}> is not a known element")));
        }
    }
}

#[test]
fn an_unknown_element_is_not_read_further_and_the_rest_of_the_file_is() {
    // What is inside the unknown element has no table to be judged by; the siblings are judged.
    let problems = problems_in("<circle colour=\"red\">\n<box nonsense=\"1\"/>\n</circle>\n<box other=\"2\"/>");
    let places: Vec<(Code, u32)> = problems.iter().map(|problem| (problem.code(), problem.line())).collect();
    assert_eq!(places, [(Code::ElementNotKnown, 2), (Code::AttributeNotTaken, 5)]);
}

#[test]
fn an_element_with_a_prefix_or_in_another_namespace_is_not_known() {
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\" xmlns:x=\"urn:other\">\n<x:box/>\n<box xmlns=\"urn:other\"/>\n</composition>";
    let problems = problems_of(text);
    let places: Vec<(Code, u32)> = problems.iter().map(|problem| (problem.code(), problem.line())).collect();
    assert_eq!(places, [(Code::ElementNotKnown, 2), (Code::ElementNotKnown, 3)]);
    assert!(
        problems[0].message().starts_with("<x:box> is not a known element"),
        "{}",
        problems[0]
    );
}

#[test]
fn a_file_in_a_namespace_of_its_own_is_read_by_its_local_names() {
    let text =
        "<composition xmlns=\"urn:framesmith\" version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<box id=\"a\"/>\n</composition>";
    assert!(read_root(text.as_bytes()).is_ok());
}

#[test]
fn an_attribute_its_element_does_not_take_is_reported_with_its_line_the_attribute_and_the_element() {
    let rect = only_problem_in("<rect\n id=\"a\"\n colour=\"red\"/>");
    assert_eq!(worded(&rect), "line 4: FS-0030: <rect> has no attribute `colour`");

    let tween = only_problem_in("<box>\n<tween duration=\"1\"\n to-width=\"10\"/>\n</box>");
    assert_eq!(worded(&tween), "line 4: FS-0030: <tween> has no attribute `to-width`");

    let set = only_problem_in("<box>\n<set at=\"1\" duration=\"1\" x=\"2\"/>\n</box>");
    assert_eq!(worded(&set), "line 3: FS-0030: <set> has no attribute `duration`");

    let nested = only_problem_in("<composition src=\"scenes/a.xml\" width=\"640\"/>");
    assert_eq!(worded(&nested), "line 2: FS-0030: <composition src> has no attribute `width`");

    let font = only_problem_in("<font src=\"f.ttf\" id=\"f\"/>");
    assert_eq!(worded(&font), "line 2: FS-0030: <font> has no attribute `id`");

    let span = only_problem_in("<text>\n<span left=\"4\"/>\n</text>");
    assert_eq!(worded(&span), "line 3: FS-0030: <span> has no attribute `left`");
}

#[test]
fn the_root_takes_only_the_attributes_of_its_table() {
    // The root's `class` names the style `y`, which the file has, so the only reports are the two
    // attributes the root does not take.
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\" id=\"x\" class=\"y\"\n src=\"a.xml\" color=\"#ffffff\">\n<style id=\"y\"/>\n</composition>";
    let problems = problems_of(text);
    let words: Vec<String> = problems.iter().map(worded).collect();
    assert_eq!(
        words,
        [
            "line 2: FS-0030: <composition> has no attribute `src`",
            "line 2: FS-0030: <composition> has no attribute `color`"
        ]
    );
}

#[test]
fn an_attribute_with_a_prefix_is_not_taken_and_is_shown_as_written() {
    let text = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\" xmlns:x=\"urn:other\">\n<box x:id=\"a\" xml:lang=\"en\"/>\n</composition>";
    let words: Vec<String> = problems_of(text).iter().map(worded).collect();
    assert_eq!(
        words,
        [
            "line 2: FS-0030: <box> has no attribute `x:id`",
            "line 2: FS-0030: <box> has no attribute `xml:lang`"
        ]
    );
}

#[test]
fn names_of_forms_that_are_not_in_the_tables_are_attributes_that_are_not_taken() {
    // Forms that other works will list as unsupported are, until then, not taken like any other.
    for (element, attribute) in [
        ("box", "z-index"),
        ("box", "display"),
        ("box", "flex-wrap"),
        ("video", "loop"),
        ("tween", "stagger"),
        ("tween", "repeat"),
        ("text", "direction"),
        ("box", "style"),
        ("box", "onclick"),
        ("box", "BACKGROUND"),
        ("image", "alt"),
    ] {
        let body = match element {
            "tween" => format!("<box>\n<tween duration=\"1\" to-x=\"1\" {attribute}=\"1\"/>\n</box>"),
            other => format!("<{other} {attribute}=\"1\"/>"),
        };
        let problem = only_problem_in(&body);
        assert_eq!(problem.code(), Code::AttributeNotTaken, "{element} {attribute}");
        assert_eq!(problem.message(), format!("<{element}> has no attribute `{attribute}`"));
    }
}

#[test]
fn an_element_in_a_place_it_may_not_stand_is_reported_with_its_line_and_the_places_it_may_stand() {
    // A <box> inside a <text>.
    let text = inside(Element::Text, "<box/>");
    let problems = problems_of(&text);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].code(), Code::ElementNotHere);
    assert_eq!(problems[0].line(), line_of(&text, "<box/>"));
    assert_eq!(
        problems[0].message(),
        "<box> is not known inside <text>; only <span>, <tween> and <set> are; a <box> stands directly in <composition>, <box> or <rect>"
    );

    // A <style>, a <font> and an <audio> stand directly in the root.
    for (element, written) in [
        ("style", "<style id=\"s\"/>"),
        ("font", "<font src=\"f.ttf\"/>"),
        ("audio", "<audio src=\"a.wav\" duration=\"1\"/>"),
    ] {
        let text = inside(Element::Box, written);
        let problems = problems_of(&text);
        assert_eq!(problems.len(), 1, "{element}: {problems:#?}");
        assert_eq!(problems[0].code(), Code::ElementNotHere);
        assert_eq!(problems[0].line(), line_of(&text, written), "{element}");
        assert_eq!(
            problems[0].message(),
            format!(
                "<{element}> is not known inside <box>; {} <{element}> stands only directly in the <composition>, never inside an element",
                if element == "audio" { "an" } else { "a" }
            )
        );
    }

    // An <audio> holds <volume> only, and a <volume> stands only in an <audio>.
    let in_audio = problems_of(&inside(Element::Audio, "<tween duration=\"1\" to-x=\"1\"/>"));
    assert_eq!(in_audio.len(), 1);
    assert_eq!(
        in_audio[0].message(),
        "<tween> is not known inside <audio>; only <volume> is; a <tween> stands directly in <box>, <rect>, <text>, <span>, <image>, <video>, <shape> or <composition src>"
    );
    let volume_in_box = only_problem_in("<box>\n<volume value=\"1\"/>\n</box>");
    assert_eq!(
        worded(&volume_in_box),
        "line 3: FS-0031: <volume> is not known inside <box>; only <box>, <rect>, <text>, <image>, <video>, <shape>, <composition src>, <tween> and <set> are; a <volume> stands directly in <audio>"
    );

    // Where a place holds nothing.
    let in_font = only_problem_in("<font src=\"f.ttf\">\n<tween duration=\"1\" to-x=\"1\"/>\n</font>");
    assert_eq!(in_font.code(), Code::ElementNotHere);
    assert!(
        in_font
            .message()
            .starts_with("<tween> is not known inside <font>; <font> holds no element; a <tween> stands directly in "),
        "{in_font}"
    );
}

#[test]
fn a_span_outside_a_text_is_not_a_known_element_and_names_where_it_stands() {
    for parent in [Element::Composition, Element::Box, Element::Image, Element::Audio, Element::Nested] {
        let text = inside(parent, "<span/>");
        let problems = problems_of(&text);
        assert_eq!(problems.len(), 1, "{}: {problems:#?}", parent.tag());
        assert_eq!(problems[0].code(), Code::ElementNotKnown, "{}", parent.tag());
        assert_eq!(problems[0].line(), line_of(&text, "<span/>"));
        assert_eq!(
            problems[0].message(),
            "<span> is not a known element here; a <span> stands directly in <text> or <span>"
        );
    }
}

#[test]
fn a_composition_without_src_inside_a_composition_is_reported() {
    for parent in [Element::Composition, Element::Box] {
        let text = inside(parent, "<composition/>");
        let problems = problems_of(&text);
        assert_eq!(problems.len(), 1, "{}: {problems:#?}", parent.tag());
        assert_eq!(problems[0].code(), Code::ElementNotHere);
        assert_eq!(problems[0].line(), line_of(&text, "<composition/>"));
        assert_eq!(
            problems[0].message(),
            "a <composition> inside a composition needs the attribute `src`; without one it stands only as the root of a file"
        );
    }
}

#[test]
fn an_element_in_the_wrong_place_still_has_its_own_attributes_and_children_judged() {
    // The <box> may not stand in the <text>; its own attribute and its own child are judged by the
    // table of a <box>, so all three are reported, in the order of the file.
    let problems = problems_in("<text>\n<box nonsense=\"1\">\n<circle/>\n</box>\n</text>");
    let places: Vec<(Code, u32)> = problems.iter().map(|problem| (problem.code(), problem.line())).collect();
    assert_eq!(
        places,
        [(Code::ElementNotHere, 3), (Code::AttributeNotTaken, 3), (Code::ElementNotKnown, 4)]
    );
}

#[test]
fn every_pair_of_parent_and_child_is_taken_exactly_when_the_table_of_the_spec_says() {
    let mut taken = 0;
    let mut refused = 0;
    for parent in Element::ALL {
        for child in Element::ALL {
            if child == Element::Composition {
                continue; // the root stands in no place; tested on its own
            }
            let text = inside(parent, &written(child));
            let problems = problems_of(&text);
            let name = format!("{} in {}", child.tag(), parent.tag());
            if parent.children().contains(&child) {
                taken += 1;
                assert!(problems.is_empty(), "{name}: {problems:#?}");
            } else {
                refused += 1;
                assert_eq!(problems.len(), 1, "{name}: {problems:#?}");
                let wanted = if child == Element::Span {
                    Code::ElementNotKnown
                } else {
                    Code::ElementNotHere
                };
                assert_eq!(problems[0].code(), wanted, "{name}");
                assert_eq!(problems[0].line(), line_of(&text, &written(child)), "{name}");
                // The place where it may stand is in the message, whichever place it is in.
                for place in child.parents() {
                    assert!(problems[0].message().contains(place.tag()), "{name}: {}", problems[0]);
                }
            }
        }
    }
    // 14 kinds of child in 15 kinds of parent.
    assert_eq!(taken + refused, 14 * 15);
    assert!(taken > 30 && refused > 100, "{taken} {refused}");
}

#[test]
fn a_composition_that_differs_from_a_valid_one_only_by_an_unknown_attribute_is_not_read() {
    // For every element: the valid composition is read, and the same one with one attribute its
    // element does not take is refused with that attribute named.
    for element in Element::ALL {
        if matches!(element, Element::Composition) {
            continue;
        }
        let valid = inside(Element::Composition, &written(element));
        let valid = match element {
            // These elements stand only inside another one in a valid file.
            Element::Volume => inside(Element::Audio, "<volume value=\"1\"/>"),
            Element::Span => inside(Element::Text, "<span/>"),
            Element::Tween => inside(Element::Box, "<tween duration=\"1\" to-x=\"1\"/>"),
            Element::Set => inside(Element::Box, "<set x=\"1\"/>"),
            _ => valid,
        };
        assert!(read_root(valid.as_bytes()).is_ok(), "{}: {:?}", element.tag(), problems_of(&valid));

        let marked = |text: &str| {
            let open = format!("<{}", element.name());
            let at = text.rfind(&open).expect("the element is in the file") + open.len();
            format!("{} unknown-form=\"1\"{}", &text[..at], &text[at..])
        };
        let changed = marked(&valid);
        let problems = problems_of(&changed);
        assert_eq!(problems.len(), 1, "{}: {problems:#?}", element.tag());
        assert_eq!(problems[0].code(), Code::AttributeNotTaken, "{}", element.tag());
        assert!(
            problems[0].message().ends_with("has no attribute `unknown-form`"),
            "{}",
            problems[0]
        );
    }
    // And the root.
    let valid = document("<box/>");
    assert!(read_root(valid.as_bytes()).is_ok());
    let changed = valid.replacen("<composition ", "<composition unknown-form=\"1\" ", 1);
    assert_eq!(problems_of(&changed)[0].code(), Code::AttributeNotTaken);
}

#[test]
fn problems_of_the_root_and_of_what_is_in_it_come_in_the_order_of_the_file() {
    let text =
        "<composition version=\"2\" width=\"641\" height=\"360\" duration=\"2\">\n<circle/>\n<box colour=\"red\" x=\"a\"/>\n</composition>";
    let problems = problems_of(text);
    let places: Vec<(&str, u32)> = problems.iter().map(|problem| (problem.code().as_str(), problem.line())).collect();
    assert_eq!(
        places,
        [("FS-0005", 1), ("FS-0007", 1), ("FS-0029", 2), ("FS-0030", 3), ("FS-0032", 3)]
    );
}
