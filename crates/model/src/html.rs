//! A small, forgiving HTML reader.
//!
//! It gives `check` the elements of a page, their attributes and the line
//! each was written on. It is not a browser: it follows the common rules
//! (void elements, raw-text elements, a few implied end tags) and keeps
//! going on anything odd. Where it reads a page differently from a real
//! browser, the render notices and stops (see `page-protocol.md`).

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attr {
    /// Lower case.
    pub name: String,
    pub value: String,
    pub line: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Element {
    /// Lower case.
    pub tag: String,
    pub attrs: Vec<Attr>,
    pub parent: Option<usize>,
    pub line: u32,
    /// The content of a `script`, `style`, `textarea` or `title` element.
    pub text: String,
    /// The line on which `text` starts.
    pub text_line: u32,
    /// Inside inline SVG or MathML (the `svg` or `math` element included).
    pub foreign: bool,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&Attr> {
        self.attrs.iter().find(|attr| attr.name == name)
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.attr(name).map(|attr| attr.value.as_str())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dom {
    /// In document order.
    pub elements: Vec<Element>,
}

impl Dom {
    /// Whether `element` is inside `ancestor` (not the same element).
    pub fn is_inside(&self, element: usize, ancestor: usize) -> bool {
        let mut at = self.elements[element].parent;
        while let Some(parent) = at {
            if parent == ancestor {
                return true;
            }
            at = self.elements[parent].parent;
        }
        false
    }
}

const VOID: [&str; 13] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr",
];
const RAW_TEXT: [&str; 4] = ["script", "style", "textarea", "title"];
/// Start tags that end an open paragraph.
const ENDS_PARAGRAPH: [&str; 31] = [
    "address",
    "article",
    "aside",
    "blockquote",
    "details",
    "div",
    "dl",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "main",
    "menu",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "ul",
    "li",
];
const PARAGRAPH_BOUNDARY: [&str; 7] = ["button", "caption", "table", "td", "th", "object", "template"];

struct Reader<'a> {
    source: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: u32,
    dom: Dom,
    open: Vec<usize>,
}

/// Reads a page. Never fails.
pub fn parse(source: &str) -> Dom {
    let mut reader = Reader {
        source,
        bytes: source.as_bytes(),
        pos: 0,
        line: 1,
        dom: Dom::default(),
        open: Vec::new(),
    };
    reader.run();
    reader.dom
}

/// The offset just after the document type declaration, or 0 when the page
/// has none. Only white space and comments may come before it.
pub fn after_doctype(source: &str) -> usize {
    let bytes = source.as_bytes();
    let mut pos = 0;
    loop {
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let rest = &source[pos..];
        if rest.starts_with("<!--") {
            match rest.find("-->") {
                Some(end) => pos += end + 3,
                None => return 0,
            }
        } else if rest.len() >= 9 && rest.is_char_boundary(9) && rest[..9].eq_ignore_ascii_case("<!doctype") {
            return match rest.find('>') {
                Some(end) => pos + end + 1,
                None => 0,
            };
        } else {
            return 0;
        }
    }
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) {
        if self.bytes.get(self.pos) == Some(&b'\n') {
            self.line += 1;
        }
        self.pos += 1;
    }

    fn skip_to(&mut self, target: usize) {
        while self.pos < target.min(self.bytes.len()) {
            self.bump();
        }
    }

    fn rest(&self) -> &str {
        &self.source[self.pos..]
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.bump();
        }
    }

    fn run(&mut self) {
        while self.pos < self.bytes.len() {
            let Some(offset) = self.rest().find('<') else {
                break;
            };
            self.skip_to(self.pos + offset);
            let rest = self.rest();
            if let Some(comment) = rest.strip_prefix("<!--") {
                let end = comment.find("-->").map_or(self.bytes.len(), |e| self.pos + 4 + e + 3);
                self.skip_to(end);
            } else if rest.starts_with("<!") || rest.starts_with("<?") {
                let end = rest.find('>').map_or(self.bytes.len(), |e| self.pos + e + 1);
                self.skip_to(end);
            } else if rest.starts_with("</") {
                self.bump();
                self.bump();
                let name = self.read_name();
                let end = self.rest().find('>').map_or(self.bytes.len(), |e| self.pos + e + 1);
                self.skip_to(end);
                self.close(&name);
            } else if self.bytes.get(self.pos + 1).is_some_and(u8::is_ascii_alphabetic) {
                self.bump();
                self.start_tag();
            } else {
                self.bump();
            }
        }
    }

    fn read_name(&mut self) -> String {
        let start = self.pos;
        while self.peek().is_some_and(|b| !b.is_ascii_whitespace() && b != b'/' && b != b'>') {
            self.bump();
        }
        self.source[start..self.pos].to_ascii_lowercase()
    }

    fn start_tag(&mut self) {
        let line = self.line;
        let tag = self.read_name();
        let mut attrs: Vec<Attr> = Vec::new();
        let mut self_closing = false;
        loop {
            self.skip_space();
            match self.peek() {
                None => break,
                Some(b'>') => {
                    self.bump();
                    break;
                }
                Some(b'/') => {
                    self.bump();
                    if self.peek() == Some(b'>') {
                        self.bump();
                        self_closing = true;
                        break;
                    }
                }
                Some(_) => {
                    let attr = self.read_attr();
                    // The first of two attributes with one name wins, as in a browser.
                    if !attr.name.is_empty() && !attrs.iter().any(|a| a.name == attr.name) {
                        attrs.push(attr);
                    }
                }
            }
        }
        self.imply_end_tags(&tag);
        let parent = self.open.last().copied();
        let parent_foreign = parent.is_some_and(|p| {
            let parent = &self.dom.elements[p];
            parent.foreign && parent.tag != "foreignobject"
        });
        let foreign = tag == "svg" || tag == "math" || parent_foreign;
        let index = self.dom.elements.len();
        self.dom.elements.push(Element {
            tag: tag.clone(),
            attrs,
            parent,
            line,
            text: String::new(),
            text_line: 0,
            foreign,
        });
        if VOID.contains(&tag.as_str()) || (self_closing && foreign) {
            return;
        }
        if RAW_TEXT.contains(&tag.as_str()) {
            self.read_raw_text(index, &tag);
            return;
        }
        self.open.push(index);
    }

    fn read_attr(&mut self) -> Attr {
        let line = self.line;
        let start = self.pos;
        // The first character is part of the name whatever it is.
        self.bump();
        while self
            .peek()
            .is_some_and(|b| !b.is_ascii_whitespace() && b != b'=' && b != b'>' && b != b'/')
        {
            self.bump();
        }
        let name = self.source[start..self.pos].to_ascii_lowercase();
        self.skip_space();
        if self.peek() != Some(b'=') {
            return Attr {
                name,
                value: String::new(),
                line,
            };
        }
        self.bump();
        self.skip_space();
        let raw = match self.peek() {
            Some(quote @ (b'"' | b'\'')) => {
                self.bump();
                let start = self.pos;
                while self.peek().is_some_and(|b| b != quote) {
                    self.bump();
                }
                let raw = &self.source[start..self.pos];
                self.bump();
                raw
            }
            _ => {
                let start = self.pos;
                while self.peek().is_some_and(|b| !b.is_ascii_whitespace() && b != b'>') {
                    self.bump();
                }
                &self.source[start..self.pos]
            }
        };
        Attr {
            name,
            value: decode_entities(raw),
            line,
        }
    }

    fn read_raw_text(&mut self, index: usize, tag: &str) {
        let text_line = self.line;
        let start = self.pos;
        let closing = format!("</{tag}");
        let lower = self.rest().to_ascii_lowercase();
        let end = lower.find(&closing).map_or(self.bytes.len(), |e| start + e);
        let element = &mut self.dom.elements[index];
        element.text = self.source[start..end].to_string();
        element.text_line = text_line;
        self.skip_to(end);
        let after = self.rest().find('>').map_or(self.bytes.len(), |e| self.pos + e + 1);
        self.skip_to(after);
    }

    /// Closes elements that a start tag ends without an end tag being written.
    fn imply_end_tags(&mut self, tag: &str) {
        match tag {
            "li" => self.close_within(&["li"], &["ul", "ol"]),
            "dt" | "dd" => self.close_within(&["dt", "dd"], &["dl"]),
            "tr" => self.close_within(&["tr"], &["table", "thead", "tbody", "tfoot"]),
            "td" | "th" => self.close_within(&["td", "th"], &["tr", "table"]),
            "option" => self.close_within(&["option"], &["select", "datalist"]),
            _ => {}
        }
        if ENDS_PARAGRAPH.contains(&tag) {
            self.close_within(&["p"], &PARAGRAPH_BOUNDARY);
        }
    }

    fn close_within(&mut self, tags: &[&str], boundary: &[&str]) {
        for depth in (0..self.open.len()).rev() {
            let element = &self.dom.elements[self.open[depth]];
            if element.foreign || boundary.contains(&element.tag.as_str()) {
                return;
            }
            if tags.contains(&element.tag.as_str()) {
                self.open.truncate(depth);
                return;
            }
        }
    }

    fn close(&mut self, tag: &str) {
        if let Some(depth) = self.open.iter().rposition(|&e| self.dom.elements[e].tag == tag) {
            self.open.truncate(depth);
        }
    }
}

fn decode_entities(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest[1..].find(';').filter(|&end| end <= 10).and_then(|end| {
            let name = &rest[1..=end];
            let ch = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ => {
                    let number = name.strip_prefix('#')?;
                    let code = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => number.parse::<u32>().ok()?,
                    };
                    char::from_u32(code)
                }
            };
            ch.map(|ch| (ch, end + 2))
        });
        match decoded {
            Some((ch, used)) => {
                out.push(ch);
                rest = &rest[used..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}
