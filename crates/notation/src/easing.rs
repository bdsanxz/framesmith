//! The easing names of a `<tween>`'s `ease` (`docs/spec/notation.md` section 8.4): the nineteen names, and `out-back(n)`
//! with `n` a number (N9). The curves themselves belong to the motion engine.

use crate::value;

/// The nineteen easing names, in the order the notation lists them.
pub const EASINGS: [&str; 19] = [
    "linear",
    "in-quad",
    "out-quad",
    "in-out-quad",
    "in-cubic",
    "out-cubic",
    "in-out-cubic",
    "in-quart",
    "out-quart",
    "in-out-quart",
    "in-quint",
    "out-quint",
    "in-out-quint",
    "in-sine",
    "out-sine",
    "in-out-sine",
    "in-expo",
    "out-expo",
    "in-out-expo",
];

/// An easing as written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    /// One of [`EASINGS`].
    Named(&'static str),
    /// `out-back(n)`.
    OutBack(f64),
}

/// The easing an `ease` value names, or `None` when it names none. White space around the value, and around `n` inside the
/// brackets, is not part of it; the names are in lower case.
pub fn easing(text: &str) -> Option<Easing> {
    let text = text.trim();
    if let Some(name) = EASINGS.iter().find(|name| **name == text) {
        return Some(Easing::Named(name));
    }
    let inner = text.strip_prefix("out-back(")?.strip_suffix(')')?;
    value::number(inner.trim()).ok().map(Easing::OutBack)
}

/// The list of the easings taken, as a message writes it.
pub(crate) fn taken() -> String {
    format!("{} and out-back(n), with n a number", EASINGS.join(", "))
}
