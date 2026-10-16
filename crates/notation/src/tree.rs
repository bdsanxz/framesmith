//! The elements of a valid composition file as a tree, in document order, with the attributes each
//! element writes and the line each is on. It is made only from a file that `structure` has judged,
//! so every element is one the notation has and stands where it may.
//!
//! The tree holds what was written and nothing about what it means: the parts that draw, lay out and
//! animate read the attributes they take through [`Attribute`], which reads the value forms of
//! `docs/spec/notation.md` section 4 (a length, a colour).

use crate::root::{shown, trimmed};
use crate::schema::Element;
use crate::value;
use crate::Colour;
use timebase::Time;

/// A written length (`docs/spec/notation.md` section 4): a number of pixels, or a percentage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Length {
    /// A number of pixels, as written: `40`, `-4`, `.5`, `1e3`.
    Pixels(f64),
    /// A percentage of the parent's size, as written: `50%` is `Percent(50.0)`.
    Percent(f64),
}

/// An attribute as it is written on an element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    name: String,
    value: String,
    line: u32,
}

impl Attribute {
    /// The name, without a prefix: the notation takes none.
    pub fn name(&self) -> &str {
        &self.name
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
            Some(number) => value::number(number).ok().map(Length::Percent),
            None => value::number(&self.value).ok().map(Length::Pixels),
        }
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

/// One element of the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    element: Element,
    line: u32,
    parent: Option<usize>,
    attributes: Vec<Attribute>,
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

/// Makes the tree of a document that `structure::judge` found no problem in.
pub(crate) fn build(document: &roxmltree::Document) -> Tree {
    let line_at = |offset: usize| document.text_pos_at(offset).row;
    let namespace = document.root_element().tag_name().namespace();
    let mut nodes: Vec<Node> = Vec::new();
    // Iterative, so that a deeply nested file cannot exhaust the stack. A node is taken after the
    // ones before it in the file, since the children of a node are put on the stack in reverse.
    let mut pending: Vec<(roxmltree::Node, Option<usize>, Element)> = vec![(document.root_element(), None, Element::Composition)];
    while let Some((node, parent, element)) = pending.pop() {
        let index = nodes.len();
        nodes.push(Node {
            element,
            line: line_at(node.range().start),
            parent,
            attributes: node
                .attributes()
                .map(|attribute| Attribute {
                    name: attribute.name().to_string(),
                    value: trimmed(attribute.value()).to_string(),
                    line: line_at(attribute.range().start),
                })
                .collect(),
        });
        let children: Vec<roxmltree::Node> = node.children().filter(roxmltree::Node::is_element).collect();
        for child in children.into_iter().rev() {
            let has_src = child
                .attributes()
                .any(|attribute| attribute.namespace().is_none() && attribute.name() == "src");
            if child.tag_name().namespace() != namespace {
                continue;
            }
            if let Some(kind) = Element::named(child.tag_name().name(), has_src) {
                pending.push((child, Some(index), kind));
            }
        }
    }
    Tree { nodes }
}
