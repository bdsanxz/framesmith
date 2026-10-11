//! What a page declares, before it is resolved. The host builds this from
//! the files; the page builds it from its own document.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawAttr {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub line: u32,
}

/// An element that carries at least one `data-fs-` attribute.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawElement {
    pub tag: String,
    pub id: Option<String>,
    /// Only the attributes in the `data-fs-` namespace.
    pub attrs: Vec<RawAttr>,
    /// The nearest enclosing element in this list.
    pub parent: Option<usize>,
    #[serde(default)]
    pub line: u32,
}

impl RawElement {
    pub fn attr(&self, name: &str) -> Option<&RawAttr> {
        self.attrs.iter().find(|attr| attr.name == name)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawMotion {
    pub text: String,
    #[serde(default)]
    pub line: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawDoc {
    pub elements: Vec<RawElement>,
    pub motion: Vec<RawMotion>,
}
