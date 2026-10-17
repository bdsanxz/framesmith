//! Shared styles: a `<style>` holds attributes, and an element takes them by naming it in `class`. The
//! element is read as if the attributes were written on it (`docs/spec/notation.md` section 9 and N5;
//! `docs/spec/check-command.md` section 3c).
//!
//! The styles of a file are found once, with the reports on the styles themselves. Each element's
//! attributes are then [`effective`]: its own attributes, and those the styles it names give it. The
//! element's own value wins, and of two styles the one named later in `class` wins. The judgement of
//! the form of the file (`structure`) and the tree that the parts which draw read are both made from
//! these attributes, so what is judged is what is drawn.

use std::collections::HashMap;

use crate::code::Code;
use crate::root::{found, shown, trimmed, Found, Spot};
use crate::schema::Element;

/// The styles of one file: each `id` with the attributes it holds, in the order they are written.
///
/// An `id` names a style, and the attributes of a style are every attribute but `id` and `class`:
/// neither a name nor a `class` is given to an element.
pub(crate) struct Styles {
    by_id: HashMap<String, Vec<(String, String)>>,
}

impl Styles {
    /// The attributes of the style called `name`, or `None` when the file has no such style.
    fn get(&self, name: &str) -> Option<&Vec<(String, String)>> {
        self.by_id.get(name)
    }
}

/// One attribute of an element as it is read: written on the element, or given to it by a style.
#[derive(Clone)]
pub(crate) struct Effective {
    /// The name, without a prefix.
    pub(crate) name: String,
    /// The name as it is written, with its prefix.
    pub(crate) written: String,
    /// The value, without the white space round it.
    pub(crate) value: String,
    /// Where the attribute is in the file. For a value a style gives, the element's place.
    pub(crate) spot: Spot,
    /// The `id` of the style that gives the value, or `None` when the element writes it.
    pub(crate) from: Option<String>,
    /// Whether the attribute is written with a prefix, which the notation does not take.
    pub(crate) prefixed: bool,
}

/// Finds the styles of a file: a `<style>` that stands directly in the root. A style with no `id`
/// is reported on its own place, and of two styles with one `id` the second is reported on its `id`;
/// neither is registered, so no element can take it.
pub(crate) fn styles(root: roxmltree::Node, spot_at: &dyn Fn(usize) -> Spot, problems: &mut Vec<Found>) -> Styles {
    let namespace = root.tag_name().namespace();
    let mut by_id: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for child in root.children().filter(roxmltree::Node::is_element) {
        if child.tag_name().namespace() != namespace || child.tag_name().name() != "style" {
            continue;
        }
        let Some(id) = child
            .attributes()
            .find(|attribute| attribute.namespace().is_none() && attribute.name() == "id")
        else {
            problems.push(found(
                Code::StyleWithoutId,
                spot_at(child.range().start),
                "<style> needs the attribute `id`: the name elements take it by".to_string(),
            ));
            continue;
        };
        let name = trimmed(id.value()).to_string();
        if by_id.contains_key(&name) {
            problems.push(found(
                Code::StyleIdTwice,
                spot_at(id.range().start),
                format!("two <style> elements have the id `{}`", shown(&name)),
            ));
            continue;
        }
        let attributes = child
            .attributes()
            .filter(|attribute| attribute.namespace().is_none() && attribute.name() != "id" && attribute.name() != "class")
            .map(|attribute| (attribute.name().to_string(), trimmed(attribute.value()).to_string()))
            .collect();
        by_id.insert(name, attributes);
    }
    Styles { by_id }
}

/// Whether the element reads the styles it names: every element that takes `class`, except a
/// `<style>`, which takes no style (section 9).
pub(crate) fn names_styles(element: Element) -> bool {
    element != Element::Style && element.takes("class")
}

/// The attributes of an element as they are read: its own, in the order written, then those the
/// styles it names give it. `text` is the source, for the names as they are written.
pub(crate) fn effective(
    node: roxmltree::Node,
    text: &str,
    element: Element,
    styles: &Styles,
    spot_at: &dyn Fn(usize) -> Spot,
) -> Vec<Effective> {
    let mut list: Vec<Effective> = node
        .attributes()
        .map(|attribute| Effective {
            name: attribute.name().to_string(),
            written: crate::structure::written_name(text, attribute.range().start),
            value: trimmed(attribute.value()).to_string(),
            spot: spot_at(attribute.range().start),
            from: None,
            prefixed: attribute.namespace().is_some(),
        })
        .collect();
    if !names_styles(element) {
        return list;
    }
    let element_spot = spot_at(node.range().start);
    for name in class_names(&list) {
        let Some(attributes) = styles.get(&name) else {
            continue;
        };
        for (attribute, value) in attributes {
            match list.iter().position(|entry| !entry.prefixed && entry.name == *attribute) {
                // The element's own value wins over every style.
                Some(index) if list[index].from.is_none() => {}
                // A later style named in `class` wins over an earlier one.
                Some(index) => {
                    list[index].value = value.clone();
                    list[index].from = Some(name.clone());
                }
                None => list.push(Effective {
                    name: attribute.clone(),
                    written: attribute.clone(),
                    value: value.clone(),
                    spot: element_spot,
                    from: Some(name.clone()),
                    prefixed: false,
                }),
            }
        }
    }
    list
}

/// The first name of the element's own `class` that no style of the file has, with the place of the
/// `class`: one report for the attribute (section 3c).
pub(crate) fn unknown_class(list: &[Effective], element: Element, styles: &Styles) -> Option<(String, Spot)> {
    if !names_styles(element) {
        return None;
    }
    let class = list.iter().find(|entry| !entry.prefixed && entry.name == "class")?;
    class
        .value
        .split_whitespace()
        .find(|name| styles.get(name).is_none())
        .map(|name| (name.to_string(), class.spot))
}

/// The names in the element's own `class`, separated by white space.
fn class_names(list: &[Effective]) -> Vec<String> {
    list.iter()
        .find(|entry| !entry.prefixed && entry.from.is_none() && entry.name == "class")
        .map(|class| class.value.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}
