//! The form of the whole file: every element must be one the notation has, in a place it may
//! stand, with attributes it takes, and the numbers and times it writes must be in the forms the
//! notation lists. `docs/spec/notation.md` sections 4, 6 and 13, and N7 to N9 of section 12.
//!
//! Nothing here repairs a value or lets a form through that the spec does not list. What an
//! attribute means, and the ranges it takes, belong to the part of the product that draws the
//! element; only the form of a number and of a time is judged here, for every element at once. An
//! `id` is a name and is not compared with another (N7), so two elements with one `id` are not
//! reported.

use crate::code::Code;
use crate::root::{found, shown, trimmed, Found, Spot};
use crate::schema::{Element, Form};
use crate::value::{self, NotANumber};

/// Judges every element under the root and the attributes of the root that it does not take.
///
/// `text` is the source, so that a name is shown as it was written, with its prefix. The values of
/// the root's own attributes are judged by `read_root`, and are not looked at here.
pub(crate) fn judge(document: &roxmltree::Document, text: &str, problems: &mut Vec<Found>) {
    let root = document.root_element();
    let namespace = root.tag_name().namespace();
    let spot_at = |offset: usize| {
        let position = document.text_pos_at(offset);
        Spot {
            line: position.row,
            column: position.col,
        }
    };

    // The elements still to judge, each with the kind it was read as. Iterative, so that a deeply
    // nested file cannot exhaust the stack.
    let mut pending: Vec<(roxmltree::Node, Element)> = vec![(root, Element::Composition)];
    while let Some((node, element)) = pending.pop() {
        attributes(node, element, text, &spot_at, problems);
        clip_pair(node, element, &spot_at, problems);

        for child in node.children().filter(roxmltree::Node::is_element) {
            let spot = spot_at(child.range().start);
            let written = written_name(text, child.range().start);
            let has_src = child
                .attributes()
                .any(|attribute| attribute.namespace().is_none() && attribute.name() == "src");
            let local = child.tag_name().name();

            if local == "composition" && child.tag_name().namespace() == namespace && !has_src {
                problems.push(found(
                    Code::ElementNotHere,
                    spot,
                    "a <composition> inside a composition needs the attribute `src`; without one it stands only as the root of a file"
                        .to_string(),
                ));
                continue;
            }
            let kind = if child.tag_name().namespace() == namespace {
                Element::named(local, has_src)
            } else {
                None
            };
            let Some(kind) = kind else {
                problems.push(found(Code::ElementNotKnown, spot, unknown(&written, element)));
                continue;
            };
            // A span stands inside a text or a span, and is not an element anywhere else.
            if kind == Element::Span && !element.children().contains(&Element::Span) {
                problems.push(found(
                    Code::ElementNotKnown,
                    spot,
                    format!(
                        "<span> is not a known element here; a <span> stands directly in {}",
                        list(&Element::Span.parents(), "or")
                    ),
                ));
                continue;
            }
            if !element.children().contains(&kind) {
                problems.push(found(Code::ElementNotHere, spot, misplaced(kind, element)));
            }
            pending.push((child, kind));
        }
    }
}

/// The attributes of one element: those it does not take, and the numbers and times it writes.
fn attributes(node: roxmltree::Node, element: Element, text: &str, spot_at: &dyn Fn(usize) -> Spot, problems: &mut Vec<Found>) {
    for attribute in node.attributes() {
        let spot = spot_at(attribute.range().start);
        let name = attribute.name();
        if attribute.namespace().is_some() || !element.takes(name) {
            let written = shown(&written_name(text, attribute.range().start));
            problems.push(found(
                Code::AttributeNotTaken,
                spot,
                format!("{} has no attribute `{written}`", element.tag()),
            ));
            continue;
        }
        let value = trimmed(attribute.value());
        match element.form(name) {
            Form::Other => {}
            Form::Number => match value::number(value) {
                Ok(_) => {}
                Err(NotANumber::Form) => problems.push(found(
                    Code::NotANumber,
                    spot,
                    format!(
                        "`{name}=\"{}\"` is not a number: an optional sign, digits with an optional point and an optional exponent, such as 12, -4, .5 or 2.5e-1",
                        shown(value)
                    ),
                )),
                Err(NotANumber::TooLarge) => problems.push(found(
                    Code::NotANumber,
                    spot,
                    format!("`{name}=\"{}\"` is too large: a number must be finite", shown(value)),
                )),
            },
            Form::WholeNumber => {
                if value::whole_number(value).is_none() {
                    problems.push(found(
                        Code::NotANumber,
                        spot,
                        format!("`{name}=\"{}\"` is not a whole number", shown(value)),
                    ));
                }
            }
            Form::Time => {
                if value::time(value).is_none() {
                    problems.push(found(
                        Code::NotATime,
                        spot,
                        format!(
                            "`{name}=\"{}\"` is not a time in seconds: digits with an optional point, such as 5 or 1.25",
                            shown(value)
                        ),
                    ));
                }
            }
        }
    }
}

/// The pair of a clip: an element that takes `start` and `duration` as a clip has both or neither
/// (`docs/spec/notation.md` section 5.2). One report, on the line of the element, for an element
/// with one of them and not the other.
fn clip_pair(node: roxmltree::Node, element: Element, spot_at: &dyn Fn(usize) -> Spot, problems: &mut Vec<Found>) {
    if !element.clip_pair() {
        return;
    }
    let written = |name: &str| {
        node.attributes()
            .any(|attribute| attribute.namespace().is_none() && attribute.name() == name)
    };
    if written("start") != written("duration") {
        problems.push(found(
            Code::ClipNotPaired,
            spot_at(node.range().start),
            format!("{} needs both start and duration, or neither", element.tag()),
        ));
    }
}

/// The message for an element the notation does not have, with what is taken where it stands.
fn unknown(written: &str, parent: Element) -> String {
    let name = shown(written);
    let taken = parent.children();
    if taken.is_empty() {
        format!("<{name}> is not a known element; no element is taken inside {}", parent.tag())
    } else {
        format!(
            "<{name}> is not a known element; inside {} the elements taken are {}",
            parent.tag(),
            list(taken, "and")
        )
    }
}

/// The message for a known element in a place it may not stand: where it may stand, and what the
/// place it is in does hold.
fn misplaced(kind: Element, parent: Element) -> String {
    let parents = kind.parents();
    if parents == [Element::Composition] {
        return format!("{} stands directly in the <composition>, not inside an element", with_article(kind));
    }
    let holds = if parent.children().is_empty() {
        format!("{} holds no element", parent.tag())
    } else {
        let taken = parent.children();
        format!("only {} {}", list(taken, "and"), if taken.len() == 1 { "is" } else { "are" })
    };
    format!(
        "{} is not known inside {}; {holds}; {} stands directly in {}",
        kind.tag(),
        parent.tag(),
        with_article(kind),
        list(&parents, "or")
    )
}

/// `a <box>`, `an <audio>`: the tag with the article that goes before it.
fn with_article(element: Element) -> String {
    let article = if element.name().starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!("{article} {}", element.tag())
}

/// `<a>`, `<a> and <b>`, `<a>, <b> and <c>`: the tags of the elements, joined with `last`.
fn list(elements: &[Element], last: &str) -> String {
    let tags: Vec<&str> = elements.iter().map(|element| element.tag()).collect();
    match tags.split_last() {
        None => String::new(),
        Some((only, [])) => (*only).to_string(),
        Some((end, rest)) => format!("{} {last} {end}", rest.join(", ")),
    }
}

/// The name of the element that opens at `offset`, as it is written, with its prefix.
fn written_name(text: &str, offset: usize) -> String {
    let rest = &text[offset..];
    let rest = rest.strip_prefix('<').unwrap_or(rest);
    rest.chars()
        .take_while(|character| !character.is_whitespace() && !matches!(character, '/' | '>' | '='))
        .collect()
}
