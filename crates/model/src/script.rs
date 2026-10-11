//! Looks through a page script for uses `check` must refuse: the browser's
//! own animation interface, and the script interface for building motion.
//!
//! It reads names, not meaning: comments and the text of strings are left
//! out, and what remains is searched. A use built at run time from pieces
//! is caught by the page during a render instead.

/// One use found in a script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Use {
    /// `FS-U016` or `FS-U017`.
    pub code: &'static str,
    /// What was written, for the message.
    pub what: &'static str,
    pub line: u32,
}

/// The script with comments and the contents of string literals blanked
/// out. Line breaks are kept, so lines still count the same.
fn code_only(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut pos = 0;
    let blank = |out: &mut Vec<u8>, slice: &[u8]| {
        out.extend(slice.iter().map(|&b| if b == b'\n' { b'\n' } else { b' ' }));
    };
    while pos < bytes.len() {
        let b = bytes[pos];
        let next = bytes.get(pos + 1).copied();
        if b == b'/' && next == Some(b'/') {
            let end = text[pos..].find('\n').map_or(bytes.len(), |e| pos + e);
            blank(&mut out, &bytes[pos..end]);
            pos = end;
        } else if b == b'/' && next == Some(b'*') {
            let end = text[pos + 2..].find("*/").map_or(bytes.len(), |e| pos + 2 + e + 2);
            blank(&mut out, &bytes[pos..end]);
            pos = end;
        } else if b == b'"' || b == b'\'' || b == b'`' {
            let mut end = pos + 1;
            while end < bytes.len() && bytes[end] != b {
                end += if bytes[end] == b'\\' { 2 } else { 1 };
            }
            let end = (end + 1).min(bytes.len());
            blank(&mut out, &bytes[pos..end]);
            pos = end;
        } else {
            out.push(b);
            pos += 1;
        }
    }
    // Only ASCII bytes were replaced, and only whole literals, so this holds.
    String::from_utf8_lossy(&out).into_owned()
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || !b.is_ascii()
}

/// Every place `name` stands as a whole name in `code`.
fn whole_names<'a>(code: &'a str, name: &'a str) -> impl Iterator<Item = usize> + 'a {
    let bytes = code.as_bytes();
    code.match_indices(name).map(|(at, _)| at).filter(move |&at| {
        let before = at.checked_sub(1).map(|i| bytes[i]);
        let after = bytes.get(at + name.len()).copied();
        !before.is_some_and(is_name_byte) && !after.is_some_and(is_name_byte)
    })
}

fn line_of(code: &str, at: usize) -> u32 {
    1 + code[..at].bytes().filter(|&b| b == b'\n').count() as u32
}

/// The last name before `at`, skipping one dot and white space: for
/// `window . framesmith`, the name `window`.
fn owner_before(code: &str, at: usize) -> Option<Option<&str>> {
    let head = code[..at].trim_end();
    let head = head.strip_suffix('.')?;
    let head = head.trim_end().strip_suffix('?').unwrap_or(head).trim_end();
    let start = head.bytes().rposition(|b| !is_name_byte(b)).map_or(0, |i| i + 1);
    Some(Some(&head[start..]).filter(|name| !name.is_empty()))
}

/// Finds the refused uses in a script. Lines count from 1.
pub fn scan(text: &str) -> Vec<Use> {
    let code = code_only(text);
    let mut uses: Vec<Use> = Vec::new();

    for (at, _) in code.match_indices(".animate") {
        let rest = code[at + ".animate".len()..].trim_start();
        if rest.starts_with('(') {
            uses.push(Use {
                code: "FS-U016",
                what: ".animate(",
                line: line_of(&code, at),
            });
        }
    }
    for name in ["Animation", "KeyframeEffect", "getAnimations"] {
        for at in whole_names(&code, name) {
            // `Animation` alone is a common word; only its construction counts.
            if name == "Animation" && !code[..at].trim_end().ends_with("new") {
                continue;
            }
            let what = match name {
                "Animation" => "new Animation",
                "KeyframeEffect" => "KeyframeEffect",
                _ => "getAnimations",
            };
            uses.push(Use {
                code: "FS-U016",
                what,
                line: line_of(&code, at),
            });
        }
    }
    for at in whole_names(&code, "timeline") {
        if owner_before(&code, at) == Some(Some("document")) {
            uses.push(Use {
                code: "FS-U016",
                what: "document.timeline",
                line: line_of(&code, at),
            });
        }
    }
    for at in whole_names(&code, "framesmith") {
        // A property of some other object is not the page global.
        let global = match owner_before(&code, at) {
            None => true,
            Some(owner) => matches!(owner, Some("window" | "globalThis" | "self")),
        };
        if global {
            uses.push(Use {
                code: "FS-U017",
                what: "framesmith",
                line: line_of(&code, at),
            });
        }
    }
    uses.sort_by_key(|u| (u.line, u.code));
    uses.dedup();
    uses
}
