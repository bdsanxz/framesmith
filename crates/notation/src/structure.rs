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
use crate::styles::{self, Effective, Styles};
use crate::value::{self, NotANumber};

/// Judges every element under the root, and the attributes of the root that it does not take.
///
/// `text` is the source, so that a name is shown as it was written, with its prefix. The values of
/// the root's own attributes are judged by `read_root`, and are not looked at here. Each element is
/// judged by the attributes it reads: its own, and those the styles it names give it (`styles`).
pub(crate) fn judge(document: &roxmltree::Document, text: &str, styles: &Styles, problems: &mut Vec<Found>) {
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
        let read = styles::effective(node, text, element, styles, &spot_at);
        if let Some((name, spot)) = styles::unknown_class(&read, element, styles) {
            problems.push(found(
                Code::ClassNamesNoStyle,
                spot,
                format!(
                    "`class` names the style `{}`, and this file has no <style> with that id",
                    shown(&name)
                ),
            ));
        }
        attributes(node, element, &read, problems);
        clip_pair(&read, element, spot_at(node.range().start), problems);
        if element == Element::Text {
            counting(node, &read, spot_at(node.range().start), &spot_at, problems);
        }

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
            // A style holds attributes only: an element written inside one is reported, and is not read further.
            if element == Element::Style {
                problems.push(found(
                    Code::ElementNotHere,
                    spot,
                    format!(
                        "the style `{}` holds {}, and a style holds attributes only; {} stands directly in {}",
                        shown(&style_id(node)),
                        kind.tag(),
                        with_article(kind),
                        list(&kind.parents(), "or")
                    ),
                ));
                continue;
            }
            if !element.children().contains(&kind) {
                problems.push(found(Code::ElementNotHere, spot, misplaced(kind, element)));
            } else if kind == Element::Tween {
                tween(child, element, spot, &spot_at, problems);
            } else if kind == Element::Set {
                set(child, element, spot, &spot_at, problems);
            }
            pending.push((child, kind));
        }
    }
}

/// The attributes of one element as it reads them: those it does not take, and the numbers and times
/// it writes. A value a style gives is judged on the element, and reported on the element's place
/// (`docs/spec/check-command.md` section 3c).
fn attributes(node: roxmltree::Node, element: Element, read: &[Effective], problems: &mut Vec<Found>) {
    for attribute in read {
        let spot = attribute.spot;
        let name = attribute.name.as_str();
        if element == Element::Style && name == "class" && !attribute.prefixed {
            problems.push(found(
                Code::AttributeNotTaken,
                spot,
                format!(
                    "the style `{}` has `class`; a style cannot take another style",
                    shown(&style_id(node))
                ),
            ));
            continue;
        }
        if attribute.prefixed || !element.takes(name) {
            let message = match &attribute.from {
                // A number that counts is for a text alone (`docs/spec/notation.md` section 7.12 and the messages of
                // section 13).
                None if !attribute.prefixed && matches!(name, "count" | "count-format") => {
                    format!("`{name}` is for a <text>, not {}", element.tag())
                }
                Some(style) => format!(
                    "{} has no attribute `{}`, which the style `{}` gives it",
                    element.tag(),
                    shown(&attribute.written),
                    shown(style)
                ),
                None => format!("{} has no attribute `{}`", element.tag(), shown(&attribute.written)),
            };
            problems.push(found(Code::AttributeNotTaken, spot, message));
            continue;
        }
        let value = attribute.value.as_str();
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
/// with one of them and not the other. A value a style gives counts as written on the element.
fn clip_pair(read: &[Effective], element: Element, spot: Spot, problems: &mut Vec<Found>) {
    if !element.clip_pair() {
        return;
    }
    let written = |name: &str| read.iter().any(|attribute| !attribute.prefixed && attribute.name == name);
    if written("start") != written("duration") {
        problems.push(found(
            Code::ClipNotPaired,
            spot,
            format!("{} needs both start and duration, or neither", element.tag()),
        ));
    }
}

/// What a `<tween>` needs, and what it may change (`docs/spec/notation.md` section 8.2): a `duration` above 0, at least one
/// `from-` or `to-` attribute (one of a name that is not an animated property counts, since it is reported on its own), and only properties that the element it moves takes as attributes (section 8.1). A missing or
/// zero duration and a tween with no property are reported on the tween's line; a property its element does not take, on the
/// attribute. A `duration` that is not a time is reported by the form check, and not again here. A tween takes no `class`, so
/// its own attributes are all it reads.
fn tween<F: Fn(usize) -> Spot>(node: roxmltree::Node, parent: Element, spot: Spot, spot_at: &F, problems: &mut Vec<Found>) {
    let own = || node.attributes().filter(|attribute| attribute.namespace().is_none());
    match own().find(|attribute| attribute.name() == "duration") {
        None => problems.push(found(
            Code::TweenNoDuration,
            spot,
            "<tween> has no `duration`; a tween needs a duration above 0".to_string(),
        )),
        Some(duration) => {
            if value::time(duration.value()).is_some_and(|time| time == timebase::Time::ZERO) {
                problems.push(found(
                    Code::TweenNoDuration,
                    spot,
                    format!(
                        "<tween> has `duration=\"{}\"`; a tween needs a duration above 0",
                        shown(duration.value())
                    ),
                ));
            }
        }
    }
    let mut changes = false;
    for attribute in own() {
        let name = attribute.name();
        let Some(property) = name.strip_prefix("from-").or_else(|| name.strip_prefix("to-")) else {
            continue;
        };
        changes = true;
        if !Element::Tween.takes(name) {
            // Not an animated property: the form check reports the attribute as one the tween does not take, and that one
            // report is the tween's.
            continue;
        }
        if !parent.takes(property) {
            problems.push(found(
                Code::PropertyNotTaken,
                spot_at(attribute.range().start),
                format!(
                    "`{name}` changes `{property}`, which {} does not take; a tween changes only the properties of the element it moves",
                    parent.tag()
                ),
            ));
        }
    }
    if let Some(ease) = own().find(|attribute| attribute.name() == "ease") {
        if crate::easing::easing(ease.value()).is_none() {
            problems.push(found(
                Code::EaseNotKnown,
                spot_at(ease.range().start),
                format!(
                    "`ease=\"{}\"` is not a known easing; the easings taken are {}",
                    shown(ease.value()),
                    crate::easing::taken()
                ),
            ));
        }
    }
    if !changes {
        problems.push(found(
            Code::TweenNoProperty,
            spot,
            "<tween> changes no property; a tween needs at least one `from-` or `to-` attribute".to_string(),
        ));
    }
}

/// What a `<set>` needs, and what it may change (`docs/spec/notation.md` section 8.3): at least one property (an attribute
/// other than `at` counts, since one that is not an animated property is reported on its own), and only properties that the
/// element it moves takes as attributes (section 8.1). A set with no property is reported on the set's line; a property its
/// element does not take, on the attribute. A set takes no `class`, so its own attributes are all it reads.
fn set<F: Fn(usize) -> Spot>(node: roxmltree::Node, parent: Element, spot: Spot, spot_at: &F, problems: &mut Vec<Found>) {
    let mut changes = false;
    for attribute in node.attributes().filter(|attribute| attribute.namespace().is_none()) {
        let name = attribute.name();
        if name == "at" {
            continue;
        }
        changes = true;
        if !Element::Set.takes(name) {
            // Not an animated property: the form check reports the attribute as one the set does not take, and that one
            // report is the set's.
            continue;
        }
        if !parent.takes(name) {
            problems.push(found(
                Code::SetPropertyNotTaken,
                spot_at(attribute.range().start),
                format!(
                    "`{name}` changes `{name}`, which {} does not take; a set changes only the properties of the element it moves",
                    parent.tag()
                ),
            ));
        }
    }
    if !changes {
        problems.push(found(
            Code::SetNoProperty,
            spot,
            "<set> changes no property; a set needs at least one property, such as `x`".to_string(),
        ));
    }
}

/// A `<text>` that counts (`docs/spec/notation.md` section 7.12): its `count-format` must be one of the forms and needs
/// `count`, each reported on the attribute; a text with `count` holds no words other than white space and no `<span>`,
/// reported once on the text's line; and a `<tween>` or `<set>` of `count` needs `count` on the text, reported on each of
/// its attributes that changes `count`. Every report is `FS-0067`. What the text's styles give it counts as written.
fn counting<F: Fn(usize) -> Spot>(node: roxmltree::Node, read: &[Effective], spot: Spot, spot_at: &F, problems: &mut Vec<Found>) {
    let given = |name: &str| read.iter().find(|attribute| !attribute.prefixed && attribute.name == name);
    let counts = given("count").is_some();
    if let Some(format) = given("count-format") {
        if crate::count::CountFormat::read(&format.value).is_none() {
            problems.push(found(
                Code::CountNotWritten,
                format.spot,
                format!(
                    "`count-format=\"{}\"` is not a form of number; the forms are `0`, `0.00`, `0,0`, `0,0.00`, `m:ss` and `m:ss.0`, with up to six decimals",
                    shown(&format.value)
                ),
            ));
        }
        if !counts {
            problems.push(found(
                Code::CountNotWritten,
                format.spot,
                "`count-format` needs `count`".to_string(),
            ));
        }
    }
    let words = node.children().any(|child| {
        (child.is_text() && !child.text().unwrap_or("").trim().is_empty()) || (child.is_element() && child.tag_name().name() == "span")
    });
    if counts && words {
        problems.push(found(
            Code::CountNotWritten,
            spot,
            "a <text> that counts holds no words of its own: its text is the number".to_string(),
        ));
    }
    if !counts {
        count_tweens(node, "needs the attribute `count` on its <text>", spot_at, problems);
    }
}

/// Reports each tween or set of `count` inside `node`, which has no `count` to change, on the line of its attribute,
/// with `why` after "a tween of `count`".
fn count_tweens<F: Fn(usize) -> Spot>(node: roxmltree::Node, why: &str, spot_at: &F, problems: &mut Vec<Found>) {
    for child in node.children().filter(roxmltree::Node::is_element) {
        let (changes, kind): (&[&str], &str) = match child.tag_name().name() {
            "tween" => (&["from-count", "to-count"], "tween"),
            "set" => (&["count"], "set"),
            _ => continue,
        };
        for attribute in child.attributes().filter(|attribute| attribute.namespace().is_none()) {
            if changes.contains(&attribute.name()) {
                problems.push(found(
                    Code::CountNotWritten,
                    spot_at(attribute.range().start),
                    format!("a {kind} of `count` {why}"),
                ));
            }
        }
    }
}

/// The `id` of a `<style>` as it is written, or nothing when it has none.
fn style_id(node: roxmltree::Node) -> String {
    node.attributes()
        .find(|attribute| attribute.namespace().is_none() && attribute.name() == "id")
        .map(|attribute| trimmed(attribute.value()).to_string())
        .unwrap_or_default()
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
        return format!(
            "{} is not known inside {}; {} stands only directly in the <composition>, never inside an element",
            kind.tag(),
            parent.tag(),
            with_article(kind)
        );
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
pub(crate) fn written_name(text: &str, offset: usize) -> String {
    let rest = &text[offset..];
    let rest = rest.strip_prefix('<').unwrap_or(rest);
    rest.chars()
        .take_while(|character| !character.is_whitespace() && !matches!(character, '/' | '>' | '='))
        .collect()
}
