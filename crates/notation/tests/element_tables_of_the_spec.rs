//! The tables of the program are the tables of `docs/spec/notation.md` section 6: for each element
//! the attributes it takes, the elements it holds and the form of each attribute that every element
//! reads alike. The spec is read here as text, so a change to either side that the other does not
//! follow fails a test that names the element.

mod common;

use notation::{Element, Form};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

fn spec() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec/notation.md");
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The text of the sections `### 6.N ...` of the spec, each with the element its heading names.
fn sections(text: &str) -> Vec<(Element, String)> {
    let mut found: Vec<(Element, String)> = Vec::new();
    let mut current: Option<(Element, String)> = None;
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("### 6.") {
            if let Some(done) = current.take() {
                found.push(done);
            }
            let element = if heading.contains("`<composition>` (the root)") {
                Some(Element::Composition)
            } else if heading.contains("`<composition src>`") {
                Some(Element::Nested)
            } else {
                Element::ALL.into_iter().find(|element| {
                    !matches!(element, Element::Composition | Element::Nested) && heading.contains(&format!("`<{}>`", element.name()))
                })
            };
            current = element.map(|element| (element, String::new()));
            continue;
        }
        if line.starts_with("## ") || line.starts_with("### ") {
            if let Some(done) = current.take() {
                found.push(done);
            }
            continue;
        }
        if let Some((_, body)) = current.as_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some(done) = current.take() {
        found.push(done);
    }
    found
}

/// The rows of the table of a section: the attribute and its value form.
fn rows(body: &str) -> Vec<(String, String)> {
    body.lines()
        .filter(|line| line.starts_with('|'))
        .filter_map(|line| {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            let name = cells.first()?.strip_prefix('`')?.strip_suffix('`')?;
            Some((name.to_string(), cells.get(1)?.to_string()))
        })
        .collect()
}

/// The elements named after `Children:` in a section.
fn children(body: &str) -> BTreeSet<&'static str> {
    let Some((_, after)) = body.split_once("Children:") else {
        return BTreeSet::new();
    };
    let end = after
        .find(';')
        .into_iter()
        .chain(after.find(". "))
        .chain(after.find(".\n"))
        .min()
        .unwrap_or(after.len());
    let list = &after[..end];
    Element::ALL
        .into_iter()
        .filter(|child| list.contains(&format!("`{}`", child.tag())))
        .map(Element::tag)
        .collect()
}

fn tags(elements: &[Element]) -> BTreeSet<&'static str> {
    elements.iter().map(|element| element.tag()).collect()
}

#[test]
fn there_is_a_section_for_every_element() {
    let sections = sections(&spec());
    let named: BTreeSet<&str> = sections.iter().map(|(element, _)| element.tag()).collect();
    let all: BTreeSet<&str> = Element::ALL.iter().map(|element| element.tag()).collect();
    assert_eq!(named, all);
    assert_eq!(sections.len(), 15);
}

#[test]
fn each_element_takes_the_attributes_of_its_table_and_no_others() {
    for (element, body) in sections(&spec()) {
        let listed: BTreeSet<String> = rows(&body).into_iter().map(|(name, _)| name).collect();
        let taken: BTreeSet<String> = element.attributes().into_iter().map(str::to_string).collect();
        if element == Element::Style {
            // A style takes `id` and any attribute of an element that takes a class (section 9).
            assert_eq!(
                listed,
                BTreeSet::from(["id".to_string(), "(any attribute of the element that takes the style)".to_string()])
            );
            assert!(taken.contains("id"));
            for name in [
                "font-size",
                "color",
                "src",
                "object-fit",
                "width",
                "version",
                "family",
                "volume",
                "x",
                "start",
            ] {
                assert!(taken.contains(name), "a style takes {name}");
            }
            for name in ["at", "ease", "from-x", "to-x", "value", "label"] {
                assert!(!taken.contains(name), "a style does not take {name}");
            }
            continue;
        }
        let missing: Vec<&String> = listed.difference(&taken).collect();
        let extra: Vec<&String> = taken.difference(&listed).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "{}: the spec lists and the program does not take {missing:?}; the program takes and the spec does not list {extra:?}",
            element.tag()
        );
        assert_eq!(
            listed.len(),
            element.attributes().len(),
            "{}: an attribute listed twice",
            element.tag()
        );
    }
}

#[test]
fn each_element_holds_the_elements_its_section_names() {
    for (element, body) in sections(&spec()) {
        if body.contains("Another name for `<box>`: same attributes, same children") {
            // A <rect> is a <box> by another name.
            assert_eq!(element.children(), Element::Box.children());
            continue;
        }
        assert_eq!(children(&body), tags(element.children()), "{}", element.tag());
    }
}

#[test]
fn the_table_of_where_an_element_may_stand_is_the_table_of_the_program() {
    let text = spec();
    let start = text.find("**Where an element may stand.**").expect("the table is in the spec");
    let table: Vec<(String, String)> = text[start..]
        .lines()
        .skip_while(|line| !line.starts_with("| Parent"))
        .skip(2)
        .take_while(|line| line.starts_with('|'))
        .map(|line| {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            (cells[0].to_string(), cells[1].to_string())
        })
        .collect();
    assert_eq!(table.len(), 6);
    let mut checked = 0;
    for (parents, holds) in &table {
        for element in Element::ALL {
            if parents.contains(&format!("`{}`", element.tag())) {
                let written: BTreeSet<&str> = Element::ALL
                    .into_iter()
                    .filter(|child| holds.contains(&format!("`{}`", child.tag())))
                    .map(Element::tag)
                    .collect();
                assert_eq!(written, tags(element.children()), "{}", element.tag());
                checked += 1;
            }
        }
    }
    // The root counts as the root in the first row and the first row alone.
    assert_eq!(checked, Element::ALL.len(), "every element is in one row of the table");
}

/// The form a spec cell names, for the forms that every element reads alike.
fn form_of(cell: &str) -> Form {
    if let Some(rest) = cell.strip_prefix("number") {
        if rest.is_empty() || rest.starts_with(", 0") || rest.starts_with(" above") {
            return Form::Number;
        }
    }
    if cell.starts_with("whole number") {
        return Form::WholeNumber;
    }
    if cell == "time" || cell.starts_with("time above") || cell.starts_with("time, counted") {
        return Form::Time;
    }
    Form::Other
}

#[test]
fn the_form_of_each_attribute_is_the_form_its_table_names() {
    let mut numbers = 0;
    let mut times = 0;
    for (element, body) in sections(&spec()) {
        // The root's attributes are judged by their own rules, and a style's by the element that
        // takes the style.
        if matches!(element, Element::Composition | Element::Style) {
            continue;
        }
        for (name, cell) in rows(&body) {
            let wanted = form_of(&cell);
            assert_eq!(element.form(&name), wanted, "{} {name}: the spec says {cell:?}", element.tag());
            match wanted {
                Form::Number => numbers += 1,
                Form::Time => times += 1,
                _ => {}
            }
        }
    }
    assert!(numbers > 150, "{numbers}");
    assert!(times > 10, "{times}");
}

#[test]
fn the_messages_of_the_spec_are_the_messages_of_the_program() {
    // Each message the program makes for the common mistakes is the one section 13 shows, or the
    // one it shows with the place filled in.
    let text = spec();
    for message in [
        "`x=\"a\"` is not a number: an optional sign, digits with an optional point and an optional exponent, such as 12, -4, .5 or 2.5e-1",
        "`x=\"1e999\"` is too large: a number must be finite",
        "`width=\"10.5\"` is not a whole number",
        "`at=\"-1\"` is not a time in seconds: digits with an optional point, such as 5 or 1.25",
        "<span> is not a known element here; a <span> stands directly in <text> or <span>",
        "a <style> stands directly in the <composition>, not inside an element",
        "<box> is not known inside <text>; only <span>, <tween> and <set> are; a <box> stands directly in <composition>, <box> or <rect>",
        "<rect> has no attribute `colour`",
        "a <composition> inside a composition needs the attribute `src`; without one it stands only as the root of a file",
    ] {
        assert!(text.contains(message), "section 13 does not hold: {message}");
    }
    // And the program makes them.
    let made = [
        common::only_problem_in("<box x=\"a\"/>").message().to_string(),
        common::only_problem_in("<rect colour=\"red\"/>").message().to_string(),
        common::only_problem_in("<box>\n<tween at=\"-1\" duration=\"1\" to-x=\"1\"/>\n</box>")
            .message()
            .to_string(),
    ];
    assert!(text.contains(&made[0]));
    assert!(text.contains(&made[1]));
    assert!(text.contains(&made[2]));
}
