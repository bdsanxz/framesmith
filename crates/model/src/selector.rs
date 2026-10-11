//! The selector forms `check` can read: type, `#id`, `.class`, `[attr]`,
//! `[attr="value"]` and `*`, joined into compounds, with descendant and
//! child combinators and comma-separated lists.
//!
//! Anything else that is valid CSS is reported as unsupported, not guessed.

use crate::html::{Dom, Element};
use crate::SelectorProblem;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Compound {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attrs: Vec<(String, Option<String>)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Link {
    Descendant,
    Child,
}

/// One selector of a list: compounds from left to right, each with the
/// combinator that joins it to the one before.
type Complex = Vec<(Link, Compound)>;

/// A parsed selector list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector {
    list: Vec<Complex>,
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || !c.is_ascii()
}

fn is_ident_char(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit() || c == '-'
}

struct Reader<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl Reader<'_> {
    fn skip_space(&mut self) -> bool {
        let mut any = false;
        while self.chars.peek().is_some_and(|c| c.is_whitespace()) {
            self.chars.next();
            any = true;
        }
        any
    }

    fn ident(&mut self) -> Result<String, SelectorProblem> {
        let mut out = String::new();
        if self.chars.peek() == Some(&'-') {
            out.push('-');
            self.chars.next();
        }
        match self.chars.peek() {
            Some(&c) if is_ident_start(c) => {}
            Some('\\') => return Err(SelectorProblem::Unsupported),
            _ => return Err(SelectorProblem::Invalid),
        }
        while let Some(&c) = self.chars.peek() {
            if c == '\\' {
                return Err(SelectorProblem::Unsupported);
            }
            if !is_ident_char(c) {
                break;
            }
            out.push(c);
            self.chars.next();
        }
        Ok(out)
    }

    fn attribute(&mut self) -> Result<(String, Option<String>), SelectorProblem> {
        self.skip_space();
        let name = self.ident()?.to_ascii_lowercase();
        self.skip_space();
        match self.chars.next() {
            Some(']') => return Ok((name, None)),
            Some('=') => {}
            // The other match operators and namespaces are valid CSS.
            Some('~' | '|' | '^' | '$' | '*') => return Err(SelectorProblem::Unsupported),
            _ => return Err(SelectorProblem::Invalid),
        }
        self.skip_space();
        let value = match self.chars.peek() {
            Some(&quote @ ('"' | '\'')) => {
                self.chars.next();
                let mut value = String::new();
                loop {
                    match self.chars.next() {
                        None => return Err(SelectorProblem::Invalid),
                        Some('\\') => return Err(SelectorProblem::Unsupported),
                        Some(c) if c == quote => break,
                        Some(c) => value.push(c),
                    }
                }
                value
            }
            _ => self.ident()?,
        };
        self.skip_space();
        match self.chars.next() {
            Some(']') => Ok((name, Some(value))),
            // A case flag such as `i`.
            Some(c) if c.is_ascii_alphabetic() => Err(SelectorProblem::Unsupported),
            _ => Err(SelectorProblem::Invalid),
        }
    }

    fn compound(&mut self) -> Result<Compound, SelectorProblem> {
        let mut compound = Compound::default();
        let mut parts = 0;
        match self.chars.peek() {
            Some('*') => {
                self.chars.next();
                parts += 1;
            }
            Some(&c) if is_ident_start(c) || c == '-' => {
                compound.tag = Some(self.ident()?.to_ascii_lowercase());
                parts += 1;
            }
            _ => {}
        }
        loop {
            match self.chars.peek() {
                Some('#') => {
                    self.chars.next();
                    // A second id can never match the same element twice, but it is valid.
                    let id = self.ident()?;
                    if compound.id.as_ref().is_some_and(|first| *first != id) {
                        compound.classes.push("\u{0}never".to_string());
                    }
                    compound.id = Some(id);
                }
                Some('.') => {
                    self.chars.next();
                    compound.classes.push(self.ident()?);
                }
                Some('[') => {
                    self.chars.next();
                    compound.attrs.push(self.attribute()?);
                }
                Some(':') => return Err(SelectorProblem::Unsupported),
                _ => break,
            }
            parts += 1;
        }
        if parts == 0 {
            return Err(SelectorProblem::Invalid);
        }
        Ok(compound)
    }

    fn complex(&mut self) -> Result<Complex, SelectorProblem> {
        let mut complex: Complex = Vec::new();
        self.skip_space();
        let mut link = Link::Descendant;
        loop {
            complex.push((link, self.compound()?));
            let spaced = self.skip_space();
            match self.chars.peek() {
                None | Some(',') => return Ok(complex),
                Some('>') => {
                    self.chars.next();
                    self.skip_space();
                    link = Link::Child;
                }
                // Sibling and column combinators are valid CSS.
                Some('+' | '~' | '|') => return Err(SelectorProblem::Unsupported),
                Some(_) if spaced => link = Link::Descendant,
                Some(_) => return Err(SelectorProblem::Invalid),
            }
        }
    }
}

/// Reads a selector list.
pub fn parse(text: &str) -> Result<Selector, SelectorProblem> {
    let mut reader = Reader {
        chars: text.chars().peekable(),
    };
    let mut list = Vec::new();
    loop {
        list.push(reader.complex()?);
        match reader.chars.next() {
            None => return Ok(Selector { list }),
            Some(',') => {}
            Some(_) => return Err(SelectorProblem::Invalid),
        }
    }
}

fn compound_matches(compound: &Compound, element: &Element) -> bool {
    if compound.tag.as_ref().is_some_and(|tag| *tag != element.tag) {
        return false;
    }
    if let Some(id) = &compound.id {
        if element.value("id") != Some(id.as_str()) {
            return false;
        }
    }
    if !compound.classes.is_empty() {
        let written = element.value("class").unwrap_or("");
        if !compound
            .classes
            .iter()
            .all(|class| written.split_ascii_whitespace().any(|c| c == class))
        {
            return false;
        }
    }
    compound.attrs.iter().all(|(name, wanted)| match (element.value(name), wanted) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(value), Some(wanted)) => value == wanted,
    })
}

/// Whether `element` matches the first `upto` compounds of `complex`, the
/// last of them against `element` itself.
fn complex_matches(dom: &Dom, complex: &Complex, upto: usize, element: usize) -> bool {
    let (link, compound) = &complex[upto - 1];
    if !compound_matches(compound, &dom.elements[element]) {
        return false;
    }
    if upto == 1 {
        return true;
    }
    let mut ancestor = dom.elements[element].parent;
    match link {
        Link::Child => ancestor.is_some_and(|parent| complex_matches(dom, complex, upto - 1, parent)),
        Link::Descendant => {
            while let Some(a) = ancestor {
                if complex_matches(dom, complex, upto - 1, a) {
                    return true;
                }
                ancestor = dom.elements[a].parent;
            }
            false
        }
    }
}

/// The elements inside `root` that `selector` matches, in document order.
/// With no root, every element of the page is a candidate. As in a browser,
/// the selector is read against the whole page; only the results are limited
/// to the inside of `root`.
pub fn matches_in(dom: &Dom, root: Option<usize>, selector: &str) -> Result<Vec<usize>, SelectorProblem> {
    let selector = parse(selector)?;
    Ok((0..dom.elements.len())
        .filter(|&element| root.is_none_or(|root| dom.is_inside(element, root)))
        .filter(|&element| {
            selector
                .list
                .iter()
                .any(|complex| complex_matches(dom, complex, complex.len(), element))
        })
        .collect())
}
