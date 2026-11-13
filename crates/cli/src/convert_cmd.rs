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
            .filter_map(|d| d.split_once(':'))
            .find(|(k, _)| k.trim() == "color")
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

/// Find the `}` matching the `{` at `open`, skipping string literals.
fn match_brace(src: &str, open: usize) -> Option<usize> {
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
            b'{' => depth += 1,
            b'}' => {
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

/// `typeOn(sel, t, cps)` definitions in one script body: `(span,
/// param names)` with the span covering the `function`/`const`
/// keyword through the matching close brace, so `walk_scripts` can
/// skip definition-internal timeline calls and `walk_typeon` can
/// expand call sites as a per-glyph macro.
fn typeon_def_spans(body: &str) -> Vec<(usize, usize, Vec<String>)> {
    let bytes = body.as_bytes();
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = body[from..].find("typeOn") {
        let tpos = from + rel;
        let after = tpos + "typeOn".len();
        let rest = &body[after..];
        let rest_trim = rest.trim_start();
        // The text before `typeOn` must be a definition head:
        // `function`, `const/let/var`, or `=`.
        let before = &body[..tpos];
        let head_ok = before.ends_with("function ")
            || before.ends_with("function\t")
            || before.trim_end().ends_with("function")
            || before.trim_end().ends_with('=')
            || ["const ", "let ", "var "].iter().any(|k| before.trim_end().ends_with(k.trim_end()));
        if !head_ok {
            from = after;
            continue;
        }
        // `typeOn(sel, t, cps)` or `typeOn = (sel, t, cps) =>`.
        let mut p = after + (rest.len() - rest_trim.len());
        if rest_trim.starts_with('=') {
            p = after + (rest.len() - rest_trim.len()) + 1;
            while p < bytes.len() && (bytes[p] == b' ' || bytes[p] == b'\t') {
                p += 1;
            }
        }
        if p >= bytes.len() || bytes[p] != b'(' {
            from = after;
            continue;
        }
        let Some(close_paren) = match_paren(body, p) else {
            from = after;
            continue;
        };
        let params: Vec<String> = split_args(&body[p + 1..close_paren])
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if params.len() != 3 {
            from = after;
            continue;
        }
        // Body brace after `)` (or after `=>`).
        let Some(brace) = body[close_paren..].find('{').map(|i| close_paren + i) else {
            from = after;
            continue;
        };
        let Some(close_brace) = match_brace(body, brace) else {
            from = after;
            continue;
        };
        out.push((tpos, close_brace + 1, params));
        from = close_brace + 1;
    }
    out
}

/// End of one `const NAME = <rhs>` right-hand side: the index just
/// past the depth-0 `;` (string-aware), or the next depth-0 `,`
/// (a following declarator), `}` (the enclosing block ends), or the
/// end of the body.
fn rhs_end_of(src: &str, from: usize) -> usize {
    let bytes = src.as_bytes();
    let mut depth = 0i32;
    let mut str_q: Option<u8> = None;
    let mut i = from;
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
            b';' if depth == 0 => return i + 1,
            b',' if depth == 0 => return i,
            b')' | b']' | b'}' if depth == 0 => return i,
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    src.len()
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

/// Bounded arithmetic evaluator for GSAP timeline numbers: numeric
/// literals, identifiers (script consts, then caller-supplied
/// bindings such as loop indices or `typeOn` params), parentheses,
/// unary `+/-`, and binary `+ - * /`. Anything else — calls, member
/// access (`a.b`), indexing (`a[i]`), comparisons, strings — fails
/// with the first offending token so callers name the call site in
/// their refusal (CAP-002 / REC-002). Division by zero and non-finite
/// results fail too. This is the numeric grammar lane GSAP positions,
/// durations and `typeOn` args share.
fn eval_arith(expr: &str, lookup: &dyn Fn(&str) -> Option<f64>) -> Result<f64, String> {
    #[derive(Debug, Clone)]
    enum Tok {
        Num(f64),
        Ident(String),
        Op(u8),
    }
    let bytes = expr.as_bytes();
    let mut toks: Vec<Tok> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            i += 1;
            continue;
        }
        if b.is_ascii_digit() || (b == b'.' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit()) {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let text = &expr[start..i];
            match text.parse::<f64>() {
                Ok(n) if n.is_finite() => toks.push(Tok::Num(n)),
                _ => return Err(format!("malformed number {text:?} in the numeric grammar")),
            }
            continue;
        }
        if (b as char).is_ascii_alphabetic() || b == b'_' || b == b'$' {
            let start = i;
            while i < bytes.len()
                && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
            {
                i += 1;
            }
            toks.push(Tok::Ident(expr[start..i].to_string()));
            continue;
        }
        match b {
            b'+' | b'-' | b'*' | b'/' | b'(' | b')' => {
                toks.push(Tok::Op(b));
                i += 1;
            }
            _ => {
                return Err(format!(
                    "unsupported token {:?} in the numeric grammar (+ - * / parens, numbers, identifiers)",
                    b as char
                ));
            }
        }
    }
    if toks.is_empty() {
        return Err("empty expression in the numeric grammar".to_string());
    }

    struct P<'x> {
        toks: &'x [Tok],
        pos: usize,
        lookup: &'x dyn Fn(&str) -> Option<f64>,
    }
    impl<'x> P<'x> {
        fn peek(&self) -> Option<&'x Tok> {
            self.toks.get(self.pos)
        }
        fn bump(&mut self) -> Option<&'x Tok> {
            let t = self.toks.get(self.pos);
            if t.is_some() {
                self.pos += 1;
            }
            t
        }
        fn expr(&mut self) -> Result<f64, String> {
            let mut v = self.term()?;
            loop {
                match self.peek() {
                    Some(Tok::Op(b'+')) => {
                        self.pos += 1;
                        v += self.term()?;
                    }
                    Some(Tok::Op(b'-')) => {
                        self.pos += 1;
                        v -= self.term()?;
                    }
                    _ => return Ok(v),
                }
                if !v.is_finite() {
                    return Err("non-finite result in the numeric grammar".to_string());
                }
            }
        }
        fn term(&mut self) -> Result<f64, String> {
            let mut v = self.factor()?;
            loop {
                match self.peek() {
                    Some(Tok::Op(b'*')) => {
                        self.pos += 1;
                        v *= self.factor()?;
                    }
                    Some(Tok::Op(b'/')) => {
                        self.pos += 1;
                        let r = self.factor()?;
                        if r == 0.0 {
                            return Err("division by zero in the numeric grammar".to_string());
                        }
                        v /= r;
                    }
                    _ => return Ok(v),
                }
                if !v.is_finite() {
                    return Err("non-finite result in the numeric grammar".to_string());
                }
            }
        }
        fn factor(&mut self) -> Result<f64, String> {
            match self.bump() {
                Some(Tok::Num(n)) => Ok(*n),
                Some(Tok::Ident(name)) => match (self.lookup)(name) {
                    Some(n) => Ok(n),
                    None => Err(format!(
                        "unbound identifier {name:?} in the numeric grammar (+ - * / parens, numbers, identifiers)"
                    )),
                },
                Some(Tok::Op(b'+')) => self.factor(),
                Some(Tok::Op(b'-')) => self.factor().map(|v| -v),
                Some(Tok::Op(b'(')) => {
                    let v = self.expr()?;
                    match self.bump() {
                        Some(Tok::Op(b')')) => Ok(v),
                        _ => Err("missing closing paren in the numeric grammar".to_string()),
                    }
                }
                Some(Tok::Op(o)) => Err(format!(
                    "unexpected operator {:?} in the numeric grammar",
                    *o as char
                )),
                None => Err("incomplete expression in the numeric grammar".to_string()),
            }
        }
    }
    let mut p = P {
        toks: &toks,
        pos: 0,
        lookup,
    };
    let v = p.expr()?;
    if p.pos < p.toks.len() {
        return Err("trailing tokens in the numeric grammar".to_string());
    }
    Ok(v)
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
    // Parameterized forms carry a call suffix: `back.out(s)` and
    // `steps(N)`. Split it off first so the family/mode logic below
    // matches on the bare name.
    let (base, param): (&str, Option<&str>) = match g.split_once('(') {
        Some((b, p)) if p.ends_with(')') && p.len() >= 2 => {
            (b.trim(), Some(p[..p.len() - 1].trim()))
        }
        _ => (g, None),
    };
    let (fam, mode) = match base.split_once('.') {
        Some((f, m)) => (f, m),
        None => (base, ""),
    };
    let fam = fam.trim();
    let mode = mode.trim();
    // Only `back` and `steps` take parameters; any other parameterized
    // ease is outside the grammar (never silently drop the parameter).
    if let Some(p) = param {
        if !matches!(fam, "back" | "steps") {
            return None;
        }
        if p.is_empty() {
            return None;
        }
    }
    let named = |base: &str| -> Option<EaseMap> {
        let kdl = match mode {
            "in" => format!("in-{base}"),
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
        "steps" => match param {
            // `steps(N)` is a native Framesmith curve with the same
            // jump-at-interval-end shape: a verbatim landing, no delta.
            Some(p) => match p.parse::<f64>() {
                Ok(n) if n.is_finite() && n.fract() == 0.0 && n >= 1.0 && n <= u32::MAX as f64 => {
                    Some(EaseMap {
                        kdl: format!("steps({})", n as u64),
                        delta: None,
                    })
                }
                _ => None,
            },
            None => None,
        },
        "back" => match mode {
            "out" => {
                // `back.out(s)` is exactly `out-back(s)` (same curve
                // formula, GSAP default s = 1.70158): no delta.
                let s: f64 = match param {
                    None => 1.70158,
                    Some(p) => match p.parse::<f64>() {
                        Ok(n) if n.is_finite() && n >= 0.0 => n,
                        _ => return None,
                    },
                };
                Some(EaseMap {
                    kdl: format!("out-back({})", fmt_num(s)),
                    delta: None,
                })
            }
            "in" | "inOut" => {
                // No in-back in framesmith: approximate with a cubic
                // bezier whose control points give the same overshoot
                // start; the max pointwise error against a true
                // quadratic back-in was measured at 0.096 normalized.
                // Only the GSAP default overshoot lands this way; any
                // other `s` names a refusal instead of guessing.
                if let Some(p) = param {
                    match p.parse::<f64>() {
                        Ok(n) if (n - 1.70158).abs() < 1e-9 => {}
                        _ => return None,
                    }
                }
                Some(EaseMap {
                    kdl: "cubic-bezier(0.36,0,0.66,-0.56)".into(),
                    delta: Some(
                        "back.in approximated as cubic-bezier(0.36,0,0.66,-0.56); max curve error 0.096 of progress"
                            .into(),
                    ),
                })
            }
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

/// Parse a GSAP `keyframes` array literal (`[{ scale: 1.25, duration:
/// 0.12 }, ...]`) into one `Vars` per stop. Anything but an array of
/// object literals (identifiers like `path`, bare objects, spreads)
/// fails naming the value, so the call site refuses (REC-002).
fn parse_keyframes(raw: &str) -> Result<Vec<Vars>, String> {
    let t = raw.trim();
    if !t.starts_with('[') || !t.ends_with(']') {
        return Err(format!(
            "keyframes value {raw:?} is not an array literal; refusing the call site"
        ));
    }
    let inner = &t[1..t.len() - 1];
    let parts = split_args(inner);
    let mut stops = Vec::new();
    for part in parts {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        match parse_vars(p) {
            Some(v) => stops.push(v),
            None => {
                return Err(format!(
                    "keyframes stop {p:?} is not an object literal; refusing the call site"
                ));
            }
        }
    }
    if stops.is_empty() {
        return Err("keyframes array is empty; refusing the call site".to_string());
    }
    Ok(stops)
}

/// One validated GSAP keyframe stop: its own duration (seconds), its
/// optional authored ease string, and its animated values (GSAP key,
/// resolved number).
struct KfStop {
    duration: f64,
    ease_raw: Option<String>,
    props: Vec<(String, f64)>,
}

/// A validated chain of GSAP keyframe stops ready to land as KDL
/// `key` blocks.
struct KfPlan {
    stops: Vec<KfStop>,
    /// Sum of the stop durations (the call's total span).
    total: f64,
    /// Whether any stop defaulted its duration to the 0.5s GSAP
    /// default (named in the fidelity delta).
    dur_defaulted: bool,
}

/// Validate a parsed `keyframes` value into a `KfPlan`, pushing a
/// named refusal and returning `None` on anything outside the
/// grammar: `set` calls, keyframes in the from-vars, runtime keys,
/// unmapped or color properties (their start values are untracked),
/// non-numeric values, non-positive durations, and unmapped stop
/// eases. Returns `Some(None)` when the call carries no keyframes.
fn plan_keyframes(
    ctx: &mut FileCtx<'_>,
    kind: &str,
    call: &str,
    line: usize,
    from_vars: Option<&Vars>,
    raw: Option<&str>,
) -> Option<Option<KfPlan>> {
    let Some(raw) = raw else {
        return Some(None);
    };
    let refuse = |ctx: &mut FileCtx<'_>, cause: String| -> Option<Option<KfPlan>> {
        ctx.entries.push(Entry::refused(&ctx.file_rel, line, call, cause));
        None
    };
    if kind == "set" {
        return refuse(
            ctx,
            "property \"keyframes\" on a `set` call has no chained form in GSAP; refusing the call site"
                .to_string(),
        );
    }
    if kind == "from" {
        return refuse(
            ctx,
            "property \"keyframes\" on a `from` call reverses the chain in GSAP; refusing the call site"
                .to_string(),
        );
    }
    if from_vars.is_some_and(|f| f.get("keyframes").is_some()) {
        return refuse(
            ctx,
            "property \"keyframes\" in the from-vars is outside the grammar; refusing the call site"
                .to_string(),
        );
    }
    let stops = match parse_keyframes(raw) {
        Ok(s) => s,
        Err(cause) => return refuse(ctx, cause),
    };
    let mut plan_stops: Vec<KfStop> = Vec::new();
    let mut total = 0.0;
    let mut dur_defaulted = false;
    for stop in &stops {
        let duration = match stop.get("duration") {
            Some(d) => match ctx.num_of(d) {
                Some(n) if n > 0.0 => n,
                Some(_) => {
                    return refuse(
                        ctx,
                        format!(
                            "keyframes stop duration {d:?} is not a positive time; refusing the call site"
                        ),
                    );
                }
                None => {
                    return refuse(
                        ctx,
                        format!(
                            "keyframes stop duration {d:?} is outside the numeric grammar; refusing the call site"
                        ),
                    );
                }
            },
            None => {
                dur_defaulted = true;
                0.5
            }
        };
        total += duration;
        let ease_raw = stop.get("ease").map(|e| {
            js_string(e).unwrap_or_else(|| e.trim().to_string())
        });
        let mut props: Vec<(String, f64)> = Vec::new();
        for (k, v) in &stop.entries {
            if k == "duration" || k == "ease" {
                continue;
            }
            if is_forbidden_key(k) {
                return refuse(
                    ctx,
                    format!(
                        "key {k:?} inside a keyframes stop is runtime/scroll behavior with no KDL equivalent; refusing the call site"
                    ),
                );
            }
            if prop_is_color(k) {
                return refuse(
                    ctx,
                    format!(
                        "property {k:?} (value {v:?}) in a keyframes stop is a color whose start value the converter does not track; refusing the call site"
                    ),
                );
            }
            if map_prop(k).is_none() {
                let _ = v;
                return refuse(
                    ctx,
                    format!(
                        "property {k:?} in a keyframes stop has no KDL motion property; refusing the call site"
                    ),
                );
            }
            // Last value per property wins (GSAP semantics).
            if let Some(slot) = props.iter_mut().find(|(pk, _)| pk == k) {
                match ctx.num_of(v) {
                    Some(n) => slot.1 = n,
                    None => {
                        return refuse(
                            ctx,
                            format!(
                                "property {k:?} value {v:?} in a keyframes stop is not a number; refusing the call site"
                            ),
                        );
                    }
                }
                continue;
            }
            match ctx.num_of(v) {
                Some(n) => props.push((k.clone(), n)),
                None => {
                    return refuse(
                        ctx,
                        format!(
                            "property {k:?} value {v:?} in a keyframes stop is not a number; refusing the call site"
                        ),
                    );
                }
            }
        }
        // A stop ease must map (it rides its segment in the KDL key);
        // the call-level ease stays the fallback for easing-less stops.
        if let Some(e) = &ease_raw {
            if map_ease(e).is_none() {
                return refuse(
                    ctx,
                    format!(
                        "ease {e:?} in a keyframes stop has no Framesmith curve; refusing the call site"
                    ),
                );
            }
        }
        plan_stops.push(KfStop {
            duration,
            ease_raw,
            props,
        });
    }
    Some(Some(KfPlan {
        stops: plan_stops,
        total,
        dur_defaulted,
    }))
}

/// A GSAP `transformOrigin` snapped to a KDL clip `origin` pivot.
struct OriginSnap {
    /// KDL placement string (3x3 keyword form).
    placement: String,
    /// Fidelity delta (always present: the snap and the 2D-pivot
    /// caveat both name a loss).
    delta: String,
}

/// Why a `transformOrigin` could not snap. `droppable` marks
/// origins that still mean something (pixel offsets whose element
/// size is unknown): the call lands the rest of its motion and the
/// fidelity report names the loss, instead of refusing the call.
struct SnapFail {
    cause: String,
    droppable: bool,
}

/// Snap a GSAP `transformOrigin` (`"0 50%"`, `"left top"`,
/// `"1015.341px 432.0px"`, …) to the nearest KDL clip `origin`
/// placement. KDL pivots are a 3x3 keyword grid, so percentages and
/// keyword origins snap to the nearest third and pixel origins
/// normalize against the element's own width/height first; every
/// snap names its error as a fraction of element size. The delta
/// also states the render caveat: the clip `origin` pivots 3D turns
/// only, while KDL 2D scale/rotation pivot at a built-in point, so
/// the GSAP 2D pivot does not fully land. Unparseable origins (or
/// pixel origins without a known element size) fail naming the value.
fn snap_origin(
    raw: &str,
    el_w: Option<f64>,
    el_h: Option<f64>,
) -> Result<OriginSnap, SnapFail> {
    let fail = |cause: String, droppable: bool| SnapFail { cause, droppable };
    let unreadable = || {
        fail(
            format!(
                "transformOrigin {raw:?} is not a readable origin; refusing the call site"
            ),
            false,
        )
    };
    let s = js_string(raw).unwrap_or_else(|| raw.trim().to_string());
    let toks: Vec<&str> = s.split_whitespace().collect();
    if toks.is_empty() || toks.len() > 2 {
        return Err(fail(
            format!(
                "transformOrigin {raw:?} is not a one- or two-part origin; refusing the call site"
            ),
            false,
        ));
    }
    // One token of one axis fixes that axis; the other settles at the
    // middle (GSAP single-keyword origins).
    let axis = |tok: &str, dim: Option<f64>| -> Result<f64, SnapFail> {
        let t = tok.to_lowercase();
        match t.as_str() {
            "left" | "top" => return Ok(0.0),
            "right" | "bottom" => return Ok(1.0),
            "center" | "middle" => return Ok(0.5),
            _ => {}
        }
        if let Some(pct) = t.strip_suffix('%') {
            match pct.parse::<f64>() {
                Ok(n) if n.is_finite() => return Ok(n / 100.0),
                _ => {}
            }
        }
        let num = t
            .strip_suffix("px")
            .map(|n| n.trim())
            .unwrap_or(t.as_str());
        match num.parse::<f64>() {
            // Zero needs no element size (0px is the edge wherever
            // the border box starts).
            Ok(n) if n.is_finite() && n == 0.0 => Ok(0.0),
            Ok(n) if n.is_finite() => match dim {
                Some(d) if d > 0.0 => Ok(n / d),
                _ => Err(fail(
                    format!(
                        "transformOrigin {raw:?} uses pixel offsets but the element size is unknown"
                    ),
                    true,
                )),
            },
            _ => Err(unreadable()),
        }
    };
    // Keywords fix their axis; positional tokens (percent/pixels)
    // fill x then y. A lone axis keyword settles the other axis in
    // the middle.
    let mut fx: Option<f64> = None;
    let mut fy: Option<f64> = None;
    for tok in &toks {
        let kw = tok.to_lowercase();
        if kw == "left" || kw == "right" {
            fx = Some(axis(tok, el_w)?);
        } else if kw == "top" || kw == "bottom" {
            fy = Some(axis(tok, el_h)?);
        } else if kw == "center" || kw == "middle" {
            if fx.is_none() {
                fx = Some(0.5);
            } else {
                fy = Some(0.5);
            }
        } else if fx.is_none() {
            fx = Some(axis(tok, el_w)?);
        } else if fy.is_none() {
            fy = Some(axis(tok, el_h)?);
        } else {
            return Err(unreadable());
        }
    }
    let fx = fx.unwrap_or(0.5);
    let fy = fy.unwrap_or(0.5);
    let snap = |f: f64| -> (f64, &'static str) {
        if f < 0.25 {
            (0.0, "left")
        } else if f < 0.75 {
            (0.5, "center")
        } else {
            (1.0, "right")
        }
    };
    let vsnap = |f: f64| -> (f64, &'static str) {
        if f < 0.25 {
            (0.0, "top")
        } else if f < 0.75 {
            (0.5, "center")
        } else {
            (1.0, "bottom")
        }
    };
    let (sx, hx) = snap(fx);
    let (sy, vy) = vsnap(fy);
    let placement = if hx == "center" && vy == "center" {
        "center".to_string()
    } else {
        format!("{hx} {vy}")
    };
    let ex = (fx - sx).abs();
    let ey = (fy - sy).abs();
    Ok(OriginSnap {
        placement: placement.clone(),
        delta: format!(
            "transformOrigin {raw:?} snapped to clip origin {:?} (KDL pivot is a 3x3 placement; pivot off by {ex:.3} x / {ey:.3} of element size); the clip origin pivots 3D turns only, while KDL 2D scale/rotation pivot at a built-in point, so the GSAP 2D pivot does not fully land",
            placement
        ),
    })
}

/// Write a snapped pivot onto the target clip's line (` origin=...`,
/// replacing any earlier origin on the same clip). Audio clips carry
/// no pivot: the call refuses naming it.
fn amend_clip_origin(ctx: &mut FileCtx<'_>, idx: usize, placement: &str) -> Result<(), String> {
    let Some(clip) = ctx.clips.iter_mut().find(|c| c.index == idx) else {
        return Err(format!(
            "transformOrigin names clip index {idx} that was not emitted; refusing the call site"
        ));
    };
    if clip.line.starts_with("  audio") {
        return Err(format!(
            "transformOrigin targets clip {idx}, an audio clip with no visual pivot; refusing the call site"
        ));
    }
    let attr = format!(" origin={}", kdl_str(placement));
    // Replace the earlier pivot (last origin wins, as in GSAP). The
    // anchor is the last ` origin="` (real attrs only: quotes inside
    // KDL string values are backslash-escaped, so they never match).
    if let Some(pos) = clip.line.rfind(" origin=\"") {
        let value_start = pos + " origin=\"".len();
        if let Some(close_rel) = clip.line[value_start..].find('"') {
            clip.line.replace_range(pos..value_start + close_rel + 1, &attr);
            return Ok(());
        }
    }
    clip.line.push_str(&attr);
    Ok(())
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
    /// Index into the emitting file's element scan (compound
    /// descendant resolution); `el_file` scopes it.
    el_index: usize,
    /// Emitting file (`FileCtx::file_rel`); scopes `el_index`.
    el_file: String,
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
    /// Decoded in-memory assets (data: URIs, inline `<svg>`
    /// extraction): `(output-relative path, bytes)`, written into
    /// the output tree on conversion success.
    data_files: Vec<(String, Vec<u8>)>,
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
    /// Nearest enclosing element per element index (`None` for
    /// roots); filled by `convert_file` for compound descendant
    /// selector and compound CSS resolution.
    parents: Vec<Option<usize>>,
    /// Decoded in-memory assets (data: URIs, inline `<svg>`
    /// extraction): `(output-relative path, bytes)`, written into
    /// the output tree on conversion success.
    data_files: Vec<(String, Vec<u8>)>,
    /// Last `set` value per (clip index, KDL property): GSAP `to`/
    /// `from` tweens capture the current value, which KDL resolves
    /// from the clip's static value instead; a tracked `set` value
    /// lets the converter emit an exact from-to pair.
    last_set_val: HashMap<(usize, String), f64>,
    /// Emitted `set` windows (motion line, target, property, start,
    /// end, value text) so a later tween on the same (clip,
    /// property) can truncate the hold where GSAP's captured-value
    /// semantics would take over.
    set_recs: Vec<SetRec>,
}

/// One emitted `set` window that a later tween may truncate.
#[derive(Debug, Clone)]
struct SetRec {
    motion_idx: usize,
    target: usize,
    prop: String,
    start: f64,
    end: f64,
    value_fmt: String,
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
                .filter_map(|d| d.split_once(':'))
                .find(|(k, _)| k.trim() == "color")
            {
                if let Some(c) = kdl_color_attr(v.trim(), self.variables) {
                    return Some(c);
                }
            }
        }
        el.get("id")
            .and_then(|id| self.css_prop(Some(id), "color"))
            .and_then(|raw| kdl_color_attr(&raw, self.variables))
            .or_else(|| {
                self.compound_prop_of(el, "color")
                    .and_then(|raw| kdl_color_attr(&raw, self.variables))
            })
    }

    /// Numeric value of a JS expression in the bounded arithmetic
    /// grammar (literals, `+/-*/` and parens, identifiers): extra
    /// bindings (loop indices, `typeOn` params) shadow script
    /// consts, then `const NAME = <number>` of the same file.
    /// `Err` names the first unresolvable token for the caller's
    /// domain-error refusal.
    fn arith_of(&self, raw: &str, extra: &[(&str, f64)]) -> Result<f64, String> {
        let consts = &self.consts;
        eval_arith(raw, &|name| {
            extra
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| *v)
                .or_else(|| consts.get(name).copied())
        })
    }

    /// Numeric value of a JS expression over script consts only.
    fn num_of(&self, raw: &str) -> Option<f64> {
        self.arith_of(raw, &[]).ok()
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

    /// Compound-CSS declaration for one element (nearest
    /// `#ancestor-id tag` / `#ancestor-id .cls` rule).
    fn compound_prop_of(&self, el: &Element, prop: &str) -> Option<String> {
        let idx = self.elements.iter().position(|e| e.pos == el.pos)?;
        compound_prop(&self.elements, &self.parents, idx, &self.css, prop)
    }

    fn el_px(&self, el: &Element, prop: &str) -> Option<f64> {
        if let Some(style) = el.get("style") {
            if let Some((_, v)) = style
                .split(';')
                .filter_map(|d| d.split_once(':'))
                .find(|(k, _)| k.trim() == prop)
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
        self.compound_prop_of(el, prop)
            .and_then(|v| css_px(&v))
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
    let own_left = own_px_full(els, parents, i, css, "left");
    let own_top = own_px_full(els, parents, i, css, "top");
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
                "img" | "image" | "video" | "svg" => {
                    own_px_full(els, parents, k, css, "height").unwrap_or(64.0)
                }
                _ => own_px_full(els, parents, k, css, "font-size").unwrap_or(24.0),
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
            .filter_map(|d| d.split_once(':'))
            .find(|(k, _)| k.trim() == prop)
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
        // `blur()` inside a CSS filter chain (Sprint 7 motion,
        // CAP-006) is the one filter function KDL animates as
        // motion; everything else in a chain is refused or named
        // as remainder (see `parse_css_filter`).
        "blur" => "blur",
        _ => return None,
    })
}

/// One parsed CSS `filter` chain: the `blur` sigma (when the chain
/// has a `blur()` atom) and the verbatim remainder atoms
/// (`brightness()`, `contrast()`, `grayscale()`, `saturate()`)
/// with no KDL motion property.
struct CssFilter {
    /// Blur sigma in canvas pixels (`blur(Npx)`), last atom wins.
    blur: Option<f64>,
    remainder: Vec<String>,
}

/// Parse a CSS `filter` function chain (`blur(6px) brightness(0.4)`
/// etc.): `blur(N)`/`blur(Npx)` atoms land as the `blur` motion
/// property; the brightness/contrast/grayscale/saturate atoms are
/// kept verbatim as the refused remainder (Sprint 11 picture
/// treatments, never drawn as motion — only `blur` animates today).
/// `none` parses to an identity chain. Any other function
/// (`hue-rotate`, `url(...)`, …) fails with the atom named, so the
/// call site refuses rather than silently dropping it (REC-005).
fn parse_css_filter(chain: &str) -> Result<CssFilter, String> {
    let t = chain.trim();
    if t.eq_ignore_ascii_case("none") {
        // No filter at all: the blur sigma resets to 0.
        return Ok(CssFilter {
            blur: Some(0.0),
            remainder: Vec::new(),
        });
    }
    if t.is_empty() {
        return Ok(CssFilter {
            blur: None,
            remainder: Vec::new(),
        });
    }
    let bytes = t.as_bytes();
    let mut i = 0usize;
    let mut blur: Option<f64> = None;
    let mut remainder: Vec<String> = Vec::new();
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let name_start = i;
        while i < bytes.len()
            && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
        {
            i += 1;
        }
        if i == name_start {
            return Err(format!(
                "filter {chain:?} is not a function chain (unexpected {:?} at byte {i})",
                t[i..].chars().next().unwrap_or('\u{fffd}')
            ));
        }
        let name = &t[name_start..i];
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'(' {
            return Err(format!("filter function {name:?} has no argument list ({chain:?})"));
        }
        let Some(close) = match_paren(t, i) else {
            return Err(format!("filter function {name:?} has unbalanced parentheses ({chain:?})"));
        };
        let arg = t[i + 1..close].trim();
        let atom = format!("{name}({arg})");
        match name {
            "blur" => {
                let num = arg.strip_suffix("px").unwrap_or(arg).trim();
                match num.parse::<f64>() {
                    Ok(n) if n.is_finite() && n >= 0.0 => {
                        // A later atom overrides (atom chain order).
                        let _ = &atom;
                        blur = Some(n);
                    }
                    Ok(_) => {
                        return Err(format!("blur value {arg:?} is negative or non-finite; refusing instead of guessing a sigma ({chain:?})"))
                    }
                    Err(_) => {
                        return Err(format!("blur value {arg:?} is not a number of pixels; refusing instead of guessing a sigma ({chain:?})"))
                    }
                }
            }
            "brightness" | "contrast" | "grayscale" | "saturate" => {
                remainder.push(atom);
            }
            other => {
                return Err(format!(
                    "filter function {other:?} has no KDL motion property; refusing instead of dropping it ({chain:?})"
                ));
            }
        }
        i = close + 1;
    }
    Ok(CssFilter { blur, remainder })
}

/// Decode a base64 payload (standard alphabet, padding optional,
/// whitespace ignored). Decodes fail with the first offending
/// character or a length error, so callers name the asset (REC-003).
fn b64_decode(payload: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Result<u32, u8> {
        match c {
            b'A'..=b'Z' => Ok((c - b'A') as u32),
            b'a'..=b'z' => Ok((c - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((c - b'0' + 52) as u32),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(c),
        }
    }
    let clean: Vec<u8> = payload
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    let mut out: Vec<u8> = Vec::with_capacity(clean.len() * 3 / 4 + 3);
    let mut i = 0usize;
    let mut acc: u32 = 0;
    let mut acc_bits = 0u32;
    while i < clean.len() {
        let c = clean[i];
        if c == b'=' {
            // Padding: only trailing pads follow.
            if clean[i..].iter().any(|&b| b != b'=') {
                return Err("non-padding data after base64 padding".to_string());
            }
            break;
        }
        let Some(a) = val(c).ok() else {
            return Err(format!("unsupported base64 character {:?}", c as char));
        };
        acc = (acc << 6) | a;
        acc_bits += 6;
        if acc_bits >= 8 {
            acc_bits -= 8;
            out.push((acc >> acc_bits) as u8);
        }
        i += 1;
    }
    // Trailing quantum: a complete quad leaves no bits. A 2-char
    // quantum leaves 4 bits (the top of the second byte); a 3-char
    // quantum leaves 2 bits (which must be zero, so they drop); a
    // lone char leaves 6 bits and can never decode.
    match acc_bits {
        0 => {}
        4 => out.push((acc >> 4) as u8),
        2 => {
            if acc & 0b11 != 0 {
                return Err("non-zero trailing bits in the final base64 quantum".to_string());
            }
        }
        _ => return Err("truncated base64 quantum (length % 4 == 1)".to_string()),
    }
    Ok(out)
}

/// Percent-decode a data: payload to raw bytes (RFC 2397): `%XX`
/// hex octets; `+` stays literal (this is not form encoding).
fn percent_decode(payload: &str) -> Result<Vec<u8>, String> {
    let bytes = payload.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(format!("truncated percent escape at byte {i}"));
            }
            let hex = |b: u8| -> Result<u8, String> {
                match b {
                    b'0'..=b'9' => Ok(b - b'0'),
                    b'a'..=b'f' => Ok(b - b'a' + 10),
                    b'A'..=b'F' => Ok(b - b'A' + 10),
                    _ => Err(format!("invalid percent escape {:?}", (b as char))),
                }
            };
            out.push((hex(bytes[i + 1])? << 4) | hex(bytes[i + 2])?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Ok(out)
}

/// A decoded data: asset: output file extension plus the raw bytes.
struct DataAsset {
    ext: &'static str,
    bytes: Vec<u8>,
    /// Whether the payload arrived base64 (`;base64`) or
    /// percent-encoded.
    base64: bool,
}

/// Decode a `data:` URI into asset bytes. The mime picks the output
/// file extension of a format the engine decodes (image: PNG, JPEG,
/// still GIF, HEIC/HEIF, SVG; video: the probed containers; audio:
/// the known ext set); any other mime fails naming it (REC-003).
fn data_uri_decode(src: &str) -> Result<DataAsset, String> {
    let rest = src.strip_prefix("data:").unwrap_or("");
    let Some((meta, payload)) = rest.split_once(',') else {
        return Err("data: URI has no payload separator ','".to_string());
    };
    let base64 = meta
        .split(';')
        .skip(1)
        .any(|p| p.eq_ignore_ascii_case("base64"));
    let mime = meta.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let ext: &'static str = match mime.as_str() {
        "image/svg+xml" => "svg",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/heic" => "heic",
        "image/heif" => "heif",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/m4a" | "audio/x-m4a" => "m4a",
        "audio/aac" => "aac",
        "audio/flac" => "flac",
        "audio/aiff" | "audio/x-aiff" => "aiff",
        other if other.is_empty() => {
            return Err("data: URI names no mime; framesmith decodes image/svg+xml, image/png, image/jpeg, image/gif, image/heic, image/heif, video/mp4, video/webm and audio/* assets only".to_string())
        }
        other => {
            return Err(format!(
                "data: URI mime {other:?} is unsupported; framesmith decodes image/svg+xml, image/png, image/jpeg, image/gif, image/heic, image/heif, video/mp4, video/webm and audio/* assets only"
            ))
        }
    };
    let bytes = if base64 {
        b64_decode(payload)?
    } else {
        percent_decode(payload)?
    };
    Ok(DataAsset { ext, bytes, base64 })
}

/// Sanitize a name fragment for a generated asset path (file names
/// cannot carry every HTML id character).
fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// Append one decoded asset to a file list, uniquifying the output
/// path deterministically (same bytes reuse the path; different
/// bytes get a `-2`, `-3`, … suffix) and returning the path used.
fn push_data_file(files: &mut Vec<(String, Vec<u8>)>, rel: String, bytes: Vec<u8>) -> String {
    if files.iter().any(|(r, b)| *r == rel && *b == bytes) {
        return rel;
    }
    if !files.iter().any(|(r, _)| *r == rel) {
        files.push((rel.clone(), bytes));
        return rel;
    }
    let (stem, ext) = rel.rsplit_once('.').unwrap_or((rel.as_str(), ""));
    let parent = &rel[..rel.len() - stem.len() - ext.len() - 1];
    for n in 2usize.. {
        let candidate = if ext.is_empty() {
            format!("{parent}{stem}-{n}")
        } else {
            format!("{parent}{stem}-{n}.{ext}")
        };
        if files.iter().any(|(r, b)| *r == candidate && *b == bytes) {
            return candidate;
        }
        if !files.iter().any(|(r, _)| *r == candidate) {
            files.push((candidate.clone(), bytes));
            return candidate;
        }
    }
    unreachable!("uniquifier loop always finds a free suffix");
}

/// Generated-asset path for a data: URI or extracted inline SVG:
/// `assets/data-<id>.<ext>` when the element has an id, else
/// `assets/data-<file-stem>-<line>.<ext>` — deterministic, so
/// re-conversion stays byte-identical.
fn data_file_path(file_rel: &str, id: Option<&str>, line: usize, ext: &str) -> String {
    let base = match id {
        Some(i) => format!("data-{}", sanitize_name(i)),
        None => {
            let stem = Path::new(file_rel)
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".to_string());
            format!("data-{}-{}", sanitize_name(&stem), line)
        }
    };
    if ext.is_empty() {
        format!("assets/{base}")
    } else {
        format!("assets/{base}.{ext}")
    }
}

/// Compound-CSS declaration for one element: `#ancestor-id tag` and
/// `#ancestor-id .cls` rules (kept verbatim in the parsed map)
/// fill in one property the inline and simple `#id` lookups did
/// not answer, nearest ancestor first. Emitted-id namespacing
/// never applies here (raw ids are canonical).
fn compound_prop(
    els: &[Element],
    parents: &[Option<usize>],
    idx: usize,
    css: &HashMap<String, HashMap<String, String>>,
    prop: &str,
) -> Option<String> {
    let el = &els[idx];
    let tag = el.tag.as_str();
    let classes: Vec<String> = el
        .get("class")
        .map(|c| c.split_whitespace().map(|s| s.to_string()).collect())
        .unwrap_or_default();
    let mut j = parents.get(idx).copied().flatten();
    while let Some(pi) = j {
        if let Some(pid) = els[pi].get("id") {
            if let Some(v) = css.get(&format!("#{pid} {tag}")).and_then(|p| p.get(prop)) {
                return Some(v.clone());
            }
            for cls in &classes {
                if let Some(v) = css.get(&format!("#{pid} .{cls}")).and_then(|p| p.get(prop)) {
                    return Some(v.clone());
                }
            }
        }
        j = parents.get(pi).copied().flatten();
    }
    None
}

/// Own left/top/width/height/font-size from inline style, the `#id`
/// rule, and compound `#ancestor-id tag` / `#ancestor-id .cls`
/// rules (in that order; compound fills what the simpler lookups
/// missed).
fn own_px_full(
    els: &[Element],
    parents: &[Option<usize>],
    idx: usize,
    css: &HashMap<String, HashMap<String, String>>,
    prop: &str,
) -> Option<f64> {
    if let Some(x) = own_px(&els[idx], css, prop) {
        return Some(x);
    }
    compound_prop(els, parents, idx, css, prop).and_then(|v| css_px(&v))
}

/// Whether a GSAP property is a color (KDL colors can't ride `set`).
fn prop_is_color(gsap: &str) -> bool {
    matches!(
        gsap,
        "backgroundColor" | "background-color" | "borderColor" | "border-color"
    )
}

/// Emit one clip for an HTML element and register it. `offset` shifts
/// the clip window (sub-composition slot start). `el_index` is the
/// element's index in the file scan (compound descendant
/// resolution).
#[allow(clippy::too_many_arguments)]
fn emit_clip(
    ctx: &mut FileCtx<'_>,
    el: &Element,
    el_index: usize,
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

    let (mut px, mut py) = ctx
        .positions
        .get(&el.pos)
        .copied()
        .unwrap_or((0.0, 0.0));
    // An inline `<svg>` positioned with own left/top (usually via a
    // compound `#ancestor-id svg` rule) is absolutely positioned
    // relative to its parent in CSS: its canvas position is the
    // parent's resolved position plus its own offsets. Other clips
    // keep the file-wide raw-left/top convention.
    if el.tag == "svg" {
        let own_left = ctx.el_px(el, "left");
        let own_top = ctx.el_px(el, "top");
        if own_left.is_some() || own_top.is_some() {
            if let Some(pi) = ctx.parents.get(el_index).copied().flatten() {
                if let Some(parent) = ctx.elements.get(pi) {
                    if let Some((ox, oy)) = ctx.positions.get(&parent.pos).copied() {
                        px = ox + own_left.unwrap_or(0.0);
                        py = oy + own_top.unwrap_or(0.0);
                    }
                }
            }
        }
    }

    // Assets: media elements carry a source reference. `data:` URIs
    // decode into the output assets tree (REC-003 keeps unsupported
    // mimes and decode failures as naming refusals); inline
    // `<svg>` is extracted the same way so compound selectors hit
    // a real picture clip.
    let src_ref = match el.tag.as_str() {
        "img" | "image" => el.get("src"),
        "audio" => el.get("src"),
        "video" => el.get("src"),
        _ => None,
    };
    let mut clip_path: String = String::new();
    if let Some(rel) = src_ref {
        if rel.starts_with("http://") || rel.starts_with("https://") {
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
        if let Some(payload) = rel.strip_prefix("data:") {
            let full = format!("data:{payload}");
            match data_uri_decode(&full) {
                Ok(decoded) => {
                    let raw = data_file_path(&ctx.file_rel, id.as_deref(), el.line, decoded.ext);
                    let n = decoded.bytes.len();
                    let how = if decoded.base64 { "base64" } else { "percent-encoded" };
                    let out_path = push_data_file(&mut ctx.data_files, raw, decoded.bytes);
                    ctx.entries.push(Entry::translated(
                        &ctx.file_rel,
                        el.line,
                        &format!("asset {}", el.tag),
                        format!(
                            "element #{} data: URI decoded to {out_path} ({n} bytes, {how})",
                            id.as_deref().unwrap_or("?")
                        ),
                    ));
                    clip_path = out_path;
                }
                Err(cause) => {
                    ctx.entries.push(Entry::refused(
                        &ctx.file_rel,
                        el.line,
                        &format!("asset {}", el.tag),
                        format!(
                            "element #{} references a data: asset that does not decode: {cause}",
                            id.as_deref().unwrap_or("?")
                        ),
                    ));
                    return None;
                }
            }
        } else {
            clip_path = rel.to_string();
            if !ctx.assets.iter().any(|(a, _)| a == &clip_path) {
                ctx.assets.push((clip_path.clone(), el.line));
            }
        }
    }
    if el.tag == "svg" && clip_path.is_empty() {
        // Inline vector picture: extract the markup beside the
        // conversion (adding the XML namespace the HTML spelling
        // omits) and carry it as an image clip at the declared
        // size.
        let close = matching_close(ctx.src, el);
        if close >= ctx.src.len() {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                el.line,
                "asset svg",
                "inline <svg> has no closing tag; refusing naming the asset".to_string(),
            ));
            return None;
        }
        let markup = &ctx.src[el.pos..close + "</svg>".len()];
        let namespaced = if markup.contains("xmlns") {
            markup.to_string()
        } else {
            let mut owned = String::with_capacity(markup.len() + 48);
            owned.push_str(&markup[..markup.find("<svg").unwrap() + 4]);
            owned.push_str(" xmlns=\"http://www.w3.org/2000/svg\"");
            owned.push_str(&markup[markup.find("<svg").unwrap() + 4..]);
            owned
        };
        let raw = data_file_path(&ctx.file_rel, id.as_deref(), el.line, "svg");
        let n = namespaced.len();
        let out_path = push_data_file(&mut ctx.data_files, raw, namespaced.into_bytes());
        ctx.entries.push(Entry::translated(
            &ctx.file_rel,
            el.line,
            "asset svg",
            format!(
                "inline <svg> extracted to {out_path} ({n} bytes; xmlns added: {})",
                !markup.contains("xmlns")
            ),
        ));
        clip_path = out_path;
    }

    let line = match el.tag.as_str() {
        "img" | "image" | "svg" => {
            let width = ctx
                .el_px(el, "width")
                .or_else(|| el.get("width").and_then(|w| css_px(w)))
                .unwrap_or(64.0)
                .max(1.0);
            let height = ctx
                .el_px(el, "height")
                .or_else(|| el.get("height").and_then(|h| css_px(h)))
                .unwrap_or(64.0)
                .max(1.0);
            format!(
                "  image{id_attr}{track_attr} path={} start={} duration={} x={} y={} width={} height={}",
                kdl_str(&clip_path),
                fmt_num(start),
                fmt_num(duration),
                fmt_num(px),
                fmt_num(py),
                fmt_num(width),
                fmt_num(height),
            )
        }
        "audio" => {
            format!(
                "  audio{id_attr}{track_attr} path={} start={} duration={}",
                kdl_str(&clip_path),
                fmt_num(start),
                fmt_num(duration),
            )
        }
        "video" => {
            format!(
                "  video{id_attr}{track_attr} path={} start={} duration={} x={} y={}",
                kdl_str(&clip_path),
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
                        .filter_map(|d| d.split_once(':'))
                        .find(|(k, _)| k.trim() == "background")
                        .map(|(_, v)| v.trim().to_string())
                });
            let bg_raw = inline_bg
                .or_else(|| ctx.css_prop(id.as_deref(), "background"))
                .or_else(|| ctx.compound_prop_of(el, "background"));
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
        el_index,
        el_file: ctx.file_rel.clone(),
        line,
    })
}

/// Descendant clips of a compound selector `#id tag` / `#id .cls`
/// (`document.querySelectorAll` semantics): every clip in the
/// current file whose element descends from an element with the
/// scope id and matches the second half — a bare tag name
/// (case-insensitive) or a class — in document order.
fn resolve_compound(ctx: &FileCtx<'_>, scope_id: &str, rest: &str, is_tag: bool) -> Vec<usize> {
    let cls = rest.strip_prefix('.').unwrap_or(rest);
    let is_descendant_match = |idx: usize| -> bool {
        let mut j = ctx.parents.get(idx).copied().flatten();
        let mut scoped = false;
        while let Some(pi) = j {
            if ctx.elements[pi].get("id") == Some(scope_id) {
                scoped = true;
                break;
            }
            j = ctx.parents.get(pi).copied().flatten();
        }
        if !scoped {
            return false;
        }
        if is_tag {
            ctx.elements[idx].tag.eq_ignore_ascii_case(rest)
        } else {
            ctx.elements[idx]
                .get("class")
                .map(|c| c.split_whitespace().any(|x| x == cls))
                .unwrap_or(false)
        }
    };
    let mut hits = Vec::new();
    for clip in &ctx.clips {
        if clip.el_file != ctx.file_rel {
            continue;
        }
        if is_descendant_match(clip.el_index) {
            hits.push(clip.index);
        }
    }
    hits
}

/// Resolve one selector string to clip indices (`#id` first-match;
/// `.class` all matches, or first only for array entries; `#id tag`
/// / `#id .cls` all descendant matches). Refusals name the call
/// site.
fn resolve_selector(
    ctx: &mut FileCtx<'_>,
    sel: &str,
    call: &str,
    line: usize,
    first_only: bool,
) -> Option<Vec<usize>> {
    // Compound descendant selectors `#id tag` / `#id .cls`: the
    // descendant combinator is the only compound shape that lands.
    let toks: Vec<&str> = sel.split_whitespace().collect();
    if toks.len() == 2 {
        if let Some(scope_id) = toks[0].strip_prefix('#') {
            let scope_clean = !scope_id.is_empty()
                && !scope_id.contains(['.', ':', '[', ']', '>', '+', '~', ',', '#']);
            if scope_clean {
                let rest = toks[1];
                let is_tag = rest
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_alphabetic())
                    .unwrap_or(false);
                let rest_ok = if is_tag {
                    rest.chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                } else if let Some(cls) = rest.strip_prefix('.') {
                    !cls.is_empty()
                        && !cls.contains(['.', ':', '[', ']', '>', '+', '~', ',', '#', ' '])
                } else {
                    false
                };
                if rest_ok {
                    let hits = resolve_compound(ctx, scope_id, rest, is_tag);
                    if hits.is_empty() {
                        ctx.entries.push(Entry::refused(
                            &ctx.file_rel,
                            line,
                            call,
                            format!("selector {sel:?} names no converted clip"),
                        ));
                        return None;
                    }
                    return Some(if first_only { vec![hits[0]] } else { hits });
                }
            }
        }
    }
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
        Some(if first_only {
            vec![hits[0]]
        } else {
            hits
        })
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

/// Resolve one GSAP target expression to clip indices. `clips` are
/// the already-emitted clips of the current file in document order.
/// Array literals (`["#a", "#b"]`) resolve per entry with the first
/// matching clip per entry; any unresolvable entry refuses the call.
fn resolve_targets(
    ctx: &mut FileCtx<'_>,
    target_raw: &str,
    call: &str,
    line: usize,
) -> Option<Vec<usize>> {
    let t = target_raw.trim();
    if t.starts_with('[') {
        if !t.ends_with(']') {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                line,
                call,
                format!("target {t:?} is a malformed array literal; refusing the call site"),
            ));
            return None;
        }
        let mut entries = split_args(&t[1..t.len() - 1]);
        while entries.last().is_some_and(|e| e.trim().is_empty()) {
            entries.pop();
        }
        if entries.is_empty() {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                line,
                call,
                format!("target {t:?} is an empty array; refusing the call site"),
            ));
            return None;
        }
        let mut out = Vec::new();
        for raw in &entries {
            let raw = raw.trim();
            let entry_sel: String;
            let sel_ref: &str = if js_string(raw).is_some() {
                raw
            } else if let Some(id) = ctx.id_aliases.get(raw) {
                entry_sel = format!("#{id}");
                &entry_sel
            } else {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    line,
                    call,
                    format!(
                        "array target entry {raw:?} is not a string selector or DOM-id alias; refusing the call site"
                    ),
                ));
                return None;
            };
            let sel = js_string(sel_ref).unwrap_or_else(|| sel_ref.to_string());
            if let Some(hits) = resolve_selector(ctx, &sel, call, line, true) {
                out.extend(hits);
            } else {
                return None;
            }
        }
        return Some(out);
    }
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
    resolve_selector(ctx, &sel, call, line, false)
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

/// Truncate earlier `set` holds on one (clip, property) to end where
/// a tween begins at `at` (GSAP captured-value semantics: the set
/// value holds until the tween takes over; a KDL hold-to-composition-
/// end window would otherwise layer over the tween and mask it).
/// Returns true when a truncated hold clamped to the 1/60s minimum,
/// masking the tween's first 1/60s (named in the fidelity delta).
fn truncate_set_holds(ctx: &mut FileCtx<'_>, target: usize, prop: &str, at: f64) -> bool {
    let mut clamped = false;
    for rp in 0..ctx.set_recs.len() {
        let rec = ctx.set_recs[rp].clone();
        if rec.target == target && rec.prop == prop && rec.start <= at && at < rec.end {
            let raw_dur = at - rec.start;
            let new_dur = raw_dur.max(MIN_DURATION);
            if raw_dur < MIN_DURATION {
                clamped = true;
            }
            let m_idx = rec.motion_idx;
            ctx.motion[m_idx].text = format!(
                "  set target={} property={} start={} duration={} value={}",
                rec.target,
                kdl_str(&rec.prop),
                fmt_num(rec.start),
                fmt_num(new_dur),
                rec.value_fmt,
            );
            ctx.set_recs[rp].end = rec.start + new_dur;
        }
    }
    clamped
}

/// Apply one already-resolved GSAP call to KDL motion lines. `from_vars`
/// holds the `from` object for `from`/`from-to` calls (None for `to`);
/// `vars` holds the object carrying duration/ease and, for `to`/`from`
/// calls, the endpoint values. `duration_note` carries the fidelity
/// delta when a positional duration argument overrode the vars one.
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
    duration_note: Option<String>,
    line: usize,
) {
    // Structural validation first: unsupported props refuse the call.
    let all_entries: Vec<(String, String)> = vars
        .entries
        .iter()
        .cloned()
        .chain(from_vars.map(|v| v.entries.iter().cloned()).unwrap_or_default())
        .collect();
    // CSS `filter` chains: `blur()` atoms land as the `blur` motion
    // property; brightness/contrast/grayscale/saturate atoms are the
    // explicitly refused remainder (named with its delta, never
    // silently dropped); any other function refuses the call site.
    let mut from_filter: Option<CssFilter> = None;
    let mut to_filter: Option<CssFilter> = None;
    let parse_filter_side = |raw: Option<&str>| -> Option<Result<CssFilter, String>> {
        let fv = raw?;
        let Some(chain) = js_string(fv) else {
            return Some(Err(format!(
                "property \"filter\" value {fv:?} is not a string literal; refusing the call site"
            )));
        };
        match parse_css_filter(&chain) {
            Ok(parsed) => Some(Ok(parsed)),
            Err(cause) => Some(Err(format!(
                "property \"filter\" (value {fv:?}): {cause}"
            ))),
        }
    };
    if let Some(res) = parse_filter_side(from_vars.and_then(|f| f.get("filter"))) {
        match res {
            Ok(p) => from_filter = Some(p),
            Err(cause) => {
                ctx.entries.push(Entry::refused(&ctx.file_rel, line, call, cause));
                return;
            }
        }
    }
    if let Some(res) = parse_filter_side(vars.get("filter")) {
        match res {
            Ok(p) => to_filter = Some(p),
            Err(cause) => {
                ctx.entries.push(Entry::refused(&ctx.file_rel, line, call, cause));
                return;
            }
        }
    }
    // GSAP `immediateRender`: KDL has no carryback flag; the
    // from-state starts at its position and earlier evaluation
    // differs — approximated with the delta named.
    let mut immediate_delta: Option<String> = None;
    for (k, v) in &all_entries {
        if k == "immediateRender" {
            let t = v.trim();
            if !matches!(t, "true" | "false" | "0" | "1" | "0.0" | "1.0") {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    line,
                    call,
                    format!("key \"immediateRender\" value {v:?} is not a boolean literal; refusing the call site"),
                ));
                return;
            }
            immediate_delta = Some(format!(
                "immediateRender {t} not expressible in KDL (no carryback flag); the from-state starts at its position {start:.4}s, so evaluation before the start differs from GSAP"
            ));
        }
    }
    // GSAP keyframes: an array of stop objects landing as KDL `key`
    // blocks (a named refusal lands here when outside the grammar).
    let Some(kf_plan) = plan_keyframes(ctx, kind, call, line, from_vars, vars.get("keyframes"))
    else {
        return;
    };
    // `transformOrigin` lands on the target clips' `origin` pivot
    // (snapped with the named delta per target); the from-vars win
    // over the to-vars, as in GSAP.
    let origin_raw: Option<&str> = from_vars
        .and_then(|f| f.get("transformOrigin"))
        .or_else(|| vars.get("transformOrigin"));
    // The keyframe stops fix the call's total span.
    let orig_duration = duration;
    let orig_duration_defaulted = duration_defaulted;
    let (duration, duration_defaulted) = match &kf_plan {
        Some(plan) => (plan.total, false),
        None => (duration, duration_defaulted),
    };
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
        if is_structural_key(k)
            || k == "filter"
            || k == "immediateRender"
            || k == "keyframes"
            || k == "transformOrigin"
        {
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
    // Blur endpoints of the chain (call-wide; per-target from-state
    // resolution happens in the loop below) plus the remainder the
    // fidelity report names.
    let blur_from = from_filter.as_ref().and_then(|f| f.blur);
    let blur_to = to_filter.as_ref().and_then(|f| f.blur);
    let has_blur = blur_from.is_some() || blur_to.is_some();
    let mut remainder_atoms: Vec<String> = Vec::new();
    if let Some(f) = &from_filter {
        remainder_atoms.extend(f.remainder.iter().cloned());
    }
    if let Some(f) = &to_filter {
        remainder_atoms.extend(f.remainder.iter().cloned());
    }
    let filter_delta = if remainder_atoms.is_empty() {
        None
    } else {
        let landed = if has_blur { "blur() part landed as motion" } else { "nothing lands" };
        Some(format!(
            "filter remainder [{}] has no KDL motion property; dropped — {landed} (only blur animates as motion; brightness/contrast/saturate/grayscale need Sprint 11 picture treatments)",
            remainder_atoms.join(", ")
        ))
    };
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
    // Motion keys that land (filter chains decomposing to `blur`
    // included; structural keys and the filter entry itself never
    // land as a key). With keyframes, the chain properties are the
    // stops' animated keys, in first-seen order.
    let animated: Vec<String> = {
        let mut keys: Vec<String> = Vec::new();
        if let Some(plan) = &kf_plan {
            for stop in &plan.stops {
                for (k, _) in &stop.props {
                    if !keys.contains(k) {
                        keys.push(k.clone());
                    }
                }
            }
        } else {
            for (k, _) in &all_entries {
                if !is_structural_key(k)
                    && k != "filter"
                    && k != "immediateRender"
                    && k != "keyframes"
                    && k != "transformOrigin"
                    && !keys.contains(k)
                {
                    keys.push(k.clone());
                }
            }
        }
        keys
    };
    // With keyframes, a mapped property outside the stop chains has
    // no landing: regular to-vars props (a stop chain governs the
    // property) and from-vars props absent from the chain (nothing
    // captures them). Refuse naming the sibling instead of dropping
    // it — from-vars capture values inside the chain are expected.
    if kf_plan.is_some() {
        let sibling = vars
            .entries
            .iter()
            .find(|(k, _)| map_prop(k).is_some())
            .or_else(|| {
                from_vars.and_then(|v| {
                    v.entries.iter().find(|(k, _)| {
                        map_prop(k).is_some() && !animated.contains(k)
                    })
                })
            });
        if let Some((k, _)) = sibling {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                line,
                call,
                format!(
                    "property {k:?} alongside keyframes is outside the grammar (the stop chains govern each property); refusing the call site"
                ),
            ));
            return;
        }
    }
    // A remainder-only filter chain lands no motion at all: the
    // call is approximated (the remainder names the loss), never a
    // "no mapped properties" refusal.
    if animated.is_empty() && !has_blur && !remainder_atoms.is_empty() {
        match filter_delta.clone() {
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
        ctx.cursor = ctx.cursor.max(start + duration);
        return;
    }
    let mut blur_unresolved = false;
    let mut mask_clamped: Option<(String, f64)> = None;
    let mut kf_deltas: Vec<String> = Vec::new();
    let mut origin_deltas: Vec<String> = Vec::new();
    for (n, idx) in targets.iter().enumerate() {
        let at = if expanded {
            start + n as f64 * stagger
        } else {
            start
        };
        // One motion line per animated property.
        let mut emitted_any = false;
        // A tween at `at` takes over from an earlier `set` hold on
        // the same (clip, property): truncate the hold where this
        // tween begins. GSAP tweens capture the value in effect at
        // their start; KDL windowed sets would otherwise layer over
        // the tween and mask it. Truncation is exact (the set value
        // holds until the tween begins); only a sub-minimum
        // truncation clamps, and that masks the tween's first 1/60s
        // where GSAP shows the tween value (named at the verdict).
        if kind != "set" {
            let mut props_here: Vec<String> = animated
                .iter()
                .filter_map(|k| map_prop(k).map(|p| p.to_string()))
                .collect();
            if has_blur {
                props_here.push("blur".to_string());
            }
            for prop in &props_here {
                if truncate_set_holds(ctx, *idx, prop, at) {
                    mask_clamped = mask_clamped.or(Some((prop.clone(), at)));
                }
            }
        }
        // `transformOrigin` lands on the clip's `origin` pivot,
        // snapped per target (pixel origins need the element size).
        if let Some(oraw) = origin_raw {
            let (el_w, el_h): (Option<f64>, Option<f64>) = ctx
                .clips
                .iter()
                .find(|c| c.index == *idx)
                .filter(|c| c.el_file == ctx.file_rel)
                .and_then(|c| ctx.elements.get(c.el_index))
                .map(|el| {
                    (
                        ctx.el_px(el, "width")
                            .or_else(|| el.get("width").and_then(|w| css_px(w))),
                        ctx.el_px(el, "height")
                            .or_else(|| el.get("height").and_then(|h| css_px(h))),
                    )
                })
                .unwrap_or((None, None));
            match snap_origin(oraw, el_w, el_h) {
                Ok(snap) => match amend_clip_origin(ctx, *idx, &snap.placement) {
                    Ok(()) => {
                        if !origin_deltas.contains(&snap.delta) {
                            origin_deltas.push(snap.delta);
                        }
                        emitted_any = true;
                    }
                    Err(cause) => {
                        ctx.entries.push(Entry::refused(&ctx.file_rel, line, call, cause));
                        return;
                    }
                },
                Err(fail) if fail.droppable => {
                    // Readable but unrenderable here (pixel pivot
                    // without a known element size): the rest of the
                    // call lands and the loss is named, never dropped
                    // silently.
                    let d = format!(
                        "{}; origin attr not written (2D scale/rotation pivot at the built-in point; only 3D turns read the clip origin)",
                        fail.cause
                    );
                    if !origin_deltas.contains(&d) {
                        origin_deltas.push(d);
                    }
                    emitted_any = true;
                }
                Err(fail) => {
                    ctx.entries
                        .push(Entry::refused(&ctx.file_rel, line, call, fail.cause));
                    return;
                }
            }
        }
        // Keyframe chains land as `key` blocks: one per chained
        // property, stops at the cumulative segment ends, starting
        // from the captured value (from-vars, tracked set, or the
        // clip's static value). Each stop's ease rides the segment
        // leaving the previous stop.
        if let Some(plan) = &kf_plan {
            // The call-level ease is the fallback for easing-less
            // stops; the ease zone always maps for tweens.
            let parent_ease: String = ease_name
                .clone()
                .unwrap_or_else(|| "linear".to_string());
            let mut ends: Vec<f64> = Vec::with_capacity(plan.stops.len());
            let mut t = at;
            for s in &plan.stops {
                t += s.duration;
                ends.push(t);
            }
            for prop_key in &animated {
                let prop = map_prop(prop_key).unwrap_or("");
                // Captured start value.
                let from_v = from_vars
                    .and_then(|f| f.get(prop_key))
                    .and_then(|x| ctx.num_of(x));
                let known = ctx
                    .last_set_val
                    .get(&(*idx, prop.to_string()))
                    .copied();
                let cap: f64 = match (from_v, known) {
                    (Some(v), _) => v,
                    (None, Some(v)) => v,
                    (None, None) => {
                        let el = ctx
                            .clips
                            .iter()
                            .find(|c| c.index == *idx && c.el_file == ctx.file_rel)
                            .and_then(|c| ctx.elements.get(c.el_index));
                        let static_val: Option<f64> = match prop {
                            "scale" => Some(1.0),
                            "opacity" => Some(1.0),
                            "rotation" | "rotation-x" | "rotation-y" | "blur" => Some(0.0),
                            "x" | "y" => el.and_then(|e| {
                                ctx.positions
                                    .get(&e.pos)
                                    .map(|(px, py)| if prop == "x" { *px } else { *py })
                            }),
                            "width" => el.and_then(|e| {
                                ctx.el_px(e, "width")
                                    .or_else(|| e.get("width").and_then(|w| css_px(w)))
                            }),
                            "height" => el.and_then(|e| {
                                ctx.el_px(e, "height")
                                    .or_else(|| e.get("height").and_then(|h| css_px(h)))
                            }),
                            "font-size" => Some(
                                el.and_then(|e| ctx.el_px(e, "font-size"))
                                    .unwrap_or(24.0),
                            ),
                            "letter-spacing" => Some(
                                el.and_then(|e| ctx.el_px(e, "letter-spacing"))
                                    .unwrap_or(0.0),
                            ),
                            _ => None,
                        };
                        match static_val {
                            Some(v) => {
                                let d = format!(
                                    "keyframes {prop_key} chain start value {} resolves from the clip's static {prop_key} value; GSAP captures the live value at the tween start, so early eval may differ",
                                    fmt_num(v)
                                );
                                if !kf_deltas.contains(&d) {
                                    kf_deltas.push(d);
                                }
                                v
                            }
                            None => {
                                ctx.entries.push(Entry::refused(
                                    &ctx.file_rel,
                                    line,
                                    call,
                                    format!(
                                        "property {prop_key:?} keyframes start value is not tracked (converter keeps no static {prop_key} for the target clip); refusing the call site"
                                    ),
                                ));
                                return;
                            }
                        }
                    }
                };
                // Chain stops: captures plus one frame per segment
                // end (gaps hold the prior value, as in GSAP).
                let mut frames: Vec<(f64, f64)> = vec![(at, cap)];
                for (i, stop) in plan.stops.iter().enumerate() {
                    let v = stop
                        .props
                        .iter()
                        .find(|(k, _)| k == prop_key)
                        .map(|(_, n)| *n)
                        .unwrap_or(cap);
                    frames.push((ends[i], v));
                }
                // Segment eases ride the leaving stop (validated in
                // plan_keyframes; the call ease is the fallback).
                let seg_ease_name = |i: usize, kf_deltas: &mut Vec<String>| -> String {
                    match &plan.stops[i].ease_raw {
                        Some(e) => match map_ease(e) {
                            Some(m) => {
                                if let Some(d) = m.delta {
                                    if !kf_deltas.contains(&d) {
                                        kf_deltas.push(d);
                                    }
                                }
                                m.kdl
                            }
                            None => unreachable!("stop eases were validated in plan_keyframes"),
                        },
                        None => parent_ease.clone(),
                    }
                };
                let mut text = format!(
                    "  key target={idx} property={}",
                    kdl_str(prop)
                );
                if !expanded && stagger != 0.0 {
                    text.push_str(&format!(" stagger={}", fmt_num(stagger)));
                    if let Some(sf) = stagger_from {
                        text.push_str(&format!(" stagger-from={}", kdl_str(sf)));
                    }
                }
                text.push_str(" {\n");
                for (j, (ft, fv)) in frames.iter().enumerate() {
                    let mut row = format!("    frame at={} value={}", fmt_num(*ft), fmt_num(*fv));
                    if j + 1 < frames.len() {
                        let en = seg_ease_name(j, &mut kf_deltas);
                        row.push_str(&format!(" ease={}", kdl_str(&en)));
                    }
                    text.push_str(&row);
                    text.push('\n');
                }
                text.push_str("  }");
                ctx.motion.push(MotionLine { text });
                // A key chain overtakes a tracked set value on this
                // (clip, property), like any tween.
                ctx.last_set_val.remove(&(*idx, prop.to_string()));
            }
            emitted_any = true;
        }
        if kf_plan.is_none() {
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
            // The value a prior `set` holds on this (clip, property):
            // GSAP `to`/`from` tweens capture it, and a tracked value
            // makes an exact from-to pair where KDL would otherwise
            // resolve the endpoint from the clip's static value.
            let known = ctx
                .last_set_val
                .get(&(*idx, prop.to_string()))
                .copied();
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
                let value_fmt = fmt_num(num.unwrap_or(0.0));
                ctx.motion.push(MotionLine {
                    text: format!(
                        "  set target={idx} property={} start={} duration={} value={}",
                        kdl_str(prop),
                        fmt_num(at),
                        fmt_num(hold),
                        value_fmt,
                    ),
                });
                ctx.set_recs.push(SetRec {
                    motion_idx: ctx.motion.len() - 1,
                    target: *idx,
                    prop: prop.to_string(),
                    start: at,
                    end: at + hold,
                    value_fmt: value_fmt.clone(),
                });
                ctx.last_set_val
                    .insert((*idx, prop.to_string()), num.unwrap_or(0.0));
            } else {
                let kind_attr = match kind {
                    "from" => "from",
                    "from-to" => "from-to",
                    _ => "to",
                };
                let (kind_attr, endpoint_attrs) = if prop_is_color(k) {
                    let to_c = color.clone().unwrap_or_default();
                    let from_c = from_color.clone().unwrap_or_default();
                    let attrs = match kind {
                        "from" => format!(" from={}", kdl_str(&to_c)),
                        "from-to" => format!(
                            " from={} to={}",
                            kdl_str(&from_c),
                            kdl_str(&to_c)
                        ),
                        _ => format!(" to={}", kdl_str(&to_c)),
                    };
                    (kind_attr.to_string(), attrs)
                } else {
                    match kind {
                        "from" => match known {
                            Some(kv) => (
                                "from-to".to_string(),
                                format!(
                                    " from={} to={}",
                                    fmt_num(num.unwrap_or(0.0)),
                                    fmt_num(kv)
                                ),
                            ),
                            None => (
                                kind_attr.to_string(),
                                num.map(|n| format!(" from={}", fmt_num(n)))
                                    .unwrap_or_default(),
                            ),
                        },
                        "from-to" => {
                            let f = from_num.or(known).unwrap_or(0.0);
                            let t = num.or(known).unwrap_or(0.0);
                            (
                                kind_attr.to_string(),
                                format!(" from={} to={}", fmt_num(f), fmt_num(t)),
                            )
                        }
                        _ => match known {
                            Some(kv) => (
                                "from-to".to_string(),
                                format!(
                                    " from={} to={}",
                                    fmt_num(kv),
                                    fmt_num(num.unwrap_or(0.0))
                                ),
                            ),
                            None => (
                                kind_attr.to_string(),
                                num.map(|n| format!(" to={}", fmt_num(n)))
                                    .unwrap_or_default(),
                            ),
                        },
                    }
                };
                let mut text = format!(
                    "  tween target={idx} property={} kind={} start={} duration={}{}",
                    kdl_str(prop),
                    kdl_str(&kind_attr),
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
                // A tween overtakes a tracked set value on this
                // (clip, property); later tweens chain off this one
                // through the engine.
                ctx.last_set_val.remove(&(*idx, prop.to_string()));
            }
            emitted_any = true;
        }
        } // end `if kf_plan.is_none()` (keyframes land via `key` blocks above)
        // The `blur()` atom of a filter chain lands as its own
        // motion line (numeric only; color never rides blur).
        if has_blur {
            let known = ctx
                .last_set_val
                .get(&(*idx, "blur".to_string()))
                .copied();
            if kind == "set" {
                let v = blur_to.unwrap_or(0.0);
                let hold = (ctx.root_duration - at).max(MIN_DURATION);
                let value_fmt = fmt_num(v);
                ctx.motion.push(MotionLine {
                    text: format!(
                        "  set target={idx} property={} start={} duration={} value={}",
                        kdl_str("blur"),
                        fmt_num(at),
                        fmt_num(hold),
                        value_fmt,
                    ),
                });
                ctx.set_recs.push(SetRec {
                    motion_idx: ctx.motion.len() - 1,
                    target: *idx,
                    prop: "blur".to_string(),
                    start: at,
                    end: at + hold,
                    value_fmt: value_fmt.clone(),
                });
                ctx.last_set_val
                    .insert((*idx, "blur".to_string()), v);
            } else {
                let (kind_attr, endpoint_attrs) = match kind {
                    "from" => match known {
                        Some(kv) => (
                            "from-to",
                            format!(
                                " from={} to={}",
                                fmt_num(blur_to.unwrap_or(0.0)),
                                fmt_num(kv)
                            ),
                        ),
                        None => (
                            "from",
                            format!(" from={}", fmt_num(blur_to.unwrap_or(0.0))),
                        ),
                    },
                    "from-to" => {
                        let f = blur_from.or(known).unwrap_or(0.0);
                        let t = blur_to.or(known).unwrap_or(0.0);
                        (
                            "from-to",
                            format!(" from={} to={}", fmt_num(f), fmt_num(t)),
                        )
                    }
                    _ => match known {
                        Some(kv) => (
                            "from-to",
                            format!(
                                " from={} to={}",
                                fmt_num(kv),
                                fmt_num(blur_to.unwrap_or(0.0))
                            ),
                        ),
                        None => ("to", format!(" to={}", fmt_num(blur_to.unwrap_or(0.0)))),
                    },
                };
                let mut text = format!(
                    "  tween target={idx} property={} kind={} start={} duration={}{}",
                    kdl_str("blur"),
                    kdl_str(kind_attr),
                    fmt_num(at),
                    fmt_num(duration.max(MIN_DURATION)),
                    endpoint_attrs,
                );
                if let Some(e) = &ease_name {
                    text.push_str(&format!(" ease={}", kdl_str(e)));
                }
                if !expanded && stagger != 0.0 {
                    text.push_str(&format!(" stagger={}", fmt_num(stagger)));
                    if let Some(sf) = stagger_from {
                        text.push_str(&format!(" stagger-from={}", kdl_str(sf)));
                    }
                }
                ctx.motion.push(MotionLine { text });
                ctx.last_set_val.remove(&(*idx, "blur".to_string()));
                // The from/to state resolves from the clip's static
                // blur 0 unless a set value or both endpoints were
                // authored; GSAP captures the authored filter state
                // instead, so the fidelity report names the delta.
                let unresolved = match kind {
                    "from-to" => {
                        (blur_from.is_none() && known.is_none())
                            || (blur_to.is_none() && known.is_none())
                    }
                    _ => known.is_none(),
                };
                if unresolved {
                    blur_unresolved = true;
                }
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
    // Every approximation names its measured delta (CAP-003: a pass
    // with unreported approximation is false).
    let mut deltas: Vec<String> = Vec::new();
    if !duration_defaulted {
        if let Some(d) = &ease_delta {
            deltas.push(d.clone());
        }
    }
    if let Some(n) = &duration_note {
        deltas.push(n.clone());
    }
    if duration_defaulted {
        deltas.push("duration defaulted 0.5s (GSAP default)".to_string());
    }
    if targets.len() > 1 && stagger != 0.0 {
        deltas.push(format!(
            "selector matched {} clips; stagger {:.4}s expanded to per-element start offsets",
            targets.len(),
            stagger
        ));
    }
    if duration < MIN_DURATION {
        deltas.push(format!(
            "duration {duration:.4}s below the 1/60s clip minimum; clamped to {:.4}s",
            MIN_DURATION
        ));
    }
    if let Some(d) = &immediate_delta {
        deltas.push(d.clone());
    }
    if let Some(d) = &filter_delta {
        deltas.push(d.clone());
    }
    if blur_unresolved {
        deltas.push(
            "blur endpoint resolves from the clip's static blur 0 (converter does not read CSS filter); GSAP animates from the captured authored filter state, so early eval differs"
                .to_string(),
        );
    }
    if let Some((prop, at)) = &mask_clamped {
        deltas.push(format!(
            "a set on {prop} at {at:.4}s would truncate below the 1/60s clip minimum; clamped to {MIN_DURATION:.4}s, masking the tween's first 1/60s where GSAP shows the tween value"
        ));
    }
    if let Some(plan) = &kf_plan {
        if plan.dur_defaulted {
            deltas.push(
                "keyframes stop duration defaulted 0.5s (GSAP default)"
                    .to_string(),
            );
        }
        if !orig_duration_defaulted && (orig_duration - plan.total).abs() > 1e-12 {
            deltas.push(format!(
                "call duration {orig_duration:.4}s not applied; keyframe durations sum to {:.4}s",
                plan.total
            ));
        }
        deltas.extend(kf_deltas);
    }
    deltas.extend(origin_deltas);
    let detail = format!("{call} on {target_raw} at start={start:.4} duration={duration:.4}");
    if deltas.is_empty() {
        ctx.entries.push(Entry::translated(&ctx.file_rel, line, call, detail));
    } else {
        ctx.entries.push(Entry::approximated(
            &ctx.file_rel,
            line,
            call,
            detail,
            deltas.join("; "),
        ));
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
        // A `for`-init binding (`for (let i = 0; ...)`) is a loop
        // variable, not a script const: its value must not resolve
        // later timeline expressions as a fixed number.
        let before = body[..i].trim_end();
        if before.ends_with('(') {
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
        // Bounded arithmetic RHS: a literal, an earlier const, or an
        // expression over them (`const P = D + 0.5`). Anything
        // outside the grammar (runtime values, objects, calls)
        // leaves the name unbound and downstream calls refuse.
        let rhs_end = rhs_end_of(body, p);
        let rhs = body[p..rhs_end].trim().trim_end_matches(';').trim();
        if !rhs.is_empty() {
            if let Ok(n) = ctx.arith_of(rhs, &[]) {
                if n.is_finite() {
                    ctx.consts.entry(name.clone()).or_insert(n);
                }
            }
        }
        i = rhs_end.max(after + 1);
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
        // typeOn helper: `typeOn(sel, t, cps)` definitions in this
        // body. Definition-internal timeline calls are skipped here;
        // call sites expand as a per-glyph macro in `walk_typeon`.
        let typeon_defs = typeon_def_spans(body);
        let has_typeon_def = !typeon_defs.is_empty();
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
            // A call inside a `typeOn` definition body is the macro
            // expansion template, never a call site; skip it.
            if let Some((_, end, _)) = typeon_defs.iter().find(|(s, e, _)| obj >= *s && obj < *e) {
                i = *end;
                continue;
            }
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
            walk_typeon(ctx, *body_start, body, &typeon_defs);
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
    let mut args = split_args(args_text);
    // A trailing comma in the argument list (`tl.to(t, v, 0.1,)`)
    // leaves an empty final arg; drop empties so the tuple forms
    // parse by their real arity.
    while args.last().is_some_and(|a| a.trim().is_empty()) {
        args.pop();
    }
    let kind = match method {
        "to" => "to",
        "from" => "from",
        "fromTo" => "from-to",
        "set" => "set",
        _ => return,
    };
    // to/from/set: (target, vars, pos?) or extended
    // (target, vars, duration, pos). fromTo: (target, from, to, pos?)
    // or extended (target, from, to, duration, pos). A positional
    // duration overrides the vars one.
    let (target_raw, from_vars, to_vars, pos_raw, pos_dur_raw): (
        String,
        Option<Vars>,
        Vars,
        Option<String>,
        Option<String>,
    ) = if method == "fromTo" {
        if args.len() < 3 || args.len() > 5 {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                call_line,
                call,
                format!("fromTo takes 3-5 arguments, found {}; refusing the call site", args.len()),
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
        let (pos_raw, pos_dur_raw) = match args.len() {
            3 => (None, None),
            4 => (args.get(3).cloned(), None),
            _ => (args.get(4).cloned(), args.get(3).cloned()),
        };
        (
            args[0].clone(),
            Some(fv),
            tv,
            pos_raw,
            pos_dur_raw,
        )
    } else {
        if args.len() < 2 || args.len() > 4 {
            ctx.entries.push(Entry::refused(
                &ctx.file_rel,
                call_line,
                call,
                format!("{method} takes 2-4 arguments, found {}; refusing the call site", args.len()),
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
        let (pos_raw, pos_dur_raw) = match args.len() {
            2 => (None, None),
            3 => (args.get(2).cloned(), None),
            _ => (args.get(3).cloned(), args.get(2).cloned()),
        };
        (args[0].clone(), None, tv, pos_raw, pos_dur_raw)
    };
    // Duration (GSAP default 0.5s) and delay.
    let mut duration = 0.5;
    let mut duration_defaulted = false;
    let mut vars_duration: Option<f64> = None;
    if let Some(d) = to_vars.get("duration") {
        match ctx.arith_of(d, &[]) {
            Ok(n) if n >= 0.0 => {
                duration = n;
                vars_duration = Some(n);
            }
            Ok(_) => {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    call,
                    format!("duration {d:?} resolves to a negative time; refusing the call site"),
                ));
                return;
            }
            Err(e) => {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    call,
                    format!("duration {d:?} is outside the numeric grammar ({e}); refusing the call site"),
                ));
                return;
            }
        }
    } else if method != "set" {
        duration_defaulted = true;
    }
    // A positional duration argument overrides the vars one.
    let mut duration_note: Option<String> = None;
    if let Some(pd) = pos_dur_raw.as_deref() {
        match ctx.arith_of(pd, &[]) {
            Ok(n) if n >= 0.0 => {
                duration = n;
                duration_defaulted = false;
                if let Some(v) = vars_duration {
                    if (n - v).abs() > 1e-12 {
                        duration_note = Some(format!(
                            "positional duration {n:.4}s overrode vars duration {v:.4}s"
                        ));
                    }
                }
            }
            Ok(_) => {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    call,
                    format!("positional duration {pd:?} resolves to a negative time; refusing the call site"),
                ));
                return;
            }
            Err(e) => {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    call,
                    format!("positional duration {pd:?} is outside the numeric grammar ({e}); refusing the call site"),
                ));
                return;
            }
        }
    }
    let mut delay = 0.0;
    if let Some(d) = to_vars.get("delay") {
        match ctx.arith_of(d, &[]) {
            Ok(n) if n >= 0.0 => delay = n,
            Ok(_) => {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    call,
                    format!("delay {d:?} resolves to a negative time; refusing the call site"),
                ));
                return;
            }
            Err(e) => {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    call,
                    format!("delay {d:?} is outside the numeric grammar ({e}); refusing the call site"),
                ));
                return;
            }
        }
    }
    // Start from the position argument, else the cursor. Numeric
    // positions evaluate in the bounded arithmetic grammar over
    // script consts; anything else refuses naming the call site.
    let start = match pos_raw.as_deref() {
        None => ctx.cursor,
        Some(p) => {
            let p = p.trim();
            if let Some(s) = js_string(p) {
                // GSAP label-relative positions (`"-=n"` / `"+=n"`).
                if let Some(rest) = s.strip_prefix("-=").or_else(|| s.strip_prefix("+=")) {
                    let negative = s.starts_with("-=");
                    match ctx.arith_of(rest, &[]) {
                        Ok(n) => {
                            if negative {
                                (ctx.cursor - n).max(0.0)
                            } else {
                                ctx.cursor + n
                            }
                        }
                        Err(e) => {
                            ctx.entries.push(Entry::refused(
                                &ctx.file_rel,
                                call_line,
                                call,
                                format!("position {p:?} is outside the numeric grammar ({e}); refusing the call site"),
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
                match ctx.arith_of(p, &[]) {
                    Ok(n) if n >= 0.0 => n,
                    Ok(_) => {
                        ctx.entries.push(Entry::refused(
                            &ctx.file_rel,
                            call_line,
                            call,
                            format!("position {p:?} resolves to a negative time; refusing the call site"),
                        ));
                        return;
                    }
                    Err(e) => {
                        ctx.entries.push(Entry::refused(
                            &ctx.file_rel,
                            call_line,
                            call,
                            format!("position {p:?} is outside the numeric grammar ({e}); refusing the call site"),
                        ));
                        return;
                    }
                }
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
        duration_note,
        call_line,
    );
}

/// Walk `typeOn("sel", t, cps)` calls. `typeOn` is a macro: for each
/// glyph of the target text clip there is one equivalent KDL line
/// with `t + i / cps` evaluated and the same clip targeting, plus
/// the text clip keeps its split/typewriter rendering (CAP-002).
/// A call whose selector, start, or rate is outside the grammar, or
/// whose definition does not match the per-glyph fromTo shape,
/// refuses naming the call site.
fn walk_typeon(
    ctx: &mut FileCtx<'_>,
    body_start: usize,
    body: &str,
    defs: &[(usize, usize, Vec<String>)],
) {
    fn norm_ws(s: &str) -> String {
        s.chars().filter(|c| !c.is_whitespace()).collect()
    }
    // One of the known per-glyph templates of a definition body, or
    // None when the body matches neither.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Tpl {
        FromToOpacity,
        SetDisplay,
    }
    // Every definition in the body must match a template: either a
    // `tl.fromTo` over `{opacity: 0}` then `{opacity: 1, ...}` with
    // duration 0.001, or a `tl.set` revealing `display`, both placed
    // at `{t} + i / {cps}` with the def's own param names. All defs
    // share one template.
    let mut template: Option<Tpl> = None;
    let mut shape_ok = true;
    let mut shape_note = String::new();
    for (start, end, params) in defs {
        let t_name = params.get(1).map(String::as_str).unwrap_or("t");
        let cps_name = params.get(2).map(String::as_str).unwrap_or("cps");
        let flat = norm_ws(&body[*start..*end]);
        let place = format!("{t_name}+i/{cps_name}");
        let tpl = if flat.contains("tl.fromTo(")
            && flat.contains("opacity:0")
            && flat.contains("opacity:1")
            && flat.contains("duration:0.001")
            && flat.contains(&place)
        {
            Some(Tpl::FromToOpacity)
        } else if flat.contains("tl.set(") && flat.contains("display:") && flat.contains(&place) {
            Some(Tpl::SetDisplay)
        } else {
            None
        };
        match (template, tpl) {
            (_, None) => {
                shape_ok = false;
                shape_note = format!(
                    "typeOn definition does not match a known per-glyph template (the per-glyph fromTo grammar, or the set display grammar at {t_name} + i / {cps_name}); refusing the call site"
                );
                break;
            }
            (None, t) => template = t,
            (Some(a), Some(b)) if a != b => {
                shape_ok = false;
                shape_note = format!(
                    "typeOn definitions in this file disagree on the per-glyph template; refusing the call site"
                );
                break;
            }
            _ => {}
        }
    }
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !body.is_char_boundary(i) {
            i += 1;
            continue;
        }
        // Definition heads are macro templates, never call sites.
        if defs.iter().any(|(s, e, _)| i >= *s && i < *e) {
            i += 1;
            continue;
        }
        let is_typeon = body[i..].starts_with("typeOn")
            && body[i + "typeOn".len()..].trim_start().starts_with('(');
        if is_typeon {
            let call_line = ctx.line(body_start + i);
            let mut open = i + "typeOn".len();
            while open < bytes.len() && (bytes[open] == b' ' || bytes[open] == b'\t') {
                open += 1;
            }
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
            if !shape_ok {
                ctx.entries.push(Entry::refused(
                    &ctx.file_rel,
                    call_line,
                    "typeOn",
                    shape_note.clone(),
                ));
                continue;
            }
            if args.len() != 3 {
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
            let t = match ctx.arith_of(&args[1], &[]) {
                Ok(n) if n >= 0.0 => n,
                Ok(_) => {
                    ctx.entries.push(Entry::refused(
                        &ctx.file_rel,
                        call_line,
                        "typeOn",
                        format!("typeOn start {:?} resolves to a negative time; refusing the call site", args[1]),
                    ));
                    continue;
                }
                Err(e) => {
                    ctx.entries.push(Entry::refused(
                        &ctx.file_rel,
                        call_line,
                        "typeOn",
                        format!("typeOn start {:?} is outside the numeric grammar ({e}); refusing the call site", args[1]),
                    ));
                    continue;
                }
            };
            let cps = match ctx.arith_of(&args[2], &[]) {
                Ok(n) => n,
                Err(e) => {
                    ctx.entries.push(Entry::refused(
                        &ctx.file_rel,
                        call_line,
                        "typeOn",
                        format!("typeOn rate {:?} is outside the numeric grammar ({e}); refusing the call site", args[2]),
                    ));
                    continue;
                }
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
            // Resolve the selector to a text clip for its length,
            // through the shared selector grammar (`#id`, `.class`,
            // `#id tag`, `#id .cls`; first match).
            let clip = resolve_selector(ctx, &sel, "typeOn", call_line, true).and_then(|hits| {
                ctx.clips.iter().find(|c| c.index == hits[0]).cloned()
            });
            let Some(clip) = clip else {
                // `resolve_selector` already refused naming the
                // call site.
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
            // Macro expansion: one line per glyph at the evaluated
            // `t + i / cps`, the same targeting as the typewriter
            // clip. The fromTo template lands from-to opacity
            // tweens (authored 0.001s clamped to the 1/60s clip
            // minimum, matching the tween emission); the set
            // template lands opacity holds (KDL has no display
            // property, so the reveal is carried as opacity).
            match template {
                Some(Tpl::FromToOpacity) => {
                    let glyph_dur = 0.001_f64.max(MIN_DURATION);
                    for i in 0..len {
                        let at = t + i as f64 / cps + ctx.offset;
                        ctx.motion.push(MotionLine {
                            text: format!(
                                "  tween target={idx} property=\"opacity\" kind=\"from-to\" start={} duration={} from=0 to=1 ease=\"out-quad\"",
                                fmt_num(at),
                                fmt_num(glyph_dur),
                            ),
                        });
                        ctx.cursor = ctx.cursor.max(at + glyph_dur);
                    }
                    // Named approximations: the engine typewriter
                    // reveal paces over the whole clip window
                    // (measured window rate vs authored cps), and
                    // the per-glyph 0.001s tween is clamped to the
                    // 1/60s clip minimum.
                    let window_rate = if window > 0.0 {
                        len as f64 / window
                    } else {
                        0.0
                    };
                    let mut delta_parts: Vec<String> = Vec::new();
                    if (window_rate - cps).abs() > 1e-9 {
                        delta_parts.push(format!(
                            "reveal paced over the clip window {window:.4}s gives {window_rate:.4} chars/s instead of {cps:.4} chars/s"
                        ));
                    }
                    delta_parts.push(format!(
                        "per-glyph duration 0.001s below the 1/60s clip minimum; clamped to {:.4}s",
                        MIN_DURATION
                    ));
                    ctx.entries.push(Entry::approximated(
                        &ctx.file_rel,
                        call_line,
                        "typeOn",
                        format!(
                            "typeOn {sel:?} at {t:.4} for {} chars at {cps:.4} chars/s (authored reveal {:.4}s) expanded to {} per-glyph from-to opacity tweens at t + i / cps plus split=\"chars\" typewriter on the text clip",
                            len, reveal, len
                        ),
                        delta_parts.join("; "),
                    ));
                }
                Some(Tpl::SetDisplay) => {
                    for i in 0..len {
                        let at = t + i as f64 / cps + ctx.offset;
                        let hold = (ctx.root_duration - at).max(MIN_DURATION);
                        ctx.motion.push(MotionLine {
                            text: format!(
                                "  set target={idx} property=\"opacity\" start={} duration={} value=1",
                                fmt_num(at),
                                fmt_num(hold),
                            ),
                        });
                        ctx.cursor = ctx.cursor.max(at + hold);
                    }
                    let window_rate = if window > 0.0 {
                        len as f64 / window
                    } else {
                        0.0
                    };
                    let mut delta_parts: Vec<String> = Vec::new();
                    delta_parts.push(
                        "set display reveal carried as opacity holds; KDL has no display/visibility motion property"
                            .to_string(),
                    );
                    if (window_rate - cps).abs() > 1e-9 {
                        delta_parts.push(format!(
                            "reveal paced over the clip window {window:.4}s gives {window_rate:.4} chars/s instead of {cps:.4} chars/s"
                        ));
                    }
                    ctx.entries.push(Entry::approximated(
                        &ctx.file_rel,
                        call_line,
                        "typeOn",
                        format!(
                            "typeOn {sel:?} at {t:.4} for {} chars at {cps:.4} chars/s (authored reveal {:.4}s) expanded to {} per-glyph opacity sets at t + i / cps plus split=\"chars\" typewriter on the text clip",
                            len, reveal, len
                        ),
                        delta_parts.join("; "),
                    ));
                }
                None => {
                    // Unreachable: shape failure refuses above.
                    let _ = reveal;
                }
            }
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
    // Compound descendant selectors and compound CSS both walk the
    // element tree; keep it on the context for emission and
    // timeline walks.
    ctx.parents = parents.clone();
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
    for (el_index, el) in elements.iter().enumerate() {
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
            || el.tag == "svg"
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
            el_index,
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
        parents: Vec::new(),
        data_files: Vec::new(),
        last_set_val: HashMap::new(),
        set_recs: Vec::new(),
    };
    if let Some(g) = guide_line(src) {
        sub.guides.push((rel.to_string(), g));
    }
    convert_file(&mut sub, project_root, offset, slot_duration);
    ctx.clips.extend(sub.clips.iter().map(|c| c.clone()));
    ctx.motion.extend(sub.motion.iter().map(|m| m.clone()));
    ctx.entries.extend(sub.entries);
    ctx.assets.extend(sub.assets);
    for (rel, bytes) in &sub.data_files {
        push_data_file(&mut ctx.data_files, rel.clone(), bytes.clone());
    }
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

/// Write the decoded assets (data: URIs, extracted inline `<svg>`)
/// into the output tree.
fn write_data_files(out_root: &Path, files: &[(String, Vec<u8>)]) -> Result<(), String> {
    for (rel, bytes) in files {
        let to = out_root.join(rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::write(&to, bytes)
            .map_err(|e| format!("cannot write {}: {e}", to.display()))?;
    }
    Ok(())
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
        parents: Vec::new(),
        data_files: Vec::new(),
        last_set_val: HashMap::new(),
        set_recs: Vec::new(),
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
        data_files: ctx.data_files.clone(),
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
                data_files: Vec::new(),
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
            // Decoded assets (data: URIs, extracted inline `<svg>`)
            // are written into the output tree beside the copied
            // files, deterministically, after the tree copy.
            if let Err(e) = write_data_files(out, &project.data_files) {
                eprintln!("framesmith convert refused: {e}");
                return 2;
            }
            let n = n + project.data_files.len();
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

