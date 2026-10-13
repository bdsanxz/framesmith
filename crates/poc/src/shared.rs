//! Shared styles: a `<style>` names a set of attributes, and an element takes them with `class`.
//!
//! The attributes are written into the elements before the composition is read, each on the line
//! of its element, so that everything after reads one kind of element and reports the right line.

use roxmltree::{Document, Node};
use std::collections::HashMap;

/// Something wrong with a style or with its use: the line, and what is wrong.
pub type Fault = (u32, String);

/// The attributes of one style, in the order they are written.
type Style<'a> = Vec<(&'a str, &'a str)>;

/// `source` with the attributes of each element's styles written on the element, and what is
/// wrong with the styles and their use. A text that is not well-formed XML is given back as it
/// is: whoever reads it next reports that.
pub fn expand(source: &str) -> (String, Vec<Fault>) {
    let Ok(document) = Document::parse(source) else {
        return (source.to_string(), Vec::new());
    };
    let line = |node: Node| document.text_pos_at(node.range().start).row;
    let mut faults: Vec<Fault> = Vec::new();
    let root = document.root_element();

    let mut styles: HashMap<&str, Style> = HashMap::new();
    for node in root.children().filter(|node| node.has_tag_name("style")) {
        let Some(id) = node.attribute("id").filter(|id| !id.trim().is_empty()) else {
            faults.push((
                line(node),
                "<style> needs the attribute `id`: the name elements take it by".to_string(),
            ));
            continue;
        };
        if node.has_attribute("class") {
            faults.push((
                line(node),
                format!("the style `{id}` has `class`; a style cannot take another style"),
            ));
        }
        if node.children().any(|child| child.is_element()) {
            faults.push((
                line(node),
                format!("the style `{id}` holds an element; a style holds attributes only"),
            ));
        }
        let own: Style = node
            .attributes()
            .filter(|attribute| !matches!(attribute.name(), "id" | "class"))
            .map(|attribute| (attribute.name(), attribute.value()))
            .collect();
        if styles.insert(id, own).is_some() {
            faults.push((line(node), format!("two <style> elements have the id `{id}`")));
        }
    }

    // What is written into the text, and where: just after the name of each element.
    let mut written: Vec<(usize, String)> = Vec::new();
    for node in root.descendants().filter(|node| node.is_element() && !node.has_tag_name("style")) {
        let Some(names) = node.attribute("class") else { continue };
        let mut taken: Style = Vec::new();
        for name in names.split_whitespace() {
            let Some(style) = styles.get(name) else {
                faults.push((
                    line(node),
                    format!("`class` names the style `{name}`, and this file has no <style> with that id"),
                ));
                continue;
            };
            for (attribute, value) in style {
                // A style named later wins over one named before it.
                taken.retain(|(earlier, _)| earlier != attribute);
                taken.push((attribute, value));
            }
        }
        // What the element writes itself wins over every style.
        taken.retain(|(attribute, _)| !node.has_attribute(*attribute));
        let tag = node.tag_name().name();
        let after_name = node.range().start + 1 + tag.len();
        if taken.is_empty() || source.get(node.range().start + 1..after_name) != Some(tag) {
            continue;
        }
        let text: String = taken
            .iter()
            .map(|(attribute, value)| format!(" {attribute}=\"{}\"", escaped(value)))
            .collect();
        written.push((after_name, text));
    }

    let mut expanded = source.to_string();
    // From the end of the text to its start, so that the places still to be written stay true.
    written.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    for (at, text) in written {
        expanded.insert_str(at, &text);
    }
    (expanded, faults)
}

/// A value as it stands between the quotes of an attribute, on one line.
fn escaped(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            other => out.push(other),
        }
    }
    out
}
