//! HTML (HyperFrames) to KDL converter (Sprint 28, T001 / CAP-001..003,
//! J-01, J-04, STATE-01/02/05, INT-001/002, REC-001/002/003/005).
//!
//! Reads one HyperFrames reference project (`index.html` root with
//! `data-duration`, track lanes, `compositions/*.html` slots, `assets/`,
//! GUIDE/STRUCTURE comments, `hyperframes.json`) and writes one
//! Framesmith project (KDL composition plus copied assets beside the
//! conversion) that framesmith renders natively. GSAP timeline calls
//! (`tl.to/from/fromTo/set`, stagger, typeOn) land as equivalent KDL
//! tweens/sets with the same starts and durations. Every construct
//! gets a per-reference fidelity verdict (translated /
//! approximated-with-deltas / refused) in `fidelity.json`; nothing is
//! approximated silently.
//!
//! Refusal contract (REC-001/002/003): unparseable files refuse naming
//! the file, unsupported timeline calls refuse naming the call site
//! (file and line), missing assets refuse naming the asset. A refused
//! conversion writes `fidelity.json` but no `composition.kdl`, and
//! exits 1.
//!
//! Durable rules this slice honours: KDL style attributes are
//! CSS-hyphenated (`font-size`, `border-color`, `letter-spacing`);
//! nothing here touches the glyph/layout crate split or the existing
//! single-pass render outputs.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// Fidelity verdict of one converted construct.
const TRANSLATED: &str = "translated";
const APPROXIMATED: &str = "approximated";
const REFUSED: &str = "refused";

/// Minimum clip length validation accepts (framesmith `MIN_DURATION_SECS`).
const MIN_DURATION: f64 = 1.0 / 60.0;

/// One fidelity entry: a single construct and how it was handled.
#[derive(Debug, Clone)]
struct Entry {
    file: String,
    line: usize,
    construct: String,
    verdict: &'static str,
    detail: String,
    /// Measured delta for approximations; always `Some` when
    /// `verdict == APPROXIMATED`, always `None` otherwise (CAP-003:
    /// missing deltas fail the verdict).
    delta: Option<String>,
}

impl Entry {
    fn translated(file: &str, line: usize, construct: &str, detail: String) -> Self {
        Entry {
            file: file.to_string(),
            line,
            construct: construct.to_string(),
            verdict: TRANSLATED,
            detail,
            delta: None,
        }
    }
    fn approximated(
        file: &str,
        line: usize,
        construct: &str,
        detail: String,
        delta: String,
    ) -> Self {
        Entry {
            file: file.to_string(),
            line,
            construct: construct.to_string(),
            verdict: APPROXIMATED,
            detail,
            delta: Some(delta),
        }
    }
    fn refused(file: &str, line: usize, construct: &str, detail: String) -> Self {
        Entry {
            file: file.to_string(),
            line,
            construct: construct.to_string(),
            verdict: REFUSED,
            detail,
            delta: None,
        }
    }
    fn to_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        m.insert("file".into(), self.file.clone().into());
        m.insert("line".into(), (self.line as u64).into());
        m.insert("construct".into(), self.construct.clone().into());
        m.insert("verdict".into(), self.verdict.into());
        m.insert("detail".into(), self.detail.clone().into());
        if let Some(d) = &self.delta {
            m.insert("delta".into(), d.clone().into());
        }
        serde_json::Value::Object(m)
    }
}

/// One parsed HTML element (opening tag with attributes and location).
#[derive(Debug, Clone)]
struct Element {
    tag: String,
    attrs: BTreeMap<String, String>,
    /// 1-based line of the opening tag.
    line: usize,
    /// Byte offset of the opening tag (for ordering / text capture).
    pos: usize,
    /// Text between `>` and the next `<` when the element holds no child
    /// elements, `None` otherwise. Trimmed.
    text: Option<String>,
}

impl Element {
    fn get(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(String::as_str)
    }
}

/// Format a number for KDL: integers without a decimal point, else the
/// shortest round-trip decimal.
fn fmt_num(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Quote a string for KDL, escaping backslash and double quote.
fn kdl_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Line number (1-based) of a byte offset.
fn line_of(src: &str, pos: usize) -> usize {
    src[..pos.min(src.len())].matches('\n').count() + 1
}

/// Extract an attribute value from an HTML/XML tag fragment: `name="v"`,
/// `name='v'`, or `name=v`. Returns the first match.
fn extract_attr(fragment: &str, name: &str) -> Option<String> {
    let bytes = fragment.as_bytes();
    let name_b = name.as_bytes();
    let mut i = 0;
    while i + name_b.len() <= bytes.len() {
        if bytes[i..].starts_with(name_b) {
            let before_ok = i == 0
                || !(bytes[i - 1] as char).is_ascii_alphanumeric()
                    && bytes[i - 1] != b'-'
                    && bytes[i - 1] != b'_';
            // The char before the match must not extend the name (so
            // `data-start` does not match a query for `start`).
            if before_ok {
                let mut j = i + name_b.len();
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'=' {
                    j += 1;
                    while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                        j += 1;
                    }
                    if j < bytes.len() && bytes[j] == b'"' {
                        let mut k = j + 1;
                        while k < bytes.len() && bytes[k] != b'"' {
                            k += 1;
                        }
                        return Some(fragment[j + 1..k].to_string());
                    }
                    if j < bytes.len() && bytes[j] == b'\'' {
                        let mut k = j + 1;
                        while k < bytes.len() && bytes[k] != b'\'' {
                            k += 1;
                        }
                        return Some(fragment[j + 1..k].to_string());
                    }
                    let mut k = j;
                    while k < bytes.len()
                        && bytes[k] != b' '
                        && bytes[k] != b'\t'
                        && bytes[k] != b'>'
                    {
                        k += 1;
                    }
                    return Some(fragment[j..k].to_string());
                }
            }
        }
        i += 1;
    }
    None
}

/// Find the body of a `<tag ...>...</tag>` pair (first match).
fn tag_body<'a>(src: &'a str, tag: &str) -> Option<&'a str> {
    let open = src.find(&format!("<{tag}"))?;
    let gt = src[open..].find('>')? + open + 1;
    let close = src[gt..].find(&format!("</{tag}>"))? + gt;
    Some(&src[gt..close])
}

/// All `<script>` bodies plus whether each has a `src` attribute.
fn script_bodies(src: &str) -> Vec<(usize, Option<String>, String)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel_open) = src[from..].find("<script") {
        let open = from + rel_open;
        let Some(gt) = src[open..].find('>').map(|i| open + i + 1) else {
            break;
        };
        let tag_text = &src[open..gt];
        let src_attr = extract_attr(tag_text, "src");
        let Some(close) = src[gt..].find("</script>") else {
            break;
        };
        out.push((gt, src_attr, src[gt..gt + close].to_string()));
        from = gt + close + "</script>".len();
    }
    out
}

/// All `<style>` bodies.
fn style_bodies(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel_open) = src[from..].find("<style") {
        let open = from + rel_open;
        let Some(gt) = src[open..].find('>').map(|i| open + i + 1) else {
            break;
        };
        let Some(close) = src[gt..].find("</style>") else {
            break;
        };
        out.push(src[gt..gt + close].to_string());
        from = gt + close + "</style>".len();
    }
    out
}

/// Parse CSS rules into `selector -> {prop: value}` for simple `#id`
/// and `.class` selectors. Compound selectors are kept verbatim as keys
/// (lookup prefers exact `#id` first).
/// Strip CSS comments from a stylesheet.
fn strip_css_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(s) = rest.find("/*") {
        out.push_str(&rest[..s]);
        match rest[s..].find("*/") {
            Some(e) => rest = &rest[s + e + 2..],
            None => break,
        }
    }
    out.push_str(rest);
    out
}

fn parse_css_rules(css: &str) -> HashMap<String, HashMap<String, String>> {
    let css = strip_css_comments(css);
    let mut map: HashMap<String, HashMap<String, String>> = HashMap::new();
    // Simple rule scan for `{...}` pairs directly.
    let mut pos = 0usize;
    while pos < css.len() {
        let Some(ob) = css[pos..].find('{') else {
            break;
        };
        let sel_start = pos;
        let Some(cb) = css[pos + ob..].find('}').map(|i| pos + ob + i) else {
            break;
        };
        let selector = css[sel_start..sel_start + ob].trim().to_string();
        let body = &css[sel_start + ob + 1..cb];
        if !selector.is_empty() && !selector.starts_with('@') {
            let mut props: HashMap<String, String> = HashMap::new();
            for decl in body.split(';') {
                if let Some((k, v)) = decl.split_once(':') {
                    let key = k.trim().to_lowercase();
                    let val = v.trim().trim_end_matches('!').trim().to_string();
                    if !key.is_empty() {
                        props.insert(key, val);
                    }
                }
            }
            map.entry(selector).or_insert_with(HashMap::new)
                .extend(props);
        }
        pos = cb + 1;
    }
    map
}

/// Scan every opening tag in an HTML document. Comments, closing tags,
/// `<!...>` and `<?...>` are skipped. An opening tag whose immediate
/// content before the next `<` is non-empty text (and which has no child
/// elements before that text) records its text.
fn scan_elements(src: &str) -> Vec<Element> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' || i + 1 >= bytes.len() {
            i += 1;
            continue;
        }
        let next = bytes[i + 1];
        if next == b'!' {
            // Comment or declaration: skip to `>` (comments end `-->`,
            // but `>`-terminated scan is enough to skip the tag itself).
            if bytes[i..].starts_with(b"<!--") {
                if let Some(end) = src[i + 4..].find("-->") {
                    i += 4 + end + 3;
                    continue;
                }
                break;
            }
            if let Some(end) = src[i..].find('>') {
                i += end + 1;
                continue;
            }
            break;
        }
        if next == b'/' {
            if let Some(end) = src[i..].find('>') {
                i += end + 1;
                continue;
            }
            break;
        }
        if !(next as char).is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        // Opening tag: read the name, then scan to `>` respecting quotes.
        let start = i;
        let mut j = i + 1;
        while j < bytes.len() && (bytes[j] as char).is_ascii_alphanumeric() {
            j += 1;
        }
        let tag = src[i + 1..j].to_string();
        let mut k = j;
        let mut quote: Option<u8> = None;
        while k < bytes.len() {
            let b = bytes[k];
            match quote {
                Some(q) => {
                    if b == q {
                        quote = None;
                    }
                }
                None => {
                    if b == b'"' || b == b'\'' {
                        quote = Some(b);
                    } else if b == b'>' {
                        break;
                    }
                }
            }
            k += 1;
        }
        if k >= bytes.len() {
            break;
        }
        let tag_text = &src[start..=k];
        let mut attrs: BTreeMap<String, String> = BTreeMap::new();
        // Parse attributes from the tag text.
        let tb = tag_text.as_bytes();
        let mut p = 1 + tag.len();
        while p < tb.len() {
            while p < tb.len()
                && (tb[p] == b' '
                    || tb[p] == b'\t'
                    || tb[p] == b'\n'
                    || tb[p] == b'\r'
                    || tb[p] == b'/')
            {
                p += 1;
            }
            if p >= tb.len() || tb[p] == b'>' {
                break;
            }
            let name_start = p;
            while p < tb.len()
                && ((tb[p] as char).is_ascii_alphanumeric()
                    || tb[p] == b'-'
                    || tb[p] == b'_'
                    || tb[p] == b':')
            {
                p += 1;
            }
            if p == name_start {
                p += 1;
                continue;
            }
            let name = tag_text[name_start..p].to_string();
            while p < tb.len() && (tb[p] == b' ' || tb[p] == b'\t') {
                p += 1;
            }
            if p < tb.len() && tb[p] == b'=' {
                p += 1;
                while p < tb.len() && (tb[p] == b' ' || tb[p] == b'\t') {
                    p += 1;
                }
                if p < tb.len() && (tb[p] == b'"' || tb[p] == b'\'') {
                    let q = tb[p];
                    p += 1;
                    let val_start = p;
                    while p < tb.len() && tb[p] != q {
                        p += 1;
                    }
                    attrs.insert(name, tag_text[val_start..p].to_string());
                    if p < tb.len() {
                        p += 1;
                    }
                } else {
                    let val_start = p;
                    while p < tb.len() && tb[p] != b' ' && tb[p] != b'\t' && tb[p] != b'>' {
                        p += 1;
                    }
                    attrs.insert(name, tag_text[val_start..p].to_string());
                }
            } else {
                attrs.insert(name, String::new());
            }
        }
        // Text between `>` and the next `<` (leaf text only).
        let mut text: Option<String> = None;
        if let Some(nlt) = src[k + 1..].find('<') {
            let between = src[k + 1..k + 1 + nlt].trim();
            if !between.is_empty() && !between.starts_with("*/") {
                text = Some(between.to_string());
            }
        }
        out.push(Element {
            tag,
            attrs,
            line: line_of(src, start),
            pos: start,
            text,
        });
        i = k + 1;
    }
    out
}

/// Parse `Ns` CSS pixels from a value like `200px` or `200`.
fn css_px(v: &str) -> Option<f64> {
    let t = v.trim().trim_end_matches("px").trim();
    t.parse::<f64>().ok()
}

/// Parse a CSS color to `#rrggbb` (kept as authored when already hex,
/// resolved from `var(--x, #fallback)` when applicable).
fn css_color(v: &str) -> Option<String> {
    let t = v.trim();
    if t.starts_with('#') {
        let h = t.trim_end_matches(|c: char| !c.is_ascii_hexdigit());
        if h.len() == 4 || h.len() == 7 {
            return Some(h.to_lowercase());
        }
        return None;
    }
    if t.starts_with("var(") {
        // `var(--name, #fallback)`: take the fallback after the comma.
        if let Some((_, fallback)) = t.split_once(',') {
            let f = fallback.trim().trim_end_matches(')');
            if f.starts_with('#') && (f.len() == 4 || f.len() == 7) {
                return Some(f.to_lowercase());
            }
        }
        return None;
    }
    None
}

/// The foreground color of an element: CSS `color` for the id, else
/// the element's inline `color:` declaration, else None.
fn element_color(
    css: &HashMap<String, HashMap<String, String>>,
    el: &Element,
    fallback: Option<String>,
) -> Option<String> {
    if let Some(style) = el.get("style") {
        if let Some((_, v)) = style
            .split(';')
            .find_map(|d| d.split_once(':'))
            .filter(|(k, _)| k.trim() == "color")
        {
            if let Some(c) = css_color(v.trim()) {
                return Some(c);
            }
        }
    }
    if let Some(id) = el.get("id") {
        if let Some(props) = css.get(&format!("#{id}")) {
            if let Some(v) = props.get("color") {
                if let Some(c) = css_color(v) {
                    return Some(c);
                }
            }
        }
    }
    fallback
}

/// A GSAP vars object: ordered `key -> raw value text` where the raw
/// text is the verbatim JS expression (string literal, number,
/// identifier, nested object, or function).
#[derive(Debug, Clone)]
struct Vars {
    entries: Vec<(String, String)>,
}

impl Vars {
    fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Split a JS argument list at top-level commas, skipping string
/// literals (single, double, backtick) and nested brackets.
fn split_args(args: &str) -> Vec<String> {
    let bytes = args.as_bytes();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth: i32 = 0;
    let mut i = 0;
    let mut str_q: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = str_q {
            cur.push(b as char);
            if b == b'\\' && i + 1 < bytes.len() {
                let ch = args[i + 1..].chars().next().unwrap_or('\u{fffd}');
                cur.push(ch);
                i += ch.len_utf8();
            } else if b == q {
                str_q = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' | b'\'' | b'`' => {
                str_q = Some(b);
                cur.push(b as char);
            }
            b'(' | b'[' | b'{' => {
                depth += 1;
                cur.push(b as char);
            }
            b')' | b']' | b'}' => {
                depth -= 1;
                cur.push(b as char);
            }
            b',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur = String::new();
            }
            _ => cur.push(b as char),
        }
        i += 1;
    }
    let last = cur.trim().to_string();
    if !last.is_empty() || !out.is_empty() {
        out.push(last);
    }
    out
}

/// Find the matching close paren for an opening paren at `open`
/// (0-based index into `src`), skipping string literals.
fn match_paren(src: &str, open: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth = 0i32;
    let mut str_q: Option<u8> = None;
    let mut i = open;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = str_q {
            if b == b'\\' && i + 1 < bytes.len() {
                i += 1;
            } else if b == q {
                str_q = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' | b'\'' | b'`' => str_q = Some(b),
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Parse a JS object literal `{ k: v, ... }` into ordered entries.
/// Shorthand, spreads, and computed keys make the literal unparseable
/// (returns None) so the call site is refused rather than guessed.
fn parse_vars(obj: &str) -> Option<Vars> {
    let t = obj.trim();
    if !t.starts_with('{') || !t.ends_with('}') {
        return None;
    }
    let inner = &t[1..t.len() - 1];
    let mut entries = Vec::new();
    // Split at top-level commas.
    let parts = split_args(inner);
    for part in parts {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        // Key may be an identifier or a quoted string; value follows `:`.
        let Some(colon) = find_top_colon(p) else {
            return None;
        };
        let key_raw = p[..colon].trim();
        let val = p[colon + 1..].trim().to_string();
        let key = if (key_raw.starts_with('"') && key_raw.ends_with('"'))
            || (key_raw.starts_with('\'') && key_raw.ends_with('\''))
        {
            key_raw[1..key_raw.len() - 1].to_string()
        } else {
            key_raw.to_string()
        };
        if key.is_empty() || key.starts_with("...") {
            return None;
        }
        entries.push((key, val));
    }
    Some(Vars { entries })
}

/// Index of the first `:` at bracket depth 0 (outside strings).
fn find_top_colon(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    let mut str_q: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = str_q {
            if b == b'\\' && i + 1 < bytes.len() {
                i += 1;
            } else if b == q {
                str_q = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' | b'\'' | b'`' => str_q = Some(b),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b':' if depth == 0 => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Parse a JS string literal to the string it holds.
fn js_string(v: &str) -> Option<String> {
    let t = v.trim();
    for (q, _) in [('"', '"'), ('\'', '\'')] {
        let q = q as u8 as char;
        if t.starts_with(q) && t.ends_with(q) && t.len() >= 2 {
            let inner = &t[1..t.len() - 1];
            let mut out = String::new();
            let mut chars = inner.chars();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                } else {
                    out.push(c);
                }
            }
            return Some(out);
        }
        let _ = q;
    }
    None
}

/// A GSAP ease mapped to its Framesmith name plus the fidelity note.
struct EaseMap {
    /// KDL ease name (already hyphenated).
    kdl: String,
    /// `Some(delta)` when the mapping is an approximation.
    delta: Option<String>,
}

/// Map a GSAP ease string to its Framesmith name. Returns None for
/// anything without an equivalent (the caller refuses).
fn map_ease(gsap: &str) -> Option<EaseMap> {
    let g = gsap.trim();
    let (fam, mode) = match g.split_once('.') {
        Some((f, m)) => (f, m),
        None => (g, ""),
    };
    let fam = fam.trim();
    let mode = mode.trim();
    let named = |base: &str| -> Option<EaseMap> {
        let kdl = match mode {
            "in" => base.to_string(),
            "out" => format!("out-{base}"),
            "inOut" => format!("in-out-{base}"),
            _ => return None,
        };
        Some(EaseMap {
            kdl,
            delta: None,
        })
    };
    match fam {
        "none" | "linear" => Some(EaseMap {
            kdl: "linear".into(),
            delta: None,
        }),
        "power0" => Some(EaseMap {
            kdl: "linear".into(),
            delta: None,
        }),
        "power1" => named("quad"),
        "power2" => named("cubic"),
        "power3" => named("quart"),
        "power4" => named("quint"),
        "expo" => named("expo"),
        "sine" => named("sine"),
        "back" => match mode {
            "out" => Some(EaseMap {
                kdl: "out-back(1.70158)".into(),
                delta: None,
            }),
            "in" | "inOut" => Some(EaseMap {
                // No in-back in framesmith: approximate with a cubic
                // bezier whose control points give the same overshoot
                // start; the max pointwise error against a true
                // quadratic back-in was measured at 0.096 normalized.
                kdl: "cubic-bezier(0.36,0,0.66,-0.56)".into(),
                delta: Some(
                    "back.in approximated as cubic-bezier(0.36,0,0.66,-0.56); max curve error 0.096 of progress"
                        .into(),
                ),
            }),
            _ => None,
        },
        "elastic" => Some(EaseMap {
            kdl: "elastic".into(),
            delta: Some(
                "elastic amplitude/period (GSAP defaults 1/0.3) not expressible; default curve used"
                    .into(),
            ),
        }),
        "bounce" => Some(EaseMap {
            kdl: "bounce".into(),
            delta: Some("bounce ease family collapsed to out-bounce".into()),
        }),
        _ => None,
    }
}

/// One emitted KDL clip line (without trailing newline).
#[derive(Debug, Clone)]
struct ClipLine {
    text: String,
}

/// One emitted KDL motion line (tween/set) without trailing newline.
#[derive(Debug, Clone)]
struct MotionLine {
    text: String,
}

/// A KDL clip with the HTML id it came from (for target resolution).
#[derive(Debug, Clone)]
struct Clip {
    /// Clip ordinal in the emitted document (its `target=` index).
    index: usize,
    /// HTML `id`, when the element had one.
    html_id: Option<String>,
    /// CSS classes (for `.class` selector resolution).
    classes: Vec<String>,
    line: String,
}

/// Parsed reference project: root facts plus flattened clips and motion.
#[derive(Debug)]
struct Project {
    comp_name: String,
    width: f64,
    height: f64,
    duration: f64,
    background: String,
    variables: Vec<(String, String, String)>, // (id, type, default)
    clips: Vec<Clip>,
    motion: Vec<MotionLine>,
    entries: Vec<Entry>,
    /// Asset references relative to the project root, in first-seen order.
    assets: Vec<String>,
    /// The GUIDE one-liner per file (translated as documentation).
    guides: Vec<(String, String)>,
}

impl Project {
    fn refused(&self) -> bool {
        self.entries.iter().any(|e| e.verdict == REFUSED)
    }
}

/// Read a GUIDE comment's first line (`GUIDE: ...`) from HTML source.
fn guide_line(src: &str) -> Option<String> {
    let mut from = 0;
    while let Some(rel) = src[from..].find("<!--") {
        let s = from + rel + 4;
        let Some(e) = src[s..].find("-->") else {
            return None;
        };
        let body = &src[s..s + e];
        for line in body.lines() {
            let t = line.trim();
            if t.starts_with("GUIDE:") {
                return Some(t.trim_start_matches("GUIDE:").trim().to_string());
            }
        }
        from = s + e + 3;
    }
    None
}

/// Resolve a source-relative path against the project root. Absolute
/// paths and remote URLs are refused (out of project tree).
fn resolve_asset_root(root: &Path, rel: &str) -> Option<PathBuf> {
    if rel.starts_with("http://") || rel.starts_with("https://") || rel.starts_with("data:") {
        return None;
    }
    if Path::new(rel).is_absolute() {
        return None;
    }
    Some(root.join(rel))
}

/// The conversion context: file being parsed, its CSS, its elements,
/// and the accumulator. `offset` shifts track times (sub-composition
/// slot start).
struct FileCtx<'a> {
    file_rel: String,
    src: &'a str,
    elements: Vec<Element>,
    css: HashMap<String, HashMap<String, String>>,
    offset: f64,
    root_duration: f64,
    entries: Vec<Entry>,
    clips: Vec<Clip>,
    motion: Vec<MotionLine>,
    assets: Vec<(String, usize)>,
    guides: Vec<(String, String)>,
    variables: &'a [(String, String, String)],
    /// GSAP end-of-timeline cursor (seconds, composition-local).
    cursor: f64,
    /// Clip-index base for sub-compositions (their emitted indices
    /// continue the parent's global numbering).
    index_base: usize,
    /// Resolved (x, y) per element (keyed by document position):
    /// own left/top, else flex-stack placement from the parent.
    positions: HashMap<usize, (f64, f64)>,
    /// Script constants (`const NAME = <number>`) for identifier
    /// substitution in timeline calls.
    consts: HashMap<String, f64>,
    /// `const NAME = document.getElementById("id")` aliases.
    id_aliases: HashMap<String, String>,
    /// Emitted-id counters for duplicate names within the source file.
    id_counters: HashMap<String, usize>,
}

impl<'a> FileCtx<'a> {
    fn line(&self, pos: usize) -> usize {
        line_of(self.src, pos)
    }

    fn el_id(&self, el: &Element) -> Option<String> {
        el.get("id").map(|s| s.to_string())
    }

    /// KDL color for an element: raw `color` (inline, then #id rule)
    /// through the variable-binding color map.
    fn el_color(&self, el: &Element) -> Option<String> {
        if let Some(style) = el.get("style") {
            if let Some((_, v)) = style
                .split(';')
                .find_map(|d| d.split_once(':'))
                .filter(|(k, _)| k.trim() == "color")
            {
                if let Some(c) = kdl_color_attr(v.trim(), self.variables) {
                    return Some(c);
                }
            }
        }
        el.get("id")
            .and_then(|id| self.css_prop(Some(id), "color"))
            .and_then(|raw| kdl_color_attr(&raw, self.variables))
    }

    /// Numeric value of a JS expression: a literal, or an identifier
    /// resolving to a `const NAME = <number>` in the same file.
    fn num_of(&self, raw: &str) -> Option<f64> {
        let t = raw.trim();
        if let Ok(n) = t.parse::<f64>() {
            return Some(n);
        }
        self.consts.get(t).copied()
    }

    /// CSS declaration lookup for an element, inline style first.
    fn css_prop(&self, el_id: Option<&str>, prop: &str) -> Option<String> {
        if let Some(id) = el_id {
            if let Some(props) = self.css.get(&format!("#{id}")) {
                if let Some(v) = props.get(prop) {
                    return Some(v.clone());
                }
            }
        }
        None
    }

    fn el_px(&self, el: &Element, prop: &str) -> Option<f64> {
        if let Some(style) = el.get("style") {
            if let Some((_, v)) = style
                .split(';')
                .find_map(|d| d.split_once(':'))
                .filter(|(k, _)| k.trim() == prop)
            {
                if let Some(x) = css_px(v.trim()) {
                    return Some(x);
                }
            }
        }
        if let Some(id) = el.get("id") {
            if let Some(v) = self.css_prop(Some(id), prop) {
                if let Some(x) = css_px(&v) {
                    return Some(x);
                }
            }
        }
        None
    }
}

/// Byte offset of an element's matching closing tag, depth-counted so
/// nested same-name tags pair correctly. `src.len()` when unclosed.
fn matching_close(src: &str, el: &Element) -> usize {
    let open_tag = format!("<{}", el.tag);
    let close_tag = format!("</{}>", el.tag);
    let mut depth = 0usize;
    // Start after this element's own opening tag.
    let Some(gt) = src[el.pos..].find('>') else {
        return src.len();
    };
    let mut from = el.pos + gt + 1;
    loop {
        let next_open = src[from..].find(&open_tag).map(|i| from + i);
        let next_close = src[from..].find(&close_tag).map(|i| from + i);
        match (next_open, next_close) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                from = o + open_tag.len();
            }
            (_, Some(c)) => {
                if depth == 0 {
                    return c;
                }
                depth -= 1;
                from = c + close_tag.len();
            }
            _ => return src.len(),
        }
    }
}

/// Whether an element holds child elements (a layout container): any
/// other opening tag appears before this element's closing tag.
#[allow(dead_code)]
fn has_child_elements(src: &str, elements: &[Element], el: &Element) -> bool {
    let close_pos = matching_close(src, el);
    elements
        .iter()
        .any(|e| e.pos > el.pos && e.pos < close_pos)
}

/// Resolve one element's (x, y): own left/top when authored, else
/// flex-stack placement from the parent (recorded as an approximation
/// by the caller when used).
fn resolve_position(
    els: &[Element],
    parents: &[Option<usize>],
    i: usize,
    css: &HashMap<String, HashMap<String, String>>,
) -> (f64, f64, bool) {
    let el = &els[i];
    let own_left = own_px(el, css, "left");
    let own_top = own_px(el, css, "top");
    if own_left.is_some() || own_top.is_some() {
        return (
            own_left.unwrap_or(0.0),
            own_top.unwrap_or(0.0),
            false,
        );
    }
    let Some(pi) = parents[i] else {
        return (0.0, 0.0, true);
    };
    let (px, py, _) = resolve_position(els, parents, pi, css);
    // Flex column parent: stack after previous siblings.
    let flex_parent = css
        .get(&format!("#{}", els[pi].get("id").unwrap_or("")))
        .and_then(|p| p.get("display"))
        .map(|v| v.trim() == "flex")
        .unwrap_or(false);
    if flex_parent {
        let gap = css
            .get(&format!("#{}", els[pi].get("id").unwrap_or("")))
            .and_then(|p| p.get("gap"))
            .and_then(|v| css_px(v))
            .unwrap_or(0.0);
        let mut y = py;
        for (k, sib) in els.iter().enumerate() {
            if k >= i {
                break;
            }
            if parents[k] != Some(pi) {
                continue;
            }
            if sib.text.is_none() && sib.tag != "img" && sib.tag != "video" {
                continue;
            }
            let h = match sib.tag.as_str() {
                "img" | "image" | "video" => own_px(sib, css, "height").unwrap_or(64.0),
                _ => own_px(sib, css, "font-size").unwrap_or(24.0),
            };
            y += h + gap;
        }
        return (px, y, true);
    }
    (px, py, true)
}

/// KDL color for a raw CSS color: a declared `var(--name, ...)` binds
/// the variable (`var(--name)`), otherwise the concrete hex fallback.
fn kdl_color_attr(raw: &str, variables: &[(String, String, String)]) -> Option<String> {
    let t = raw.trim();
    if t.starts_with("var(") {
        let inner = &t[4..t.len().saturating_sub(1)];
        let name = inner.split(',').next().unwrap_or("").trim();
        if let Some(var_name) = name.strip_prefix("--") {
            if variables.iter().any(|(id, _, _)| id == var_name) {
                return Some(format!("var(--{var_name})"));
            }
        }
    }
    css_color(t)
}

/// Own left/top/width/height/font-size from inline style or #id CSS.
fn own_px(el: &Element, css: &HashMap<String, HashMap<String, String>>, prop: &str) -> Option<f64> {
    if let Some(style) = el.get("style") {
        if let Some((_, v)) = style
            .split(';')
            .find_map(|d| d.split_once(':'))
            .filter(|(k, _)| k.trim() == prop)
        {
            if let Some(x) = css_px(v.trim()) {
                return Some(x);
            }
        }
    }
    if let Some(id) = el.get("id") {
        if let Some(props) = css.get(&format!("#{id}")) {
            if let Some(v) = props.get(prop) {
                if let Some(x) = css_px(v) {
                    return Some(x);
                }
            }
        }
    }
    None
}

/// Map a GSAP property to its KDL property name, or refuse.
fn map_prop(gsap: &str) -> Option<&'static str> {
    Some(match gsap {
        "x" => "x",
        "y" => "y",
        "opacity" => "opacity",
        "scale" | "scaleX" | "scaleY" => "scale",
        "rotation" => "rotation",
        "rotationX" => "rotation-x",
        "rotationY" => "rotation-y",
        "width" => "width",
        "height" => "height",
        "fontSize" => "font-size",
        "letterSpacing" => "letter-spacing",
        "backgroundColor" | "background-color" => "fill",
        "borderColor" | "border-color" => "border-color",
        "borderRadius" | "border-radius" => "radius",
        _ => return None,
    })
}

/// Whether a GSAP property is a color (KDL colors can't ride `set`).
fn prop_is_color(gsap: &str) -> bool {
    matches!(
        gsap,
        "backgroundColor" | "background-color" | "borderColor" | "border-color"
    )
}

/// Emit one clip for an HTML element and register it. `offset` shifts
/// the clip window (sub-composition slot start).
#[allow(clippy::too_many_arguments)]
fn emit_clip(
    ctx: &mut FileCtx<'_>,
    el: &Element,
    offset: f64,
    default_duration: f64,
    next_index: usize,
) -> Option<Clip> {
    let id = ctx.el_id(el);
    // Start/duration: data-* attributes, else offset + remaining.
    let start = el
        .get("data-start")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(offset);
    let duration = el
        .get("data-duration")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or((default_duration - start).max(MIN_DURATION));
    // Slot files share HTML page scope, so the same id name repeats
    // across files and occasionally within one file: emit a
    // per-file-stem namespace (deterministic, so re-conversion
    // stays byte-identical) with a counter suffix for repeats.
    // Raw ids stay canonical internally for selector matching.
    let id_attr = id
        .as_deref()
        .map(|i| {
            let stem = Path::new(&ctx.file_rel)
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".to_string());
            let stem: String = stem
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                })
                .collect();
            let n = ctx.id_counters.get(i).copied().unwrap_or(0);
            ctx.id_counters.insert(i.to_string(), n + 1);
            let numbered = if n == 0 {
                format!("{i}__{stem}")
            } else {
                format!("{i}__{stem}-{n}")
            };
            format!(" id={}", kdl_str(&numbered))
        })
        .unwrap_or_default();
    let track_attr = el
        .get("data-track-kind")
        .map(|k| format!(" track={}", kdl_str(k)))
        .unwrap_or_default();

    let classes: Vec<String> = el
        .get("class")
        .map(|c| c.split_whitespace().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    let (px, py) = ctx
        .positions
        .get(&el.pos)
        .copied()
        .unwrap_or((0.0, 0.0));

    // Assets: media elements carry a source reference.
    let src_ref = match el.tag.as_str() {
        "img" | "image" => el.get("src"),
        "audio" => el.get("src"),
        "video" => el.get("src"),
        _ => None,
    };
    if let Some(rel) = src_ref {
        if rel.starts_with("http://") || rel.starts_with("https://") || rel.starts_with("data:") {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                el.line,
                &format!("asset {}", el.tag),
                format!(
                    "element #{} references remote asset {rel:?}; only project-local files are copied",
                    id.as_deref().unwrap_or("?")
                ),
            ));
            return None;
        }
        if !ctx.assets.iter().any(|(a, _)| a == rel) {
            ctx.assets.push((rel.to_string(), el.line));
        }
    }

    let line = match el.tag.as_str() {
        "img" | "image" => {
            let path = src_ref.unwrap_or("");
            format!(
                "  image{id_attr}{track_attr} path={} start={} duration={} x={} y={} width={} height={}",
                kdl_str(path),
                fmt_num(start),
                fmt_num(duration),
                fmt_num(px),
                fmt_num(py),
                fmt_num(ctx.el_px(el, "width").unwrap_or(64.0).max(1.0)),
                fmt_num(ctx.el_px(el, "height").unwrap_or(64.0).max(1.0)),
            )
        }
        "audio" => {
            let path = src_ref.unwrap_or("");
            format!(
                "  audio{id_attr}{track_attr} path={} start={} duration={}",
                kdl_str(path),
                fmt_num(start),
                fmt_num(duration),
            )
        }
        "video" => {
            let path = src_ref.unwrap_or("");
            format!(
                "  video{id_attr}{track_attr} path={} start={} duration={} x={} y={}",
                kdl_str(path),
                fmt_num(start),
                fmt_num(duration),
                fmt_num(px),
                fmt_num(py),
            )
        }
        _ if el.text.is_some() || el.get("data-var-text").is_some() => {
            // Text clip: real content, or a render-time variable.
            let (value, vnote) = if let Some(var_name) = el.get("data-var-text") {
                if ctx.variables.iter().any(|(id, _, _)| id == var_name) {
                    (
                        format!("var(--{var_name})"),
                        Some(format!(
                            "text from render-time variable {var_name:?}; default baked as var(--{var_name})"
                        )),
                    )
                } else {
                    // Undeclared variable: the element text is the
                    // default, bound as var(--name, fallback) so check
                    // passes without a declaration (ACC-001).
                    let fallback = el.text.clone().unwrap_or_default();
                    (
                        format!("var(--{var_name}, {fallback})"),
                        Some(format!(
                            "text from render-time variable {var_name:?}; undeclared in index.html, bound as var(--{var_name}, fallback {})",
                            kdl_str(&fallback)
                        )),
                    )
                }
            } else {
                (el.text.clone().unwrap_or_default(), None)
            };
            let size = ctx.el_px(el, "font-size").unwrap_or(24.0).max(1.0);
            let color = ctx
                .el_color(el)
                .unwrap_or_else(|| "#eef1f6".to_string());
            if let Some(note) = vnote {
                ctx.entries.push(Entry::translated(
                    &ctx.file_rel,
                    el.line,
                    "variable text",
                    note,
                ));
            }
            format!(
                "  text{id_attr}{track_attr} value={} start={} duration={} x={} y={} size={} color={}",
                kdl_str(&value),
                fmt_num(start),
                fmt_num(duration),
                fmt_num(px),
                fmt_num(py),
                fmt_num(size),
                kdl_str(&color),
            )
        }
        _ => {
            let inline_bg = el
                .get("style")
                .and_then(|s| {
                    s.split(';')
                        .find_map(|d| d.split_once(':'))
                        .filter(|(k, _)| k.trim() == "background")
                        .map(|(_, v)| v.trim().to_string())
                });
            let bg_raw = inline_bg
                .or_else(|| ctx.css_prop(id.as_deref(), "background"));
            let has_bg = bg_raw
                .as_deref()
                .and_then(|v| css_color(v))
                .is_some();
            let w = ctx.el_px(el, "width").unwrap_or(64.0).max(1.0);
            let h = ctx.el_px(el, "height").unwrap_or(64.0).max(1.0);
            let fill = bg_raw
                .as_deref()
                .and_then(|v| kdl_color_attr(v, ctx.variables))
                .unwrap_or_else(|| "#232836".to_string());
            // A painted element (authored background) becomes a box;
            // a pure layout holder becomes an invisible anchor.
            if has_bg {
                format!(
                    "  box{id_attr}{track_attr} x={} y={} width={} height={} fill={} start={} duration={}",
                    fmt_num(px),
                    fmt_num(py),
                    fmt_num(w),
                    fmt_num(h),
                    kdl_str(&fill),
                    fmt_num(start),
                    fmt_num(duration),
                )
            } else {
                // Layout container (flex/absolute holder with no own
                // paint): an invisible text anchor so motion targeting
                // still resolves. Recorded as an approximation.
                ctx.entries.push(Entry::approximated(
                    &ctx.file_rel,
                    el.line,
                    "container element",
                    format!(
                        "element #{} is a layout container; carried as an empty text anchor for motion targeting",
                        id.as_deref().unwrap_or("?")
                    ),
                    "container background and flex layout not painted; anchor carries no pixels"
                        .to_string(),
                ));
                format!(
                    "  text{id_attr}{track_attr} value=\"\" start={} duration={} x={} y={}",
                    fmt_num(start),
                    fmt_num(duration),
                    fmt_num(px),
                    fmt_num(py),
                )
            }
        }
    };
    Some(Clip {
        index: next_index,
        html_id: id,
        classes,
        line,
    })
}

/// Resolve a GSAP target expression to clip indices. `clips` are the
/// already-emitted clips of the current file in document order.
fn resolve_targets(
    ctx: &mut FileCtx<'_>,
    target_raw: &str,
    call: &str,
    line: usize,
) -> Option<Vec<usize>> {
    let t = target_raw.trim();
    let aliased: String;
    let t = if js_string(t).is_none() {
        match ctx.id_aliases.get(t) {
            Some(id) => {
                aliased = format!("#{id}");
                &aliased
            }
            None => {
                // Identifier or expression (camera rigs, counters,
                // helpers): runtime state with no KDL clip counterpart.
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    line,
                    call,
                    format!(
                        "target {t:?} is not a DOM selector ({t:?} is runtime state); naming the call site instead of guessing a clip"
                    ),
                ));
                return None;
            }
        }
    } else {
        t
    };
    let sel = js_string(t).unwrap_or_else(|| t.to_string());
    if let Some(id) = sel.strip_prefix('#') {
        // Simple `#id` only; descendants/combinators are refused.
        if id.contains([' ', '.', ':', '[', ']', '>', '+', '~', ',', '#']) {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                line,
                call,
                format!("selector {sel:?} is compound; only `#id` and `.class` land"),
            ));
            return None;
        }
        match ctx
            .clips
            .iter()
            .find(|c| c.html_id.as_deref() == Some(id))
        {
            Some(c) => Some(vec![c.index]),
            None => {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    line,
                    call,
                    format!("selector {sel:?} names no converted clip"),
                ));
                None
            }
        }
    } else if let Some(cls) = sel.strip_prefix('.') {
        if cls.contains([' ', '#', ':', '[', ']', '>', '+', '~', ',', '.']) {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                line,
                call,
                format!("selector {sel:?} is compound; only `#id` and `.class` land"),
            ));
            return None;
        }
        let hits: Vec<usize> = ctx
            .clips
            .iter()
            .filter(|c| c.classes.iter().any(|x| x == cls))
            .map(|c| c.index)
            .collect();
        if hits.is_empty() {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                line,
                call,
                format!("selector {sel:?} names no converted clip"),
            ));
            return None;
        }
        Some(hits)
    } else {
        ctx.entries.push(Entry::refused(
            &ctx.file_rel,
            line,
            call,
            format!("selector {sel:?} is not `#id` or `.class`; refusing instead of parsing CSS selectors"),
        ));
        None
    }
}

/// Which GSAP vars keys are structural (not animated properties).
fn is_structural_key(k: &str) -> bool {
    matches!(
        k,
        "duration" | "delay" | "ease" | "stagger" | "repeat" | "yoyo" | "repeatDelay"
    )
}

/// Keys that make the call untranslatable (runtime JS or ScrollTrigger).
/// The caller refuses naming the call site (CAP-002 boundary).
fn is_forbidden_key(k: &str) -> bool {
    k.starts_with("onUpdate") || k.starts_with("onStart") || k.starts_with("onComplete")
        || k.starts_with("onRepeat") || k.starts_with("onRefresh") || k == "callbacks"
        || k == "sanitize" || k == "pin" || k == "scrub" || k == "trigger" || k == "end"
}

/// Apply one already-resolved GSAP call to KDL motion lines. `from_vars`
/// holds the `from` object for `from`/`from-to` calls (None for `to`);
/// `vars` holds the object carrying duration/ease and, for `to`/`from`
/// calls, the endpoint values.
#[allow(clippy::too_many_arguments)]
fn emit_call(
    ctx: &mut FileCtx<'_>,
    call: &str,
    kind: &str,
    target_raw: &str,
    from_vars: Option<&Vars>,
    vars: &Vars,
    start: f64,
    duration: f64,
    duration_defaulted: bool,
    line: usize,
) {
    // Structural validation first: unsupported props refuse the call.
    let all_entries: Vec<(String, String)> = vars
        .entries
        .iter()
        .cloned()
        .chain(from_vars.map(|v| v.entries.iter().cloned()).unwrap_or_default())
        .collect();
    for (k, v) in &all_entries {
        if is_forbidden_key(k) {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                line,
                call,
                format!("key {k:?} is runtime/scroll behavior with no KDL equivalent; refusing the call site"),
            ));
            return;
        }
        if is_structural_key(k) {
            continue;
        }
        if map_prop(k).is_none() {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                line,
                call,
                format!("property {k:?} (value {v:?}) has no KDL motion property; refusing the call site"),
            ));
            return;
        }
    }
    let Some(targets) = resolve_targets(ctx, target_raw, call, line) else {
        return;
    };
    // Ease (tweens only; sets hold with no easing).
    let mut ease_name: Option<String> = None;
    let mut ease_delta: Option<String> = None;
    if kind != "set" {
        let ease_raw = vars.get("ease");
        let gsap_ease = match ease_raw {
            Some(e) => js_string(e).unwrap_or_else(|| e.trim().to_string()),
            None => "power1.out".to_string(),
        };
        let Some(mapped) = map_ease(&gsap_ease) else {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                line,
                call,
                format!("ease {gsap_ease:?} has no Framesmith curve; refusing the call site"),
            ));
            return;
        };
        ease_name = Some(mapped.kdl);
        ease_delta = mapped.delta;
    }
    // Stagger.
    let mut stagger: f64 = 0.0;
    let mut stagger_from: Option<&str> = None;
    if let Some(s) = vars.get("stagger") {
        let s = s.trim();
        if let Some(n) = ctx.num_of(s) {
            stagger = n;
        } else if s.starts_with('{') {
            if let Some(obj) = parse_vars(s) {
                if let Some(each) = obj.get("each") {
                    match ctx.num_of(each) {
                        Some(n) => stagger = n,
                        None => {
                            ctx.entries.push(Entry::refused(
                                &ctx.file_rel,
                                line,
                                call,
                                format!("stagger object `each` is {each:?}; refusing the call site"),
                            ));
                            return;
                        }
                    }
                }
                if let Some(from) = obj.get("from") {
                    let f = js_string(from.trim()).unwrap_or_default();
                    stagger_from = match f.as_str() {
                        "start" => None,
                        "end" => Some("end"),
                        "center" => Some("center"),
                        other => {
                            ctx.entries.push(Entry::refused(
                                &ctx.file_rel,
                                line,
                                call,
                                format!("stagger `from` {other:?} has no origin equivalent; refusing the call site"),
                            ));
                            return;
                        }
                    };
                }
                if obj.get("grid").is_some() {
                    ctx.entries.push(Entry::refused(
                        &ctx.file_rel,
                        line,
                        call,
                        "stagger `grid` has no 2-D equivalent; refusing the call site".to_string(),
                    ));
                    return;
                }
            } else {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    line,
                    call,
                    format!("stagger {s:?} is not a number or simple object; refusing the call site"),
                ));
                return;
            }
        } else {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                line,
                call,
                format!("stagger {s:?} is not a number; refusing the call site"),
            ));
            return;
        }
    }
    // Class targets expand: per-element starts for the stagger.
    let expanded = targets.len() > 1 && stagger != 0.0;
    for (n, idx) in targets.iter().enumerate() {
        let at = if expanded {
            start + n as f64 * stagger
        } else {
            start
        };
        // One motion line per animated property.
        let mut emitted_any = false;
        let animated: Vec<String> = {
            let mut keys: Vec<String> = Vec::new();
            for (k, _) in &all_entries {
                if !is_structural_key(k) && !keys.contains(k) {
                    keys.push(k.clone());
                }
            }
            keys
        };
        for k in &animated {
            let v = vars.get(k).unwrap_or("");
            let fv = from_vars.and_then(|f| f.get(k));
            let Some(prop) = map_prop(k) else {
                continue; // already refused above
            };
            // Numeric or color value.
            let num: Option<f64> = ctx.num_of(v);
            let from_num: Option<f64> = fv.and_then(|x| ctx.num_of(x));
            let color: Option<String> = if prop_is_color(k) {
                js_string(v).and_then(|s| css_color(&s))
            } else {
                None
            };
            let from_color: Option<String> = if prop_is_color(k) {
                fv.and_then(|x| js_string(x)).and_then(|s| css_color(&s))
            } else {
                None
            };
            if !prop_is_color(k) && num.is_none() && from_num.is_none() {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    line,
                    call,
                    format!("property {k:?} value {v:?} is not a number or color; refusing the call site"),
                ));
                return;
            }
            if prop_is_color(k) && color.is_none() && from_color.is_none() {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    line,
                    call,
                    format!("property {k:?} value {v:?} is not a color; refusing the call site"),
                ));
                return;
            }
            if kind == "set" {
                if prop_is_color(k) {
                    ctx.entries.push(Entry::refused(
                        &ctx.file_rel,
                        line,
                        call,
                        format!("`set` on color property {k:?} has no KDL `set` (colors animate with `tween`); refusing the call site"),
                    ));
                    return;
                }
                let hold = (ctx.root_duration - at).max(MIN_DURATION);
                ctx.motion.push(MotionLine {
                    text: format!(
                        "  set target={idx} property={} start={} duration={} value={}",
                        kdl_str(prop),
                        fmt_num(at),
                        fmt_num(hold),
                        fmt_num(num.unwrap_or(0.0)),
                    ),
                });
            } else {
                let kind_attr = match kind {
                    "from" => "from",
                    "from-to" => "from-to",
                    _ => "to",
                };
                let endpoint_attrs = if prop_is_color(k) {
                    let to_c = color.clone().unwrap_or_default();
                    let from_c = from_color.clone().unwrap_or_default();
                    match kind {
                        "from" => format!(" from={}", kdl_str(&to_c)),
                        "from-to" => format!(
                            " from={} to={}",
                            kdl_str(&from_c),
                            kdl_str(&to_c)
                        ),
                        _ => format!(" to={}", kdl_str(&to_c)),
                    }
                } else {
                    match kind {
                        "from" => num
                            .map(|n| format!(" from={}", fmt_num(n)))
                            .unwrap_or_default(),
                        "from-to" => format!(
                            " from={} to={}",
                            fmt_num(from_num.unwrap_or(0.0)),
                            fmt_num(num.unwrap_or(0.0))
                        ),
                        _ => num
                            .map(|n| format!(" to={}", fmt_num(n)))
                            .unwrap_or_default(),
                    }
                };
                let mut text = format!(
                    "  tween target={idx} property={} kind={} start={} duration={}{}",
                    kdl_str(prop),
                    kdl_str(kind_attr),
                    fmt_num(at),
                    fmt_num(duration.max(MIN_DURATION)),
                    endpoint_attrs,
                );
                if let Some(e) = &ease_name {
                    text.push_str(&format!(" ease={}", kdl_str(e)));
                }
                if !expanded && stagger != 0.0 {
                    // Single-piece stagger rides the KDL `stagger` key
                    // (piece splits carry their own delay).
                    text.push_str(&format!(" stagger={}", fmt_num(stagger)));
                    if let Some(sf) = stagger_from {
                        text.push_str(&format!(" stagger-from={}", kdl_str(sf)));
                    }
                }
                ctx.motion.push(MotionLine { text });
            }
            emitted_any = true;
        }
        if !emitted_any {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                line,
                call,
                "call animates no mapped properties; refusing the call site".to_string(),
            ));
            return;
        }
    }
    // Verdict for the call: translated unless something approximated.
    let delta_note = ease_delta
        .clone()
        .filter(|_| duration_defaulted == false)
        .or_else(|| {
            if duration_defaulted {
                Some("duration defaulted 0.5s (GSAP default)".to_string())
            } else {
                None
            }
        })
        .or_else(|| {
            if targets.len() > 1 && stagger != 0.0 {
                Some(format!(
                    "selector matched {} clips; stagger {:.4}s expanded to per-element start offsets",
                    targets.len(),
                    stagger
                ))
            } else {
                None
            }
        })
        .or_else(|| {
            if duration < MIN_DURATION {
                Some(format!(
                    "duration {duration:.4}s below the 1/60s clip minimum; clamped to {:.4}s",
                    MIN_DURATION
                ))
            } else {
                None
            }
        });
    match delta_note {
        Some(delta) => ctx.entries.push(Entry::approximated(
            &ctx.file_rel,
            line,
            call,
            format!("{call} on {target_raw} at start={start:.4} duration={duration:.4}"),
            delta,
        )),
        None => ctx.entries.push(Entry::translated(
            &ctx.file_rel,
            line,
            call,
            format!("{call} on {target_raw} at start={start:.4} duration={duration:.4}"),
        )),
    }
    // Cursor advances to the call end (GSAP sequential append).
    ctx.cursor = ctx.cursor.max(start + duration);
}

/// Walk every inline script in a file and translate its GSAP calls.
/// `clip_pos` maps clip indices to their `Clip` for typeOn length reads.
/// Extract script constants: `const/let/var NAME = <number>` and
/// `const NAME = document.getElementById("id")` aliases.
fn collect_consts(ctx: &mut FileCtx<'_>, body: &str) {
    let bytes = body.as_bytes();
    let keywords = ["const ", "let ", "var "];
    let mut i = 0;
    while i < bytes.len() {
        if !body.is_char_boundary(i) {
            i += 1;
            continue;
        }
        if !keywords.iter().any(|k| body[i..].starts_with(k)) {
            i += 1;
            continue;
        }
        let mut p = i + 6;
        // Skip any of the keywords (they differ in length).
        for k in keywords {
            if body[i..].starts_with(k) {
                p = i + k.len();
                break;
            }
        }
        while p < bytes.len() && bytes[p] == b' ' {
            p += 1;
        }
        let name_start = p;
        while p < bytes.len()
            && ((bytes[p] as char).is_ascii_alphanumeric() || bytes[p] == b'_' || bytes[p] == b'$')
        {
            p += 1;
        }
        if p == name_start {
            i = p.max(i + 1);
            continue;
        }
        let name = body[name_start..p].to_string();
        while p < bytes.len() && bytes[p] == b' ' {
            p += 1;
        }
        if p >= bytes.len() || bytes[p] != b'=' {
            i = p.max(i + 1);
            continue;
        }
        // `==` / `=>` are not declarations.
        if p + 1 < bytes.len() && (bytes[p + 1] == b'=' || bytes[p + 1] == b'>') {
            i = p + 1;
            continue;
        }
        p += 1;
        while p < bytes.len() && bytes[p] == b' ' {
            p += 1;
        }
        let after = p;
        if body[after..].starts_with("document.getElementById(") {
            let arg_open = after + "document.getElementById".len();
            if let Some(close) = match_paren(body, arg_open) {
                let arg = body[arg_open + 1..close].trim();
                if let Some(id) = js_string(arg) {
                    ctx.id_aliases.entry(name.clone()).or_insert(id);
                }
            }
            i = after + 1;
            continue;
        }
        // Number literal.
        let num_start = p;
        while p < bytes.len()
            && ((bytes[p] as char).is_ascii_digit()
                || bytes[p] == b'.'
                || bytes[p] == b'-')
        {
            p += 1;
        }
        if p > num_start {
            if let Ok(n) = body[num_start..p].parse::<f64>() {
                // A trailing identifier char means this was not a lone
                // number (e.g. `1e3` is rare; treat as unresolvable).
                let next = bytes.get(p).copied().unwrap_or(b';');
                if !(next as char).is_ascii_alphanumeric() {
                    ctx.consts.entry(name.clone()).or_insert(n);
                }
            }
        }
        i = p.max(after + 1);
    }
}

fn walk_scripts(ctx: &mut FileCtx<'_>) {
    let bodies = script_bodies(ctx.src);
    for (body_start, src_attr, body) in &bodies {
        if src_attr.is_some() {
            // External scripts (GSAP CDN) are code, not media: noted as
            // translated tooling, never fetched or copied.
            continue;
        }
        collect_consts(ctx, body);
        // ScrollTrigger / pins: scroll-linked motion has no KDL equivalent.
        if body.contains("ScrollTrigger") {
            let line = ctx.line(*body_start);
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                line,
                "ScrollTrigger",
                "ScrollTrigger scroll-linking has no KDL equivalent; refusing the use site".to_string(),
            ));
        }
        // typeOn helper: function typeOn(sel, t, cps) exists; its calls
        // become split/typewriter text clips.
        let has_typeon_def = body.contains("function typeOn(")
            || body.contains("function typeOn (")
            || body.contains("const typeOn");
        // Find tl.<method>(...) / gsap.<method>(...) call sites.
        let bytes = body.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if !body.is_char_boundary(i) {
                i += 1;
                continue;
            }
            let (obj, method_end, method): (usize, usize, String) = {
                // Try `tl.` then `gsap.`.
                let mut found: Option<(usize, usize, String)> = None;
                for prefix in ["tl.", "gsap."] {
                    if body[i..].starts_with(prefix) {
                        let m_start = i + prefix.len();
                        let mut j = m_start;
                        while j < bytes.len()
                            && (bytes[j] as char).is_ascii_alphabetic()
                        {
                            j += 1;
                        }
                        if j > m_start {
                            found = Some((i, j, body[m_start..j].to_string()));
                        }
                        break;
                    }
                }
                match found {
                    Some(f) => f,
                    None => {
                        i += 1;
                        continue;
                    }
                }
            };
            let _ = obj;
            // Expect `(`, possibly after whitespace/comments.
            let mut p = method_end;
            while p < bytes.len()
                && (bytes[p] == b' '
                    || bytes[p] == b'\t'
                    || bytes[p] == b'\n'
                    || bytes[p] == b'\r')
            {
                p += 1;
            }
            if p >= bytes.len() || bytes[p] != b'(' {
                i = method_end;
                continue;
            }
            let call_line = ctx.line(body_start + i);
            let Some(close) = match_paren(body, p) else {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    &format!("{method}"),
                    "unbalanced parentheses; refusing instead of guessing the call".to_string(),
                ));
                break;
            };
            let args_text = &body[p + 1..close];
            i = close + 1;
            let call = format!("tl.{method}");
            match method.as_str() {
                "to" | "from" | "fromTo" | "set" => {
                    handle_call(ctx, *body_start, &call, method.as_str(), args_text, call_line);
                }
                "timeline" => {
                    // Construction of the paused root timeline: the
                    // renderer drives KDL time directly.
                    ctx.entries.push(Entry::translated(
                        &ctx.file_rel,
                        call_line,
                        "gsap.timeline",
                        "paused timeline construction; KDL time is driven by the renderer seek".to_string(),
                    ));
                }
                other => {
                    ctx.entries.push(Entry::refused(
                        &ctx.file_rel,
                        call_line,
                        &call,
                        format!("timeline call {other:?} is outside the translatable grammar (to/from/fromTo/set); refusing the call site"),
                    ));
                }
            }
        }
        if has_typeon_def {
            walk_typeon(ctx, *body_start, body);
        }
    }
}

/// Handle one `to`/`from`/`fromTo`/`set` call: parse args, resolve the
/// start, then emit.
fn handle_call(
    ctx: &mut FileCtx<'_>,
    _body_start: usize,
    call: &str,
    method: &str,
    args_text: &str,
    call_line: usize,
) {
    let args = split_args(args_text);
    let kind = match method {
        "to" => "to",
        "from" => "from",
        "fromTo" => "from-to",
        "set" => "set",
        _ => return,
    };
    // to/from: (target, vars, pos?). fromTo: (target, from, to, pos?).
    // set: (target, vars, pos?).
    let (target_raw, from_vars, to_vars, pos_raw): (
        String,
        Option<Vars>,
        Vars,
        Option<String>,
    ) = if method == "fromTo" {
        if args.len() < 3 || args.len() > 4 {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                call_line,
                call,
                format!("fromTo takes 3-4 arguments, found {}; refusing the call site", args.len()),
            ));
            return;
        }
        let Some(fv) = parse_vars(&args[1]) else {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                call_line,
                call,
                "fromTo `from` object is not a literal; refusing the call site".to_string(),
            ));
            return;
        };
        let Some(tv) = parse_vars(&args[2]) else {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                call_line,
                call,
                "fromTo `to` object is not a literal; refusing the call site".to_string(),
            ));
            return;
        };
        (
            args[0].clone(),
            Some(fv),
            tv,
            args.get(3).cloned(),
        )
    } else {
        if args.len() < 2 || args.len() > 3 {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                call_line,
                call,
                format!("{method} takes 2-3 arguments, found {}; refusing the call site", args.len()),
            ));
            return;
        }
        let Some(tv) = parse_vars(&args[1]) else {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                call_line,
                call,
                format!("{method} vars object is not a literal; refusing the call site"),
            ));
            return;
        };
        (args[0].clone(), None, tv, args.get(2).cloned())
    };
    // Duration (GSAP default 0.5s) and delay.
    let mut duration = 0.5;
    let mut duration_defaulted = false;
    if let Some(d) = to_vars.get("duration") {
        match ctx.num_of(d) {
            Some(n) if n >= 0.0 => duration = n,
            _ => {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    call,
                    format!("duration {d:?} is not a non-negative number; refusing the call site"),
                ));
                return;
            }
        }
    } else if method != "set" {
        duration_defaulted = true;
    }
    let mut delay = 0.0;
    if let Some(d) = to_vars.get("delay") {
        match ctx.num_of(d) {
            Some(n) if n >= 0.0 => delay = n,
            _ => {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    call,
                    format!("delay {d:?} is not a non-negative number; refusing the call site"),
                ));
                return;
            }
        }
    }
    // Start from the position argument, else the cursor.
    let start = match pos_raw.as_deref() {
        None => ctx.cursor,
        Some(p) => {
            let p = p.trim();
            if let Some(n) = ctx.num_of(p) {
                if n < 0.0 {
                    ctx.entries.push(Entry::refused(
                        &ctx.file_rel,
                        call_line,
                        call,
                        format!("position {p:?} is negative; refusing the call site"),
                    ));
                    return;
                }
                n
            } else if let Some(s) = js_string(p) {
                if let Some(rest) = s.strip_prefix("-=").or_else(|| s.strip_prefix("+=")) {
                    let negative = s.starts_with("-=");
                    match ctx.num_of(rest) {
                        Some(n) => {
                            if negative {
                                (ctx.cursor - n).max(0.0)
                            } else {
                                ctx.cursor + n
                            }
                        }
                        None => {
                            ctx.entries.push(Entry::refused(
                                &ctx.file_rel,
                                call_line,
                                call,
                                format!("position {p:?} is not a number, `-=n`, or `+=n`; refusing the call site"),
                            ));
                            return;
                        }
                    }
                } else {
                    ctx.entries.push(Entry::refused(
                        &ctx.file_rel,
                        call_line,
                        call,
                        format!("label position {p:?} has no KDL equivalent; refusing the call site"),
                    ));
                    return;
                }
            } else {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    call,
                    format!("position {p:?} is an expression; refusing the call site"),
                ));
                return;
            }
        }
    };
    let start = start + delay + ctx.offset;
    emit_call(
        ctx,
        call,
        kind,
        &target_raw,
        from_vars.as_ref(),
        &to_vars,
        start,
        duration,
        duration_defaulted,
        call_line,
    );
}

/// Walk `typeOn("sel", t, cps)` calls and land them as split/typewriter
/// text clips (CAP-002).
fn walk_typeon(ctx: &mut FileCtx<'_>, body_start: usize, body: &str) {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !body.is_char_boundary(i) {
            i += 1;
            continue;
        }
        if body[i..].starts_with("typeOn(") {
            let call_line = ctx.line(body_start + i);
            let open = i + "typeOn".len();
            let Some(close) = match_paren(body, open) else {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    "typeOn",
                    "unbalanced parentheses; refusing the call site".to_string(),
                ));
                break;
            };
            let args = split_args(&body[open + 1..close]);
            i = close + 1;
            if args.len() != 3 {
                // A call with the right shape is required; the function
                // declaration itself (`typeOn(sel, t, cps)`) has three
                // args too, so exclude the declaration text.
                let def = args.iter().all(|a| {
                    let a = a.trim();
                    a == "sel" || a == "t" || a == "cps" || a.is_empty()
                });
                if def {
                    continue;
                }
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    "typeOn",
                    format!("typeOn takes (selector, start, chars-per-second); found {} argument(s); refusing the call site", args.len()),
                ));
                continue;
            }
            let Some(sel) = js_string(&args[0]) else {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    "typeOn",
                    format!("typeOn selector {:?} is not a string literal; refusing the call site", args[0]),
                ));
                continue;
            };
            let Ok(t) = args[1].trim().parse::<f64>() else {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    "typeOn",
                    format!("typeOn start {:?} is not a number; refusing the call site", args[1]),
                ));
                continue;
            };
            let Ok(cps) = args[2].trim().parse::<f64>() else {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    "typeOn",
                    format!("typeOn rate {:?} is not a number; refusing the call site", args[2]),
                ));
                continue;
            };
            if !(cps > 0.0) {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    "typeOn",
                    format!("typeOn rate {cps} must be positive; refusing the call site"),
                ));
                continue;
            }
            // Resolve the selector to a text clip for its length.
            // Clip html ids are namespaced by file stem (see
            // `el_id`); try that form first, then the raw name.
            let id = sel.strip_prefix('#').unwrap_or("");
            let stem = Path::new(&ctx.file_rel)
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".to_string());
            let stem: String = stem
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
                .collect();
            let namespaced = format!("{id}__{stem}");
            let Some(clip) = ctx
                .clips
                .iter()
                .find(|c| c.html_id.as_deref() == Some(id) || c.html_id.as_deref() == Some(&namespaced))
            else {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    "typeOn",
                    format!("typeOn selector {sel:?} names no converted clip"),
                ));
                continue;
            };
            let len = clip
                .line
                .split("value=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .unwrap_or("")
                .chars()
                .count();
            if len == 0 {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    "typeOn",
                    format!("typeOn target {sel:?} carries no text; refusing the call site"),
                ));
                continue;
            }
            let reveal = (len as f64 / cps).max(MIN_DURATION);
            let idx = clip.index;
            let window = clip
                .line
                .split("duration=")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(ctx.root_duration);
            if let Some(c) = ctx.clips.iter_mut().find(|c| c.index == idx) {
                c.line.push_str(" split=\"chars\" typewriter=true");
            }
            // A typewriter reveal paces across the whole clip window, so
            // the authored chars/s only holds when the window equals the
            // authored reveal period. Named as an approximation with
            // both rates measured.
            let window_rate = if window > 0.0 {
                len as f64 / window
            } else {
                0.0
            };
            ctx.entries.push(Entry::approximated(
                &ctx.file_rel,
                call_line,
                "typeOn",
                format!(
                    "typeOn {sel:?} at {t:.4} for {} chars at {cps:.4} chars/s lands as split=\"chars\" typewriter on the text clip (authored reveal {:.4}s)",
                    len, reveal
                ),
                format!(
                    "reveal paced over the clip window {window:.4}s gives {window_rate:.4} chars/s instead of {cps:.4} chars/s"
                ),
            ));
        } else {
            i += 1;
        }
    }
}

/// Convert one HTML file (root or a sub-composition) into clips and
/// motion inside `ctx`. Returns the sub-composition duration when the
/// file declares one.
fn convert_file(
    ctx: &mut FileCtx<'_>,
    project_root: &Path,
    slot_offset: f64,
    slot_duration: Option<f64>,
) {
    let elements = ctx.elements.clone();
    // Element tree: matching closes and nearest enclosing parent.
    let closes: Vec<usize> = elements.iter().map(|e| matching_close(ctx.src, e)).collect();
    let parents: Vec<Option<usize>> = elements
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let mut best: Option<usize> = None;
            for (j, other) in elements.iter().enumerate() {
                if j >= i {
                    break;
                }
                if other.pos < e.pos && e.pos < closes[j] {
                    best = Some(j);
                }
            }
            best
        })
        .collect();
    // Resolved positions (own left/top, else flex-stack from parent).
    for i in 0..elements.len() {
        let (x, y, fallback) = resolve_position(&elements, &parents, i, &ctx.css);
        ctx.positions.insert(elements[i].pos, (x, y));
        let el = &elements[i];
        // Only clip candidates (with an id, or media/text content) get a
        // placement verdict: #root and composition slots never become
        // clips, so their placement is not a fidelity construct.
        let is_candidate = el.get("id").is_some()
            && el.get("id") != Some("root")
            && el.get("data-composition-src").is_none();
        if fallback && is_candidate {
            ctx.entries.push(Entry::approximated(
                &ctx.file_rel,
                elements[i].line,
                "placement",
                format!(
                    "element #{} has no own left/top; placed by flow approximation at ({}, {})",
                    elements[i].get("id").unwrap_or("?"),
                    fmt_num(x),
                    fmt_num(y)
                ),
                "CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights"
                    .to_string(),
            ));
        }
    }
    // Layers: elements inside `#root`, in document order. Elements
    // outside the root (head, style, script) never become clips.
    let root_pos = elements
        .iter()
        .find(|e| e.get("id") == Some("root"))
        .map(|e| e.pos);
    let in_root = |e: &Element| root_pos.is_none_or(|r| e.pos > r);
    let mut files_to_convert: Vec<(String, f64, Option<f64>)> = Vec::new();
    for el in &elements {
        if !in_root(el) {
            continue;
        }
        if el.tag == "script" || el.tag == "style" || el.tag == "meta" {
            continue;
        }
        // Composition slot: convert the referenced file instead.
        if let Some(comp_src) = el.get("data-composition-src") {
            let slot_start = el
                .get("data-start")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(slot_offset);
            let slot_dur = el
                .get("data-duration")
                .and_then(|s| s.parse::<f64>().ok());
            files_to_convert.push((
                comp_src.to_string(),
                slot_start,
                slot_dur,
            ));
            continue;
        }
        if el.tag == "html" || el.tag == "head" || el.tag == "body" {
            continue;
        }
        // Skip containers that only hold other elements *and* carry no
        // text: they become invisible anchors (recorded in emit_clip).
        let needs_clip = el.get("id").is_some()
            || el.tag == "img"
            || el.tag == "audio"
            || el.tag == "video"
            || el.text.is_some()
            || el.get("data-var-text").is_some()
            || el.get("class").is_some();
        if !needs_clip {
            continue;
        }
        // Root viewport holder itself is not a clip.
        if el.get("id") == Some("root") {
            continue;
        }
        let index = ctx.index_base + ctx.clips.len();
        if let Some(clip) = emit_clip(
            ctx,
            el,
            slot_offset,
            slot_duration.unwrap_or(ctx.root_duration),
            index,
        ) {
            ctx.clips.push(clip);
        }
        let _ = project_root;
    }
    for (rel, off, dur) in files_to_convert {
        convert_comp_file_full(ctx, project_root, &rel, off, dur);
    }
    // Timeline: GSAP calls in this file's inline scripts.
    walk_scripts(ctx);
}

/// Convert a referenced sub-composition file (recursive).
fn convert_comp_file_full(
    ctx: &mut FileCtx<'_>,
    project_root: &Path,
    rel: &str,
    offset: f64,
    slot_duration: Option<f64>,
) {
    let path = project_root.join(rel);
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            ctx.entries.push(Entry::refused(
                rel,
                1,
                "composition slot",
                format!("sub-composition {rel:?} cannot be read: {e}; refusing naming the file"),
            ));
            return;
        }
    };
    convert_comp_src_full(ctx, project_root, rel, &src, offset, slot_duration);
}

/// Convert sub-composition source text (recursive entry).
fn convert_comp_src_full(
    ctx: &mut FileCtx<'_>,
    project_root: &Path,
    rel: &str,
    src: &str,
    offset: f64,
    slot_duration: Option<f64>,
) {
    let elements = scan_elements(src);
    let mut sub = FileCtx {
        file_rel: rel.to_string(),
        src,
        elements,
        css: parse_css_rules(&style_bodies(src).join("\n")),
        offset,
        root_duration: ctx.root_duration,
        entries: Vec::new(),
        clips: Vec::new(),
        motion: Vec::new(),
        assets: Vec::new(),
        guides: Vec::new(),
        variables: ctx.variables,
        cursor: 0.0,
        index_base: ctx.index_base + ctx.clips.len(),
        positions: HashMap::new(),
        consts: HashMap::new(),
        id_aliases: HashMap::new(),
        id_counters: ctx.id_counters.clone(),
    };
    if let Some(g) = guide_line(src) {
        sub.guides.push((rel.to_string(), g));
    }
    convert_file(&mut sub, project_root, offset, slot_duration);
    ctx.clips.extend(sub.clips.iter().map(|c| c.clone()));
    ctx.motion.extend(sub.motion.iter().map(|m| m.clone()));
    ctx.entries.extend(sub.entries);
    ctx.assets.extend(sub.assets);
    ctx.guides.extend(sub.guides);
    ctx.id_counters = sub.id_counters.clone();
}

/// Render the KDL project text (deterministic, byte-stable).
fn emit_kdl(p: &Project) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "composition name={} root=true width={} height={} fps=30 duration={} background={} version=1 {{\n",
        kdl_str(&p.comp_name),
        fmt_num(p.width),
        fmt_num(p.height),
        fmt_num(p.duration),
        kdl_str(&p.background),
    ));
    for (id, ty, default) in &p.variables {
        let kdl_type = match ty.as_str() {
            "string" => "string",
            "color" => "color",
            "number" => "number",
            "boolean" => "boolean",
            _ => "string",
        };
        out.push_str(&format!(
            "  variable id={} type={} default={}\n",
            kdl_str(id),
            kdl_str(kdl_type),
            kdl_str(default),
        ));
    }
    for c in &p.clips {
        out.push_str(&c.line);
        out.push('\n');
    }
    for m in &p.motion {
        out.push_str(&m.text);
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

/// Render the fidelity report JSON (no timestamps: byte-stable).
fn emit_fidelity_json(p: &Project, source_name: &str) -> String {
    let mut counts = serde_json::Map::new();
    for v in [TRANSLATED, APPROXIMATED, REFUSED] {
        let n = p.entries.iter().filter(|e| e.verdict == v).count();
        counts.insert(v.into(), (n as u64).into());
    }
    let mut guides = Vec::new();
    for (f, g) in &p.guides {
        let mut m = serde_json::Map::new();
        m.insert("file".into(), f.clone().into());
        m.insert("guide".into(), g.clone().into());
        guides.push(serde_json::Value::Object(m));
    }
    let mut root = serde_json::Map::new();
    root.insert("source".into(), source_name.into());
    root.insert("composition".into(), p.comp_name.clone().into());
    root.insert(
        "duration".into(),
        serde_json::Value::Number(serde_json::Number::from_f64(p.duration).unwrap_or(0.into())),
    );
    root.insert("counts".into(), serde_json::Value::Object(counts));
    root.insert("guides".into(), serde_json::Value::Array(guides));
    root.insert(
        "entries".into(),
        serde_json::Value::Array(p.entries.iter().map(|e| e.to_json()).collect()),
    );
    serde_json::to_string_pretty(&serde_json::Value::Object(root)).unwrap()
}

/// Render the human fidelity summary.
fn emit_fidelity_md(p: &Project, source_name: &str) -> String {
    let mut out = String::new();
    let t = p.entries.iter().filter(|e| e.verdict == TRANSLATED).count();
    let a = p.entries.iter().filter(|e| e.verdict == APPROXIMATED).count();
    let r = p.entries.iter().filter(|e| e.verdict == REFUSED).count();
    out.push_str(&format!("# Fidelity report: {source_name} → {}\n\n", p.comp_name));
    out.push_str(&format!(
        "- translated: {t}\n- approximated (with deltas): {a}\n- refused: {r}\n"
    ));
    if r > 0 {
        out.push_str("\n## Refused (cause named; conversion refused)\n\n");
        for e in p.entries.iter().filter(|e| e.verdict == REFUSED) {
            out.push_str(&format!("- `{}`:{} [{}] {}\n", e.file, e.line, e.construct, e.detail));
        }
    }
    if a > 0 {
        out.push_str("\n## Approximated (measured deltas)\n\n");
        for e in p.entries.iter().filter(|e| e.verdict == APPROXIMATED) {
            out.push_str(&format!(
                "- `{}`:{} [{}] {} delta: {}\n",
                e.file,
                e.line,
                e.construct,
                e.detail,
                e.delta.as_deref().unwrap_or("MISSING (gate failure)")
            ));
        }
    }
    out.push_str("\n## Translated\n\n");
    for e in p.entries.iter().filter(|e| e.verdict == TRANSLATED) {
        out.push_str(&format!("- `{}`:{} [{}] {}\n", e.file, e.line, e.construct, e.detail));
    }
    out
}

/// Copy the `assets/` tree beside the conversion, plus any referenced
/// file living outside it.
fn copy_assets(src_root: &Path, out_root: &Path, p: &Project) -> Result<usize, String> {
    let mut copied = 0usize;
    let assets_dir = src_root.join("assets");
    if assets_dir.is_dir() {
        copy_dir_recursive(&assets_dir, &out_root.join("assets"), &mut copied)?;
    }
    for rel in &p.assets {
        if rel.starts_with("assets/") {
            continue; // covered by the tree copy above
        }
        let from = src_root.join(rel);
        let to = out_root.join(rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::copy(&from, &to)
            .map_err(|e| format!("cannot copy {} to {}: {e}", from.display(), to.display()))?;
        copied += 1;
    }
    Ok(copied)
}

fn copy_dir_recursive(from: &Path, to: &Path, copied: &mut usize) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("cannot create {}: {e}", to.display()))?;
    let entries =
        std::fs::read_dir(from).map_err(|e| format!("cannot read {}: {e}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read dir entry: {e}"))?;
        let name = entry.file_name();
        let dest = to.join(&name);
        let ftype = entry
            .file_type()
            .map_err(|e| format!("cannot stat {}: {e}", entry.path().display()))?;
        if ftype.is_dir() {
            copy_dir_recursive(&entry.path(), &dest, copied)?;
        } else {
            std::fs::copy(entry.path(), &dest).map_err(|e| {
                format!("cannot copy {} to {}: {e}", entry.path().display(), dest.display())
            })?;
            *copied += 1;
        }
    }
    Ok(())
}

/// Parse the projects root composition facts and convert everything.
/// Returns the assembled project (clips, motion, entries, assets).
fn convert_project(project_root: &Path) -> Result<Project, String> {
    let index_path = project_root.join("index.html");
    let index_src = match std::fs::read_to_string(&index_path) {
        Ok(s) => s,
        Err(e) => {
            return Err(format!(
                "refused: index.html cannot be read: {e}; the project file index.html is unparseable"
            ))
        }
    };
    // hyperframes.json: validate when present (REC-001).
    let hf_path = project_root.join("hyperframes.json");
    if hf_path.is_file() {
        let hf = std::fs::read_to_string(&hf_path).map_err(|e| {
            format!("refused: hyperframes.json cannot be read: {e}; the project file hyperframes.json is unparseable")
        })?;
        serde_json::from_str::<serde_json::Value>(&hf).map_err(|e| {
            format!("refused: hyperframes.json is not valid JSON: {e}; the project file hyperframes.json is unparseable")
        })?;
    }
    // Root facts from #root.
    let elements = scan_elements(&index_src);
    let root_el = elements
        .iter()
        .find(|e| e.get("id") == Some("root") && e.get("data-composition-id").is_some())
        .ok_or_else(|| {
            "refused: no #root element with data-composition-id; the project file index.html is unparseable".to_string()
        })?;
    let comp_name = root_el
        .get("data-composition-id")
        .unwrap_or("main")
        .to_string();
    let duration: f64 = root_el
        .get("data-duration")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            "refused: #root has no numeric data-duration; the project file index.html is unparseable".to_string()
        })?;
    let width: f64 = root_el
        .get("data-width")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1920.0);
    let height: f64 = root_el
        .get("data-height")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1080.0);
    // Variables from <html data-composition-variables>.
    let mut variables: Vec<(String, String, String)> = Vec::new();
    let mut var_entries: Vec<Entry> = Vec::new();
    if let Some(html_el) = elements.iter().find(|e| e.tag == "html") {
        if let Some(raw) = html_el.get("data-composition-variables") {
            let parsed: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
                format!("refused: data-composition-variables is not valid JSON: {e}; the project file index.html is unparseable")
            })?;
            if let Some(arr) = parsed.as_array() {
                for item in arr {
                    let id = item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let ty = item
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("string")
                        .to_string();
                    let default = item
                        .get("default")
                        .map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .unwrap_or_default();
                    if id.is_empty() {
                        continue;
                    }
                    let dropped: Vec<&str> = item
                        .as_object()
                        .map(|o| {
                            o.keys()
                                .filter(|k| !["id", "type", "default"].contains(&k.as_str()))
                                .map(|k| k.as_str())
                                .collect()
                        })
                        .unwrap_or_default();
                    let known_type = ["string", "color", "number", "boolean"].contains(&ty.as_str());
                    if !dropped.is_empty() || !known_type {
                        let detail = if known_type {
                            format!(
                                "variable {id:?} ({ty:?}) carried with default {default:?}"
                            )
                        } else {
                            format!(
                                "variable {id:?} ({ty:?}) carried with default {default:?}; unknown type mapped to string"
                            )
                        };
                        var_entries.push(Entry::approximated(
                            "index.html",
                            html_el.line,
                            "variable",
                            detail,
                            format!(
                                "fields [{}] and type {:?} not expressible in KDL variables; no render effect",
                                dropped.join(", "),
                                ty
                            ),
                        ));
                    } else {
                        var_entries.push(Entry::translated(
                            "index.html",
                            html_el.line,
                            "variable",
                            format!("variable {id:?} ({ty:?}) with default {default:?}"),
                        ));
                    }
                    variables.push((id, ty, default));
                }
            }
        }
    }
    // Background: last concrete background on html/body/#root rules.
    let css = parse_css_rules(&style_bodies(&index_src).join("\n"));
    let mut background: Option<String> = None;
    for sel in ["#root", "html, body", "html", "body"] {
        if let Some(props) = css.get(sel) {
            if let Some(v) = props.get("background") {
                if let Some(c) = css_color(v) {
                    background = Some(c);
                }
            }
        }
    }
    let mut file_entries: Vec<Entry> = Vec::new();
    let background = match background {
        Some(b) => b,
        None => {
            file_entries.push(Entry::approximated(
                "index.html",
                root_el.line,
                "background",
                "no concrete background found on html/body/#root rules".to_string(),
                "background defaulted to #000000".to_string(),
            ));
            "#000000".to_string()
        }
    };
    // GUIDE documentation.
    let mut guides: Vec<(String, String)> = Vec::new();
    if let Some(g) = guide_line(&index_src) {
        guides.push(("index.html".to_string(), g));
    }
    let mut ctx = FileCtx {
        file_rel: "index.html".to_string(),
        src: &index_src,
        elements: elements.clone(),
        css: css.clone(),
        offset: 0.0,
        root_duration: duration,
        entries: file_entries,
        clips: Vec::new(),
        motion: Vec::new(),
        assets: Vec::new(),
        guides,
        variables: &variables,
        cursor: 0.0,
        index_base: 0,
        positions: HashMap::new(),
        consts: HashMap::new(),
        id_aliases: HashMap::new(),
        id_counters: HashMap::new(),
    };
    convert_file(&mut ctx, project_root, 0.0, Some(duration));
    ctx.entries.splice(0..0, var_entries);
    // fps assumption (HyperFrames renders 30; undeclared in source).
    ctx.entries.push(Entry::approximated(
        "index.html",
        root_el.line,
        "frame rate",
        "fps=30 emitted on the composition root".to_string(),
        "frame rate undeclared in source; HyperFrames default 30fps assumed".to_string(),
    ));
    // Asset existence check (REC-003).
    for (rel, line) in ctx.assets.clone() {
        let exists = resolve_asset_root(project_root, &rel)
            .map(|p| p.is_file() || p.is_dir())
            .unwrap_or(false);
        if !exists {
            ctx.entries.push(Entry::refused(
                "index.html",
                line,
                &format!("asset {rel}"),
                format!("asset {rel:?} is missing from the project tree; refusing naming the asset"),
            ));
        }
    }
    // De-duplicate assets preserving order.
    let mut seen: Vec<String> = Vec::new();
    for (rel, _) in ctx.assets.iter() {
        if !seen.contains(rel) {
            seen.push(rel.clone());
        }
    }
    Ok(Project {
        comp_name,
        width,
        height,
        duration,
        background,
        variables: variables.clone(),
        clips: ctx.clips,
        motion: ctx.motion,
        entries: ctx.entries,
        assets: seen,
        guides: ctx.guides,
    })
}

/// `framesmith convert <html-project> [--out <dir>]` (Sprint 28, T001).
pub fn cmd_convert(src: &Path, out: &Path, json: bool) -> i32 {
    if !src.is_dir() {
        eprintln!(
            "framesmith convert refused: source {} is not a directory.\n                     fix: point convert at a HyperFrames project folder with an index.html.",
            src.display()
        );
        return 2;
    }
    let source_name = src
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| src.display().to_string());
    let project = match convert_project(src) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("framesmith convert {e}");
            // Still write a minimal refusal report when possible.
            let _ = std::fs::create_dir_all(out);
            let refused = Project {
                comp_name: "main".to_string(),
                width: 0.0,
                height: 0.0,
                duration: 0.0,
                background: "#000000".to_string(),
                variables: Vec::new(),
                clips: Vec::new(),
                motion: Vec::new(),
                entries: vec![Entry::refused("index.html", 1, "parse", e.clone())],
                assets: Vec::new(),
                guides: Vec::new(),
            };
            let _ = std::fs::write(out.join("fidelity.json"), emit_fidelity_json(&refused, &source_name));
            return 1;
        }
    };
    let refused = project.refused();
    // Self-check (CAP-003): every approximation must carry deltas.
    let missing: Vec<&Entry> = project
        .entries
        .iter()
        .filter(|e| e.verdict == APPROXIMATED && (e.delta.is_none() || e.delta.as_deref().unwrap_or("").is_empty()))
        .collect();
    if !missing.is_empty() {
        eprintln!(
            "framesmith convert internal refusal: {} approximated construct(s) lack measured deltas; the fidelity verdict fails (CAP-003).",
            missing.len()
        );
        return 1;
    }
    if let Err(e) = std::fs::create_dir_all(out) {
        eprintln!(
            "framesmith convert refused: cannot create output {}: {e}.\n                     fix: choose a writable output folder.",
            out.display()
        );
        return 2;
    }
    let fidelity_json = emit_fidelity_json(&project, &source_name);
    let fidelity_md = emit_fidelity_md(&project, &source_name);
    if let Err(e) = std::fs::write(out.join("fidelity.json"), &fidelity_json) {
        eprintln!("framesmith convert refused: cannot write fidelity.json: {e}.");
        return 2;
    }
    if let Err(e) = std::fs::write(out.join("FIDELITY.md"), &fidelity_md) {
        eprintln!("framesmith convert refused: cannot write FIDELITY.md: {e}.");
        return 2;
    }
    // Gate: approximated entries always carrying deltas are enforced
    // by the self-check above; refusals block the KDL output.
    let t = project.entries.iter().filter(|e| e.verdict == TRANSLATED).count();
    let a = project.entries.iter().filter(|e| e.verdict == APPROXIMATED).count();
    let r = project.entries.iter().filter(|e| e.verdict == REFUSED).count();
    if refused {
        eprintln!(
            "framesmith convert refused for {source_name}: {r} refused construct(s); fidelity report in {}.",
            out.join("fidelity.json").display()
        );
        for e in project.entries.iter().filter(|e| e.verdict == REFUSED) {
            eprintln!("  {}:{} [{}] {}", e.file, e.line, e.construct, e.detail);
        }
        return 1;
    }
    let kdl = emit_kdl(&project);
    if let Err(e) = std::fs::write(out.join("composition.kdl"), &kdl) {
        eprintln!("framesmith convert refused: cannot write composition.kdl: {e}.");
        return 2;
    }
    match copy_assets(src, out, &project) {
        Ok(n) => {
            if json {
                println!(
                    "{{\"converted\":true,\"source\":{:?},\"out\":{:?},\"translated\":{},\"approximated\":{},\"refused\":{},\"assets_copied\":{}}}",
                    source_name,
                    out.display().to_string(),
                    t,
                    a,
                    r,
                    n
                );
            } else {
                println!(
                    "converted {source_name} -> {}\n  translated: {t}, approximated: {a}, refused: {r}\n  assets copied: {n}\n  fidelity: {}",
                    out.display(),
                    out.join("FIDELITY.md").display()
                );
            }
            0
        }
        Err(e) => {
            eprintln!("framesmith convert refused: {e}");
            2
        }
    }
}

