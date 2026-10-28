//! The elements of a valid composition file as a tree, in document order, with the attributes each
//! element writes and the line each is on. It is made only from a file that `structure` has judged,
//! so every element is one the notation has and stands where it may.
//!
//! The tree holds what was written and nothing about what it means: the parts that draw, lay out and
//! animate read the attributes they take through [`Attribute`], which reads the value forms of
//! `docs/spec/notation.md` section 4 (a length, a colour).

use std::collections::HashMap;

use crate::structure::Judged;

use crate::root::{shown, Spot};
use crate::schema::Element;
use crate::styles::{self, Styles};
use crate::value;
use crate::Colour;
use timebase::Time;

/// A written length (`docs/spec/notation.md` section 4): a number of pixels, or a percentage.
#[derive(Clone, Debug, PartialEq)]
pub enum Length {
    /// A number of pixels, as written: `40`, `-4`, `.5`, `1e3`.
    Pixels(Number),
    /// A percentage of the parent's size, as written: `50%` is 50, with every digit it is written with.
    Percent(Number),
}

/// A number as it is written (N9), in its parts, so that no digit is lost: the value is the digits, read as a
/// whole number, times 10 to the power of the exponent (`1.4` is the digits `14` at the exponent -1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Number {
    negative: bool,
    digits: String,
    exponent: i32,
}

impl Number {
    /// A number from its parts, the digits being ASCII digits, most significant first.
    pub(crate) fn new(negative: bool, digits: String, exponent: i32) -> Number {
        Number {
            negative,
            digits,
            exponent,
        }
    }

    /// A whole number.
    pub fn whole(value: i64) -> Number {
        Number::new(value < 0, value.unsigned_abs().to_string(), 0)
    }

    /// Whether the sign is minus.
    pub fn negative(&self) -> bool {
        self.negative
    }

    /// The digits as written, most significant first, with no point.
    pub fn digits(&self) -> &str {
        &self.digits
    }

    /// The exponent: the value is the digits times 10 to this power.
    pub fn exponent(&self) -> i32 {
        self.exponent
    }

    /// Whether the value has no fraction. A fraction written as zeros, such as `1.0`, is whole.
    pub fn is_whole(&self) -> bool {
        self.exponent >= 0
            || self
                .digits
                .bytes()
                .rev()
                .take(self.exponent.unsigned_abs() as usize)
                .all(|digit| digit == b'0')
    }

    /// The nearest float to the value, which is infinite for a value beyond a float's range.
    pub fn to_f64(&self) -> f64 {
        let sign = if self.negative { "-" } else { "" };
        format!("{sign}{}e{}", self.digits, self.exponent).parse().unwrap_or(f64::NAN)
    }
}

/// An attribute as it is written on an element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    name: String,
    value: String,
    line: u32,
    given_by: Option<String>,
}

impl Attribute {
    /// The name, without a prefix: the notation takes none.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The `id` of the style that gives this value to the element, or `None` when the element writes
    /// it itself (`docs/spec/notation.md` section 9).
    pub fn given_by(&self) -> Option<&str> {
        self.given_by.as_deref()
    }

    /// The value as written, without the white space round it.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The value as a message shows it: one line, whatever the value holds.
    pub fn shown(&self) -> String {
        shown(&self.value)
    }

    /// The line the attribute is on, counted from 1.
    pub fn line(&self) -> u32 {
        self.line
    }

    /// The value as a length, or `None` when it is not one: a number (N9) with an optional `%`
    /// after it, and no unit.
    pub fn length(&self) -> Option<Length> {
        match self.value.strip_suffix('%') {
            Some(number) => value::written_number(number).ok().map(Length::Percent),
            None => value::written_number(&self.value).ok().map(Length::Pixels),
        }
    }

    /// The value as lengths separated by white space, each a number (N9) with an optional `%` after it, in the order
    /// written; `None` when a part is not a length.
    pub fn lengths(&self) -> Option<Vec<Length>> {
        self.value
            .split_whitespace()
            .map(|part| match part.strip_suffix('%') {
                Some(number) => value::written_number(number).ok().map(Length::Percent),
                None => value::written_number(part).ok().map(Length::Pixels),
            })
            .collect()
    }

    /// The value as a number (N9), or `None` when it is not one.
    pub fn number(&self) -> Option<f64> {
        value::number(&self.value).ok()
    }

    /// The value as a time in seconds, read exactly (`docs/spec/notation.md` section 5 and N8), or
    /// `None` when it is not one.
    pub fn time(&self) -> Option<Time> {
        value::time(&self.value)
    }

    /// The value as a colour written `#rrggbb` or `#rrggbbaa`, or `None` when it is not one.
    pub fn colour(&self) -> Option<Colour> {
        value::colour(&self.value).map(|[red, green, blue, alpha]| Colour { red, green, blue, alpha })
    }

    /// The value as sides (`docs/spec/notation.md` section 4): one to four numbers, each read as a
    /// number (N9), separated by white space, in the order the value gives them. `None` when the
    /// value is not that: a percentage, a unit, a word, or five or more numbers.
    pub fn sides(&self) -> Option<Vec<f64>> {
        let parts: Vec<&str> = self.value.split_whitespace().collect();
        if !(1..=4).contains(&parts.len()) {
            return None;
        }
        parts.into_iter().map(|part| value::number(part).ok()).collect()
    }
}

/// One part of the text of a `<text>` or a `<span>`, in document order (`docs/spec/text.md`, section 6.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// Words as they are written, their white space unchanged (`docs/spec/text.md`, section 5, collapses it).
    Words(String),
    /// A `<span>` inside the text, by its place in [`Tree::nodes`].
    Span(usize),
}

/// One element of the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    element: Element,
    line: u32,
    parent: Option<usize>,
    attributes: Vec<Attribute>,
    parts: Vec<Part>,
}

impl Node {
    /// Which element of the notation it is.
    pub fn element(&self) -> Element {
        self.element
    }

    /// The line its opening tag starts on, counted from 1.
    pub fn line(&self) -> u32 {
        self.line
    }

    /// The place of the element it stands in within [`Tree::nodes`]; `None` for the root.
    pub fn parent(&self) -> Option<usize> {
        self.parent
    }

    /// The attributes in the order they are written.
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }

    /// The attribute called `name`, when it is written.
    pub fn attribute(&self, name: &str) -> Option<&Attribute> {
        self.attributes.iter().find(|attribute| attribute.name == name)
    }

    /// The words and spans of a `<text>` or a `<span>`, in the order they are written. Empty for every other element.
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }
}

/// The elements of a composition file, root first, each before the elements inside it and after
/// the elements before it: the order in which they are drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    /// Every element in document order; the first is the root.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// The root `<composition>`.
    pub fn root(&self) -> &Node {
        &self.nodes[0]
    }
}

/// The parts of a `<text>` or a `<span>`: its words, as written, and the `<span>`s among them, in document order. A
/// child that is not a `<span>` is not a part; the structure check refuses any such child before the tree is made.
fn parts_of(node: roxmltree::Node, span_at: &HashMap<roxmltree::NodeId, usize>) -> Vec<Part> {
    let mut parts: Vec<Part> = Vec::new();
    for child in node.children() {
        if child.is_text() {
            let Some(text) = child.text() else {
                continue;
            };
            match parts.last_mut() {
                Some(Part::Words(words)) => words.push_str(text),
                _ => parts.push(Part::Words(text.to_string())),
            }
        } else if let Some(&index) = span_at.get(&child.id()) {
            parts.push(Part::Span(index));
        }
    }
    parts
}

/// Makes the tree of a document, without what `structure::judge` leaves out: the elements it found a problem on and what they
/// hold, and the attributes it reported as forms this version does not support.
pub(crate) fn build(document: &roxmltree::Document, text: &str, styles: &Styles, judged: &Judged) -> Tree {
    let line_at = |offset: usize| document.text_pos_at(offset).row;
    let spot_at = |offset: usize| {
        let position = document.text_pos_at(offset);
        Spot {
            line: position.row,
            column: position.col,
        }
    };
    let namespace = document.root_element().tag_name().namespace();
    let mut nodes: Vec<Node> = Vec::new();
    // The document node of each element of `nodes`, in the same order, so that the parts can be read after the tree is made.
    let mut handles: Vec<roxmltree::Node> = Vec::new();
    // Iterative, so that a deeply nested file cannot exhaust the stack. A node is taken after the
    // ones before it in the file, since the children of a node are put on the stack in reverse.
    let mut pending: Vec<(roxmltree::Node, Option<usize>, Element)> = vec![(document.root_element(), None, Element::Composition)];
    while let Some((node, parent, element)) = pending.pop() {
        let index = nodes.len();
        nodes.push(Node {
            element,
            line: line_at(node.range().start),
            parent,
            attributes: styles::effective(node, text, element, styles, &spot_at)
                .into_iter()
                .filter(|entry| !judged.refused.get(&node.id()).is_some_and(|names| names.contains(&entry.name)))
                .map(|entry| Attribute {
                    name: entry.name,
                    value: entry.value,
                    line: entry.spot.line,
                    given_by: entry.from,
                })
                .collect(),
            parts: Vec::new(),
        });
        handles.push(node);
        let children: Vec<roxmltree::Node> = node.children().filter(roxmltree::Node::is_element).collect();
        for child in children.into_iter().rev() {
            let has_src = child
                .attributes()
                .any(|attribute| attribute.namespace().is_none() && attribute.name() == "src");
            if child.tag_name().namespace() != namespace || judged.faulty.contains(&child.id()) {
                continue;
            }
            if let Some(kind) = Element::named(child.tag_name().name(), has_src) {
                pending.push((child, Some(index), kind));
            }
        }
    }
    let span_at: HashMap<roxmltree::NodeId, usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.element == Element::Span)
        .map(|(index, _)| (handles[index].id(), index))
        .collect();
    let parts: Vec<Vec<Part>> = nodes
        .iter()
        .zip(&handles)
        .map(|(node, handle)| match node.element {
            Element::Text | Element::Span => parts_of(*handle, &span_at),
            _ => Vec::new(),
        })
        .collect();
    for (node, node_parts) in nodes.iter_mut().zip(parts) {
        node.parts = node_parts;
    }
    Tree { nodes }
}
