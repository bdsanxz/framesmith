//! A small JSON reader that remembers the line of every value, so a problem
//! in a motion block can be reported where the author wrote it.

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    /// The number as written, kept as text so times can be read exactly.
    Number(String),
    String(String),
    Array(Vec<Node>),
    Object(Vec<Member>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub value: Json,
    /// Line of the value's first character, counted from 1 within the text.
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub key: String,
    pub line: u32,
    pub value: Node,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError {
    pub line: u32,
    pub message: String,
}

impl Json {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Json::Null => "null",
            Json::Bool(_) => "true or false",
            Json::Number(_) => "a number",
            Json::String(_) => "text",
            Json::Array(_) => "a list",
            Json::Object(_) => "an object",
        }
    }
}

pub fn parse(text: &str) -> Result<Node, JsonError> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        pos: 0,
        line: 1,
    };
    reader.skip_space();
    if reader.pos >= reader.bytes.len() {
        return Err(reader.error("the block is empty"));
    }
    let node = reader.value(0)?;
    reader.skip_space();
    if reader.pos < reader.bytes.len() {
        return Err(reader.error("there is text after the end of the JSON value"));
    }
    Ok(node)
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: u32,
}

const MAX_DEPTH: usize = 64;

impl Reader<'_> {
    fn error(&self, message: &str) -> JsonError {
        JsonError {
            line: self.line,
            message: message.to_string(),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_space(&mut self) {
        while let Some(b) = self.peek() {
            match b {
                b'\n' => {
                    self.line += 1;
                    self.pos += 1;
                }
                b' ' | b'\t' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn value(&mut self, depth: usize) -> Result<Node, JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.error("the JSON is nested too deeply"));
        }
        let line = self.line;
        let value = match self.peek() {
            None => return Err(self.error("the JSON ends before a value")),
            Some(b'{') => self.object(depth)?,
            Some(b'[') => self.array(depth)?,
            Some(b'"') => Json::String(self.string()?),
            Some(b't') => self.word("true", Json::Bool(true))?,
            Some(b'f') => self.word("false", Json::Bool(false))?,
            Some(b'n') => self.word("null", Json::Null)?,
            Some(b'-' | b'0'..=b'9') => self.number()?,
            Some(other) => {
                return Err(self.error(&format!("unexpected character '{}'", printable(other))));
            }
        };
        Ok(Node { value, line })
    }

    fn word(&mut self, word: &str, value: Json) -> Result<Json, JsonError> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error("unexpected word; JSON allows only true, false and null"))
        }
    }

    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        let digits = |r: &mut Self| {
            let from = r.pos;
            while matches!(r.peek(), Some(b'0'..=b'9')) {
                r.pos += 1;
            }
            r.pos > from
        };
        if !digits(self) {
            return Err(self.error("a number needs at least one digit"));
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if !digits(self) {
                return Err(self.error("a number needs a digit after the decimal point"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !digits(self) {
                return Err(self.error("a number needs a digit in its exponent"));
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or_default();
        Ok(Json::Number(text.to_string()))
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.pos += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(b) = self.peek() else {
                return Err(self.error("the JSON ends inside a text value"));
            };
            self.pos += 1;
            match b {
                b'"' => break,
                b'\n' => return Err(self.error("a text value cannot contain a line break")),
                b'\\' => {
                    let Some(escape) = self.peek() else {
                        return Err(self.error("the JSON ends inside a text value"));
                    };
                    self.pos += 1;
                    match escape {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let mut code = self.hex4()?;
                            if (0xD800..0xDC00).contains(&code) && self.bytes[self.pos..].starts_with(b"\\u") {
                                self.pos += 2;
                                let low = self.hex4()?;
                                code = 0x10000 + ((code - 0xD800) << 10) + (low.wrapping_sub(0xDC00) & 0x3FF);
                            }
                            let ch = char::from_u32(code).unwrap_or('\u{FFFD}');
                            out.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
                        }
                        _ => return Err(self.error("unknown escape in a text value")),
                    }
                }
                _ => out.push(b),
            }
        }
        String::from_utf8(out).map_err(|_| self.error("a text value is not valid UTF-8"))
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let digits = self.bytes.get(self.pos..self.pos + 4);
        let parsed = digits
            .and_then(|d| std::str::from_utf8(d).ok())
            .and_then(|d| u32::from_str_radix(d, 16).ok());
        match parsed {
            Some(code) => {
                self.pos += 4;
                Ok(code)
            }
            None => Err(self.error("a \\u escape needs four hexadecimal digits")),
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_space();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.skip_space();
            items.push(self.value(depth + 1)?);
            self.skip_space();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.error("expected ',' or ']' in a list")),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.pos += 1;
        let mut members = Vec::new();
        self.skip_space();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Object(members));
        }
        loop {
            self.skip_space();
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a field name in double quotes"));
            }
            let line = self.line;
            let key = self.string()?;
            self.skip_space();
            if self.peek() != Some(b':') {
                return Err(self.error("expected ':' after a field name"));
            }
            self.pos += 1;
            self.skip_space();
            let value = self.value(depth + 1)?;
            members.push(Member { key, line, value });
            self.skip_space();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Object(members));
                }
                _ => return Err(self.error("expected ',' or '}' in an object")),
            }
        }
    }
}

fn printable(b: u8) -> char {
    if b.is_ascii_graphic() {
        b as char
    } else {
        '?'
    }
}
